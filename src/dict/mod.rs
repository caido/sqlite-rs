use crate::dict::errors::DictError;
use crate::setup::{DictStore, SetupConnection};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use zstd::dict::{DecoderDictionary, EncoderDictionary};

pub mod errors;
mod train;

pub use train::train;

static DICT_TABLE_NAME: &str = "_zstd_dicts";

static ENCODER_CACHE: OnceLock<RwLock<HashMap<u32, Arc<EncoderDictionary<'static>>>>> =
    OnceLock::new();

static DECODER_CACHE: OnceLock<RwLock<HashMap<u32, Arc<DecoderDictionary<'static>>>>> =
    OnceLock::new();

pub static LATEST_DICT_ID: AtomicU32 = AtomicU32::new(0);

fn load_raw_dict<C: DictStore>(dict_id: u32, conn: &mut C) -> Result<Vec<u8>, DictError> {
    let rows = conn
        .query_blobs(&format!(
            "SELECT dict FROM \"{DICT_TABLE_NAME}\" WHERE id = {dict_id}"
        ))
        .map_err(|e| DictError::Connection(e.into()))?;

    if rows.is_empty() {
        return Err(DictError::NotReady);
    }

    rows.into_iter().next().ok_or(DictError::NotFound(dict_id))
}

pub fn get_encoder_cached(dict_id: u32) -> Option<Arc<EncoderDictionary<'static>>> {
    ENCODER_CACHE.get()?.read().unwrap().get(&dict_id).cloned()
}

pub fn get_decoder_cached(dict_id: u32) -> Option<Arc<DecoderDictionary<'static>>> {
    DECODER_CACHE.get()?.read().unwrap().get(&dict_id).cloned()
}

pub fn get_encoder<C>(
    dict_id: u32,
    conn: &mut C,
    level: i32,
) -> Result<Arc<EncoderDictionary<'static>>, DictError>
where
    C: DictStore,
{
    let cache = ENCODER_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    if let Some(d) = cache.read().unwrap().get(&dict_id) {
        return Ok(d.clone());
    }

    let raw = load_raw_dict(dict_id, conn)?;
    let encoder = Arc::new(EncoderDictionary::copy(&raw, level));

    cache.write().unwrap().insert(dict_id, encoder.clone());
    LATEST_DICT_ID.store(dict_id, Ordering::Relaxed);
    Ok(encoder)
}

pub fn get_decoder<C>(
    dict_id: u32,
    conn: &mut C,
) -> Result<Arc<DecoderDictionary<'static>>, DictError>
where
    C: DictStore,
{
    let cache = DECODER_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    if let Some(d) = cache.read().unwrap().get(&dict_id) {
        return Ok(d.clone());
    }

    let raw = load_raw_dict(dict_id, conn)?;
    let decoder = Arc::new(DecoderDictionary::copy(&raw));

    cache.write().unwrap().insert(dict_id, decoder.clone());
    LATEST_DICT_ID.store(dict_id, Ordering::Relaxed);
    Ok(decoder)
}

pub fn warm_cache<C>(conn: &mut C, level: i32) -> Result<(), DictError>
where
    C: SetupConnection,
{
    ENCODER_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    DECODER_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    let latest = conn
        .query_i64("SELECT COALESCE(MAX(id), 0) FROM \"_zstd_dicts\"")
        .map_err(|e| DictError::Connection(e.into()))?;

    if latest == 0 {
        return Ok(());
    }

    let dict_id = u32::try_from(latest).map_err(|_| DictError::NotFound(0))?;
    let _ = get_encoder(dict_id, conn, level)?;
    let _ = get_decoder(dict_id, conn)?;
    Ok(())
}

pub fn insert_into_caches(dict_id: u32, dictionary: &[u8], level: i32) {
    let encoder = Arc::new(EncoderDictionary::copy(dictionary, level));
    let decoder = Arc::new(DecoderDictionary::copy(dictionary));

    ENCODER_CACHE
        .get_or_init(|| RwLock::new(HashMap::new()))
        .write()
        .unwrap()
        .insert(dict_id, encoder);

    DECODER_CACHE
        .get_or_init(|| RwLock::new(HashMap::new()))
        .write()
        .unwrap()
        .insert(dict_id, decoder);

    LATEST_DICT_ID.store(dict_id, Ordering::Relaxed);
}
