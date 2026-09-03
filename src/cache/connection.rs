use std::collections::HashMap;

use parking_lot::Mutex;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{cache::lru::DictLru, ColumnKey, DictId};

pub(crate) struct ConnectionCache {
    current_ids: Mutex<HashMap<ColumnKey, DictId>>,
    pub(crate) encoders: Mutex<DictLru<EncoderDictionary<'static>>>,
    pub(crate) decoders: Mutex<DictLru<DecoderDictionary<'static>>>,
}

impl ConnectionCache {
    pub(crate) fn new() -> Self {
        Self {
            current_ids: Mutex::new(HashMap::new()),
            encoders: Mutex::new(DictLru::new()),
            decoders: Mutex::new(DictLru::new()),
        }
    }

    pub(crate) fn current_id(&self, column: &ColumnKey) -> Option<DictId> {
        self.current_ids.lock().get(column).copied()
    }

    pub(crate) fn set_current_id(&self, column: ColumnKey, id: DictId) {
        self.current_ids.lock().insert(column, id);
    }

    pub(crate) fn clear_current_id(&self, column: &ColumnKey) {
        self.current_ids.lock().remove(column);
    }
}
