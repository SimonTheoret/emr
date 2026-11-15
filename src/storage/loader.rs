use std::{
    collections::HashMap,
    fmt,
    path::{Path, PathBuf},
};

use super::{Exchange, FormatKind, Id};

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

pub(super) fn load(_dir: &Path, _format: FormatKind) -> Result<HashMap<Id, Exchange>, LoadError> {
    todo!("load the exchanges at the top level of the directory")
}
