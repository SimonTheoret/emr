use uuid::Uuid;

use super::{ERMRequestParts, HeadersIR, TimedFrame};

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FinalRequest {
    pub parts: ERMRequestParts,
    pub body: Vec<TimedFrame>,
}

impl FinalRequest {
    pub fn headers(&self) -> &HeadersIR {
        &self.parts.headers
    }

    pub fn id(&self) -> &Uuid {
        &self.parts.id
    }
}

#[cfg(test)]
mod tests {
    use http::Uri;

    use super::*;
    use crate::ir::{MethodIR, UriIR, VersionIR};

    fn request() -> FinalRequest {
        FinalRequest {
            parts: ERMRequestParts {
                method: MethodIR::Get,
                version: VersionIR::Http11,
                headers: HeadersIR(Vec::new()),
                uri: UriIR(Uri::from_static("/")),
                id: Uuid::from_u128(42),
            },
            body: Vec::new(),
        }
    }

    #[test]
    fn headers_returns_the_request_headers() {
        let request = request();

        assert!(std::ptr::eq(request.headers(), &request.parts.headers));
    }

    #[test]
    fn id_returns_the_request_id() {
        let request = request();

        assert!(std::ptr::eq(request.id(), &request.parts.id));
    }
}
