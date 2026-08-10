use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, OnceLock,
};

use parking_lot::Mutex;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{
    dict::{errors::DictError, lru::DictLru},
    setup::{DictStore, SetupConnection},
    utils::quote_identifier,
};

pub mod errors;
mod lru;
mod train;
mod types;

pub use train::train;
pub use types::DictId;

pub static DICT_TABLE_NAME: &str = "__zstd_dicts";

type EncoderCache = Mutex<DictLru<EncoderDictionary<'static>>>;
type DecoderCache = Mutex<DictLru<DecoderDictionary<'static>>>;

static ENCODER_CACHE: OnceLock<EncoderCache> = OnceLock::new();
static DECODER_CACHE: OnceLock<DecoderCache> = OnceLock::new();

pub static LATEST_DICT_ID: AtomicU32 = AtomicU32::new(0);

/// Load the raw dictionary from the database.
fn load_raw_dict<C: DictStore>(dict_id: DictId, conn: &mut C) -> Result<Vec<u8>, DictError> {
    let rows = conn
        .query_blobs(&format!(
            "SELECT dict FROM \"{DICT_TABLE_NAME}\" WHERE id = {}",
            dict_id.get()
        ))
        .map_err(|e| DictError::Connection(e.into()))?;

    if rows.is_empty() {
        return Err(DictError::NotReady);
    }

    rows.into_iter().next().ok_or(DictError::NotFound(dict_id))
}

pub fn get_encoder_cached(dict_id: DictId) -> Option<Arc<EncoderDictionary<'static>>> {
    ENCODER_CACHE.get()?.lock().peek(dict_id)
}

pub fn get_decoder_cached(dict_id: DictId) -> Option<Arc<DecoderDictionary<'static>>> {
    DECODER_CACHE.get()?.lock().peek(dict_id)
}

pub fn get_encoder<C>(
    dict_id: DictId,
    conn: &mut C,
    level: i32,
) -> Result<Arc<EncoderDictionary<'static>>, DictError>
where
    C: DictStore,
{
    let cache = ENCODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));

    if let Some(d) = cache.lock().get(dict_id) {
        return Ok(d.clone());
    }

    let raw = load_raw_dict(dict_id, conn)?;
    let encoder = Arc::new(EncoderDictionary::copy(&raw, level));

    {
        let mut cache = cache.lock();
        if let Some(d) = cache.get(dict_id) {
            return Ok(d);
        }
        cache.insert(dict_id, encoder.clone());
    }

    LATEST_DICT_ID.store(dict_id.get(), Ordering::Relaxed);
    Ok(encoder)
}

pub fn get_decoder<C>(
    dict_id: DictId,
    conn: &mut C,
) -> Result<Arc<DecoderDictionary<'static>>, DictError>
where
    C: DictStore,
{
    let cache = DECODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));

    if let Some(d) = cache.lock().get(dict_id) {
        return Ok(d.clone());
    }

    let raw = load_raw_dict(dict_id, conn)?;
    let decoder = Arc::new(DecoderDictionary::copy(&raw));

    {
        let mut cache = cache.lock();
        if let Some(d) = cache.get(dict_id) {
            return Ok(d);
        }
        cache.insert(dict_id, decoder.clone());
    }

    LATEST_DICT_ID.store(dict_id.get(), Ordering::Relaxed);
    Ok(decoder)
}

/// Warm is require to keep sync between the database and the caches.
/// It is done by checking the latest dictionary id in the database.
/// If the latest dictionary id is 0, the cache is empty.
/// If the latest dictionary id is not 0, the cache is not empty.
/// Then, the dictionary is loaded from the database and inserted into the caches.
/// The dictionary is inserted into the caches is mandatory to ensure the dictionary is ready to be used.
/// By [get_decoder_cached](crate::dict::get_decoder_cached), [get_encoder_cached](crate::dict::get_encoder_cached),
/// the dictionary is cached in memory.
pub fn warm_cache<C>(conn: &mut C, level: i32) -> Result<(), DictError>
where
    C: SetupConnection,
{
    ENCODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));
    DECODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));

    let quoted_dict_table_name = quote_identifier(DICT_TABLE_NAME);

    let latest = conn
        .query_i64(&format!(
            "SELECT COALESCE(MAX(id), 0) FROM {quoted_dict_table_name}"
        ))
        .map_err(|e| DictError::Connection(e.into()))?;

    if latest == 0 {
        return Ok(());
    }

    let dict_id = u32::try_from(latest)
        .map(DictId::new)
        .map_err(|_| DictError::NotFound(DictId::new(0)))?;
    let _ = get_encoder(dict_id, conn, level)?;
    let _ = get_decoder(dict_id, conn)?;
    Ok(())
}

pub fn insert_into_caches(dict_id: DictId, dictionary: &[u8], level: i32) {
    let encoder = Arc::new(EncoderDictionary::copy(dictionary, level));
    let decoder = Arc::new(DecoderDictionary::copy(dictionary));

    ENCODER_CACHE
        .get_or_init(|| Mutex::new(DictLru::new()))
        .lock()
        .insert(dict_id, encoder);

    DECODER_CACHE
        .get_or_init(|| Mutex::new(DictLru::new()))
        .lock()
        .insert(dict_id, decoder);

    LATEST_DICT_ID.store(dict_id.get(), Ordering::Relaxed);
}
