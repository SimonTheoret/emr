use std::{
    collections::HashMap,
    ffi::OsStr,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use super::{Exchange, FormatKind, Id, format::Format};

#[derive(Debug)]
pub struct FileError {
    pub path: PathBuf,
    pub cause: String,
}

/// Every file that failed to load, so the user can fix them in one pass.
#[derive(Debug)]
pub struct LoadError {
    pub files: Vec<FileError>,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for file in &self.files {
            writeln!(f, "{}: {}", file.path.display(), file.cause)?;
        }
        Ok(())
    }
}

impl std::error::Error for LoadError {}

pub(super) fn load(dir: &Path, format: FormatKind) -> Result<HashMap<Id, Exchange>, LoadError> {
    let format = format.format();
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            tracing::warn!(
                "{} does not exist, so there are no exchanges to replay",
                dir.display()
            );
            return Ok(HashMap::new());
        }
        Err(error) => {
            return Err(LoadError {
                files: vec![FileError {
                    path: dir.to_path_buf(),
                    cause: format!("cannot read the directory: {error}"),
                }],
            });
        }
    };

    let mut exchanges = HashMap::new();
    let mut files = Vec::new();
    for entry in entries {
        let outcome = match entry {
            Ok(entry) => load_entry(&entry.path(), format),
            Err(error) => Some(Err(FileError {
                path: dir.to_path_buf(),
                cause: format!("cannot list the directory: {error}"),
            })),
        };
        match outcome {
            None => {}
            Some(Ok(exchange)) => {
                exchanges.insert(exchange.id.clone(), exchange);
            }
            Some(Err(error)) => files.push(error),
        }
    }

    if !files.is_empty() {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        return Err(LoadError { files });
    }
    if exchanges.is_empty() {
        tracing::warn!("{} holds no exchanges to replay", dir.display());
    }
    Ok(exchanges)
}

/// `None` when `path` is not an `<id>.<ext>` file: a directory, a `.tmp` file, or another extension.
fn load_entry(path: &Path, format: &dyn Format) -> Option<Result<Exchange, FileError>> {
    if path.extension() != Some(OsStr::new(format.extension())) {
        return None;
    }
    let cause = match fs::metadata(path) {
        Ok(metadata) if !metadata.is_file() => return None,
        Ok(_) => match parse_file(path, format) {
            Ok(exchange) => return Some(Ok(exchange)),
            Err(cause) => cause,
        },
        Err(error) => format!("cannot read the file: {error}"),
    };
    Some(Err(FileError {
        path: path.to_path_buf(),
        cause,
    }))
}

fn parse_file(path: &Path, format: &dyn Format) -> Result<Exchange, String> {
    let extension = format.extension();
    let stem = path.file_stem().unwrap_or_default();
    let name = Id::parse(stem.as_encoded_bytes())
        .map_err(|error| format!("the file name is not a valid id: {error}"))?;
    let bytes = fs::read(path).map_err(|error| format!("cannot read the file: {error}"))?;
    let exchange = format
        .deserialize(&bytes)
        .map_err(|error| format!("the file is not a valid {extension} exchange: {error:#}"))?;
    if exchange.id != name {
        return Err(format!(
            "the file name says `{name}` but the file holds the id `{}`; rename it to `{}.{extension}`",
            exchange.id, exchange.id
        ));
    }
    Ok(exchange)
}

#[cfg(all(test, feature = "ron"))]
mod tests {
    use proptest::{
        strategy::{Strategy, ValueTree},
        test_runner::TestRunner,
    };
    use tempfile::TempDir;

    use super::*;
    use crate::storage::exchange::tests::exchange_strategy;

    fn exchange(id: &str) -> Exchange {
        let mut exchange = exchange_strategy()
            .new_tree(&mut TestRunner::deterministic())
            .unwrap()
            .current();
        exchange.id = Id::parse(id.as_bytes()).unwrap();
        exchange
    }

