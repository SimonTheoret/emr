use std::time::Duration;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimedFrame {
    /// The content of the frame.
    pub content: Bytes,
    /// Time since the last frame.
    pub tslf: Duration,
}
