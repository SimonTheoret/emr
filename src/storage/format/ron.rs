use super::Format;
use crate::storage::Exchange;

pub(super) struct Ron;

impl Format for Ron {
    fn extension(&self) -> &'static str {
        "ron"
    }

    fn serialize(&self, exchange: &Exchange) -> eyre::Result<Vec<u8>> {
        let text = ::ron::ser::to_string_pretty(exchange, ::ron::ser::PrettyConfig::default())?;
        Ok(text.into_bytes())
    }

    fn deserialize(&self, bytes: &[u8]) -> eyre::Result<Exchange> {
        Ok(::ron::de::from_bytes(bytes)?)
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::storage::exchange::tests::exchange_strategy;

    proptest! {
        #[test]
        fn exchanges_round_trip(exchange in exchange_strategy()) {
            let bytes = Ron.serialize(&exchange).unwrap();
            prop_assert_eq!(Ron.deserialize(&bytes).unwrap(), exchange);
        }
    }
}
