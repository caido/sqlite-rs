use std::{
    collections::{HashMap, hash_map::Entry},
    sync::{Arc, LazyLock},
};

use parking_lot::Mutex;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{dict::DictKey, functions::Level};

const MAX_PREPARED_DICTS: usize = 64;

type PreparedMap<T> = HashMap<DictKey, Arc<T>>;

static ENCODERS: LazyLock<Mutex<PreparedMap<EncoderDictionary<'static>>>> =
    LazyLock::new(|| Mutex::new(HashMap::with_capacity(MAX_PREPARED_DICTS)));

static DECODERS: LazyLock<Mutex<PreparedMap<DecoderDictionary<'static>>>> =
    LazyLock::new(|| Mutex::new(HashMap::with_capacity(MAX_PREPARED_DICTS)));

pub(crate) fn get_encoder(
    key: &DictKey,
    level: Level,
    load: impl FnOnce() -> Result<Vec<u8>, crate::DictError>,
) -> Result<Arc<EncoderDictionary<'static>>, crate::DictError> {
    if let Some(dict) = ENCODERS.lock().get(key).cloned() {
        return Ok(dict);
    }

    let raw = load()?;

    let mut cache = ENCODERS.lock();
    if let Some(dict) = cache.get(key).cloned() {
        return Ok(dict);
    }

    let encoder = Arc::new(EncoderDictionary::copy(&raw, level.get()));
    insert_capped(&mut cache, key.clone(), encoder.clone());
    Ok(encoder)
}

pub(crate) fn get_decoder(
    key: &DictKey,
    load: impl FnOnce() -> Result<Vec<u8>, crate::DictError>,
) -> Result<Arc<DecoderDictionary<'static>>, crate::DictError> {
    if let Some(dict) = DECODERS.lock().get(key).cloned() {
        return Ok(dict);
    }

    let raw = load()?;

    let mut cache = DECODERS.lock();
    if let Some(dict) = cache.get(key).cloned() {
        return Ok(dict);
    }

    let decoder = Arc::new(DecoderDictionary::copy(&raw));
    insert_capped(&mut cache, key.clone(), decoder.clone());
    Ok(decoder)
}

fn insert_capped<T>(map: &mut PreparedMap<T>, key: DictKey, value: Arc<T>) {
    let at_cap = map.len() >= MAX_PREPARED_DICTS;

    match map.entry(key) {
        Entry::Occupied(mut e) => {
            e.insert(value);
        }
        Entry::Vacant(e) => {
            if at_cap {
                let key = e.into_key();
                map.clear();
                map.insert(key, value);
            } else {
                e.insert(value);
            }
        }
    }
}

pub(crate) fn insert_prepared(key: DictKey, dictionary: &[u8], level: Level) {
    insert_capped(
        &mut ENCODERS.lock(),
        key.clone(),
        Arc::new(EncoderDictionary::copy(dictionary, level.get())),
    );
    insert_capped(
        &mut DECODERS.lock(),
        key,
        Arc::new(DecoderDictionary::copy(dictionary)),
    );
}