    fn ron(exchange: &Exchange) -> Vec<u8> {
        FormatKind::Ron.format().serialize(exchange).unwrap()
    }

    fn write(dir: &Path, name: &str, exchange: &Exchange) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, ron(exchange)).unwrap();
        path
    }

    fn failed_paths(error: &LoadError) -> Vec<&Path> {
        error.files.iter().map(|file| file.path.as_path()).collect()
    }

    #[test]
    fn q3_ignores_a_tmp_file_next_to_its_exchange() {
        let dir = TempDir::new().unwrap();
        let login = exchange("login");
        write(dir.path(), "login.ron", &login);
        let mut partial = ron(&exchange("login"));
        partial.truncate(partial.len() / 2);
        fs::write(dir.path().join("login.ron.tmp"), partial).unwrap();

        let exchanges =
            load(dir.path(), FormatKind::Ron).expect("a .tmp file does not fail the load");

        assert_eq!(
            exchanges,
            HashMap::from([(login.id.clone(), login)]),
            "only login.ron is loaded"
        );
    }

    #[test]
    fn q4_lists_a_truncated_file_and_a_renamed_file() {
        let dir = TempDir::new().unwrap();
        let mut truncated = ron(&exchange("login"));
        truncated.truncate(truncated.len() / 2);
        let truncated_path = dir.path().join("login.ron");
        fs::write(&truncated_path, truncated).unwrap();
        let renamed_path = write(dir.path(), "logout.ron", &exchange("checkout"));

        let error = load(dir.path(), FormatKind::Ron).expect_err("corrupted files fail the load");

        assert_eq!(
            failed_paths(&error),
            [truncated_path.as_path(), renamed_path.as_path()],
            "both corrupted files are listed, sorted by path"
        );
        assert!(
            !error.files[0].cause.is_empty(),
            "the truncated file has a cause"
        );
        assert!(
            error.files[1].cause.contains("checkout"),
            "the renamed file's cause names the id it holds: {}",
            error.files[1].cause
        );
    }

    #[test]
    fn a_file_name_that_is_not_a_valid_id_is_corrupted() {
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "Login.ron", &exchange("login"));

        let error =
            load(dir.path(), FormatKind::Ron).expect_err("an invalid file name fails the load");

        assert_eq!(
            failed_paths(&error),
            [path.as_path()],
            "Login.ron is listed"
        );
    }

    #[test]
    fn ignores_subdirectories_and_other_extensions() {
        let dir = TempDir::new().unwrap();
        let login = exchange("login");
        write(dir.path(), "login.ron", &login);
        fs::create_dir(dir.path().join("nested.ron")).unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        write(
            &dir.path().join("sub"),
            "checkout.ron",
            &exchange("checkout"),
        );
        fs::write(dir.path().join("notes.txt"), "not an exchange").unwrap();
        fs::write(dir.path().join("logout.json"), "not an exchange").unwrap();
        fs::write(dir.path().join("README"), "not an exchange").unwrap();

        let exchanges =
            load(dir.path(), FormatKind::Ron).expect("ignored entries do not fail the load");

        assert_eq!(
            exchanges,
            HashMap::from([(login.id.clone(), login)]),
            "only the top-level login.ron is loaded"
        );
    }

    #[test]
    fn a_missing_directory_loads_no_exchanges() {
        let dir = TempDir::new().unwrap();

        let exchanges = load(&dir.path().join("missing"), FormatKind::Ron)
            .expect("a missing directory does not fail the load");

        assert!(exchanges.is_empty(), "no exchanges are loaded");
    }

    #[test]
    fn an_empty_directory_loads_no_exchanges() {
        let dir = TempDir::new().unwrap();

        let exchanges =
            load(dir.path(), FormatKind::Ron).expect("an empty directory does not fail the load");

        assert!(exchanges.is_empty(), "no exchanges are loaded");
    }
}
