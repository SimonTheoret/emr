use serde::{Deserialize, Serialize};

use super::Id;
use crate::ir::{FinalRequest, FinalResponse};

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exchange {
    pub id: Id,
    pub request: FinalRequest,
    pub response: FinalResponse,
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{
        ir::{final_request_strategy, final_response_strategy},
        storage::id::tests::id_strategy,
    };

    pub(crate) fn exchange_strategy() -> impl Strategy<Value = Exchange> {
        (
            id_strategy(),
            final_request_strategy(),
            final_response_strategy(),
        )
            .prop_map(|(id, request, response)| Exchange {
                id,
                request,
                response,
            })
    }
}
