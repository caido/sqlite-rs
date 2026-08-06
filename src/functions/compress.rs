use std::sync::atomic::Ordering;

use crate::conn::SqliteConn;
use crate::dict::errors::DictError;
use crate::dict::{get_encoder, get_encoder_cached, LATEST_DICT_ID};
use crate::functions::errors::CodecError;
use crate::functions::DEFAULT_LEVEL;
use sqlite_loadable::prelude::*;
use sqlite_loadable::{api, Result};
use zstd::bulk::Compressor;

fn compress_with_encoder(
    dict_id: u32,
    data: &[u8],
    encoder: &zstd::dict::EncoderDictionary<'static>,
) -> std::io::Result<Vec<u8>> {
    let mut compressor = Compressor::with_prepared_dictionary(encoder)?;
    let compressed = compressor.compress(data)?;
    let mut out = Vec::with_capacity(4 + compressed.len());
    out.extend_from_slice(&dict_id.to_le_bytes());
    out.extend_from_slice(&compressed);
    Ok(out)
}

fn compress_raw(data: &[u8], level: i32) -> std::result::Result<Vec<u8>, CodecError> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend(zstd::stream::encode_all(data, level).map_err(CodecError::CompressionFailed)?);
    Ok(out)
}

pub fn compress(data: &[u8], level: i32) -> std::result::Result<Vec<u8>, CodecError> {
    let id = LATEST_DICT_ID.load(Ordering::Relaxed);
    if id != 0 {
        if let Some(encoder) = get_encoder_cached(id) {
            return compress_with_encoder(id, data, &encoder)
                .map_err(CodecError::CompressionFailed);
        }
    }
    compress_raw(data, level)
}

pub fn sqlite_compress(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) -> Result<()> {
    let value = values
        .first()
        .ok_or(CodecError::CompressionRequiresOneArgument)?;

    let data = api::value_blob(value);

    let id = LATEST_DICT_ID.load(Ordering::Relaxed);
    let mut conn = SqliteConn::from_context(context);

    let compressed = if id != 0 {
        match get_encoder(id, &mut conn, DEFAULT_LEVEL) {
            Ok(encoder) => {
                compress_with_encoder(id, data, &encoder).map_err(CodecError::CompressionFailed)?
            }
            Err(DictError::NotReady) => compress_raw(data, DEFAULT_LEVEL)?,
            Err(e) => return Err(CodecError::DictError(e).into()),
        }
    } else {
        compress_raw(data, DEFAULT_LEVEL)?
    };
    api::result_blob(context, &compressed);
    Ok(())
}
