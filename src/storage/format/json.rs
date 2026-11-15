//! JSON in which every serde byte sequence is a padded, standard-alphabet base64 string.
//!
//! `Base64` wraps serde_json's serializer and deserializer and rewrites only serde's `bytes`
//! type, at any depth, so this format follows the IR without naming any of its fields.

use std::fmt;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{
    Deserialize, Serialize,
    de::{self, DeserializeSeed, Deserializer, EnumAccess, MapAccess, SeqAccess, VariantAccess},
    ser::{self, Serializer},
};

use super::Format;
use crate::storage::Exchange;

pub(super) struct Json;

impl Format for Json {
    fn extension(&self) -> &'static str {
        "json"
    }

    fn serialize(&self, exchange: &Exchange) -> eyre::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        exchange.serialize(Base64(&mut serde_json::Serializer::pretty(&mut bytes)))?;
        Ok(bytes)
    }

    fn deserialize(&self, bytes: &[u8]) -> eyre::Result<Exchange> {
        let mut json = serde_json::Deserializer::from_slice(bytes);
        let exchange = Exchange::deserialize(Base64(&mut json))?;
        json.end()?;
        Ok(exchange)
    }
}

/// Behaves like the wrapped serde value, except that byte sequences pass through as base64
/// strings. It wraps every serializer, deserializer, visitor, seed, and access object on the way
/// down, so nested bytes are rewritten too.
struct Base64<T>(T);

/// Decodes a base64 string and hands the bytes to the wrapped visitor.
struct DecodeBytes<V>(V);

macro_rules! forward_serialize {
    ($($method:ident($ty:ty)),* $(,)?) => {$(
        fn $method(self, value: $ty) -> Result<S::Ok, S::Error> {
            self.0.$method(value)
        }
    )*};
}

