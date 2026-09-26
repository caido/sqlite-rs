use std::collections::HashMap;

use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{ColumnKey, DictId, cache::lru::DictLru};
/// Per-connection cache of prepared zstd dictionaries and the current dict id per column.
///
/// `encoders` and `decoders` are independent LRUs keyed by [`DictKey`] (schema + dict id).
///
/// `current_ids` maps a [`ColumnKey`] to the dict id that `compress` should use. It is
/// filled by `warm_cache` (latest row in `__compress_dicts`) and `insert_into_caches`
/// (after training). `clear_current_id` drops the entry when that column has no dictionary.
/// A missing id is treated as raw zstd (`DictId` 0).
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
