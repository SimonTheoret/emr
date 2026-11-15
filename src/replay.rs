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
    let replayer: Arc<dyn Replayer> = Arc::new(storage::replayer(&args.mock_dir, args.format)?);
    let router = Router::new().fallback(replay).with_state(replayer);
    Ok(server::serve(args.port, router).await?)
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
    // TODO: these two conversions unwrap, so a hand-edited file with an invalid header name or
    // status panics here. See "Open questions" in the rationale.
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
