use std::collections::HashMap;
use std::any::Any

use axum::{Router, body, extract::FromRequest, http::Request, routing::any};

struct RawRequest {
    raw: Request<body::Body>,
}

impl FromRequest<()> for RawRequest {
    type Rejection = ();
    async fn from_request(
        req: axum::extract::Request,
        _state: &(),
    ) -> Result<Self, Self::Rejection> {
        Ok(Self { raw: req })
    }
}

struct Visitor;

impl RawRequest {
    fn test_to_str(&self) -> &str {
        let all: HashMap<&'static str, Option<String>> = HashMap::from([
            ("method", Some(self.raw.method().as_str().to_owned())),
            ("uri", Some(self.raw.uri().to_string())),
            ("version", Some(format!("{:?}", self.raw.version()))),
            ("headers", Some),
        ]);
    }
}

#[tokio::main]
async fn main() {
    // build our application with a single route
    let app = Router::new().route("/", any(|| async { "Hello, World!" }));

    // run our app with hyper, listening globally on port 3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