impl<S: Serializer> Serializer for Base64<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = Base64<S::SerializeSeq>;
    type SerializeTuple = Base64<S::SerializeTuple>;
    type SerializeTupleStruct = Base64<S::SerializeTupleStruct>;
    type SerializeTupleVariant = Base64<S::SerializeTupleVariant>;
    type SerializeMap = Base64<S::SerializeMap>;
    type SerializeStruct = Base64<S::SerializeStruct>;
    type SerializeStructVariant = Base64<S::SerializeStructVariant>;

    forward_serialize!(
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_i128(i128),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_u128(u128),
        serialize_f32(f32),
        serialize_f64(f64),
        serialize_char(char),
        serialize_str(&str),
        serialize_unit_struct(&'static str),
    );

    fn serialize_bytes(self, value: &[u8]) -> Result<S::Ok, S::Error> {
        self.0.serialize_str(&STANDARD.encode(value))
    }

    fn serialize_none(self) -> Result<S::Ok, S::Error> {
        self.0.serialize_none()
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<S::Ok, S::Error> {
        self.0.serialize_some(&Base64(value))
    }

    fn serialize_unit(self) -> Result<S::Ok, S::Error> {
        self.0.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<S::Ok, S::Error> {
        self.0.serialize_unit_variant(name, index, variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<S::Ok, S::Error> {
        self.0.serialize_newtype_struct(name, &Base64(value))
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<S::Ok, S::Error> {
        self.0
            .serialize_newtype_variant(name, index, variant, &Base64(value))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, S::Error> {
        self.0.serialize_seq(len).map(Base64)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, S::Error> {
        self.0.serialize_tuple(len).map(Base64)
    }

    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, S::Error> {
        self.0.serialize_tuple_struct(name, len).map(Base64)
    }

    fn serialize_tuple_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, S::Error> {
        self.0
            .serialize_tuple_variant(name, index, variant, len)
            .map(Base64)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, S::Error> {
        self.0.serialize_map(len).map(Base64)
    }

    fn serialize_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, S::Error> {
        self.0.serialize_struct(name, len).map(Base64)
    }

    fn serialize_struct_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, S::Error> {
        self.0
            .serialize_struct_variant(name, index, variant, len)
            .map(Base64)
    }

    fn collect_str<T: ?Sized + fmt::Display>(self, value: &T) -> Result<S::Ok, S::Error> {
        self.0.collect_str(value)
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

impl<T: ?Sized + Serialize> Serialize for Base64<&T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(Base64(serializer))
    }
}

macro_rules! forward_compound {
    ($($trait:ident { $($method:ident($($arg:ident: $ty:ty),*);)* })*) => {$(
        impl<C: ser::$trait> ser::$trait for Base64<C> {
            type Ok = C::Ok;
            type Error = C::Error;

            $(
                fn $method<T: ?Sized + Serialize>(
                    &mut self,
                    $($arg: $ty,)*
                    value: &T,
                ) -> Result<(), C::Error> {
                    self.0.$method($($arg,)* &Base64(value))
                }
            )*

            fn end(self) -> Result<C::Ok, C::Error> {
                self.0.end()
            }
        }
    )*};
}

forward_compound! {
    SerializeSeq { serialize_element(); }
    SerializeTuple { serialize_element(); }
    SerializeTupleStruct { serialize_field(); }
    SerializeTupleVariant { serialize_field(); }
    SerializeStruct { serialize_field(key: &'static str); }
    SerializeStructVariant { serialize_field(key: &'static str); }
}

impl<C: ser::SerializeMap> ser::SerializeMap for Base64<C> {
    type Ok = C::Ok;
    type Error = C::Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), C::Error> {
        self.0.serialize_key(&Base64(key))
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), C::Error> {
        self.0.serialize_value(&Base64(value))
    }

    fn end(self) -> Result<C::Ok, C::Error> {
        self.0.end()
    }
}

macro_rules! forward_deserialize {
    ($($method:ident($($arg:ident: $ty:ty),*);)*) => {$(
        fn $method<V: de::Visitor<'de>>(
            self,
            $($arg: $ty,)*
            visitor: V,
        ) -> Result<V::Value, D::Error> {
            self.0.$method($($arg,)* Base64(visitor))
        }
    )*};
}

impl<'de, D: Deserializer<'de>> Deserializer<'de> for Base64<D> {
    type Error = D::Error;

    forward_deserialize! {
        deserialize_any();
        deserialize_bool();
        deserialize_i8();
        deserialize_i16();
        deserialize_i32();
        deserialize_i64();
        deserialize_i128();
        deserialize_u8();
        deserialize_u16();
        deserialize_u32();
        deserialize_u64();
        deserialize_u128();
        deserialize_f32();
        deserialize_f64();
        deserialize_char();
        deserialize_str();
        deserialize_string();
        deserialize_option();
        deserialize_unit();
        deserialize_unit_struct(name: &'static str);
        deserialize_newtype_struct(name: &'static str);
        deserialize_seq();
        deserialize_tuple(len: usize);
        deserialize_tuple_struct(name: &'static str, len: usize);
        deserialize_map();
        deserialize_struct(name: &'static str, fields: &'static [&'static str]);
        deserialize_enum(name: &'static str, variants: &'static [&'static str]);
        deserialize_identifier();
        deserialize_ignored_any();
    }

    fn deserialize_bytes<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        self.0.deserialize_str(DecodeBytes(visitor))
    }

    fn deserialize_byte_buf<V: de::Visitor<'de>>(self, visitor: V) -> Result<V::Value, D::Error> {
        self.0.deserialize_str(DecodeBytes(visitor))
    }

    fn is_human_readable(&self) -> bool {
        self.0.is_human_readable()
    }
}

impl<'de, V: de::Visitor<'de>> de::Visitor<'de> for DecodeBytes<V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a padded base64 string")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<V::Value, E> {
        let bytes = STANDARD
            .decode(value)
            .map_err(|error| E::custom(format_args!("invalid base64 {value:?}: {error}")))?;
        self.0.visit_byte_buf(bytes)
    }
}

macro_rules! forward_visit {
    ($($method:ident($ty:ty)),* $(,)?) => {$(
        fn $method<E: de::Error>(self, value: $ty) -> Result<V::Value, E> {
            self.0.$method(value)
        }
    )*};
}

impl<'de, V: de::Visitor<'de>> de::Visitor<'de> for Base64<V> {
    type Value = V::Value;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.expecting(f)
    }

    forward_visit!(
        visit_bool(bool),
        visit_i8(i8),
        visit_i16(i16),
        visit_i32(i32),
        visit_i64(i64),
        visit_i128(i128),
        visit_u8(u8),
        visit_u16(u16),
        visit_u32(u32),
        visit_u64(u64),
        visit_u128(u128),
        visit_f32(f32),
        visit_f64(f64),
        visit_char(char),
        visit_str(&str),
        visit_borrowed_str(&'de str),
        visit_string(String),
        visit_bytes(&[u8]),
        visit_borrowed_bytes(&'de [u8]),
        visit_byte_buf(Vec<u8>),
    );

    fn visit_none<E: de::Error>(self) -> Result<V::Value, E> {
        self.0.visit_none()
    }

    fn visit_unit<E: de::Error>(self) -> Result<V::Value, E> {
        self.0.visit_unit()
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<V::Value, D::Error> {
        self.0.visit_some(Base64(deserializer))
    }

    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<V::Value, D::Error> {
        self.0.visit_newtype_struct(Base64(deserializer))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<V::Value, A::Error> {
        self.0.visit_seq(Base64(seq))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<V::Value, A::Error> {
        self.0.visit_map(Base64(map))
    }

    fn visit_enum<A: EnumAccess<'de>>(self, data: A) -> Result<V::Value, A::Error> {
        self.0.visit_enum(Base64(data))
    }
}

impl<'de, T: DeserializeSeed<'de>> DeserializeSeed<'de> for Base64<T> {
    type Value = T::Value;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<T::Value, D::Error> {
        self.0.deserialize(Base64(deserializer))
    }
}

impl<'de, A: SeqAccess<'de>> SeqAccess<'de> for Base64<A> {
    type Error = A::Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, A::Error> {
        self.0.next_element_seed(Base64(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

impl<'de, A: MapAccess<'de>> MapAccess<'de> for Base64<A> {
    type Error = A::Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, A::Error> {
        self.0.next_key_seed(Base64(seed))
    }

    fn next_value_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<T::Value, A::Error> {
        self.0.next_value_seed(Base64(seed))
    }

    fn size_hint(&self) -> Option<usize> {
        self.0.size_hint()
    }
}

impl<'de, A: EnumAccess<'de>> EnumAccess<'de> for Base64<A> {
    type Error = A::Error;
    type Variant = Base64<A::Variant>;

    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, Self::Variant), A::Error> {
        self.0
            .variant_seed(Base64(seed))
            .map(|(value, variant)| (value, Base64(variant)))
    }
}

impl<'de, A: VariantAccess<'de>> VariantAccess<'de> for Base64<A> {
    type Error = A::Error;

    fn unit_variant(self) -> Result<(), A::Error> {
        self.0.unit_variant()
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, A::Error> {
        self.0.newtype_variant_seed(Base64(seed))
    }

    fn tuple_variant<V: de::Visitor<'de>>(
        self,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        self.0.tuple_variant(len, Base64(visitor))
    }

    fn struct_variant<V: de::Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, A::Error> {
        self.0.struct_variant(fields, Base64(visitor))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use proptest::{prelude::*, strategy::ValueTree, test_runner::TestRunner};
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        ir::{HeaderNameIR, HeadersIR, TimedFrame},
        storage::exchange::tests::exchange_strategy,
    };

    /// A generated exchange with a known header and body frame, so the test names no IR field
    /// outside the ones it checks.
    fn login_exchange() -> Exchange {
        let mut exchange = exchange_strategy()
            .new_tree(&mut TestRunner::deterministic())
            .unwrap()
            .current();
        exchange.request.parts.headers = HeadersIR(vec![(
            HeaderNameIR::new(Bytes::from_static(b"x-erm-match")),
            Bytes::from_static(b"login"),
        )]);
        exchange.request.body = vec![TimedFrame {
            content: Bytes::from_static(&[0x00, 0xff, 0xfe]),
            tslf: Duration::from_millis(15),
        }];
        exchange
    }

    fn login_json() -> String {
        String::from_utf8(Json.serialize(&login_exchange()).unwrap()).unwrap()
    }

    fn has_number_array(value: &Value) -> bool {
        match value {
            Value::Array(items) => {
                (!items.is_empty() && items.iter().all(Value::is_number))
                    || items.iter().any(has_number_array)
            }
            Value::Object(fields) => fields.values().any(has_number_array),
            _ => false,
        }
    }

    proptest! {
        #[test]
        fn exchanges_round_trip(exchange in exchange_strategy()) {
            let bytes = Json.serialize(&exchange).unwrap();
            let value: Value = serde_json::from_slice(&bytes).unwrap();
            prop_assert!(!has_number_array(&value), "no byte sequence is a JSON number array: {value}");
            prop_assert_eq!(Json.deserialize(&bytes).unwrap(), exchange);
        }
    }

    #[test]
    fn writes_body_frames_and_headers_as_padded_base64_strings() {
        let document: Value = serde_json::from_str(&login_json()).unwrap();

        assert_eq!(
            document["request"]["parts"]["headers"],
            json!([["eC1lcm0tbWF0Y2g=", "bG9naW4="]]),
            "header name and value are base64 strings",
        );
        assert_eq!(
            document["request"]["body"][0]["content"],
            json!("AP/+"),
            "body frame is a base64 string",
        );
    }

    #[test]
    fn rejects_malformed_json() {
        let valid = login_json();
        let cases = [
            ("empty", String::new()),
            ("truncated", valid[..valid.len() / 2].to_owned()),
            ("trailing data", format!("{valid} {{}}")),
        ];
        for (case, text) in cases {
            assert!(Json.deserialize(text.as_bytes()).is_err(), "{case}");
        }
    }

    #[test]
    fn rejects_bytes_that_are_not_padded_base64_strings() {
        let valid = login_json();
        let cases = [
            ("missing padding", "\"bG9naW4=\"", "\"bG9naW4\""),
            ("outside the alphabet", "\"bG9naW4=\"", "\"bG9n*W4=\""),
            ("url-safe alphabet", "\"AP/+\"", "\"AP_-\""),
            ("number array", "\"bG9naW4=\"", "[108,111,103,105,110]"),
        ];
        for (case, target, replacement) in cases {
            let text = valid.replacen(target, replacement, 1);
            assert_ne!(text, valid, "{case} changes the document");
            assert!(Json.deserialize(text.as_bytes()).is_err(), "{case}");
        }
    }
}
