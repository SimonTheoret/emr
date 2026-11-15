//! Recorded exchanges. See `docs/storage/spec.md`.

mod exchange;
mod format;
mod id;
mod loader;
mod replayer;

use std::path::Path;

pub use exchange::Exchange;
pub use format::FormatKind;
pub use id::{Id, InvalidId};
pub use loader::LoadError;

pub trait Recorder: Send + Sync + 'static {
    /// Queues `exchange` for the writer thread and returns without waiting for the disk.
    fn save(&self, exchange: Exchange);
}

pub trait Replayer: Send + Sync + 'static {
    fn find(&self, id: &Id) -> Option<&Exchange>;
}

/// Loads every exchange at the top level of `dir`.
pub fn replayer(dir: &Path, format: FormatKind) -> Result<impl Replayer, LoadError> {
    loader::load(dir, format).map(replayer::MemoryReplayer::new)
}
