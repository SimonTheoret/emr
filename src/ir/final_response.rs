use uuid::Uuid;

use super::{ERMResponseParts, TimedFrame};

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FinalResponse {
    pub parts: ERMResponseParts,
    pub body: Vec<TimedFrame>,
}

impl FinalResponse {
    pub fn id(&self) -> &Uuid {
        &self.parts.id
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ir::{
        erm_response_parts::tests::response_parts_strategy,
        timed_frame::tests::timed_frame_strategy,
    };

    pub(crate) fn final_response_strategy() -> impl Strategy<Value = FinalResponse> {
        (
            response_parts_strategy(),
            prop::collection::vec(timed_frame_strategy(), 0..8),
        )
            .prop_map(|(parts, body)| FinalResponse { parts, body })
    }
}
