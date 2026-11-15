//! JSON in which every serde byte sequence is a padded, standard-alphabet base64 string.
//!
//! `Base64` wraps serde_json's serializer and rewrites only serde's `bytes` type, at any depth,
//! so this format follows the IR without naming any of its fields.

use std::fmt;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{
    Serialize,
    ser::{self, Serializer},
};

/// Behaves like the wrapped serde value, except that byte sequences pass through as base64
/// strings. It wraps every serializer and compound serializer on the way down, so nested bytes
/// are rewritten too.
struct Base64<T>(T);

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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bytes::Bytes;
    use proptest::{prelude::*, strategy::ValueTree, test_runner::TestRunner};
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        ir::{HeaderNameIR, HeadersIR, TimedFrame},
        storage::{Exchange, exchange::tests::exchange_strategy},
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
        let mut bytes = Vec::new();
        login_exchange()
            .serialize(Base64(&mut serde_json::Serializer::pretty(&mut bytes)))
            .unwrap();
        String::from_utf8(bytes).unwrap()
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
}
