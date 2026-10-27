use axum::body::BodyDataStream;

#[derive(Debug)]
pub struct BodyStream(pub BodyDataStream);
