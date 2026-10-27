use std::cmp::Ordering;

use http::Uri;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UriIR(pub Uri);

impl Serialize for UriIR {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UriIR {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let uri = String::deserialize(deserializer)?;
        uri.parse::<Uri>().map(UriIR).map_err(D::Error::custom)
    }
}

impl PartialOrd for UriIR {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for UriIR {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.to_string().cmp(&other.0.to_string())
    }
}

impl From<UriIR> for Uri {
    fn from(value: UriIR) -> Self {
        value.0
    }
}

impl From<Uri> for UriIR {
    fn from(value: Uri) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_the_uri_string() {
        let uri = UriIR("/resource?q=value".parse().unwrap());

        assert_eq!(ron::to_string(&uri).unwrap(), r#""/resource?q=value""#);
    }

    #[test]
    fn deserializes_a_valid_uri_string() {
        let uri: UriIR = ron::from_str(r#""https://example.test/resource?q=value""#).unwrap();

        assert_eq!(
            uri,
            UriIR("https://example.test/resource?q=value".parse().unwrap())
        );
    }

    #[test]
    fn rejects_an_invalid_uri_string() {
        let result = ron::from_str::<UriIR>(r#""not a valid uri""#);

        assert!(result.is_err());
    }

    #[test]
    fn orders_uris_by_their_string_representations() {
        let earlier = UriIR("/a".parse().unwrap());
        let equal = UriIR("/a".parse().unwrap());
        let later = UriIR("/b".parse().unwrap());

        assert_eq!(earlier.cmp(&equal), Ordering::Equal);
        assert_eq!(earlier.cmp(&later), Ordering::Less);
        assert_eq!(later.cmp(&earlier), Ordering::Greater);
    }

    #[test]
    fn partially_orders_uris_by_their_string_representations() {
        let earlier = UriIR("/a".parse().unwrap());
        let equal = UriIR("/a".parse().unwrap());
        let later = UriIR("/b".parse().unwrap());

        assert_eq!(earlier.partial_cmp(&equal), Some(Ordering::Equal));
        assert_eq!(earlier.partial_cmp(&later), Some(Ordering::Less));
        assert_eq!(later.partial_cmp(&earlier), Some(Ordering::Greater));
    }

    #[test]
    fn converts_from_http_uri_without_changing_it() {
        let uri: Uri = "/resource?q=value".parse().unwrap();

        assert_eq!(UriIR::from(uri.clone()), UriIR(uri));
    }

    #[test]
    fn converts_to_http_uri_without_changing_it() {
        let uri: Uri = "/resource?q=value".parse().unwrap();

        assert_eq!(Uri::from(UriIR(uri.clone())), uri);
    }
}
