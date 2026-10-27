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
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn version_strategy() -> impl Strategy<Value = VersionIR> {
        version_case_strategy().prop_map(|(version, _)| version)
    }

    fn version_case_strategy() -> impl Strategy<Value = (VersionIR, Version)> {
        prop::sample::select(version_cases().to_vec())
    }

    pub(crate) fn version_cases() -> [(VersionIR, Version); 5] {
        [
            (VersionIR::Http09, Version::HTTP_09),
            (VersionIR::Http10, Version::HTTP_10),
            (VersionIR::Http11, Version::HTTP_11),
            (VersionIR::Http2, Version::HTTP_2),
            (VersionIR::Http3, Version::HTTP_3),
        ]
    }

    proptest! {
        #[test]
        fn converts_supported_versions_in_both_directions(
            (version, http_version) in version_case_strategy()
        ) {
            prop_assert_eq!(VersionIR::from(http_version), version);
            prop_assert_eq!(Version::from(version), http_version);
        }
    }
}
