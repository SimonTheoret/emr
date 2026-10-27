use serde::{Deserialize, Serialize};

use super::{FinalRequest, FinalResponse};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mock {
    Request(FinalRequest),
    Response(FinalResponse),
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU16, time::Duration};

    use bytes::Bytes;
    use proptest::prelude::*;
    use url::Url;
    use uuid::Uuid;

    use super::*;
    use crate::ir::{
        ERMRequestParts, ERMResponseParts, HeaderNameIR, HeadersIR, MethodIR, StatusCodeIR,
        TimedFrame, UriIR, UrlIR, VersionIR,
    };

    fn bytes_strategy() -> impl Strategy<Value = Bytes> {
        prop::collection::vec(any::<u8>(), 0..64).prop_map(Bytes::from)
    }

    fn header_names_strategy() -> impl Strategy<Value = HeaderNameIR> {
        prop::collection::vec(any::<u8>(), 0..64)
            .prop_map(|value| HeaderNameIR::new(Bytes::from_owner(value)))
    }

    fn headers_strategy() -> impl Strategy<Value = HeadersIR> {
        prop::collection::vec((header_names_strategy(), bytes_strategy()), 0..32)
            .prop_map(HeadersIR)
    }

    fn url_strategy() -> impl Strategy<Value = UrlIR> {
        (0_u16..1000)
            .prop_map(|id| UrlIR(Url::parse(&format!("https://example.test/{id}")).unwrap()))
    }

    fn uri_strategy() -> impl Strategy<Value = UriIR> {
        (0_u16..1000).prop_map(|id| UriIR(format!("/resource/{id}?q={id}").parse().unwrap()))
    }

    fn status_strategy() -> impl Strategy<Value = StatusCodeIR> {
        (100_u16..=999).prop_map(|code| StatusCodeIR(NonZeroU16::new(code).unwrap()))
    }

    fn uuid_strategy() -> impl Strategy<Value = Uuid> {
        any::<u128>().prop_map(Uuid::from_u128)
    }

    fn method_strategy() -> impl Strategy<Value = MethodIR> {
        prop_oneof![
            Just(MethodIR::Options),
            Just(MethodIR::Get),
            Just(MethodIR::Post),
            Just(MethodIR::Put),
            Just(MethodIR::Delete),
            Just(MethodIR::Head),
            Just(MethodIR::Trace),
            Just(MethodIR::Connect),
            Just(MethodIR::Patch),
            Just(MethodIR::Query),
        ]
    }

    fn version_strategy() -> impl Strategy<Value = VersionIR> {
        prop_oneof![
            Just(VersionIR::Http09),
            Just(VersionIR::Http10),
            Just(VersionIR::Http11),
            Just(VersionIR::Http2),
            Just(VersionIR::Http3),
        ]
    }

    fn timed_frame_strategy() -> impl Strategy<Value = TimedFrame> {
        (bytes_strategy(), 0_u64..=60_000).prop_map(|(content, millis)| TimedFrame {
            content,
            tslf: Duration::from_millis(millis),
        })
    }

    fn request_strategy() -> impl Strategy<Value = FinalRequest> {
        (
            method_strategy(),
            version_strategy(),
            headers_strategy(),
            uri_strategy(),
            uuid_strategy(),
            prop::collection::vec(timed_frame_strategy(), 0..8),
        )
            .prop_map(|(method, version, headers, uri, id, body)| FinalRequest {
                parts: ERMRequestParts {
                    method,
                    version,
                    headers,
                    uri,
                    id,
                },
                body,
            })
    }

    fn response_strategy() -> impl Strategy<Value = FinalResponse> {
        (
            url_strategy(),
            version_strategy(),
            headers_strategy(),
            status_strategy(),
            uuid_strategy(),
            prop::collection::vec(timed_frame_strategy(), 0..8),
        )
            .prop_map(|(url, version, headers, status, id, body)| FinalResponse {
                parts: ERMResponseParts {
                    url,
                    version,
                    headers,
                    status,
                    id,
                },
                body,
            })
    }

    proptest! {
        #[test]
        fn request_variant_round_trips_through_ron(request in request_strategy()) {
            let mock = Mock::Request(request);
            let serialized = ron::ser::to_string_pretty(
                &mock,
                ron::ser::PrettyConfig::default(),
            ).unwrap();

            let deserialized: Mock = ron::from_str(&serialized).unwrap();

            prop_assert_eq!(deserialized, mock);
        }

        #[test]
        fn response_variant_round_trips_through_ron(response in response_strategy()) {
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
