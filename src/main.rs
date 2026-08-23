use axum::body::to_bytes;
use eyre::{Context, Report as EyreReport, eyre};
use futures::{Stream, StreamExt};
use reqwest::{Client, ClientBuilder, Response};
use std::time::{Duration, Instant};
use url::Url;

use axum::{
    Router,
    extract::{FromRequest, Request, State},
    response::IntoResponse,
    routing::any,
};
use bytes::Bytes;
use http::{HeaderMap, HeaderValue, Method, Uri, Version, request::Parts};

#[derive(Debug)]
struct MethodWrapper(Method);
#[derive(Debug)]
struct UriWrapper(Uri);
#[derive(Debug)]
struct VersionWrapper(Version);
#[derive(Debug)]
struct HeadersMapWrapper(HeaderMap<HeaderValue>);
#[derive(Debug)]
struct UrlWrapper(Url);

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

async fn extract_time_frames(res: Response) -> Result<Vec<TimedFrame>> {
    let mut stream = res.bytes_stream();
    let (l, opt_u) = stream.size_hint();
    let capacity = opt_u.unwrap_or_else(|| 2 * l);
    let mut acc = Vec::with_capacity(capacity);
    // 1 and 2 make it such that the first tslf is 0.0
    let mut last_frame_instant: Option<Instant> = None; // 1
    while let Some(r_frame) = stream.next().await {
        let now = Instant::now();
        let tslf = now - last_frame_instant.unwrap_or(now); // 2
        last_frame_instant = Some(now);
        let frame = match r_frame {
            Ok(f) => f,
            Err(e) => return Err(Report(eyre!("Error when receiving frame: {}", e))),
        };
        acc.push(TimedFrame {
            content: frame,
            tslf,
        });
    }
    Ok(acc)
}

#[derive(Debug)]
struct TimedFrame {
    /// The content of the frame
    content: Bytes,
    /// Time since last frame (ms)
    tslf: Duration,
}

#[derive(Debug)]
struct RawRequest {
    pub body: Bytes,
    pub method: MethodWrapper,
    pub uri: UriWrapper,
    pub version: VersionWrapper,
    pub headers: HeadersMapWrapper,
}

#[derive(Debug)]
struct RawResponse {
    pub body: Vec<TimedFrame>,
    pub url: UrlWrapper,
    pub version: VersionWrapper,
    pub headers: HeadersMapWrapper,
}

async fn send_request(req: RawRequest, state: AppState) -> Result<RawResponse> {
    let client = state.client;
    let config = state.config;
    let new_req = client
        .request(req.method.0, &config.destination)
        .headers(req.headers.0)
        .version(req.version.0)
        .build()
        .map_err(|e| Report(eyre!("Error when sending the request: {}", e)))?;
    let res = client
        .execute(new_req)
        .await
        .wrap_err("Could not send the request")?;
    let raw_res = RawResponse {
        url: UrlWrapper(res.url().clone()),
        version: VersionWrapper(res.version()),
        headers: HeadersMapWrapper(res.headers().clone()),
        body: extract_time_frames(res).await?,
    };
    Ok(raw_res)
}

#[derive(Clone)]
struct AppState {
    client: Client,
    config: Config,
}

impl<S> FromRequest<S> for RawRequest
where
    S: Send + Sync,
{
    type Rejection = Report;

    async fn from_request(req: axum::extract::Request, _state: &S) -> Result<Self> {
        let (
            Parts {
                method,
                uri,
                version,
                headers,
                ..
            },
            body_req,
        ) = req.into_parts();
        let body = to_bytes(body_req, usize::MAX)
            .await
            .expect("Could not extract the request's body ");

        Ok(RawRequest {
            body,
            method: MethodWrapper(method),
            uri: UriWrapper(uri),
            version: VersionWrapper(version),
            headers: HeadersMapWrapper(headers),
        })
    }
}

#[tokio::main]
async fn main() {
    let config = Config {
        destination: "https://example.com".into(),
    };
    let client = ClientBuilder::new()
        .build()
        .expect("could not build the Reqwest client");
    let state = AppState { client, config };
    // build our application with a single route
    let app = Router::new()
        .route("/", any(handle_request))
        .route("/{*wildcard}", any(handle_request))
        .with_state(state);

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

#[axum::debug_handler]
async fn handle_request(State(app_state): State<AppState>, req: Request) {
    let raw_req = RawRequest::from_request(req, &())
        .await
        .expect("Error when extracting the request");
    println!("{:#?}", raw_req);
    let res = send_request(raw_req, app_state)
        .await
        .expect("Error when sending the request");
    println!("{:#?}", res);
}
