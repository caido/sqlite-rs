use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use parking_lot::Mutex;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{
    cache::{connection::ConnectionCache, db_key::DbKey},
    dict::{ColumnKey, DictId, DictKey},
};

pub static REGISTRY: LazyLock<Mutex<CacheRegistry>> =
    LazyLock::new(|| Mutex::new(CacheRegistry::new()));

pub trait CacheKeySource {
    fn db_key(&self) -> DbKey;
}

pub(super) struct CacheRegistry {
    caches: HashMap<DbKey, ConnectionCache>,
}

impl CacheRegistry {
    fn new() -> Self {
        Self {
            caches: HashMap::new(),
        }
    }

    pub(crate) fn cache_for(&mut self, key: DbKey) -> &ConnectionCache {
        self.caches.entry(key).or_insert_with(ConnectionCache::new)
    }

    pub(crate) fn current_id(&self, db_key: DbKey, column: &ColumnKey) -> Option<DictId> {
        self.caches.get(&db_key)?.current_id(column)
    }

    pub(crate) fn peek_encoder(
        &self,
        db_key: DbKey,
        key: &DictKey,
    ) -> Option<Arc<EncoderDictionary<'static>>> {
        self.caches.get(&db_key)?.encoders.lock().peek(key)
    }

    pub(crate) fn peek_decoder(
        &self,
        db_key: DbKey,
        key: &DictKey,
    ) -> Option<Arc<DecoderDictionary<'static>>> {
        self.caches.get(&db_key)?.decoders.lock().peek(key)
    }
}

pub(crate) fn remove_cache(key: DbKey) {
    REGISTRY.lock().caches.remove(&key);
}

pub(crate) fn with_conn<C, R, F>(conn: &C, f: F) -> R
where
    C: CacheKeySource,
    F: FnOnce(&ConnectionCache) -> R,
{
    let key = conn.db_key();
    let mut registry = REGISTRY.lock();
    let cache = registry.cache_for(key);
    f(cache)
}

pub(crate) fn find_current_id<C: CacheKeySource>(conn: &C, column: &ColumnKey) -> Option<DictId> {
    let db_key = conn.db_key();
    REGISTRY.lock().current_id(db_key, column)
}

#[cfg(test)]
pub(crate) fn test_has_cache(key: DbKey) -> bool {
    REGISTRY.lock().caches.contains_key(&key)
}
