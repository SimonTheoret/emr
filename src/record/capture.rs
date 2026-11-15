//! Turns one proxied request and its response into one saved `Exchange`.
//!
//! Each body is wrapped in a `Tap` that owns its own frames. A tap hands its frames over a
//! oneshot only if its body was read to the end, so an error or an early drop leaves the
//! receiver with nothing. One task per exchange joins the two receivers and saves.

#![allow(dead_code)]

use std::{
    convert::Infallible,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};

use bytes::Bytes;
use http::request;
use http_body::{Body, Frame, SizeHint};
use tokio::sync::{mpsc, oneshot};

use crate::{
    ir::{ERMRequestParts, TimedFrame},
    storage::{Id, Recorder},
};

/// The only path to the `Recorder`. Every clone keeps `Drained::wait` pending, and every
/// exchange still capturing holds one, so `Writer::finish` cannot run while a `save` is
/// still possible.
#[derive(Clone)]
pub(super) struct Saver {
    recorder: Arc<dyn Recorder>,
    _alive: mpsc::Sender<Infallible>,
}

/// Resolves once every `Saver` clone is dropped.
pub(super) struct Drained(mpsc::Receiver<Infallible>);

/// One exchange whose request body is being captured. Dropping it saves nothing, which is
/// how the `502` path and a client that leaves before the response head both end.
pub(super) struct Capture {
    saver: Saver,
    id: Id,
    request: ERMRequestParts,
    request_body: oneshot::Receiver<Vec<TimedFrame>>,
}

/// Passes a body through unchanged, including `is_end_stream` and `size_hint`, and records
/// each data frame with its delay.
pub(super) struct Tap<B: Body<Data = Bytes> + Unpin> {
    inner: B,
    /// `None` once the outcome is decided: the frames were sent, or the body failed.
    capturing: Option<Capturing>,
}

struct Capturing {
    frames: Vec<TimedFrame>,
    last: Instant,
    done: oneshot::Sender<Vec<TimedFrame>>,
}

impl Saver {
    pub(super) fn new(recorder: impl Recorder) -> (Self, Drained) {
        let (alive, gone) = mpsc::channel(1);
        let saver = Self {
            recorder: Arc::new(recorder),
            _alive: alive,
        };
        (saver, Drained(gone))
    }

    /// Starts capturing the exchange `id`. Forward the returned body in place of `body`.
    pub(super) fn capture<B: Body<Data = Bytes> + Unpin>(
        &self,
        id: Id,
        request: &request::Parts,
        body: B,
    ) -> (Capture, Tap<B>) {
        let (body, request_body) = Tap::new(body);
        let capture = Capture {
            saver: self.clone(),
            id,
            request: ERMRequestParts::from(request),
            request_body,
        };
        (capture, body)
    }
}

impl Capture {
    /// Taps the upstream response body. The exchange is saved once both bodies are read to
    /// their end, in whichever order they finish.
    pub(super) fn respond(self, upstream: reqwest::Response) -> http::Response<Tap<reqwest::Body>> {
        // TODO:
        //   let url = UrlIR(upstream.url().clone());
        //   let (head, body) = http::Response::from(upstream).into_parts();
        //   let response = ERMResponseParts { url, version: head.version.into(),
        //       headers: head.headers.clone().into(), status: head.status.into() };
        //   let (body, response_body) = Tap::new(body);
        //   let Self { saver, id, request, request_body } = self;
        //   tokio::spawn(async move {
        //       // Fails fast on the first receiver whose tap was dropped early or saw an error.
        //       let Ok((request_frames, response_frames)) =
        //           futures::future::try_join(request_body, response_body).await
        //       else { return };
        //       saver.recorder.save(Exchange {
        //           id,
        //           request: FinalRequest { parts: request, body: request_frames },
        //           response: FinalResponse { parts: response, body: response_frames },
        //       });
        //   });
        //   http::Response::from_parts(head, body)
        todo!("tap {} for {}", upstream.url(), self.id)
    }
}

impl Drained {
    pub(super) async fn wait(mut self) {
        // `recv` returns `None` once every sender is gone, and `Infallible` means it returns
        // nothing else.
        let _ = self.0.recv().await;
    }
}

impl<B: Body<Data = Bytes> + Unpin> Tap<B> {
    fn new(inner: B) -> (Self, oneshot::Receiver<Vec<TimedFrame>>) {
        let (done, frames) = oneshot::channel();
        let tap = Self {
            inner,
            capturing: Some(Capturing {
                frames: Vec::new(),
                last: Instant::now(),
                done,
            }),
        };
        (tap, frames)
    }
}

impl Capturing {
    fn push(&mut self, content: Bytes) {
        let now = Instant::now();
        self.frames.push(TimedFrame {
            content,
            tslf: now - self.last,
        });
        self.last = now;
    }

    fn finish(self) {
        // A closed receiver means the exchange was already abandoned.
        let _ = self.done.send(self.frames);
    }
}

impl<B: Body<Data = Bytes> + Unpin> Body for Tap<B> {
    type Data = Bytes;
    type Error = B::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, B::Error>>> {
        // TODO:
        //   match ready!(Pin::new(&mut self.inner).poll_frame(cx)) {
        //       Some(Ok(frame)) => {
        //           if let (Some(data), Some(capturing)) = (frame.data_ref(), &mut self.capturing) {
        //               capturing.push(data.clone());
        //           }
        //           Poll::Ready(Some(Ok(frame)))
        //       }
        //       Some(Err(error)) => {
        //           self.capturing = None;
        //           Poll::Ready(Some(Err(error)))
        //       }
        //       None => {
        //           if let Some(capturing) = self.capturing.take() { capturing.finish() }
        //           Poll::Ready(None)
        //       }
        //   }
        let _ = (&mut self.inner, cx);
        todo!("pass the frame through and record its data")
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<B: Body<Data = Bytes> + Unpin> Drop for Tap<B> {
    fn drop(&mut self) {
        // hyper drops a body without polling it to `None` once it reports `is_end_stream`, and
        // always for HEAD, 204, and 304 responses. By the `Body` contract such a body has no
        // frames left, so it is complete.
        if self.inner.is_end_stream()
            && let Some(capturing) = self.capturing.take()
        {
            capturing.finish();
        }
    }
}
