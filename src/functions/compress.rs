use std::sync::atomic::Ordering;

use sqlite_loadable::{api, prelude::*, Result};
use zstd::bulk::Compressor;

use crate::{
    conn::SqliteConn,
    dict::{errors::DictError, get_encoder, get_encoder_cached, DictId, LATEST_DICT_ID},
    functions::{errors::CodecError, header::wrap, types::Level, DEFAULT_LEVEL},
};

fn compress_with_encoder(
    dict_id: DictId,
    data: &[u8],
    encoder: &zstd::dict::EncoderDictionary<'static>,
) -> std::result::Result<Vec<u8>, CodecError> {
    let mut compressor =
        Compressor::with_prepared_dictionary(encoder).map_err(CodecError::CompressionFailed)?;
    let compressed = compressor
        .compress(data)
        .map_err(CodecError::CompressionFailed)?;
    wrap(dict_id, data.len(), compressed)
}

fn compress_raw(data: &[u8], level: Level) -> std::result::Result<Vec<u8>, CodecError> {
    let compressed =
        zstd::stream::encode_all(data, level.get()).map_err(CodecError::CompressionFailed)?;
    wrap(DictId::from(0), data.len(), compressed)
}

pub fn compress(data: &[u8], level: Level) -> std::result::Result<Vec<u8>, CodecError> {
    let id = DictId::from(LATEST_DICT_ID.load(Ordering::Relaxed));
    if id.get() != 0 {
        if let Some(encoder) = get_encoder_cached(id) {
            return compress_with_encoder(id, data, &encoder);
        }
    }
    compress_raw(data, level)
}

pub fn sqlite_compress(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) -> Result<()> {
    let value = values
        .first()
        .ok_or(CodecError::CompressionRequiresOneArgument)?;

    let data = api::value_blob(value);

    let id = DictId::from(LATEST_DICT_ID.load(Ordering::Relaxed));
    let mut conn = SqliteConn::from_context(context);

    let compressed = if id.get() != 0 {
        match get_encoder(id, &mut conn, DEFAULT_LEVEL) {
            Ok(encoder) => compress_with_encoder(id, data, &encoder)?,
            Err(DictError::NotReady) => compress_raw(data, DEFAULT_LEVEL)?,
            Err(e) => return Err(CodecError::DictError(e).into()),
        }
    } else {
        compress_raw(data, DEFAULT_LEVEL)?
    };
    api::result_blob(context, &compressed);
    Ok(())
}
