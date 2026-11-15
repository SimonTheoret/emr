mod capture;

use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    response::{IntoResponse, Response},
};
use http::{StatusCode, request};
use reqwest::Client;

use self::capture::{Saver, Tap};
use crate::{cli::RecordArgs, ir::UrlIR, match_id::MatchId, proxy, server, storage};

/// Proxies to `args.destination` and records every complete exchange until shutdown.
pub async fn run(args: RecordArgs) -> eyre::Result<()> {
    let client = Client::builder()
        .use_rustls_tls()
        .user_agent("erm")
        .build()?;
    let (recorder, writer) = storage::recorder(&args.serve.mock_dir, args.serve.format)?;
    let (saver, drained) = Saver::new(recorder);
    let state = Recording {
        saver,
        client,
        destination: args.destination,
    };
    let served = server::serve(
        args.serve.port,
        Router::new().fallback(record).with_state(state),
    )
    .await;
    drained.wait().await;
    writer.finish();
    Ok(served?)
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
