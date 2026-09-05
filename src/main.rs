use std::fmt::Debug;
use tracing::{info, instrument};

use axum::body::BodyDataStream;
use axum::{
    RequestExt, Router, extract::FromRequestParts, extract::Request, extract::State,
    response::IntoResponse, routing::any,
};
use bytes::{Bytes, BytesMut};
use eyre::{Context, Report as EyreReport, eyre};
use futures::StreamExt;
use http::{HeaderMap, HeaderValue, Method, StatusCode, Version, request::Parts};
use http_body_util::BodyDataStream as HttpBodyDataStream;
use reqwest::{Client, ClientBuilder};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::{spawn, spawn_blocking},
};
use url::Url;

mod streaming;

#[derive(Debug, Clone)]
struct MethodWrapper(Method);
#[derive(Debug, Clone)]
struct VersionWrapper(Version);
#[derive(Debug, Clone)]
struct HeadersMapWrapper(HeaderMap<HeaderValue>);
#[derive(Debug, Clone)]
struct UrlWrapper(Url);
#[derive(Debug)]
struct BodyWrapper(BodyDataStream);

#[derive(Debug, Clone)]
struct StatusCodeWrapper(StatusCode);

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

// async fn extract_time_frames(res: Response) -> Result<Vec<TimedFrame>> {
//     let mut stream = res.bytes_stream();
//     let (l, opt_u) = stream.size_hint();
//     let capacity = opt_u.unwrap_or_else(|| 2 * l);
//     let mut acc = Vec::with_capacity(capacity);
//     // 1 and 2 make it such that the first tslf is 0.0
//     let mut last_frame_instant: Option<Instant> = None; // 1
//     while let Some(r_frame) = stream.next().await {
//         let now = Instant::now();
//         let tslf = now - last_frame_instant.unwrap_or(now); // 2
//         last_frame_instant = Some(now);
//         let frame = match r_frame {
//             Ok(f) => f,
//             Err(e) => return Err(Report(eyre!("Error when receiving frame: {}", e))),
//         };
//         acc.push(TimedFrame {
//             content: frame,
//             tslf,
//         });
//     }
//     Ok(acc)
// }

// #[derive(Debug, Clone)]
// struct TimedFrame {
//     /// The content of the frame
//     content: Bytes,
//     /// Time since last frame (ms)
//     tslf: Duration,
// }

#[derive(Clone, Debug)]
struct RequestSerializer(BytesMut);

#[derive(Debug)]
struct RawRequest {
    body: BodyWrapper,
    method: MethodWrapper,
    version: VersionWrapper,
    headers: HeadersMapWrapper,
}

#[derive(Clone, Debug)]
struct ResponseSerializer(BytesMut);

#[derive(Debug)]
struct RawResponse {
    body: BodyWrapper,
    url: UrlWrapper,
    version: VersionWrapper,
    headers: HeadersMapWrapper,
    status: StatusCodeWrapper,
    parts: http::response::Parts,
}

impl IntoResponse for RawResponse {
    fn into_response(self) -> axum::response::Response {
        let parts = self.parts;
        // println!("Les parties: {:?}", parts);
        axum::response::Response::from_parts(parts, axum::body::Body::from_stream(self.body.0))
    }
}

