use axum::extract::Request;
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

impl ERMRequestParts {
    pub fn clone_from_request(req: &Request, id: Uuid) -> Self {
        Self {
            method: req.method().clone().into(),
            version: req.version().into(),
            headers: req.headers().clone().into(),
            uri: req.uri().clone().into(),
            id,
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use http::{Method, Request, Version};

    use super::*;

    #[test]
    fn clone_from_request_copies_all_request_parts() {
        let request = Request::builder()
            .method(Method::PATCH)
            .version(Version::HTTP_2)
            .uri("/resource?query=value")
            .header("x-test", "value")
            .header("set-cookie", "first=1")
            .header("set-cookie", "second=2")
            .body(Body::empty())
            .unwrap();
        let id = Uuid::from_u128(42);

        let parts = ERMRequestParts::clone_from_request(&request, id);

        assert_eq!(parts.method, MethodIR::Patch);
        assert_eq!(parts.version, VersionIR::Http2);
        assert_eq!(parts.uri.0, request.uri().clone());
        let headers: http::HeaderMap = parts.headers.into();
        assert_eq!(headers["x-test"], "value");
        assert_eq!(headers.get_all("set-cookie").iter().count(), 2);
        assert_eq!(parts.id, id);
    }
}
