use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

use super::{HeaderNameIR, HeaderValueIR};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeadersIR(pub Vec<(HeaderNameIR, HeaderValueIR)>);

impl From<HeaderMap> for HeadersIR {
    fn from(value: HeaderMap) -> Self {
        let mut headers = Vec::with_capacity(1 << 5);
        let mut last_name: Option<HeaderName> = None;

        for (name, value) in value {
            let name = if let Some(name) = name {
                last_name = Some(name.clone());
                name
            } else {
                last_name
                    .clone()
                    .expect("HeaderMap values after the first have a preceding name")
            };
            headers.push((name.into(), Bytes::from_owner(value)));
        }

        Self(headers)
    }
}

impl From<HeadersIR> for HeaderMap {
    fn from(value: HeadersIR) -> Self {
        let mut map = Self::with_capacity(1 << 5);

        for (name, value) in value.0 {
            let name_bytes: Bytes = name.into();
            let name = HeaderName::from_bytes(&name_bytes).unwrap();
            let value = HeaderValue::from_bytes(&value).unwrap();
            map.append(name, value);
        }

        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_an_empty_header_map_to_ir() {
        let headers = HeadersIR::from(HeaderMap::new());

        assert!(headers.0.is_empty());
    }

    #[test]
    fn converts_a_single_header_map_value_to_ir() {
        let mut map = HeaderMap::new();
        map.insert("x-test", HeaderValue::from_static("value"));

        let headers = HeadersIR::from(map);

        assert_eq!(
            headers.0,
            vec![(
                HeaderNameIR::new(Bytes::from_static(b"x-test")),
                Bytes::from_static(b"value"),
            )]
        );
    }

    #[test]
    fn converts_repeated_header_map_values_to_ir_without_losing_values() {
        let mut map = HeaderMap::new();
        map.append("set-cookie", HeaderValue::from_static("first=1"));
        map.append("set-cookie", HeaderValue::from_static("second=2"));

        let headers = HeadersIR::from(map);

        assert_eq!(
            headers.0,
            vec![
                (
                    HeaderNameIR::new(Bytes::from_static(b"set-cookie")),
                    Bytes::from_static(b"first=1"),
                ),
                (
                    HeaderNameIR::new(Bytes::from_static(b"set-cookie")),
                    Bytes::from_static(b"second=2"),
                ),
            ]
        );
    }

    #[test]
    fn converts_empty_ir_headers_to_a_header_map() {
        let map = HeaderMap::from(HeadersIR(Vec::new()));

        assert!(map.is_empty());
    }

    #[test]
    fn converts_a_single_ir_header_to_a_header_map() {
        let headers = HeadersIR(vec![(
            HeaderNameIR::new(Bytes::from_static(b"x-test")),
            Bytes::from_static(b"value"),
        )]);

        let map = HeaderMap::from(headers);

        assert_eq!(map.get("x-test").unwrap(), "value");
    }

    #[test]
    fn converts_repeated_ir_headers_without_losing_values() {
        let name = HeaderNameIR::new(Bytes::from_static(b"set-cookie"));
        let headers = HeadersIR(vec![
            (name.clone(), Bytes::from_static(b"first=1")),
            (name, Bytes::from_static(b"second=2")),
        ]);

        let map = HeaderMap::from(headers);
        let values: Vec<_> = map
            .get_all("set-cookie")
            .iter()
            .map(HeaderValue::as_bytes)
            .collect();

        assert_eq!(values, [b"first=1".as_slice(), b"second=2".as_slice()]);
    }
}
