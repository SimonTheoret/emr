use std::{convert::Infallible, sync::Arc};

use axum::{
    Router,
    body::Body,
    extract::State,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use futures::{Stream, StreamExt, stream};
use http::StatusCode;

use crate::{
    cli::ServeArgs,
    ir::TimedFrame,
    match_id::MatchId,
    proxy, server,
    storage::{self, Id, Replayer},
};

/// Answers every request from the exchanges in `args.mock_dir`. A corrupted file fails here,
/// before the port is bound.
pub async fn run(args: ServeArgs) -> eyre::Result<()> {
    let replayer = storage::replayer(&args.mock_dir, args.format)?;
    Ok(server::serve(args.port, router(Arc::new(replayer))).await?)
}

pub(crate) fn router(replayer: Arc<dyn Replayer>) -> Router {
    Router::new().fallback(replay).with_state(replayer)
}

/// The state is only a `Replayer`, so this handler has no way to save or to reach an upstream.
async fn replay(
    State(replayer): State<Arc<dyn Replayer>>,
    MatchId(id): MatchId,
) -> Result<Response, NotFound> {
    let Some(exchange) = replayer.find(&id) else {
        return Err(NotFound(id));
    };
    let response = &exchange.response;
    Ok(proxy::client_response(
        response.parts.status.into(),
        response.parts.headers.clone().into(),
        Body::from_stream(paced(response.body.clone())),
    ))
}

/// Yields each frame after waiting its `tslf`. `then` starts a frame's wait only once the
/// previous frame was taken, so each delay runs from the previous send.
fn paced(frames: Vec<TimedFrame>) -> impl Stream<Item = Result<Bytes, Infallible>> + Send {
    stream::iter(frames).then(|frame| async move {
        tokio::time::sleep(frame.tslf).await;
        Ok(frame.content)
    })
}

/// No recorded exchange has this `id`.
struct NotFound(Id);

impl IntoResponse for NotFound {
    fn into_response(self) -> Response {
        let message = format!("no recorded exchange has the id {}", self.0);
        (StatusCode::NOT_FOUND, message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, time::Duration};

    use http::{HeaderMap, HeaderValue, Request, header};
    use tokio::time::Instant;
    use tower::Service;

    use super::*;
    use crate::{
        ir::{ERMRequestParts, ERMResponseParts, FinalRequest, FinalResponse, VersionIR},
        storage::Exchange,
    };

    const MATCH: &str = "x-erm-match";

    impl Replayer for HashMap<Id, Exchange> {
        fn find(&self, id: &Id) -> Option<&Exchange> {
            self.get(id)
        }
    }

    /// An exchange whose `200` response carries `headers` and `frames`.
    fn exchange(
        id: &str,
        headers: &[(&'static str, &'static str)],
        frames: &[(&'static [u8], Duration)],
    ) -> Exchange {
        let (request, ()) = Request::new(()).into_parts();
        let mut response_headers = HeaderMap::new();
        for (name, value) in headers {
            response_headers.append(*name, HeaderValue::from_static(value));
        }
        Exchange {
            id: Id::parse(id.as_bytes()).unwrap(),
            request: FinalRequest {
                parts: ERMRequestParts::from(&request),
                body: Vec::new(),
            },
            response: FinalResponse {
                parts: ERMResponseParts {
                    url: "http://upstream.test/".parse().unwrap(),
                    version: VersionIR::Http11,
                    headers: response_headers.into(),
                    status: StatusCode::OK.into(),
                },
                body: frames
                    .iter()
                    .map(|&(content, tslf)| TimedFrame {
                        content: Bytes::from_static(content),
                        tslf,
                    })
                    .collect(),
            },
        }
    }

    fn replaying(exchanges: impl IntoIterator<Item = Exchange>) -> Router {
        let exchanges: HashMap<Id, Exchange> = exchanges
            .into_iter()
            .map(|exchange| (exchange.id.clone(), exchange))
            .collect();
        router(Arc::new(exchanges))
    }

    /// Sends a request with one `x-erm-match` header per value.
    async fn call(router: &Router, ids: &[&'static str]) -> Response {
        let mut request = Request::new(Body::empty());
        for id in ids {
            request
                .headers_mut()
                .append(MATCH, HeaderValue::from_static(id));
        }
        router.clone().call(request).await.unwrap()
    }

    async fn text(response: Response) -> String {
        let mut body = response.into_body().into_data_stream();
        let mut text = Vec::new();
        while let Some(chunk) = body.next().await {
            text.extend_from_slice(&chunk.unwrap());
        }
        String::from_utf8(text).unwrap()
    }

    #[tokio::test]
    async fn a_missing_repeated_or_invalid_id_gets_400() {
        let router = replaying([exchange("login", &[], &[(b"welcome", Duration::ZERO)])]);

        let cases: [(&str, &[&'static str]); 3] = [
            ("missing", &[]),
            ("repeated", &["login", "login"]),
            ("invalid", &["Login"]),
        ];
        for (case, ids) in cases {
            let response = call(&router, ids).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{case}");
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/plain; charset=utf-8",
                "{case}"
            );
            let message = text(response).await;
            assert!(
                message.contains(MATCH),
                "{case} names the header: {message}"
            );
        }
        let hit = call(&router, &["login"]).await;
        assert_eq!(hit.status(), StatusCode::OK, "a valid id is answered");
        assert_eq!(text(hit).await, "welcome");
    }

    #[tokio::test]
    async fn a_miss_gets_404_naming_the_id() {
        let router = replaying([exchange("login", &[], &[])]);

        let response = call(&router, &["logout"]).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
        let message = text(response).await;
        assert!(message.contains("logout"), "names the id: {message}");
    }

    #[tokio::test(start_paused = true)]
    async fn each_frame_arrives_in_order_after_its_recorded_delay() {
        let frames: [(&[u8], Duration); 4] = [
            (b"\x00first", Duration::from_millis(30)),
            (b"second", Duration::ZERO),
            (b"third\xff", Duration::from_millis(250)),
            (b"fourth", Duration::from_secs(2)),
        ];
        let router = replaying([exchange("paced", &[], &frames)]);

        let response = call(&router, &["paced"]).await;
        assert_eq!(response.status(), StatusCode::OK);
        let mut body = response.into_body().into_data_stream();
        let mut previous = Instant::now();
        let mut arrivals = Vec::new();
        while let Some(chunk) = body.next().await {
            let now = Instant::now();
            arrivals.push((chunk.unwrap(), now - previous));
            previous = now;
        }

        let expected: Vec<(Bytes, Duration)> = frames
            .iter()
            .map(|&(content, tslf)| (Bytes::from_static(content), tslf))
            .collect();
        assert_eq!(
            arrivals, expected,
            "time is paused, so each frame arrives exactly its tslf after the one before"
        );
    }

    #[tokio::test]
    async fn a_hit_replays_the_recorded_status_and_repeated_headers() {
        let router = replaying([exchange(
            "cookies",
            &[
                ("set-cookie", "first=1"),
                ("set-cookie", "second=2"),
                ("transfer-encoding", "chunked"),
            ],
            &[],
        )]);

        let response = call(&router, &["cookies"]).await;

        assert_eq!(response.status(), StatusCode::OK);
        let cookies: Vec<_> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .collect();
        assert_eq!(cookies, ["first=1", "second=2"], "both values, in order");
        assert!(
            !response.headers().contains_key(header::TRANSFER_ENCODING),
            "the recorded framing header is not replayed"
        );
    }

    #[cfg(feature = "ron")]
    #[test]
    fn a_file_with_an_invalid_status_or_header_fails_to_load() {
        use std::fs;

        use tempfile::TempDir;

        use crate::storage::{FormatKind, Recorder};

        let cases = [
            ("status 42", "status: 200", "status: 42"),
            ("header name", r#"b"x-test""#, r#"b"x test""#),
            ("header value", r#"b"value""#, r#"b"line\nbreak""#),
        ];
        for (case, valid, invalid) in cases {
            let dir = TempDir::new().unwrap();
            let (recorder, writer) = storage::recorder(dir.path(), FormatKind::Ron).unwrap();
            recorder.save(exchange("edited", &[("x-test", "value")], &[]));
            writer.finish();
            let path = dir.path().join("edited.ron");
            let recorded = fs::read_to_string(&path).unwrap();
            assert!(recorded.contains(valid), "{case}: the file holds {valid}");
            fs::write(&path, recorded.replacen(valid, invalid, 1)).unwrap();

            let Err(error) = storage::replayer(dir.path(), FormatKind::Ron) else {
                panic!("{case}: the edited file loads");
            };

            let paths: Vec<_> = error.files.iter().map(|file| &file.path).collect();
            assert_eq!(paths, [&path], "{case}: the edited file is reported");
        }
    }
}
