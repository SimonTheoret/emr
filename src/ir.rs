use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::{fmt::Debug, num::NonZeroU16};

use axum::body::BodyDataStream;
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Version};
use url::Url;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
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

#[derive(Debug, Default, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadersIR(pub Vec<(HeaderNameIR, HeaderValueIR)>);

impl From<HeaderMap> for HeadersIR {
    fn from(value: HeaderMap) -> Self {
        let mut vec = Vec::with_capacity(2 << 5);
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
        let mut map = HeaderMap::with_capacity(2 << 5);
        for (n, v) in value.0.into_iter() {
            let name = HeaderName::from_bytes(&n).unwrap();
            let value = HeaderValue::from_bytes(&v).unwrap();
            map.insert(name, value);
        }
        map
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UrlIR(pub Url);

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
