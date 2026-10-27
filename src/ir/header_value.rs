use bytes::Bytes;

pub type HeaderValueIR = Bytes;

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    pub(crate) fn header_value_strategy() -> impl Strategy<Value = HeaderValueIR> {
        prop::collection::vec(any::<u8>(), 0..64).prop_map(Bytes::from)
    }
}
