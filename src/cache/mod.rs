mod connection;
mod db_key;
mod lru;
mod prepared;
mod registry;

pub use db_key::DbKey;
pub(crate) use prepared::{get_decoder, get_encoder, insert_into_caches, warm_cache};
pub(crate) use registry::find_current_id;
pub use registry::CacheKeySource;
#[cfg(test)]
pub(crate) use registry::{remove_cache, test_has_cache};

/// Drops cache state only when `address` still belongs to `key`.
///
/// This guard prevents a delayed close callback for an old connection from
/// removing state belonging to a newer connection at the same address.
pub(crate) fn remove_connection_cache(address: usize, key: DbKey) {
    if DbKey::remove_handle_if_matches(address, key) {
        registry::remove_cache(key);
    }
}
