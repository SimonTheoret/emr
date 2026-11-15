use std::time::Duration;

use bytes::Bytes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimedFrame {
    /// The content of the frame.
    pub content: Bytes,
    /// Time since the previous frame. For the first frame, time since the head arrived, so
    /// replay reproduces a slow first byte.
    pub tslf: Duration,
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn timed_frame_strategy() -> impl Strategy<Value = TimedFrame> {
        (
            prop::collection::vec(any::<u8>(), 0..64).prop_map(Bytes::from),
            0_u64..=60_000,
        )
            .prop_map(|(content, millis)| TimedFrame {
                content,
                tslf: Duration::from_millis(millis),
            })
    }
}
