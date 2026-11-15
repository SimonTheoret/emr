use bytes::Bytes;

pub type HeaderValueIR = Bytes;

#[cfg(test)]
pub(crate) mod tests {
    use proptest::prelude::*;

    use super::*;

    /// Values that `http::HeaderValue` accepts: no control byte other than tab.
    pub(crate) fn header_value_strategy() -> impl Strategy<Value = HeaderValueIR> {
        let byte = any::<u8>().prop_filter("a control byte", |&byte| {
            byte == b'\t' || (byte >= 0x20 && byte != 0x7f)
        });
        prop::collection::vec(byte, 0..64).prop_map(Bytes::from)
    }
}
