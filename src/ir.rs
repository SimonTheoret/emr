use axum::{body::BodyDataStream, extract::Request, response::IntoResponse};
use std::time::Instant;
use uuid::Uuid;

use eyre::Result;
use futures::StreamExt;
use http::request::Parts;

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, num::NonZeroU16, time::Duration};
use tokio::sync::mpsc::UnboundedSender;
use url::Url;

type Delay = Duration;
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum MethodIR {
    Options,
    #[default]
    Get,
    Post,
    Put,
    Delete,
    Head,
    Trace,
    Connect,
    Patch,
    Query,
}

impl From<Method> for MethodIR {
    fn from(value: Method) -> Self {
        if value == Method::GET {
            Self::Get
        } else if value == Method::OPTIONS {
            Self::Options
        } else if value == Method::POST {
            Self::Post
        } else if value == Method::PUT {
            Self::Put
        } else if value == Method::DELETE {
            Self::Delete
        } else if value == Method::HEAD {
            Self::Head
        } else if value == Method::TRACE {
            Self::Trace
        } else if value == Method::CONNECT {
            Self::Connect
        } else if value == Method::PATCH {
            Self::Patch
        } else if value == Method::QUERY {
            Self::Query
        } else {
            Self::default()
        }
    }
}

impl From<MethodIR> for Method {
    fn from(value: MethodIR) -> Self {
        match value {
            MethodIR::Options => Method::OPTIONS,
            MethodIR::Get => Method::GET,
            MethodIR::Post => Method::POST,
            MethodIR::Put => Method::PUT,
            MethodIR::Delete => Method::DELETE,
            MethodIR::Head => Method::HEAD,
            MethodIR::Trace => Method::TRACE,
            MethodIR::Connect => Method::CONNECT,
            MethodIR::Patch => Method::PATCH,
            MethodIR::Query => Method::QUERY,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub enum VersionIR {
    Http09,
    Http10,
    #[default]
    Http11,
    Http2,
    Http3,
}

impl From<Version> for VersionIR {
    fn from(value: Version) -> Self {
        if value == Version::HTTP_09 {
            Self::Http09
        } else if value == Version::HTTP_10 {
            Self::Http10
        } else if value == Version::HTTP_11 {
            Self::Http11
        } else if value == Version::HTTP_2 {
            Self::Http2
        } else if value == Version::HTTP_3 {
            Self::Http3
        } else {
            Self::default()
        }
    }
}

impl From<VersionIR> for Version {
    fn from(value: VersionIR) -> Self {
        match value {
            VersionIR::Http09 => Version::HTTP_09,
            VersionIR::Http10 => Version::HTTP_10,
            VersionIR::Http11 => Version::HTTP_11,
            VersionIR::Http2 => Version::HTTP_2,
            VersionIR::Http3 => Version::HTTP_3,
        }
    }
}

type HeaderNameIR = Bytes;
type HeaderValueIR = Bytes;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeadersIR(pub Vec<(HeaderNameIR, HeaderValueIR)>);

impl From<HeaderMap> for HeadersIR {
    fn from(value: HeaderMap) -> Self {
        let mut vec = Vec::with_capacity(1 << 5);
        let mut last_name: Option<HeaderName> = None;
        for (opt_name, value) in value.into_iter() {
            let v = Bytes::from_owner(value);
            // This is an invariant from the HeaderMap iterator. If the header's name is `None`, we
            // reuse the last name.
            let name = if let Some(h_name) = opt_name {
                last_name = Some(h_name.clone());
                h_name
            } else {
                last_name.clone().unwrap()
            };
            let n = Bytes::from_owner(name);
            vec.push((n, v));
        }
        HeadersIR(vec)
    }
}

impl From<HeadersIR> for HeaderMap {
    fn from(value: HeadersIR) -> Self {
        let mut map = HeaderMap::with_capacity(1 << 5);
        for (n, v) in value.0.into_iter() {
            let name = HeaderName::from_bytes(&n).unwrap();
            let value = HeaderValue::from_bytes(&v).unwrap();
            map.insert(name, value);
        }
        map
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UrlIR(pub Url);

impl From<Url> for UrlIR {
    fn from(value: Url) -> Self {
        Self(value)
    }
}

#[derive(Debug)]
pub struct BodyStream(pub BodyDataStream);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatusCodeIR(pub NonZeroU16);

impl From<StatusCode> for StatusCodeIR {
    fn from(value: StatusCode) -> Self {
        StatusCodeIR(NonZeroU16::new(value.as_u16()).unwrap())
    }
}

impl From<StatusCodeIR> for StatusCode {
    fn from(value: StatusCodeIR) -> Self {
        StatusCode::from_u16(u16::from(value.0)).unwrap()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct TimedFrame {
    /// The content of the frame
    pub content: Bytes,
    /// Time since last frame (ms)
    pub tslf: Delay,
}

#[derive(Debug)]
pub struct ERMRequest {
    pub body: BodyStream,
    pub method: MethodIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
}

impl ERMRequest {
    pub async fn from_request(
        req: axum::extract::Request,
        channel: UnboundedSender<TimedFrame>,
    ) -> Result<Self> {
        let (parts, body_req) = req.into_parts();

        let Parts {
            method,
            version,
            headers,
            ..
        } = parts.clone();
        let mut last_frame_time: Option<Instant> = None;
        let body = BodyStream(
            axum::body::Body::from_stream(body_req.into_data_stream().map(move |b| match b {
                Err(e) => Err(e),
                Ok(bytes) => {
                    let now = Instant::now();
                    channel
                        .send(TimedFrame {
                            content: bytes.clone(),
                            tslf: now - last_frame_time.unwrap_or(now),
                        })
                        .expect("Error when sending bytes over a channel (Building request)");
                    last_frame_time = Some(now);
                    Ok(bytes)
                }
            }))
            .into_data_stream(),
        );

        Ok(ERMRequest {
            body,
            method: MethodIR::from(method),
            version: VersionIR::from(version),
            headers: HeadersIR::from(headers),
        })
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ERMRequestParts {
    pub method: MethodIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
    pub id: Uuid,
}

impl ERMRequestParts {
    pub fn clone_from_request(req: &Request, id: Uuid) -> Self {
        Self {
            method: req.method().clone().into(),
            version: req.version().into(),
            headers: req.headers().clone().into(),
            id,
        }
    }
}

#[derive(Debug)]
pub struct ERMResponse {
    pub body: BodyStream,
    pub url: UrlIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
    pub status: StatusCodeIR,
    pub id: Uuid,
}

impl ERMResponse {
    pub fn clone_parts(&self) -> ERMResponseParts {
        ERMResponseParts {
            url: self.url.clone(),
            version: self.version,
            headers: self.headers.clone(),
            status: self.status,
            id: self.id,
        }
    }
}

impl IntoResponse for ERMResponse {
    fn into_response(self) -> axum::response::Response {
        let mut incomplete_resp = axum::response::Response::builder()
            .status(self.status)
            .version(self.version.into());
        let empty_headers = incomplete_resp
            .headers_mut()
            .expect("Error when fetching the headers from ERMResponse");
        let current_headers: HeaderMap = self.headers.into();
        empty_headers.extend(current_headers);
        incomplete_resp
            .body(axum::body::Body::from_stream(self.body.0))
            .expect("Error when building a response")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ERMResponseParts {
    pub url: UrlIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
    pub status: StatusCodeIR,
    pub id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FinalResponse {
    pub parts: ERMResponseParts,
    pub body: Vec<TimedFrame>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FinalRequest {
    pub parts: ERMRequestParts,
    pub body: Vec<TimedFrame>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mock {
    Request(FinalRequest),
    Response(FinalResponse),
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::num::NonZeroU16;

    fn bytes_strategy() -> impl Strategy<Value = Bytes> {
        prop::collection::vec(any::<u8>(), 0..64).prop_map(Bytes::from)
    }

    fn headers_strategy() -> impl Strategy<Value = HeadersIR> {
        prop::collection::vec((bytes_strategy(), bytes_strategy()), 0..32).prop_map(HeadersIR)
    }

    fn url_strategy() -> impl Strategy<Value = UrlIR> {
        (0_u16..1000)
            .prop_map(|id| UrlIR(Url::parse(&format!("https://example.test/{id}")).unwrap()))
    }

    fn status_strategy() -> impl Strategy<Value = StatusCodeIR> {
        (100_u16..=599).prop_map(|code| StatusCodeIR(NonZeroU16::new(code).unwrap()))
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

    fn request_parts_strategy() -> impl Strategy<Value = ERMRequestParts> {
        (
            method_strategy(),
            version_strategy(),
            headers_strategy(),
            uuid_strategy(),
        )
            .prop_map(|(method, version, headers, id)| ERMRequestParts {
                method,
                version,
                headers,
                id,
            })
    }

    fn response_parts_strategy() -> impl Strategy<Value = ERMResponseParts> {
        (
            url_strategy(),
            version_strategy(),
            headers_strategy(),
            status_strategy(),
            uuid_strategy(),
        )
            .prop_map(|(url, version, headers, status, id)| ERMResponseParts {
                url,
                version,
                headers,
                status,
                id,
            })
    }

    fn mock_strategy() -> impl Strategy<Value = Mock> {
        prop_oneof![
            (
                request_parts_strategy(),
                prop::collection::vec(timed_frame_strategy(), 0..8)
            )
                .prop_map(|(parts, body)| Mock::Request(FinalRequest { parts, body })),
            (
                response_parts_strategy(),
                prop::collection::vec(timed_frame_strategy(), 0..8)
            )
                .prop_map(|(parts, body)| Mock::Response(FinalResponse { parts, body })),
        ]
    }

    proptest! {
        #[test]
        fn mock_round_trips_through_ron(mock in mock_strategy()) {
            let serialized = ron::ser::to_string_pretty(&mock, ron::ser::PrettyConfig::default()).unwrap();
            let deserialized: Mock = ron::from_str(&serialized).unwrap();

            prop_assert_eq!(deserialized, mock);
        }
    }
}
