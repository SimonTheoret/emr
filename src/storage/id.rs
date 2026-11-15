use std::fmt;

use serde::{Deserialize, Serialize};

const MAX_LEN: usize = 200;
const RESERVED: [&str; 4] = ["con", "prn", "aux", "nul"];

/// A validated `x-erm-match` value. It is also the file name of its exchange.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidId {
    Empty,
    TooLong,
    Character(u8),
    Reserved,
}

impl Id {
    pub fn parse(value: &[u8]) -> Result<Self, InvalidId> {
        if value.is_empty() {
            return Err(InvalidId::Empty);
        }
        if value.len() > MAX_LEN {
            return Err(InvalidId::TooLong);
        }
        if let Some(&byte) = value
            .iter()
            .find(|byte| !matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
        {
            return Err(InvalidId::Character(byte));
        }
        let value: String = value.iter().map(|&byte| char::from(byte)).collect();
        if is_reserved(&value) {
            return Err(InvalidId::Reserved);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_reserved(value: &str) -> bool {
    RESERVED.contains(&value)
        || matches!(
            value.as_bytes(),
            [b'c', b'o', b'm', b'0'..=b'9'] | [b'l', b'p', b't', b'0'..=b'9']
        )
}

impl TryFrom<String> for Id {
    type Error = InvalidId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value.as_bytes())
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        value.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for InvalidId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "the id is empty"),
            Self::TooLong => write!(f, "the id is longer than {MAX_LEN} characters"),
            Self::Character(byte) => write!(
                f,
                "the id contains {:?}; only a-z, 0-9, '-', and '_' are allowed",
                char::from(*byte)
            ),
            Self::Reserved => write!(f, "the id is a file name that Windows reserves"),
        }
    }
}

impl std::error::Error for InvalidId {}

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn id_strategy() -> impl Strategy<Value = Id> {
        "[a-z0-9_-]{1,40}"
            .prop_filter_map("reserved name", |value| Id::parse(value.as_bytes()).ok())
    }

    #[test]
    fn rejects_values_outside_the_spec() {
        let too_long = "a".repeat(MAX_LEN + 1);
        let cases: [(&[u8], InvalidId); 8] = [
            (b"", InvalidId::Empty),
            (too_long.as_bytes(), InvalidId::TooLong),
            (b"Login", InvalidId::Character(b'L')),
            (b"a/b", InvalidId::Character(b'/')),
            (b"a.b", InvalidId::Character(b'.')),
            (b"con", InvalidId::Reserved),
            (b"com1", InvalidId::Reserved),
            (b"lpt0", InvalidId::Reserved),
        ];
        for (value, expected) in cases {
            assert_eq!(Id::parse(value), Err(expected), "{value:?}");
        }
    }

    #[test]
    fn accepts_values_inside_the_spec() {
        let longest = "a".repeat(MAX_LEN);
        for value in ["login", "a-b_c9", "console", "com10", longest.as_str()] {
            assert_eq!(Id::parse(value.as_bytes()).unwrap().as_str(), value);
        }
    }

    proptest! {
        #[test]
        fn every_allowed_value_that_is_not_reserved_parses(value in "[a-z0-9_-]{1,200}") {
            prop_assume!(!is_reserved(&value));
            let id = Id::parse(value.as_bytes()).unwrap();
            prop_assert_eq!(id.as_str(), value.as_str());
        }
    }
}
