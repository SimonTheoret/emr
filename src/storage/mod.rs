//! Recorded exchanges. See `docs/storage/spec.md`.

mod exchange;
mod format;
mod id;

pub use exchange::Exchange;
pub use format::FormatKind;
pub use id::{Id, InvalidId};

pub trait Recorder: Send + Sync + 'static {
    /// Queues `exchange` for the writer thread and returns without waiting for the disk.
    fn save(&self, exchange: Exchange);
}

pub trait Replayer: Send + Sync + 'static {
    fn find(&self, id: &Id) -> Option<&Exchange>;
}
