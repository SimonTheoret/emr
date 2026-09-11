use std::fmt;

use tracing::instrument;

use uuid::Uuid;

use crate::ir::{FinalRequest, FinalResponse, HeaderNameIR, HeaderValueIR, Mock};

pub const DEFAULT_MATCHING_HEADER: &[u8] = b"x-erm-match";

/// Why [`match_by_header`] did not find a recorded response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchByHeaderError {
    /// No recorded request carries the matching header with the incoming value.
    NoMatchingRequest,
    /// A recorded request matched, but no recorded response shares its id.
    NoResponseForRequest(Uuid),
}

impl fmt::Display for MatchByHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMatchingRequest => write!(f, "no recorded request matches the header value"),
            Self::NoResponseForRequest(id) => {
                write!(f, "no recorded response for matched request {id}")
            }
        }
    }
}

impl std::error::Error for MatchByHeaderError {}

/// Finds a recorded request carrying `matching_header` with `matching_value`.
///
/// If several recorded requests match, the first one yielded by `mocks` wins.
fn find_matching_request<'a>(
    mocks: impl IntoIterator<Item = &'a Mock>,
    matching_header: &HeaderNameIR,
    matching_value: &HeaderValueIR,
) -> Option<&'a FinalRequest> {
    mocks
        .into_iter()
        .filter_map(|m| match m {
            Mock::Request(req) => Some(req),
            _ => None,
        })
        .find(|req| {
            req.headers()
                .0
                .iter()
                .any(|(k, v)| k == matching_header && v == matching_value)
        })
}

/// Finds the recorded response whose id is `id`.
fn find_response_by_id<'a>(
    mocks: impl IntoIterator<Item = &'a Mock>,
    id: &Uuid,
) -> Option<&'a FinalResponse> {
    mocks
        .into_iter()
        .filter_map(|m| match m {
            Mock::Response(res) => Some(res),
            _ => None,
        })
        .find(|res| res.id() == id)
}

