use serde::{Deserialize, Serialize};

use super::{FinalRequest, FinalResponse};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mock {
    Request(FinalRequest),
    Response(FinalResponse),
}

impl Mock {
    pub(crate) fn to_pretty_ron(&self) -> ron::error::Result<String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
    }
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
        TimedFrame, UriIR, UrlIR, VersionIR, final_request::tests::final_request_strategy,
        final_response::tests::final_response_strategy,
    };

    fn request_mock() -> Mock {
        Mock::Request(FinalRequest {
            parts: ERMRequestParts {
                method: MethodIR::Post,
                version: VersionIR::Http2,
                headers: HeadersIR(vec![
                    (
                        HeaderNameIR::new(Bytes::from_static(b"x-erm-match")),
                        Bytes::from_static(b"snapshot-request"),
                    ),
                    (
                        HeaderNameIR::new(Bytes::from_static(b"set-cookie")),
                        Bytes::from_static(b"first=1"),
                    ),
                    (
                        HeaderNameIR::new(Bytes::from_static(b"set-cookie")),
                        Bytes::from_static(b"second=2"),
                    ),
                ]),
                uri: UriIR("/resource/%E2%9C%93?query=hello%20world".parse().unwrap()),
                id: Uuid::from_u128(1),
            },
            body: vec![
                TimedFrame {
                    content: Bytes::from_static(b"first"),
                    tslf: Duration::ZERO,
                },
                TimedFrame {
                    content: Bytes::from_static(b"\x00\xffsecond"),
                    tslf: Duration::new(1, 250_000_000),
                },
            ],
        })
    }

    fn response_mock() -> Mock {
        Mock::Response(FinalResponse {
            parts: ERMResponseParts {
                url: UrlIR(
                    Url::parse("https://example.test/resource?query=hello%20world").unwrap(),
                ),
                version: VersionIR::Http2,
                headers: HeadersIR(vec![
                    (
                        HeaderNameIR::new(Bytes::from_static(b"content-type")),
                        Bytes::from_static(b"application/octet-stream"),
                    ),
                    (
                        HeaderNameIR::new(Bytes::from_static(b"set-cookie")),
                        Bytes::from_static(b"first=1"),
                    ),
                    (
                        HeaderNameIR::new(Bytes::from_static(b"set-cookie")),
                        Bytes::from_static(b"second=2"),
                    ),
                ]),
                status: StatusCodeIR(NonZeroU16::new(206).unwrap()),
                id: Uuid::from_u128(1),
            },
            body: vec![
                TimedFrame {
                    content: Bytes::new(),
                    tslf: Duration::ZERO,
                },
                TimedFrame {
                    content: Bytes::from_static(b"response"),
                    tslf: Duration::from_millis(125),
                },
            ],
        })
    }

    #[test]
    fn request_ron_format_is_stable() {
        insta::assert_snapshot!(request_mock().to_pretty_ron().unwrap());
    }

    #[test]
    fn response_ron_format_is_stable() {
        insta::assert_snapshot!(response_mock().to_pretty_ron().unwrap());
    }

    #[test]
    fn reads_v0_request_fixture() {
        let fixture = include_str!("fixtures/v0/request.ron");

        assert_eq!(ron::from_str::<Mock>(fixture).unwrap(), request_mock());
    }

    #[test]
    fn reads_v0_response_fixture() {
        let fixture = include_str!("fixtures/v0/response.ron");

        assert_eq!(ron::from_str::<Mock>(fixture).unwrap(), response_mock());
    }

    proptest! {
        #[test]
        fn request_variant_round_trips_through_ron(request in final_request_strategy()) {
            let mock = Mock::Request(request);
            let serialized = mock.to_pretty_ron().unwrap();

            let deserialized: Mock = ron::from_str(&serialized).unwrap();

            prop_assert_eq!(deserialized, mock);
        }

        #[test]
        fn response_variant_round_trips_through_ron(response in final_response_strategy()) {
            let mock = Mock::Response(response);
            let serialized = mock.to_pretty_ron().unwrap();

            let deserialized: Mock = ron::from_str(&serialized).unwrap();

            prop_assert_eq!(deserialized, mock);
        }
    }
}
