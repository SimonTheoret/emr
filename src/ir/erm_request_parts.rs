use http::request::Parts;
use uuid::Uuid;

use super::{HeadersIR, MethodIR, UriIR, VersionIR};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ERMRequestParts {
    pub method: MethodIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
    pub uri: UriIR,
    pub id: Uuid,
}

/// Records the request as it arrived, before `sanitize_headers` touches the forwarded copy.
impl From<&Parts> for ERMRequestParts {
    fn from(parts: &Parts) -> Self {
        Self {
            method: parts.method.clone().into(),
            version: parts.version.into(),
            headers: parts.headers.clone().into(),
            uri: parts.uri.clone().into(),
            // The record mode has no use for the old recorder's UUID, which goes away with it.
            id: Uuid::nil(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use http::{Method, Request, Version};
    use proptest::prelude::*;

    use super::*;
    use crate::ir::{
        headers::tests::headers_strategy, method::tests::method_strategy, uri::tests::uri_strategy,
        version::tests::version_strategy,
    };

    pub(crate) fn request_parts_strategy() -> impl Strategy<Value = ERMRequestParts> {
        (
            method_strategy(),
            version_strategy(),
            headers_strategy(),
            uri_strategy(),
            any::<u128>().prop_map(Uuid::from_u128),
        )
            .prop_map(|(method, version, headers, uri, id)| ERMRequestParts {
                method,
                version,
                headers,
                uri,
                id,
            })
    }

    #[test]
    fn copies_every_request_part() {
        let (request, ()) = Request::builder()
            .method(Method::PATCH)
            .version(Version::HTTP_2)
            .uri("/resource?query=value")
            .header("x-test", "value")
            .header("set-cookie", "first=1")
            .header("set-cookie", "second=2")
            .body(())
            .unwrap()
            .into_parts();

        let parts = ERMRequestParts::from(&request);

        assert_eq!(parts.method, MethodIR::Patch);
        assert_eq!(parts.version, VersionIR::Http2);
        assert_eq!(parts.uri.0, request.uri);
        let headers: http::HeaderMap = parts.headers.into();
        assert_eq!(headers["x-test"], "value");
        assert_eq!(headers.get_all("set-cookie").iter().count(), 2);
        assert_eq!(headers.keys().count(), 2);
        assert_eq!(headers.len(), 3);
    }
}
