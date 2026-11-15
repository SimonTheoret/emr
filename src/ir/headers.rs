use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::{HeaderNameIR, HeaderValueIR};

/// Headers in their recorded order, repeated names included.
///
/// Deserializing rejects any pair that `HeaderMap` would reject, so a hand-edited file with
/// an invalid name or value is a corrupted file, not a replay panic.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct HeadersIR(
    #[serde(deserialize_with = "deserialize_valid")] pub Vec<(HeaderNameIR, HeaderValueIR)>,
);

fn deserialize_valid<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<(HeaderNameIR, HeaderValueIR)>, D::Error> {
    let headers = Vec::<(HeaderNameIR, HeaderValueIR)>::deserialize(deserializer)?;
    for (name, value) in &headers {
        parse(name, value).map_err(D::Error::custom)?;
    }
    Ok(headers)
}

fn parse(name: &HeaderNameIR, value: &HeaderValueIR) -> Result<(HeaderName, HeaderValue), String> {
    let name = Bytes::from(name.clone());
    let name =
        HeaderName::from_bytes(&name).map_err(|_| format!("invalid header name {name:?}"))?;
    let value = HeaderValue::from_bytes(value)
        .map_err(|_| format!("invalid value {value:?} for header {name}"))?;
    Ok((name, value))
}

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
        let mut map = Self::with_capacity(value.0.len());

        for (name, value) in &value.0 {
            let (name, value) = parse(name, value)
                .expect("a HeadersIR comes from a HeaderMap or from a validated file");
            map.append(name, value);
        }

        map
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::ir::{
        header_name::tests::header_name_strategy, header_value::tests::header_value_strategy,
    };

    pub(crate) fn headers_strategy() -> impl Strategy<Value = HeadersIR> {
        prop::collection::vec((header_name_strategy(), header_value_strategy()), 0..32)
            .prop_map(HeadersIR)
    }

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
