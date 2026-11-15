use http::{StatusCode, status::InvalidStatusCode};
use serde::{Deserialize, Serialize};

/// A status code from 100 to 999, stored as a bare number. Deserializing anything else fails,
/// so a hand-edited file with `status: 42` is a corrupted file, not a replay panic.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(try_from = "u16", into = "u16")]
pub struct StatusCodeIR(pub StatusCode);

impl From<StatusCode> for StatusCodeIR {
    fn from(value: StatusCode) -> Self {
        Self(value)
    }
}

impl From<StatusCodeIR> for StatusCode {
    fn from(value: StatusCodeIR) -> Self {
        value.0
    }
}

impl TryFrom<u16> for StatusCodeIR {
    type Error = InvalidStatusCode;

    fn try_from(code: u16) -> Result<Self, Self::Error> {
        StatusCode::from_u16(code).map(Self)
    }
}

impl From<StatusCodeIR> for u16 {
    fn from(value: StatusCodeIR) -> Self {
        value.0.as_u16()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn status_code_strategy() -> impl Strategy<Value = StatusCodeIR> {
        (100_u16..=999).prop_map(|code| StatusCodeIR::try_from(code).unwrap())
    }

    #[test]
    fn accepts_every_code_from_100_to_999() {
        for code in 100..=999 {
            let status = StatusCodeIR::try_from(code).unwrap();

            assert_eq!(u16::from(status), code);
            assert_eq!(StatusCode::from(status).as_u16(), code);
        }
    }

    #[test]
    fn rejects_codes_outside_100_to_999() {
        for code in [0, 42, 99, 1000, u16::MAX] {
            assert!(StatusCodeIR::try_from(code).is_err(), "{code}");
        }
    }
}
