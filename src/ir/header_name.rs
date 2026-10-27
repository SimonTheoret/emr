use bytes::Bytes;
use http::HeaderName;
use serde::{Deserialize, Serialize};

/// A lowercase representation of an HTTP header name.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(from = "Bytes")]
#[serde(into = "Bytes")]
pub struct HeaderNameIR(Bytes);

impl HeaderNameIR {
    pub fn new(bytes: Bytes) -> Self {
        Self(Bytes::copy_from_slice(&bytes.to_ascii_lowercase()))
    }
}

impl From<HeaderName> for HeaderNameIR {
    fn from(value: HeaderName) -> Self {
        Self(Bytes::from_owner(value))
    }
}

impl From<Bytes> for HeaderNameIR {
    fn from(value: Bytes) -> Self {
        Self::new(value)
    }
}

impl From<HeaderNameIR> for Bytes {
    fn from(value: HeaderNameIR) -> Self {
        value.0
    }
}

impl PartialEq<Vec<u8>> for HeaderNameIR {
    fn eq(&self, other: &Vec<u8>) -> bool {
        self.0 == other
    }
}

impl PartialEq<Bytes> for HeaderNameIR {
    fn eq(&self, other: &Bytes) -> bool {
        self.0 == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_lowercases_ascii_letters() {
        let name = HeaderNameIR::new(Bytes::from_static(b"X-CuStOm-HeAdEr"));

        assert_eq!(name, Bytes::from_static(b"x-custom-header"));
    }

    #[test]
    fn new_accepts_an_empty_name() {
        let name = HeaderNameIR::new(Bytes::new());

        assert_eq!(name, Bytes::new());
    }

    #[test]
    fn new_leaves_non_ascii_bytes_unchanged() {
        let name = HeaderNameIR::new(Bytes::from_static(b"X-\x80\xff"));

        assert_eq!(name, Bytes::from_static(b"x-\x80\xff"));
    }

    #[test]
    fn converts_from_http_header_name() {
        let http_name = HeaderName::from_bytes(b"X-Custom").unwrap();

        let name = HeaderNameIR::from(http_name);

        assert_eq!(name, Bytes::from_static(b"x-custom"));
    }

    #[test]
    fn converts_from_bytes_and_normalizes_case() {
        let name = HeaderNameIR::from(Bytes::from_static(b"Content-TYPE"));

        assert_eq!(name, Bytes::from_static(b"content-type"));
    }

    #[test]
    fn converts_into_bytes_without_changing_content() {
        let name = HeaderNameIR::new(Bytes::from_static(b"X-Test"));

        let bytes = Bytes::from(name);

        assert_eq!(bytes, Bytes::from_static(b"x-test"));
    }

    #[test]
    fn compares_with_byte_vectors() {
        let name = HeaderNameIR::new(Bytes::from_static(b"X-Test"));

        assert_eq!(name, b"x-test".to_vec());
        assert_ne!(name, b"x-other".to_vec());
    }

    #[test]
    fn compares_with_bytes() {
        let name = HeaderNameIR::new(Bytes::from_static(b"X-Test"));

        assert_eq!(name, Bytes::from_static(b"x-test"));
        assert_ne!(name, Bytes::from_static(b"x-other"));
    }

    #[test]
    fn serialization_uses_the_normalized_bytes() {
        let name = HeaderNameIR::new(Bytes::from_static(b"X-Test"));

        let serialized = ron::to_string(&name).unwrap();
        let bytes: Bytes = ron::from_str(&serialized).unwrap();

        assert_eq!(bytes, Bytes::from_static(b"x-test"));
    }

    #[test]
    fn deserialization_normalizes_ascii_case() {
        let serialized = ron::to_string(&Bytes::from_static(b"X-TEST")).unwrap();

        let name: HeaderNameIR = ron::from_str(&serialized).unwrap();

        assert_eq!(name, Bytes::from_static(b"x-test"));
    }

    #[test]
    fn normalized_name_round_trips_through_ron() {
        let name = HeaderNameIR::new(Bytes::from_static(b"X-Test"));

        let serialized = ron::to_string(&name).unwrap();
        let deserialized: HeaderNameIR = ron::from_str(&serialized).unwrap();

        assert_eq!(deserialized, name);
    }
}
