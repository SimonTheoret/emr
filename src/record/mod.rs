mod capture;

use axum::{
    body::Body,
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use http::{StatusCode, request};
use reqwest::Client;

use self::capture::{Saver, Tap};
use crate::{ir::UrlIR, match_id::MatchId, proxy};

/// The client that forwards each request to the upstream.
fn client() -> reqwest::Result<Client> {
    Client::builder().use_rustls_tls().user_agent("erm").build()
}

/// Everything record mode's handler can reach. Replay mode never builds one, so no
/// replay handler can save.
#[derive(Clone)]
struct Recording {
    saver: Saver,
    client: Client,
    destination: UrlIR,
}

async fn record(
    State(state): State<Recording>,
    MatchId(id): MatchId,
    request: Request,
) -> Result<Response, BadGateway> {
    let (parts, body) = request.into_parts();
    let (capture, body) = state.saver.capture(id, &parts, body);
    let upstream = state.forward(parts, body).await.map_err(BadGateway)?;
    let (head, body) = capture.respond(upstream).into_parts();
    Ok(proxy::client_response(
        head.status,
        head.headers,
        Body::new(body),
    ))
}

impl Recording {
    /// Sends the request to the upstream and waits for the response head.
    async fn forward(
        &self,
        parts: request::Parts,
        body: Tap<Body>,
    ) -> reqwest::Result<reqwest::Response> {
        let url = proxy::upstream_url(&self.destination, &parts.uri.into());
        let mut headers = parts.headers;
        proxy::sanitize_headers(&mut headers);
        self.client
            .request(parts.method, url.0)
            .version(parts.version)
            .headers(headers)
            // `reqwest::Body::wrap` needs a `Sync` body and axum's is not, so the tap goes in
            // as a stream, as it does today.
            .body(reqwest::Body::wrap_stream(
                Body::new(body).into_data_stream(),
            ))
            .send()
            .await
    }
}

/// The upstream request failed before a response head arrived. Nothing is saved.
struct BadGateway(reqwest::Error);

impl IntoResponse for BadGateway {
    fn into_response(self) -> Response {
        let message = format!("could not reach the upstream: {}", self.0);
        (StatusCode::BAD_GATEWAY, message).into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        convert::Infallible,
        future::IntoFuture,
        io,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    use axum::{
        Router,
        response::AppendHeaders,
        routing::{any, get},
    };
    use bytes::Bytes;
    use futures::{StreamExt, stream};
    use http::{Method, header};
    use reqwest::redirect;
    use tokio::{
        net::TcpListener,
        sync::oneshot,
        task::JoinHandle,
        time::{sleep, timeout},
    };

    use super::{
        capture::{
            Drained,
            tests::{saved, saves},
        },
        *,
    };
    use crate::{
        ir::{MethodIR, TimedFrame},
        storage::{Exchange, Recorder},
    };

    const MATCH: &str = "x-erm-match";
    const TIMEOUT: Duration = Duration::from_secs(10);
    const FRAMES: [&[u8]; 3] = [b"\x00one", b"\xfftwo", b"three\x01"];
    const FRAME_DELAY: Duration = Duration::from_millis(40);

    /// A router on a loopback port, served until `stop` is sent.
    struct Served {
        url: String,
        stop: oneshot::Sender<()>,
        task: JoinHandle<io::Result<()>>,
    }

    async fn serve(router: Router) -> Served {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (stop, stopped) = oneshot::channel::<()>();
        let task = tokio::spawn(
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = stopped.await;
                })
                .into_future(),
        );
        Served { url, stop, task }
    }

    /// Record mode in front of `destination`, wired the way `run` wires it.
    struct Proxy {
        served: Served,
        drained: Drained,
    }

    impl Proxy {
        async fn start(destination: &str, recorder: impl Recorder) -> Self {
            let (saver, drained) = Saver::new(recorder);
            let state = Recording {
                saver,
                client: client().unwrap(),
                destination: destination.parse().unwrap(),
            };
            let served = serve(Router::new().fallback(record).with_state(state)).await;
            Self { served, drained }
        }

        fn url(&self, path: &str) -> String {
            format!("{}{path}", self.served.url)
        }

        /// Stops the way `run` does: open connections finish, then every `Saver` is gone.
        async fn stop(self) {
            let _ = self.served.stop.send(());
            timeout(TIMEOUT, async {
                self.served.task.await.unwrap().unwrap();
                self.drained.wait().await;
            })
            .await
            .expect("the proxy stops once its callers are done");
        }
    }

    async fn upstream() -> Served {
        serve(
            Router::new()
                .route("/frames", any(frames))
                .route("/fails", get(fails))
                .route("/hangs", get(hangs))
                .route("/no-content", get(|| async { StatusCode::NO_CONTENT })),
        )
        .await
    }

    /// Reads the whole request, then answers with two `set-cookie` headers and `FRAMES`, each
    /// sent `FRAME_DELAY` after the one before.
    async fn frames(_request: Bytes) -> impl IntoResponse {
        let body = stream::iter(FRAMES).then(|frame| async move {
            sleep(FRAME_DELAY).await;
            Ok::<_, Infallible>(Bytes::from_static(frame))
        });
        (
            AppendHeaders([
                (header::SET_COOKIE, "first=1"),
                (header::SET_COOKIE, "second=2"),
            ]),
            Body::from_stream(body),
        )
    }

    /// Fails after the head and a first frame went out, so the failure is in the body.
    async fn fails() -> Body {
        let first = stream::once(async { Ok(Bytes::from_static(b"first")) });
        let failure = stream::once(async {
            sleep(FRAME_DELAY).await;
            Err(io::Error::other("the upstream broke"))
        });
        Body::from_stream(first.chain(failure))
    }

    async fn hangs() -> Body {
        let first = stream::once(async { Ok::<_, Infallible>(Bytes::from_static(b"first")) });
        Body::from_stream(first.chain(stream::pending()))
    }

    /// A client that shows each response as ERM sent it, without following redirects.
    fn caller() -> Client {
        Client::builder()
            .redirect(redirect::Policy::none())
            .build()
            .unwrap()
    }

    async fn send(method: Method, url: String, id: &str) -> reqwest::Response {
        caller()
            .request(method, url)
            .header(MATCH, id)
            .send()
            .await
            .unwrap()
    }

    fn content(frames: &[TimedFrame]) -> Vec<u8> {
        frames
            .iter()
            .flat_map(|frame| frame.content.clone())
            .collect()
    }

    fn ids(exchanges: &[Exchange]) -> Vec<&str> {
        exchanges
            .iter()
            .map(|exchange| exchange.id.as_str())
            .collect()
    }

    #[tokio::test]
    async fn a_body_read_to_its_end_saves_the_exchange_once() {
        let upstream = upstream().await;
        let (recorder, receiver) = saves();
        let proxy = Proxy::start(&upstream.url, recorder).await;

        let response = caller()
            .post(proxy.url("/frames?q=1"))
            .header(MATCH, "frames")
            .body("request body")
            .send()
            .await
            .unwrap();
        let body = response.bytes().await.unwrap();
        proxy.stop().await;

        assert_eq!(body, FRAMES.concat(), "the caller reads the upstream body");
        let exchanges = saved(receiver);
        assert_eq!(ids(&exchanges), ["frames"], "one exchange is saved");
        let exchange = &exchanges[0];
        assert_eq!(exchange.request.parts.method, MethodIR::Post);
        assert_eq!(exchange.request.parts.uri.to_string(), "/frames?q=1");
        assert_eq!(content(&exchange.request.body), b"request body");
        assert_eq!(
            content(&exchange.response.body),
            body,
            "the saved response body is the body the caller read"
        );
    }

    #[tokio::test]
    async fn an_upstream_body_error_saves_nothing() {
        let upstream = upstream().await;
        let (recorder, receiver) = saves();
        let proxy = Proxy::start(&upstream.url, recorder).await;

        let response = send(Method::GET, proxy.url("/fails"), "fails").await;
        assert_eq!(response.status(), StatusCode::OK, "the head arrives");
        assert!(
            response.bytes().await.is_err(),
            "the caller sees the body fail"
        );
        proxy.stop().await;

        assert!(saved(receiver).is_empty(), "nothing is saved");
    }

    #[tokio::test]
    async fn a_caller_that_drops_the_response_mid_body_saves_nothing() {
        let upstream = upstream().await;
        let (recorder, receiver) = saves();
        let proxy = Proxy::start(&upstream.url, recorder).await;

        let mut response = send(Method::GET, proxy.url("/hangs"), "hangs").await;
        let first = response.chunk().await.unwrap();
        assert_eq!(
            first.as_deref(),
            Some(&b"first"[..]),
            "the first frame arrives"
        );
        drop(response);
        proxy.stop().await;

        assert!(saved(receiver).is_empty(), "nothing is saved");
    }

    #[tokio::test]
    async fn a_head_request_and_a_204_response_save_an_empty_body() {
        let upstream = upstream().await;
        let (recorder, receiver) = saves();
        let proxy = Proxy::start(&upstream.url, recorder).await;

        let head = send(Method::HEAD, proxy.url("/frames"), "head").await;
        assert_eq!(head.status(), StatusCode::OK);
        assert!(head.bytes().await.unwrap().is_empty());
        let no_content = send(Method::GET, proxy.url("/no-content"), "no-content").await;
        assert_eq!(no_content.status(), StatusCode::NO_CONTENT);
        assert!(no_content.bytes().await.unwrap().is_empty());
        proxy.stop().await;

        let mut exchanges = saved(receiver);
        exchanges.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(ids(&exchanges), ["head", "no-content"], "both are saved");
        for exchange in &exchanges {
            assert!(exchange.request.body.is_empty(), "{}", exchange.id);
            assert!(exchange.response.body.is_empty(), "{}", exchange.id);
        }
        let statuses: Vec<StatusCode> = exchanges
            .iter()
            .map(|exchange| exchange.response.parts.status.into())
            .collect();
        assert_eq!(statuses, [StatusCode::OK, StatusCode::NO_CONTENT]);
    }

    #[tokio::test]
    async fn a_missing_repeated_or_invalid_id_gets_400_and_is_not_forwarded_or_saved() {
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = hits.clone();
        let upstream = serve(Router::new().fallback(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            async { "ok" }
        }))
        .await;
        let (recorder, receiver) = saves();
        let proxy = Proxy::start(&upstream.url, recorder).await;

        let cases: [(&str, &[&str]); 3] = [
            ("missing", &[]),
            ("repeated", &["one", "two"]),
            ("invalid", &["Login"]),
        ];
        for (case, values) in cases {
            let mut request = caller().get(proxy.url("/"));
            for value in values {
                request = request.header(MATCH, *value);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{case}");
            assert_eq!(
                response.headers()[header::CONTENT_TYPE],
                "text/plain; charset=utf-8",
                "{case}"
            );
            let message = response.text().await.unwrap();
            assert!(
                message.contains(MATCH),
                "{case} names the header: {message}"
            );
        }
        assert_eq!(hits.load(Ordering::SeqCst), 0, "nothing is forwarded");
        let valid = send(Method::GET, proxy.url("/"), "valid").await;
        assert_eq!(valid.status(), StatusCode::OK);
        valid.bytes().await.unwrap();
        proxy.stop().await;

        assert_eq!(hits.load(Ordering::SeqCst), 1, "a valid id is forwarded");
        assert_eq!(
            ids(&saved(receiver)),
            ["valid"],
            "only the valid id is saved"
        );
    }

    #[tokio::test]
    async fn an_unreachable_upstream_gets_502_and_saves_nothing() {
        let closed = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", listener.local_addr().unwrap())
        };
        let (recorder, receiver) = saves();
        let proxy = Proxy::start(&closed, recorder).await;

        let response = send(Method::GET, proxy.url("/"), "unreachable").await;
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        response.bytes().await.unwrap();
        proxy.stop().await;

        assert!(saved(receiver).is_empty(), "nothing is saved");
    }
}
