use http::Method;
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_every_supported_http_method_to_ir() {
        let cases = [
            (Method::OPTIONS, MethodIR::Options),
            (Method::GET, MethodIR::Get),
            (Method::POST, MethodIR::Post),
            (Method::PUT, MethodIR::Put),
            (Method::DELETE, MethodIR::Delete),
            (Method::HEAD, MethodIR::Head),
            (Method::TRACE, MethodIR::Trace),
            (Method::CONNECT, MethodIR::Connect),
            (Method::PATCH, MethodIR::Patch),
            (Method::QUERY, MethodIR::Query),
        ];

        for (method, expected) in cases {
            assert_eq!(MethodIR::from(method), expected);
        }
    }

    #[test]
    fn an_unknown_http_method_falls_back_to_get() {
        let custom = Method::from_bytes(b"CUSTOM").unwrap();

        assert_eq!(MethodIR::from(custom), MethodIR::Get);
    }

    #[test]
    fn converts_every_ir_method_to_http() {
        let cases = [
            (MethodIR::Options, Method::OPTIONS),
            (MethodIR::Get, Method::GET),
            (MethodIR::Post, Method::POST),
            (MethodIR::Put, Method::PUT),
            (MethodIR::Delete, Method::DELETE),
            (MethodIR::Head, Method::HEAD),
            (MethodIR::Trace, Method::TRACE),
            (MethodIR::Connect, Method::CONNECT),
            (MethodIR::Patch, Method::PATCH),
            (MethodIR::Query, Method::QUERY),
        ];

        for (method, expected) in cases {
            assert_eq!(Method::from(method), expected);
        }
    }
}
