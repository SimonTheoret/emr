use super::Format;
use crate::storage::Exchange;

pub(super) struct Ron;

impl Format for Ron {
    fn extension(&self) -> &'static str {
        "ron"
    }

    fn serialize(&self, _exchange: &Exchange) -> eyre::Result<Vec<u8>> {
        todo!("RON serialization")
    }

    fn deserialize(&self, _bytes: &[u8]) -> eyre::Result<Exchange> {
        todo!("RON deserialization")
    }
}
