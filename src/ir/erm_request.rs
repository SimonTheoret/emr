use std::time::Instant;

use eyre::Result;
use futures::StreamExt;
use http::request::Parts;
use tokio::sync::mpsc::UnboundedSender;

use super::{BodyStream, HeadersIR, MethodIR, TimedFrame, VersionIR};

#[derive(Debug)]
pub struct ERMRequest {
    pub body: BodyStream,
    pub method: MethodIR,
    pub version: VersionIR,
    pub headers: HeadersIR,
}

impl ERMRequest {
    pub async fn from_request(
        req: axum::extract::Request,
        channel: UnboundedSender<TimedFrame>,
    ) -> Result<Self> {
        let (parts, body_req) = req.into_parts();

        let Parts {
            method,
            version,
            headers,
            ..
        } = parts;
        let mut last_frame_time: Option<Instant> = None;
        let body = BodyStream(
            axum::body::Body::from_stream(body_req.into_data_stream().map(move |result| {
                result.inspect(|bytes| {
                    let now = Instant::now();
                    channel
                        .send(TimedFrame {
                            content: bytes.clone(),
                            tslf: now - last_frame_time.unwrap_or(now),
                        })
                        .expect("Error when sending bytes over a channel (Building request)");
                    last_frame_time = Some(now);
                })
            }))
            .into_data_stream(),
        );

        Ok(Self {
            body,
            method: method.into(),
            version: version.into(),
            headers: headers.into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use axum::body::Body;
    use bytes::Bytes;
    use futures::{StreamExt, stream};
    use http::{Method, Request, Version};
    use tokio::sync::mpsc::{error::TryRecvError, unbounded_channel};

    use super::*;

    #[tokio::test]
    async fn from_request_extracts_parts_and_records_every_streamed_body_chunk() {
        let body = Body::from_stream(stream::iter([
            Ok::<_, Infallible>(Bytes::from_static(b"first")),
            Ok(Bytes::from_static(b"second")),
        ]));
        let request = Request::builder()
            .method(Method::POST)
            .version(Version::HTTP_2)
            .header("x-test", "value")
            .header("set-cookie", "first=1")
            .header("set-cookie", "second=2")
            .body(body)
            .unwrap();
        let (sender, mut receiver) = unbounded_channel();

        let mut request = ERMRequest::from_request(request, sender).await.unwrap();
        let first = request.body.0.next().await.unwrap().unwrap();
        let second = request.body.0.next().await.unwrap().unwrap();
        assert!(request.body.0.next().await.is_none());
        let first_frame = receiver.try_recv().unwrap();
        let second_frame = receiver.try_recv().unwrap();

        assert_eq!(request.method, MethodIR::Post);
        assert_eq!(request.version, VersionIR::Http2);
        let headers: http::HeaderMap = request.headers.clone().into();
        assert_eq!(headers["x-test"], "value");
        assert_eq!(headers.get_all("set-cookie").iter().count(), 2);
        assert_eq!(headers.keys().count(), 2);
        assert_eq!(headers.len(), 3);
        assert_eq!(first, Bytes::from_static(b"first"));
        assert_eq!(second, Bytes::from_static(b"second"));
        assert_eq!(first_frame.content, first);
        assert_eq!(second_frame.content, second);
        assert!(first_frame.tslf.is_zero());
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    }

    #[tokio::test]
    async fn from_request_forwards_body_errors_without_recording_a_frame() {
        let body = Body::from_stream(stream::iter([Err::<Bytes, _>(std::io::Error::other(
            "broken stream",
        ))]));
        let request = Request::new(body);
        let (sender, mut receiver) = unbounded_channel();

        let mut request = ERMRequest::from_request(request, sender).await.unwrap();
        let result = request.body.0.next().await.unwrap();

        assert!(result.is_err());
        assert_eq!(receiver.try_recv(), Err(TryRecvError::Empty));
    }
}
