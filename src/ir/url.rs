use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct UrlIR(pub Url);

impl From<Url> for UrlIR {
    fn from(value: Url) -> Self {
        Self(value)
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
