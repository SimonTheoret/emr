use uuid::Uuid;

use super::{ERMResponseParts, TimedFrame};

#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FinalResponse {
    pub parts: ERMResponseParts,
    pub body: Vec<TimedFrame>,
}

impl FinalResponse {
    pub fn id(&self) -> &Uuid {
        &self.parts.id
    }
}

#[cfg(test)]
mod tests {
    use http::{HeaderMap, StatusCode};
    use url::Url;

    use super::*;
    use crate::ir::VersionIR;

    #[test]
    fn id_returns_the_response_id() {
        let response = FinalResponse {
            parts: ERMResponseParts {
                url: Url::parse("https://example.test/").unwrap().into(),
                version: VersionIR::Http11,
                headers: HeaderMap::new().into(),
                status: StatusCode::OK.into(),
                id: Uuid::from_u128(42),
            },
            body: Vec::new(),
        };

        assert!(std::ptr::eq(response.id(), &response.parts.id));
    }
}
