#[cfg(feature = "json")]
mod json;
#[cfg(feature = "ron")]
mod ron;

use std::{fmt, str::FromStr};

use super::Exchange;

/// The file format that the filesystem storage reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatKind {
    #[cfg(feature = "ron")]
    Ron,
}

impl FormatKind {
    pub(super) fn format(self) -> &'static dyn Format {
        match self {
            #[cfg(feature = "ron")]
            Self::Ron => &ron::Ron,
        }
    }
}

impl FromStr for FormatKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            #[cfg(feature = "ron")]
            "ron" => Ok(Self::Ron),
            other => Err(format!("unknown or disabled storage format: {other}")),
        }
    }
}

impl fmt::Display for FormatKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.format().extension())
    }
}

pub(super) trait Format: Send + Sync {
    /// The file extension, without the dot.
    fn extension(&self) -> &'static str;
    fn serialize(&self, exchange: &Exchange) -> eyre::Result<Vec<u8>>;
    fn deserialize(&self, bytes: &[u8]) -> eyre::Result<Exchange>;
}
