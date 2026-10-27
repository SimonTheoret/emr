use serde::{Deserialize, Serialize};

use super::{FinalRequest, FinalResponse};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mock {
    Request(FinalRequest),
    Response(FinalResponse),
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ir::{
        final_request::tests::final_request_strategy,
        final_response::tests::final_response_strategy,
    };

    proptest! {
        #[test]
        fn request_variant_round_trips_through_ron(request in final_request_strategy()) {
            let mock = Mock::Request(request);
            let serialized = ron::ser::to_string_pretty(
                &mock,
                ron::ser::PrettyConfig::default(),
            ).unwrap();

            let deserialized: Mock = ron::from_str(&serialized).unwrap();

            prop_assert_eq!(deserialized, mock);
        }

        #[test]
        fn response_variant_round_trips_through_ron(response in final_response_strategy()) {
            let mock = Mock::Response(response);
            let serialized = ron::ser::to_string_pretty(
                &mock,
                ron::ser::PrettyConfig::default(),
            ).unwrap();

            let deserialized: Mock = ron::from_str(&serialized).unwrap();

            prop_assert_eq!(deserialized, mock);
        }
    }
}
