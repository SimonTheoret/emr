use std::{
    fmt, fs, io, panic,
    path::{Path, PathBuf},
    process,
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
};

use super::{Exchange, FormatKind, Recorder, format::Format};

/// What the writer thread takes from its queue, in order.
#[expect(
    clippy::large_enum_variant,
    reason = "nearly every job is a Save, so boxing it would add an allocation and save no memory"
)]
enum Job {
    Save(Exchange),
    Finish,
}

pub(super) struct FsRecorder {
    jobs: Sender<Job>,
}

/// Owns the writer thread.
pub struct Writer {
    jobs: Sender<Job>,
    thread: JoinHandle<()>,
}

impl Writer {
    /// Blocks until every exchange queued before this call is written, then stops the writer thread.
    pub fn finish(self) {
        // The send fails only if the writer already stopped after a failure.
        let _ = self.jobs.send(Job::Finish);
        if let Err(panic) = self.thread.join() {
            panic::resume_unwind(panic);
        }
    }
}

impl Recorder for FsRecorder {
    fn save(&self, exchange: Exchange) {
        // The send fails only once the writer has stopped, and then the exchange is dropped.
        let _ = self.jobs.send(Job::Save(exchange));
    }
}

pub(super) fn start(dir: &Path, format: FormatKind) -> io::Result<(FsRecorder, Writer)> {
    start_with(dir, format, report_and_exit)
}

/// `start`, with `on_failure` in place of exiting the process, so tests can observe a failed write.
fn start_with(
    dir: &Path,
    format: FormatKind,
    on_failure: impl FnOnce(WriteError) + Send + 'static,
) -> io::Result<(FsRecorder, Writer)> {
    fs::create_dir_all(dir)?;
    let (jobs, queue) = mpsc::channel();
    let dir = dir.to_path_buf();
    let format = format.format();
    let thread = thread::Builder::new()
        .name("erm-writer".into())
        .spawn(move || {
            // Stops at `Finish`, or once every sender is dropped.
            while let Ok(Job::Save(exchange)) = queue.recv() {
                if let Err(error) = write_exchange(&dir, format, &exchange) {
                    on_failure(error);
                    return;
                }
            }
        })?;
    Ok((FsRecorder { jobs: jobs.clone() }, Writer { jobs, thread }))
}

fn report_and_exit(error: WriteError) {
    eprintln!("error: {error}");
    process::exit(1);
}

/// Writes `<id>.<ext>.tmp`, then renames it over `<id>.<ext>`.
fn write_exchange(dir: &Path, format: &dyn Format, exchange: &Exchange) -> Result<(), WriteError> {
    let name = format!("{}.{}", exchange.id, format.extension());
    let path = dir.join(&name);
    let bytes = format
        .serialize(exchange)
        .map_err(|cause| WriteError::Serialize {
            path: path.clone(),
            cause,
        })?;
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, bytes).map_err(|cause| WriteError::Write {
        path: tmp.clone(),
        cause,
    })?;
    fs::rename(&tmp, &path).map_err(|cause| WriteError::Rename {
        from: tmp,
        to: path,
        cause,
    })
}

#[derive(Debug)]
enum WriteError {
    Serialize {
        path: PathBuf,
        cause: eyre::Report,
    },
    Write {
        path: PathBuf,
        cause: io::Error,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
        cause: io::Error,
    },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialize { path, cause } => {
                write!(
                    f,
                    "could not serialize the exchange for {}: {cause:#}",
                    path.display()
                )
            }
            Self::Write { path, cause } => write!(f, "could not write {}: {cause}", path.display()),
            Self::Rename { from, to, cause } => write!(
                f,
                "could not rename {} to {}: {cause}",
                from.display(),
                to.display()
            ),
        }
    }
}

#[cfg(all(test, feature = "ron"))]
mod tests {
    use std::{env, process::Command, time::Duration};

    use bytes::Bytes;
    use http::StatusCode;
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::*;
    use crate::{
        ir::{
            ERMRequestParts, ERMResponseParts, FinalRequest, FinalResponse, HeadersIR, MethodIR,
            StatusCodeIR, TimedFrame, VersionIR,
        },
        storage::Id,
    };

    const EXIT_TEST: &str = "storage::recorder::tests::a_failed_write_exits_with_status_1";
    const EXIT_TEST_DIR: &str = "ERM_RECORDER_EXIT_TEST_DIR";

    fn exchange(id: &str, body: &str) -> Exchange {
        Exchange {
            id: Id::parse(id.as_bytes()).unwrap(),
            request: FinalRequest {
                parts: ERMRequestParts {
                    method: MethodIR::Get,
                    version: VersionIR::Http11,
                    headers: HeadersIR(Vec::new()),
                    uri: "/login".parse().unwrap(),
                    id: Uuid::nil(),
                },
                body: Vec::new(),
            },
            response: FinalResponse {
                parts: ERMResponseParts {
                    url: "http://upstream.test/login".parse().unwrap(),
                    version: VersionIR::Http11,
                    headers: HeadersIR(Vec::new()),
                    status: StatusCodeIR::from(StatusCode::OK),
                    id: Uuid::nil(),
                },
                body: vec![TimedFrame {
                    content: Bytes::copy_from_slice(body.as_bytes()),
                    tslf: Duration::from_millis(5),
                }],
            },
        }
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    fn read(path: &Path) -> Exchange {
        let bytes = fs::read(path).unwrap();
        FormatKind::Ron.format().deserialize(&bytes).unwrap()
    }

    #[test]
    fn start_creates_a_missing_directory() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("recordings").join("today");
        let (_recorder, writer) = start(&dir, FormatKind::Ron).unwrap();
        writer.finish();
        assert!(dir.is_dir(), "start created {}", dir.display());
    }

