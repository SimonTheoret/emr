use axum::body::BodyDataStream;
use eyre::{Context, Report as EyreReport, eyre};
use reqwest::{Client, ClientBuilder};
use url::Url;

use axum::{
    Router,
    extract::{FromRequest, Request, State},
    response::IntoResponse,
    routing::any,
};
use http::{HeaderMap, HeaderValue, Method, StatusCode, Version, request::Parts};

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

#[derive(Debug)]
struct RawRequest {
    body: BodyWrapper,
    method: MethodWrapper,
    version: VersionWrapper,
    headers: HeadersMapWrapper,
}

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
        println!("Les parties: {:?}", parts);
        axum::response::Response::from_parts(parts, axum::body::Body::new(self.body.0))
    }
}

async fn send_request(req: RawRequest, state: AppState) -> Result<RawResponse> {
    let (client, config) = (state.client, state.config);
    let new_req = client
        .request(req.method.0, &config.destination)
        .headers(req.headers.0)
        .version(req.version.0)
        .body(reqwest::Body::wrap_stream(req.body.0))
        .build()
        .map_err(|e| Report(eyre!("Error when sending the request: {}", e)))?;
    let res = client
        .execute(new_req)
        .await
        .wrap_err("Could not send the request")?;
    let url = res.url().clone();
    let status = res.status();
    let res_http: http::Response<reqwest::Body> = res.into();
    let (parts, body) = res_http.into_parts();
    let raw_res = RawResponse {
        url: UrlWrapper(url),
        version: VersionWrapper(parts.version),
        headers: HeadersMapWrapper(parts.headers.clone()),
        status: StatusCodeWrapper(status),
        body: BodyWrapper(axum::body::Body::new(body).into_data_stream()),
        parts,
    };
    println!("raw response: {:#?}", raw_res);
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
        let (parts, body_req) = req.into_parts();

        let Parts {
            method,
            version,
            headers,
            ..
        } = parts.clone();

        let body = BodyWrapper(body_req.into_data_stream());

        Ok(RawRequest {
            body,
            method: MethodWrapper(method.clone()),
            version: VersionWrapper(version),
            headers: HeadersMapWrapper(headers),
        })
    }
}

#[tokio::main]
async fn main() {
    let config = Config {
        destination: "https://www.google.com".into(),
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
async fn handle_request(State(app_state): State<AppState>, req: Request) -> RawResponse {
    let raw_req = RawRequest::from_request(req, &())
        .await
        .expect("Error when extracting the request");
    println!("{:#?}", raw_req);

    send_request(raw_req, app_state)
        .await
        .expect("Error when sending the request")
}
