use std::collections::HashMap;

use super::{Exchange, Id, Replayer};

pub(super) struct MemoryReplayer {
    exchanges: HashMap<Id, Exchange>,
}

impl MemoryReplayer {
    pub(super) fn new(exchanges: HashMap<Id, Exchange>) -> Self {
        Self { exchanges }
    }
}

impl Replayer for MemoryReplayer {
    fn find(&self, id: &Id) -> Option<&Exchange> {
        self.exchanges.get(id)
    }
}
