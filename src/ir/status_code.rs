use std::num::NonZeroU16;

use http::StatusCode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatusCodeIR(pub NonZeroU16);

impl From<StatusCode> for StatusCodeIR {
    fn from(value: StatusCode) -> Self {
        StatusCodeIR(NonZeroU16::new(value.as_u16()).unwrap())
    }
}

impl From<StatusCodeIR> for StatusCode {
    fn from(value: StatusCodeIR) -> Self {
        StatusCode::from_u16(u16::from(value.0)).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_every_valid_status_code_to_ir() {
        for code in 100..=999 {
            let status = status_code(code);
            let converted = StatusCodeIR::from(status);

            assert_eq!(converted.0.get(), code);
        }
    }

    #[test]
    fn converts_every_valid_ir_status_code_to_http() {
        for code in 100..=999 {
            let ir = StatusCodeIR(NonZeroU16::new(code).unwrap());

            assert_eq!(StatusCode::from(ir), status_code(code));
        }
    }

    fn status_code(code: u16) -> StatusCode {
        StatusCode::from_u16(code).unwrap()
    }
}
