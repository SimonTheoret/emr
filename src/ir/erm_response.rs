use axum::response::IntoResponse;
use http::HeaderMap;
use uuid::Uuid;

use super::{BodyStream, ERMResponseParts, HeadersIR, StatusCodeIR, UrlIR, VersionIR};

#[derive(Debug)]
pub struct ERMResponse {
    pub body: BodyStream,
    pub url: UrlIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
    pub status: StatusCodeIR,
    pub id: Uuid,
}

impl ERMResponse {
    pub fn clone_parts(&self) -> ERMResponseParts {
        ERMResponseParts {
            url: self.url.clone(),
            version: self.version,
            headers: self.headers.clone(),
            status: self.status,
            id: self.id,
        }
    }
}

impl IntoResponse for ERMResponse {
    fn into_response(self) -> axum::response::Response {
        let mut incomplete_response = axum::response::Response::builder()
            .status(self.status)
            .version(self.version.into());
        let response_headers = incomplete_response
            .headers_mut()
            .expect("Error when fetching the headers from ERMResponse");
        let current_headers: HeaderMap = self.headers.into();
        response_headers.extend(current_headers);
        incomplete_response
            .body(axum::body::Body::from_stream(self.body.0))
            .expect("Error when building a response")
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::body::Body;
    use bytes::Bytes;
    use futures::{StreamExt, stream};
    use http::{HeaderMap, StatusCode, Version};
    use url::Url;

    use super::*;

    fn response() -> ERMResponse {
        let mut headers = HeaderMap::new();
        headers.insert("x-test", "value".parse().unwrap());
        headers.append("set-cookie", "first=1".parse().unwrap());
        headers.append("set-cookie", "second=2".parse().unwrap());
        let body = Body::from_stream(stream::iter([
            Ok::<_, Infallible>(Bytes::from_static(b"first")),
            Ok(Bytes::from_static(b"second")),
        ]));

        ERMResponse {
            body: BodyStream(body.into_data_stream()),
            url: Url::parse("https://example.test/resource").unwrap().into(),
            version: VersionIR::Http2,
            headers: headers.into(),
            status: StatusCode::CREATED.into(),
            id: Uuid::from_u128(42),
        }
    }

    #[test]
    fn clone_parts_copies_every_response_part() {
        let response = response();

        let parts = response.clone_parts();

        assert_eq!(parts.url, response.url);
        assert_eq!(parts.version, response.version);
        assert_eq!(parts.headers, response.headers);
        assert_eq!(parts.status, response.status);
        assert_eq!(parts.id, response.id);
    }

    #[tokio::test]
    async fn into_response_builds_an_axum_response() {
        let response = response().into_response();

        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.version(), Version::HTTP_2);
        assert_eq!(response.headers()["x-test"], "value");
        assert_eq!(response.headers().get_all("set-cookie").iter().count(), 2);

        let mut body = response.into_body().into_data_stream();
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"first")
        );
        assert_eq!(
            body.next().await.unwrap().unwrap(),
            Bytes::from_static(b"second")
        );
        assert!(body.next().await.is_none());
    }
}
