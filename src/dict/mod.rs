use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, OnceLock},
};

use parking_lot::Mutex;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{
    dict::{errors::DictError, lru::DictLru},
    functions::Level,
    setup::{DictStore, SetupConnection},
    utils::{quote_literal, quote_qualified},
};

pub mod errors;
mod lru;
mod train;
mod types;

pub use train::{train_all, train_by_column};
pub use types::{ColumnKey, DictId, DictKey};

pub static DICT_TABLE_NAME: &str = "__compress_dicts";

pub fn dict_table(schema: &str) -> String {
    quote_qualified(schema, DICT_TABLE_NAME)
}

type EncoderCache = Mutex<DictLru<EncoderDictionary<'static>>>;
type DecoderCache = Mutex<DictLru<DecoderDictionary<'static>>>;

static ENCODER_CACHE: OnceLock<EncoderCache> = OnceLock::new();
static DECODER_CACHE: OnceLock<DecoderCache> = OnceLock::new();

pub static CURRENT_DICT_IDS: LazyLock<Mutex<HashMap<ColumnKey, DictId>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Load the raw dictionary from the database.
fn load_raw_dict<C: DictStore>(dict_key: &DictKey, conn: &mut C) -> Result<Vec<u8>, DictError> {
    let table = dict_table(&dict_key.schema);
    let rows = conn
        .query_blobs(&format!(
            "SELECT dict AS value FROM {table} WHERE id = {}",
            dict_key.id.get()
        ))
        .map_err(|e| DictError::Connection(e.into()))?;

    if rows.is_empty() {
        return Err(DictError::NotReady);
    }

    rows.into_iter()
        .next()
        .ok_or(DictError::NotFound(dict_key.id))
}

pub fn get_encoder_cached(
    schema: &str,
    dict_id: DictId,
) -> Option<Arc<EncoderDictionary<'static>>> {
    ENCODER_CACHE
        .get()?
        .lock()
        .peek(&DictKey::new(schema, dict_id))
}

pub fn get_decoder_cached(
    schema: &str,
    dict_id: DictId,
) -> Option<Arc<DecoderDictionary<'static>>> {
    DECODER_CACHE
        .get()?
        .lock()
        .peek(&DictKey::new(schema, dict_id))
}

pub fn get_encoder<C>(
    schema: &str,
    dict_id: DictId,
    conn: &mut C,
    level: Level,
) -> Result<Arc<EncoderDictionary<'static>>, DictError>
where
    C: DictStore,
{
    let cache = ENCODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));
    let key = DictKey::new(schema, dict_id);

    if let Some(d) = cache.lock().get(&key) {
        return Ok(d.clone());
    }

    let raw = load_raw_dict(&key, conn)?;
    let encoder = Arc::new(EncoderDictionary::copy(&raw, level.get()));

    {
        let mut cache = cache.lock();
        if let Some(d) = cache.get(&key) {
            return Ok(d);
        }
        cache.insert(key, encoder.clone());
    }

    Ok(encoder)
}

pub fn get_decoder<C>(
    schema: &str,
    dict_id: DictId,
    conn: &mut C,
) -> Result<Arc<DecoderDictionary<'static>>, DictError>
where
    C: DictStore,
{
    let cache = DECODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));
    let key = DictKey::new(schema, dict_id);

    if let Some(d) = cache.lock().get(&key) {
        return Ok(d.clone());
    }

    let raw = load_raw_dict(&key, conn)?;
    let decoder = Arc::new(DecoderDictionary::copy(&raw));

    {
        let mut cache = cache.lock();
        if let Some(d) = cache.get(&key) {
            return Ok(d);
        }
        cache.insert(key, decoder.clone());
    }

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
pub fn warm_cache<C>(conn: &mut C, column: &ColumnKey, level: Level) -> Result<(), DictError>
where
    C: SetupConnection,
{
    ENCODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));
    DECODER_CACHE.get_or_init(|| Mutex::new(DictLru::new()));

    let table = dict_table(column.schema());
    let latest = conn
        .query_i64(&format!(
            "SELECT COALESCE(MAX(id), 0) FROM {table} \
             WHERE table_name = {} AND column_name = {}",
            quote_literal(column.table()),
            quote_literal(column.column())
        ))
        .map_err(|e| DictError::Connection(e.into()))?;

    if latest == 0 {
        return Ok(());
    }

    let dict_id = u32::try_from(latest)
        .map(DictId::new)
        .map_err(|_| DictError::NotFound(DictId::new(0)))?;

    CURRENT_DICT_IDS.lock().insert(column.clone(), dict_id);

    let _ = get_encoder(column.schema(), dict_id, conn, level)?;
    let _ = get_decoder(column.schema(), dict_id, conn)?;

    Ok(())
}

pub fn insert_into_caches(schema: &str, dict_id: DictId, dictionary: &[u8], level: Level) {
    let encoder = Arc::new(EncoderDictionary::copy(dictionary, level.get()));
    let decoder = Arc::new(DecoderDictionary::copy(dictionary));
    let key = DictKey::new(schema, dict_id);

    ENCODER_CACHE
        .get_or_init(|| Mutex::new(DictLru::new()))
        .lock()
        .insert(key.clone(), encoder);

    DECODER_CACHE
        .get_or_init(|| Mutex::new(DictLru::new()))
        .lock()
        .insert(key, decoder);
}