#[instrument(skip_all)]
pub fn match_by_header<'a>(
    mocks: impl IntoIterator<Item = &'a Mock, IntoIter: Clone>,
    matching_header: &HeaderNameIR,
    matching_value: &HeaderValueIR,
) -> Result<&'a FinalResponse, MatchByHeaderError> {
    let mocks = mocks.into_iter();
    let matched_req = find_matching_request(mocks.clone(), matching_header, matching_value)
        .ok_or(MatchByHeaderError::NoMatchingRequest)?;
    let id = matched_req.id();
    find_response_by_id(mocks, id).ok_or(MatchByHeaderError::NoResponseForRequest(*id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        ERMRequestParts, ERMResponseParts, FinalRequest, HeaderNameIR, HeadersIR, StatusCodeIR,
        UriIR, UrlIR,
    };
    use bytes::Bytes;
    use proptest::proptest;
    use std::num::NonZeroU16;
    use std::{collections::HashMap, path::PathBuf};
    use url::Url;
    use uuid::Uuid;

    fn headers(pairs: &[(String, String)]) -> HeadersIR {
        HeadersIR(
            pairs
                .iter()
                .map(|(k, v)| {
                    (
                        HeaderNameIR::new(Bytes::copy_from_slice(k.as_bytes())),
                        Bytes::copy_from_slice(v.as_bytes()),
                    )
                })
                .collect(),
        )
    }

    fn request(id: Uuid, headers: HeadersIR) -> Mock {
        Mock::Request(FinalRequest {
            parts: ERMRequestParts {
                method: Default::default(),
                version: Default::default(),
                headers,
                uri: UriIR("/".parse().unwrap()),
                id,
            },
            body: vec![],
        })
    }

    fn response(id: Uuid, status: u16) -> Mock {
        Mock::Response(FinalResponse {
            parts: ERMResponseParts {
                url: UrlIR(Url::parse("https://example.test/").unwrap()),
                version: Default::default(),
                headers: headers(&[]),
                status: StatusCodeIR(NonZeroU16::new(status).unwrap()),
                id,
            },
            body: vec![],
        })
    }

    fn mocks(entries: Vec<Mock>) -> HashMap<PathBuf, Mock> {
        entries
            .into_iter()
            .enumerate()
            .map(|(i, m)| (PathBuf::from(format!("{i}.ron")), m))
            .collect()
    }

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn matching_header() -> HeaderNameIR {
        HeaderNameIR::new(Bytes::from_static(DEFAULT_MATCHING_HEADER))
    }
    proptest! {
        #[test]
        fn returns_response_paired_with_matched_request(header in ".*") {
            let mocks = mocks(vec![
                request(id(1), headers(&[("x-erm-match".to_string(), header.clone())])),
                response(id(1), 200),
            ]);

            let found = match_by_header(mocks.values(), &matching_header(), &Bytes::from(header)).unwrap();

            assert_eq!(found.id(), &id(1));
        }
    }

    #[test]
    fn picks_the_response_by_id_among_many() {
        let mocks = mocks(vec![
            request(
                id(1),
                headers(&[("x-erm-match".to_owned(), "a".to_owned())]),
            ),
            request(
                id(2),
                headers(&[("x-erm-match".to_owned(), "b".to_owned())]),
            ),
            request(
                id(3),
                headers(&[("x-erm-match".to_owned(), "c".to_owned())]),
            ),
            response(id(1), 201),
            response(id(2), 202),
            response(id(3), 203),
        ]);

        let found = match_by_header(
            mocks.values(),
            &matching_header(),
            &Bytes::from_static(b"b"),
        )
        .unwrap();

        assert_eq!(found.id(), &id(2));
        assert_eq!(found.parts.status.0.get(), 202);
    }

    #[test]
    fn unknown_header_value_is_a_miss() {
        let mocks = mocks(vec![
            request(
                id(1),
                headers(&[("x-erm-match".to_owned(), "a".to_owned())]),
            ),
            response(id(1), 200),
        ]);

        assert_eq!(
            match_by_header(
                mocks.values(),
                &matching_header(),
                &Bytes::from_static(b"z")
            )
            .unwrap_err(),
            MatchByHeaderError::NoMatchingRequest
        );
    }

    #[test]
    fn value_under_another_header_name_does_not_match() {
        let mocks = mocks(vec![
            request(id(1), headers(&[("x-other".to_owned(), "a".to_owned())])),
            response(id(1), 200),
        ]);

        assert_eq!(
            match_by_header(
                mocks.values(),
                &matching_header(),
                &Bytes::from_static(b"a")
            )
            .unwrap_err(),
            MatchByHeaderError::NoMatchingRequest
        );
    }

    #[test]
    fn matched_request_without_response_is_a_miss() {
        let mocks = mocks(vec![
            request(
                id(1),
                headers(&[("x-erm-match".to_owned(), "a".to_owned())]),
            ),
            response(id(2), 200),
        ]);

        assert_eq!(
            match_by_header(
                mocks.values(),
                &matching_header(),
                &Bytes::from_static(b"a")
            )
            .unwrap_err(),
            MatchByHeaderError::NoResponseForRequest(id(1))
        );
    }

    #[test]
    fn response_without_request_is_never_returned() {
        let mocks = mocks(vec![response(id(1), 200)]);

        assert_eq!(
            match_by_header(
                mocks.values(),
                &matching_header(),
                &Bytes::from_static(b"a")
            )
            .unwrap_err(),
            MatchByHeaderError::NoMatchingRequest
        );
    }

    #[test]
    fn empty_mocks_is_a_miss() {
        assert_eq!(
            match_by_header([], &matching_header(), &Bytes::from_static(b"a")).unwrap_err(),
            MatchByHeaderError::NoMatchingRequest
        );
    }

    #[test]
    fn matches_any_value_of_a_repeated_header_on_the_recorded_request() {
        let mocks = mocks(vec![
            request(
                id(1),
                headers(&[
                    ("x-erm-match".to_owned(), "a".to_owned()),
                    ("x-erm-match".to_owned(), "b".to_owned()),
                ]),
            ),
            response(id(1), 200),
        ]);

        let found = match_by_header(
            mocks.values(),
            &matching_header(),
            &Bytes::from_static(b"b"),
        )
        .unwrap();

        assert_eq!(found.id(), &id(1));
    }
}
