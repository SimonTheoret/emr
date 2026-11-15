use super::Format;
use crate::storage::Exchange;

pub(super) struct Json;

impl Format for Json {
    fn extension(&self) -> &'static str {
        "json"
    }

    fn serialize(&self, _exchange: &Exchange) -> eyre::Result<Vec<u8>> {
        todo!("JSON serialization")
    }

    fn deserialize(&self, _bytes: &[u8]) -> eyre::Result<Exchange> {
        todo!("JSON deserialization")
    }
}
