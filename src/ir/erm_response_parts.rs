use uuid::Uuid;

use super::{HeadersIR, StatusCodeIR, UrlIR, VersionIR};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ERMResponseParts {
    pub url: UrlIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
    pub status: StatusCodeIR,
    pub id: Uuid,
}
