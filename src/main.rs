use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::time::{Duration, Instant};
use tracing::{info, instrument};

use axum::{Router, extract::Request, extract::State, response::IntoResponse, routing::any};
use bytes::Bytes;
use eyre::{Context, Report as EyreReport};
use futures::StreamExt;
use http::{HeaderMap, request::Parts};
use http_body_util::BodyDataStream as HttpBodyDataStream;

use reqwest::{Client, ClientBuilder};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::spawn,
};

mod ir;
use crate::ir::{BodyStream, HeadersIR, MethodIR, StatusCodeIR, UrlIR, VersionIR};

type Delay = Duration;

#[derive(Debug)]
struct Report(EyreReport);
type Result<T> = std::result::Result<T, Report>;

impl From<EyreReport> for Report {
    fn from(value: EyreReport) -> Self {
        Report(value)
    }
}

#[derive(Clone)]
struct Config {
    destination: String,
}

impl IntoResponse for Report {
    fn into_response(self) -> axum::response::Response {
        self.0.to_string().into_response()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TimedFrame {
    /// The content of the frame
    content: Bytes,
    /// Time since last frame (ms)
    tslf: Delay,
}

#[derive(Debug)]
struct ERMRequest {
    body: BodyStream,
    method: MethodIR,
    version: VersionIR,
    headers: HeadersIR,
}

#[derive(Serialize, Deserialize)]
struct ERMRequestParts {
    method: MethodIR,
    version: VersionIR,
    headers: HeadersIR,
}

impl ERMRequestParts {
    fn clone_from_request(req: &Request) -> Self {
        Self {
            method: req.method().clone().into(),
            version: req.version().into(),
            headers: req.headers().clone().into(),
        }
    }
}

#[derive(Debug)]
struct ERMResponse {
    body: BodyStream,
    url: UrlIR,
    version: VersionIR,
    headers: HeadersIR,
    status: StatusCodeIR,
}

impl ERMResponse {
    pub fn clone_parts(&self) -> ERMResponseParts {
        ERMResponseParts {
            url: self.url.clone(),
            version: self.version,
            headers: self.headers.clone(),
            status: self.status,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct ERMResponseParts {
    url: UrlIR,
    version: VersionIR,
    headers: HeadersIR,
    status: StatusCodeIR,
}

#[derive(Serialize, Deserialize)]
struct FinalResponse {
    parts: ERMResponseParts,
    body: Vec<TimedFrame>,
}
#[derive(Serialize, Deserialize)]
struct FinalRequest {
    parts: ERMRequestParts,
    body: Vec<TimedFrame>,
}

impl IntoResponse for ERMResponse {
    fn into_response(self) -> axum::response::Response {
        let mut incomplete_resp = axum::response::Response::builder()
            .status(self.status)
            .version(self.version.into());
        let empty_headers = incomplete_resp
            .headers_mut()
            .expect("Error when fetching the headers from ERMResponse");
        let current_headers: HeaderMap = self.headers.into();
        empty_headers.extend(current_headers);
        incomplete_resp
            .body(axum::body::Body::from_stream(self.body.0))
            .expect("Error when building a response")
    }
}

async fn send_request(
    req: ERMRequest,
    state: AppState,
    channel: UnboundedSender<TimedFrame>,
) -> Result<ERMResponse> {
    let (client, config) = (state.client, state.config);
    let res = client
        .request(req.method.into(), &config.destination)
        .headers(req.headers.into())
        .version(req.version.into())
        .body(reqwest::Body::wrap_stream(req.body.0))
        .send()
        .await
        .wrap_err("Could not send the request")?;
    let url = res.url().clone();
    let status = res.status();
    let res_http: http::Response<reqwest::Body> = res.into();
    let (parts, body) = res_http.into_parts();
    let mut last_frame_time: Option<Instant> = None;
    let raw_res = ERMResponse {
        url: UrlIR(url),
        version: VersionIR::from(parts.version),
        headers: HeadersIR::from(parts.headers),
        status: StatusCodeIR::from(status),
        body: BodyStream(
            axum::body::Body::new(axum::body::Body::from_stream(
                HttpBodyDataStream::new(body).map(move |b| match b {
                    Err(e) => Err(e),
                    Ok(bytes) => {
                        let now = Instant::now();
                        channel
                            .send(TimedFrame {
                                content: bytes.clone(),
                                tslf: now - last_frame_time.unwrap_or(now),
                            })
                            .expect("Error when sending bytes over a channel (Sending request)");
                        last_frame_time = Some(now);
                        Ok(bytes)
                    }
                }),
            ))
            .into_data_stream(),
        ),
    };
    Ok(raw_res)
}

#[derive(Clone)]
struct AppState {
    client: Client,
    config: Config,
}

impl ERMRequest {
    async fn from_request(
        req: axum::extract::Request,
        channel: UnboundedSender<TimedFrame>,
    ) -> Result<Self> {
        let (parts, body_req) = req.into_parts();

        let Parts {
            method,
            version,
            headers,
            ..
        } = parts.clone();
        let mut last_frame_time: Option<Instant> = None;
        let body = BodyStream(
            axum::body::Body::from_stream(body_req.into_data_stream().map(move |b| match b {
                Err(e) => Err(e),
                Ok(bytes) => {
                    let now = Instant::now();
                    channel
                        .send(TimedFrame {
                            content: bytes.clone(),
                            tslf: now - last_frame_time.unwrap_or(now),
                        })
                        .expect("Error when sending bytes over a channel (Building request)");
                    last_frame_time = Some(now);
                    Ok(bytes)
                }
            }))
            .into_data_stream(),
        );

        Ok(ERMRequest {
            body,
            method: MethodIR::from(method),
            version: VersionIR::from(version),
            headers: HeadersIR::from(headers),
        })
    }
}

async fn serialize_response(parts: ERMResponseParts, mut channel: UnboundedReceiver<TimedFrame>) {
    let mut data = Vec::with_capacity(1 << 5);
    while let Some(tf) = channel.recv().await {
        data.push(tf);
    }
    let ser_res = FinalResponse { parts, body: data };
    let content = ron::ser::to_string_pretty(&ser_res, PrettyConfig::default()).unwrap();
    std::fs::write("serialized_response.ron", content).unwrap();
}

async fn serialize_request(parts: ERMRequestParts, mut channel: UnboundedReceiver<TimedFrame>) {
    let mut data = Vec::with_capacity(1 << 5);
    while let Some(tf) = channel.recv().await {
        data.push(tf);
    }
    let ser_res = FinalRequest { parts, body: data };
    let content = ron::ser::to_string_pretty(&ser_res, PrettyConfig::default()).unwrap();
    std::fs::write("serialized_request.ron", content).unwrap();
}

#[instrument(skip(app_state))]
async fn handle_request(State(app_state): State<AppState>, req: Request) -> ERMResponse {
    let (req_sender, req_recv) = unbounded_channel::<TimedFrame>();
    let (res_sender, res_recv) = unbounded_channel::<TimedFrame>();

    spawn(serialize_request(
        ERMRequestParts::clone_from_request(&req),
        req_recv,
    ));
    let raw_req = ERMRequest::from_request(req, req_sender)
        .await
        .expect("Error when extracting the request");
    let res = send_request(raw_req, app_state, res_sender)
        .await
        .expect("Error when sending the request");
    spawn(serialize_response(res.clone_parts(), res_recv));
    res
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    info!("Tracing subscriber initialized");
    let config = Config {
        destination: "https://example.com".into(),
    };
    let client = ClientBuilder::new()
        .use_rustls_tls()
        .user_agent("erm")
        .build()
        .expect("could not build the Reqwest client");
    let state = AppState { client, config };
    let app = Router::new()
        .route("/", any(handle_request))
        .route("/{*wildcard}", any(handle_request))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
