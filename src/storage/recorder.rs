use std::{io, path::Path};

use super::{Exchange, FormatKind, Recorder};

pub(super) struct FsRecorder {}

/// Owns the writer thread.
pub struct Writer {}

impl Writer {
    /// Blocks until every exchange queued before this call is written, then stops the writer thread.
    pub fn finish(self) {
        todo!("drain the queue and join the writer thread")
    }
}

impl Recorder for FsRecorder {
    fn save(&self, _exchange: Exchange) {
        todo!("queue the exchange for the writer thread")
    }
}

pub(super) fn start(_dir: &Path, _format: FormatKind) -> io::Result<(FsRecorder, Writer)> {
    todo!("create the directory and start the writer thread")
}
