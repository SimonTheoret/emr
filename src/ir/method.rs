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
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn method_strategy() -> impl Strategy<Value = MethodIR> {
        method_case_strategy().prop_map(|(method, _)| method)
    }

    fn method_case_strategy() -> impl Strategy<Value = (MethodIR, Method)> {
        prop::sample::select(method_cases().to_vec())
    }

    fn method_cases() -> [(MethodIR, Method); 10] {
        [
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
        ]
    }

    #[test]
    fn an_unknown_http_method_falls_back_to_get() {
        let custom = Method::from_bytes(b"CUSTOM").unwrap();

        assert_eq!(MethodIR::from(custom), MethodIR::Get);
    }

    proptest! {
        #[test]
        fn converts_supported_methods_in_both_directions(
            (method, http_method) in method_case_strategy()
        ) {
            prop_assert_eq!(MethodIR::from(http_method.clone()), method);
            prop_assert_eq!(Method::from(method), http_method);
        }
    }
}
