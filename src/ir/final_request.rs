use uuid::Uuid;

use super::{ERMRequestParts, HeadersIR, TimedFrame};

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FinalRequest {
    pub parts: ERMRequestParts,
    pub body: Vec<TimedFrame>,
}

impl FinalRequest {
    pub fn headers(&self) -> &HeadersIR {
        &self.parts.headers
    }

    pub fn id(&self) -> &Uuid {
        &self.parts.id
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ir::{
        erm_request_parts::tests::request_parts_strategy, timed_frame::tests::timed_frame_strategy,
    };

    pub(crate) fn final_request_strategy() -> impl Strategy<Value = FinalRequest> {
        (
            request_parts_strategy(),
            prop::collection::vec(timed_frame_strategy(), 0..8),
        )
            .prop_map(|(parts, body)| FinalRequest { parts, body })
    }
}