async fn send_request(
    req: RawRequest,
    state: AppState,
    channel: UnboundedSender<Bytes>,
) -> Result<RawResponse> {
    let (client, config) = (state.client, state.config);
    // let builder = client.request(req.method.0, &config.destination);
    let res = client
        .request(req.method.0, &config.destination)
        .headers(
            req.headers
                .0
                .into_iter()
                .map(|(opt_h, v)| (opt_h.unwrap(), v))
                .collect(),
        )
        .version(req.version.0)
        .body(reqwest::Body::wrap_stream(req.body.0))
        .send()
        .await
        .wrap_err("Could not send the request")?;
    // .map_err(|e| Report(eyre!("Error when sending the request: {}", e)))?;
    // println!("Sending the following request:\n{:#?}", new_req);
    // let res = client
    //     .execute(new_req)
    //     .await
    //     .wrap_err("Could not send the request")?;
    let url = res.url().clone();
    let status = res.status();
    let res_http: http::Response<reqwest::Body> = res.into();
    let (parts, body) = res_http.into_parts();
    let raw_res = RawResponse {
        url: UrlWrapper(url),
        version: VersionWrapper(parts.version),
        headers: HeadersMapWrapper(parts.headers.clone()),
        status: StatusCodeWrapper(status),
        body: BodyWrapper(
            axum::body::Body::new(axum::body::Body::from_stream(
                HttpBodyDataStream::new(body).map(move |b| match b {
                    Err(e) => Err(e),
                    Ok(bytes) => {
                        channel
                            .send(bytes.clone())
                            .expect("Error when sending bytes over a channel (Sending request)");
                        Ok(bytes)
                    }
                }),
            ))
            .into_data_stream(),
        ),
        parts,
    };
    // println!("raw response: {:#?}", raw_res);
    Ok(raw_res)
}

#[derive(Clone)]
struct AppState {
    client: Client,
    config: Config,
}

impl RawRequest {
    async fn from_request(
        req: axum::extract::Request,
        channel: UnboundedSender<Bytes>,
    ) -> Result<Self> {
        let (parts, body_req) = req.into_parts();

        let Parts {
            method,
            version,
            headers,
            ..
        } = parts.clone();
        let body = BodyWrapper(
            axum::body::Body::from_stream(body_req.into_data_stream().map(move |b| match b {
                Err(e) => Err(e),
                Ok(bytes) => {
                    channel
                        .send(bytes.clone())
                        .expect("Error when sending bytes over a channel (Building request)");
                    Ok(bytes)
                }
            }))
            .into_data_stream(),
        );

        Ok(RawRequest {
            body,
            method: MethodWrapper(method.clone()),
            version: VersionWrapper(version),
            headers: HeadersMapWrapper(headers),
        })
    }
}
fn copy_parts_from_req(req: &Request) -> Result<Parts> {
    let mut incomplete_builder = http::Request::builder()
        .method(req.method().clone())
        .uri(req.uri().clone())
        .version(req.version().clone())
        .extension(req.extensions().clone());
    for (h_n, h_v) in req.headers().iter() {
        incomplete_builder = incomplete_builder.header(h_n, h_v);
    }
    incomplete_builder
        .body(())
        .map(|r| {
            let (parts, ()) = r.into_parts();
            parts
        })
        .wrap_err_with(|| "Error when extracting the parts of a request")
        .map_err(|err| Report(err))
}

fn copy_parts_from_res(req: &RawResponse) -> Result<Parts> {
    let mut incomplete_builder = http::Response::builder()
        .uri(req.uri().clone())
        .version(req.version().clone())
        .extension(req.extensions().clone());
    for (h_n, h_v) in req.headers().iter() {
        incomplete_builder = incomplete_builder.header(h_n, h_v);
    }
    incomplete_builder
        .body(())
        .map(|r| {
            let (parts, ()) = r.into_parts();
            parts
        })
        .wrap_err_with(|| "Error when extracting the parts of a request")
        .map_err(|err| Report(err))
}

async fn serialize(parts: Parts, mut channel: UnboundedReceiver<Bytes>) {
    let mut data = Vec::<u8>::with_capacity(2 >> 10);
    while let Some(b) = channel.recv().await {
        data.extend_from_slice(&b);
    }
    println!("Finished reading bytes:\n{:#?}", data);
}

#[instrument(skip(app_state))]
async fn handle_request(State(app_state): State<AppState>, req: Request) -> RawResponse {
    let (req_sender, req_recv) = unbounded_channel::<Bytes>();
    let (res_sender, res_recv) = unbounded_channel::<Bytes>();
    spawn(serialize(copy_parts_from_req(&req).unwrap(), req_recv));
    let raw_req = RawRequest::from_request(req, req_sender)
        .await
        .expect("Error when extracting the request");
    info!("Request constructed");
    let res = send_request(raw_req, app_state, res_sender)
        .await
        .expect("Error when sending the request");
    spawn(serialize(copy_parts_from_res(&res), res_recv));
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
