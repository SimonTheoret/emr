use http::Version;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub enum VersionIR {
    Http09,
    Http10,
    #[default]
    Http11,
    Http2,
    Http3,
}

impl From<Version> for VersionIR {
    fn from(value: Version) -> Self {
        if value == Version::HTTP_09 {
            Self::Http09
        } else if value == Version::HTTP_10 {
            Self::Http10
        } else if value == Version::HTTP_11 {
            Self::Http11
        } else if value == Version::HTTP_2 {
            Self::Http2
        } else if value == Version::HTTP_3 {
            Self::Http3
        } else {
            Self::default()
        }
    }
}

impl From<VersionIR> for Version {
    fn from(value: VersionIR) -> Self {
        match value {
            VersionIR::Http09 => Version::HTTP_09,
            VersionIR::Http10 => Version::HTTP_10,
            VersionIR::Http11 => Version::HTTP_11,
            VersionIR::Http2 => Version::HTTP_2,
            VersionIR::Http3 => Version::HTTP_3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_every_supported_http_version_to_ir() {
        let cases = [
            (Version::HTTP_09, VersionIR::Http09),
            (Version::HTTP_10, VersionIR::Http10),
            (Version::HTTP_11, VersionIR::Http11),
            (Version::HTTP_2, VersionIR::Http2),
            (Version::HTTP_3, VersionIR::Http3),
        ];

        for (version, expected) in cases {
            assert_eq!(VersionIR::from(version), expected);
        }
    }

    #[test]
    fn converts_every_ir_version_to_http() {
        let cases = [
            (VersionIR::Http09, Version::HTTP_09),
            (VersionIR::Http10, Version::HTTP_10),
            (VersionIR::Http11, Version::HTTP_11),
            (VersionIR::Http2, Version::HTTP_2),
            (VersionIR::Http3, Version::HTTP_3),
        ];

        for (version, expected) in cases {
            assert_eq!(Version::from(version), expected);
        }
    }
}
