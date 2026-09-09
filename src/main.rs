use ron::ser::PrettyConfig;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tracing::{info, instrument};
use uuid::Uuid;

use axum::{Router, extract::Request, extract::State, routing::any};
use eyre::{Context, Result};
use futures::StreamExt;
use http_body_util::BodyDataStream as HttpBodyDataStream;

use reqwest::{Client, ClientBuilder};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::spawn,
};

mod cli;
mod ir;
mod storage;
use crate::{
    ir::{
        BodyStream, ERMRequest, ERMRequestParts, ERMResponse, ERMResponseParts, FinalRequest,
        FinalResponse, HeadersIR, Mock, StatusCodeIR, TimedFrame, UrlIR, VersionIR,
    },
    storage::MockFileStorage,
};

#[derive(Clone)]
struct Config {
    destination: String,
    mock_dir: PathBuf,
    serialization_extension: String,
}

async fn send_request(
    req: ERMRequest,
    state: AppState,
    id: Uuid,
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
        id,
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
    storage: Arc<Mutex<MockFileStorage>>,
}

async fn serialize_response(
    parts: ERMResponseParts,
    mut channel: UnboundedReceiver<TimedFrame>,
    mut dir: PathBuf,
) {
    let id = parts.id;
    let mut data = Vec::with_capacity(1 << 5);
    while let Some(tf) = channel.recv().await {
        data.push(tf);
    }
    let ser_res = Mock::Response(FinalResponse { parts, body: data });
    let content = ron::ser::to_string_pretty(&ser_res, PrettyConfig::default()).unwrap();
    dir.push(format!("response_{}.ron", id.clone()));
    std::fs::write(dir, content).unwrap();
}

async fn serialize_request(
    parts: ERMRequestParts,
    mut channel: UnboundedReceiver<TimedFrame>,
    mut dir: PathBuf,
) {
    let id = parts.id;
    let mut data = Vec::with_capacity(1 << 5);
    while let Some(tf) = channel.recv().await {
        data.push(tf);
    }
    let ser_res = Mock::Request(FinalRequest { parts, body: data });
    let content = ron::ser::to_string_pretty(&ser_res, PrettyConfig::default()).unwrap();
    dir.push(format!("request_{}.ron", id.clone()));
    std::fs::write(dir, content).unwrap();
}

#[instrument(skip(app_state))]
async fn handle_request(State(app_state): State<AppState>, req: Request) -> ERMResponse {
    let id = Uuid::now_v7();
    let mock_dir = app_state.config.mock_dir.clone();
    let storage = app_state.storage.clone();
    let (req_sender, req_recv) = unbounded_channel::<TimedFrame>();
    let (res_sender, res_recv) = unbounded_channel::<TimedFrame>();

    spawn(serialize_request(
        ERMRequestParts::clone_from_request(&req, id),
        req_recv,
        mock_dir.clone(),
    ));
    let raw_req = ERMRequest::from_request(req, req_sender)
        .await
        .expect("Error when extracting the request");
    let res = send_request(raw_req, app_state, id, res_sender)
        .await
        .expect("Error when sending the request");
    spawn(serialize_response(res.clone_parts(), res_recv, mock_dir));
    let mut guard = storage.lock().unwrap();
    let files = guard
        .files()
        .expect("Error when reading the files from the storage");
    info!("Files: {:#?}", files);
    res
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();
    info!("Tracing subscriber initialized");
    let config = Config {
        destination: "https://example.com".into(),
        mock_dir: PathBuf::from("mocks"),
        serialization_extension: String::from("ron"),
    };
    let storage = Arc::new(Mutex::new(
        MockFileStorage::new(&config.mock_dir, config.serialization_extension.clone())
            .expect("Error when creating the storage"),
    ));
    let client = ClientBuilder::new()
        .use_rustls_tls()
        .user_agent("erm")
        .build()
        .expect("could not build the Reqwest client");
    let state = AppState {
        client,
        config,
        storage,
    };

    let app = Router::new()
        .route("/", any(handle_request))
        .route("/{*wildcard}", any(handle_request))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
