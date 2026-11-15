// The old recorder's types and file format, deleted by the next changes.
#[allow(dead_code)]
mod body_stream;
#[allow(dead_code)]
mod erm_request;
mod erm_request_parts;
#[allow(dead_code)]
mod erm_response;
mod erm_response_parts;
mod final_request;
mod final_response;
mod header_name;
mod header_value;
mod headers;
mod method;
#[allow(dead_code)]
mod mock;
mod status_code;
mod timed_frame;
mod uri;
mod url;
mod version;

#[allow(unused_imports)]
pub use body_stream::BodyStream;
#[allow(unused_imports)]
pub use erm_request::ERMRequest;
pub use erm_request_parts::ERMRequestParts;
#[allow(unused_imports)]
pub use erm_response::ERMResponse;
pub use erm_response_parts::ERMResponseParts;
pub use final_request::FinalRequest;
pub use final_response::FinalResponse;
pub use header_name::HeaderNameIR;
pub use header_value::HeaderValueIR;
pub use headers::HeadersIR;
pub use method::MethodIR;
pub use mock::Mock;
pub use status_code::StatusCodeIR;
pub use timed_frame::TimedFrame;
pub use uri::UriIR;
pub use url::UrlIR;
pub use version::VersionIR;

#[cfg(test)]
pub(crate) use final_request::tests::final_request_strategy;
#[cfg(test)]
pub(crate) use final_response::tests::final_response_strategy;
