use serde::{Deserialize, Serialize};
use std::{
    ops::{Deref, DerefMut},
    str::FromStr,
};
use url::Url;

use eyre::{Context, Report, Result};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UrlIR(pub Url);

impl FromStr for UrlIR {
    type Err = Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url =
            Url::parse(s).wrap_err_with(|| format!("Error when parsing {} into an UrlIR", s))?;
        Ok(Self(url))
    }
}

impl UrlIR {
    pub fn parse(s: &str) -> Result<Self> {
        Self::from_str(s)
    }
}

impl From<Url> for UrlIR {
    fn from(value: Url) -> Self {
        Self(value)
    }
}
impl Deref for UrlIR {
    type Target = Url;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for UrlIR {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl AsRef<str> for UrlIR {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}
impl UrlIR {
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn url_strategy() -> impl Strategy<Value = UrlIR> {
        (0_u16..1000)
            .prop_map(|id| Url::parse(&format!("https://example.test/{id}")).unwrap())
            .prop_map(UrlIR::from)
    }
}
