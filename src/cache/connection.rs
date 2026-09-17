use std::collections::HashMap;

use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{cache::lru::DictLru, ColumnKey, DictId};

pub(crate) struct DictCache {
    current_ids: HashMap<ColumnKey, DictId>,
    pub(crate) encoders: DictLru<EncoderDictionary<'static>>,
    pub(crate) decoders: DictLru<DecoderDictionary<'static>>,
}

impl DictCache {
    pub(crate) fn new() -> Self {
        Self {
            current_ids: HashMap::new(),
            encoders: DictLru::new(),
            decoders: DictLru::new(),
        }
    }

    pub(crate) fn current_id(&self, column: &ColumnKey) -> Option<DictId> {
        self.current_ids.get(column).copied()
    }

    pub(crate) fn set_current_id(&mut self, column: ColumnKey, id: DictId) {
        self.current_ids.insert(column, id);
    }

    pub(crate) fn clear_current_id(&mut self, column: &ColumnKey) {
        self.current_ids.remove(column);
    }
}
