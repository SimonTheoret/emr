use std::fmt;

use axum::{
    extract::FromRequestParts,
    response::{IntoResponse, Response},
};
use http::{HeaderMap, StatusCode, request::Parts};

use crate::storage::{Id, InvalidId};

const MATCH_HEADER: &str = "x-erm-match";

/// The request's `id`: its one `x-erm-match` value, validated.
///
/// As a handler argument it rejects the request with `400` before the handler runs, so a
/// handler never sees, forwards, or saves a request without a valid `id`.
pub struct MatchId(pub Id);

#[derive(Debug, PartialEq, Eq)]
pub enum MatchIdRejection {
    Missing,
    Repeated,
    Invalid(InvalidId),
}

impl<S: Send + Sync> FromRequestParts<S> for MatchId {
    type Rejection = MatchIdRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        match_id(&parts.headers).map(Self)
    }
}

fn match_id(headers: &HeaderMap) -> Result<Id, MatchIdRejection> {
    let mut values = headers.get_all(MATCH_HEADER).iter();
    match (values.next(), values.next()) {
        (None, _) => Err(MatchIdRejection::Missing),
        (Some(_), Some(_)) => Err(MatchIdRejection::Repeated),
        (Some(value), None) => Id::parse(value.as_bytes()).map_err(MatchIdRejection::Invalid),
    }
}

impl fmt::Display for MatchIdRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(f, "the request has no {MATCH_HEADER} header"),
            Self::Repeated => write!(f, "the request has more than one {MATCH_HEADER} header"),
            Self::Invalid(cause) => write!(f, "the {MATCH_HEADER} header is invalid: {cause}"),
        }
    }
}

impl IntoResponse for MatchIdRejection {
    fn into_response(self) -> Response {
        (StatusCode::BAD_REQUEST, self.to_string()).into_response()
    }
}
