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
mod tests {
    use super::*;

    #[test]
    fn converts_url_to_ir_without_changing_it() {
        let url = Url::parse("https://example.test:8443/a/path?q=value#fragment").unwrap();

        assert_eq!(UrlIR::from(url.clone()), UrlIR(url));
    }
}