    #[test]
    fn saving_an_id_again_replaces_its_file() {
        let dir = TempDir::new().unwrap();
        let (recorder, writer) = start(dir.path(), FormatKind::Ron).unwrap();
        recorder.save(exchange("login", "first"));
        recorder.save(exchange("login", "second"));
        writer.finish();
        assert_eq!(
            file_names(dir.path()),
            ["login.ron"],
            "one file holds the id, with no .tmp file left behind"
        );
        assert_eq!(
            read(&dir.path().join("login.ron")),
            exchange("login", "second"),
            "the file holds the second exchange"
        );
    }

    #[test]
    fn no_temporary_file_remains_after_a_save() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("login.ron.tmp"), "interrupted").unwrap();
        let (recorder, writer) = start(dir.path(), FormatKind::Ron).unwrap();
        recorder.save(exchange("login", "body"));
        writer.finish();
        assert_eq!(
            file_names(dir.path()),
            ["login.ron"],
            "the save replaced the temporary file an interrupted write left behind"
        );
        assert_eq!(
            read(&dir.path().join("login.ron")),
            exchange("login", "body"),
            "the file holds the saved exchange"
        );
    }

    #[test]
    fn finish_writes_every_queued_exchange() {
        let dir = TempDir::new().unwrap();
        let (recorder, writer) = start(dir.path(), FormatKind::Ron).unwrap();
        let ids: Vec<String> = (0..1000).map(|i| format!("exchange-{i}")).collect();
        thread::scope(|scope| {
            for chunk in ids.chunks(250) {
                let recorder = &recorder;
                scope.spawn(move || {
                    for id in chunk {
                        recorder.save(exchange(id, id));
                    }
                });
            }
        });
        writer.finish();

        let mut expected: Vec<String> = ids.iter().map(|id| format!("{id}.ron")).collect();
        expected.sort();
        assert_eq!(
            file_names(dir.path()),
            expected,
            "every queued exchange has its file, with no .tmp file left behind"
        );
        for id in &ids {
            assert_eq!(
                read(&dir.path().join(format!("{id}.ron"))),
                exchange(id, id),
                "{id}.ron holds its exchange"
            );
        }
    }

    #[test]
    fn a_save_after_finish_does_not_panic() {
        let dir = TempDir::new().unwrap();
        let (recorder, writer) = start(dir.path(), FormatKind::Ron).unwrap();
        writer.finish();
        recorder.save(exchange("late", "body"));
    }

    #[test]
    fn a_failed_write_reaches_the_failure_path_with_the_file_and_the_cause() {
        let dir = TempDir::new().unwrap();
        let (failures, failure) = mpsc::channel();
        let (recorder, writer) = start_with(dir.path(), FormatKind::Ron, move |error| {
            failures.send(error).unwrap()
        })
        .unwrap();
        fs::remove_dir(dir.path()).unwrap();
        recorder.save(exchange("login", "body"));
        writer.finish();

        let error = failure
            .try_recv()
            .expect("the writer called the failure path before finish returned");
        let WriteError::Write { path, cause } = &error else {
            panic!("expected writing the temporary file to fail, got: {error}");
        };
        assert_eq!(
            path,
            &dir.path().join("login.ron.tmp"),
            "the error names the file"
        );
        assert_eq!(
            cause.kind(),
            io::ErrorKind::NotFound,
            "the cause is the missing directory"
        );
        let message = error.to_string();
        assert!(
            message.contains(&path.display().to_string()) && message.contains(&cause.to_string()),
            "the printed error names the file and the cause: {message}"
        );
    }

    #[test]
    fn a_failed_write_exits_with_status_1() {
        if let Some(dir) = env::var_os(EXIT_TEST_DIR) {
            let (recorder, writer) = start(Path::new(&dir), FormatKind::Ron).unwrap();
            fs::remove_dir(&dir).unwrap();
            recorder.save(exchange("login", "body"));
            writer.finish();
            unreachable!("finish returned after a failed write");
        }

        let dir = TempDir::new().unwrap();
        let output = Command::new(env::current_exe().unwrap())
            .args([EXIT_TEST, "--exact", "--nocapture"])
            .env(EXIT_TEST_DIR, dir.path())
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(1),
            "the recorder exits with status 1; stderr: {stderr}"
        );
        let file = dir.path().join("login.ron.tmp");
        assert!(
            stderr.contains(&file.display().to_string()),
            "stderr names {}: {stderr}",
            file.display()
        );
    }
}
