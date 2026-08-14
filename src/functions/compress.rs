use sqlite_loadable::{api, prelude::*, Result};
use zstd::bulk::Compressor;

use crate::{
    conn::SqliteConn,
    dict::{
        errors::DictError, get_encoder, get_encoder_cached, ColumnKey, DictId, CURRENT_DICT_IDS,
    },
    functions::{errors::CodecError, header::wrap, types::Level, DEFAULT_LEVEL},
};

fn compress_with_encoder(
    dict_id: DictId,
    schema: &str,
    data: &[u8],
    encoder: &zstd::dict::EncoderDictionary<'static>,
) -> std::result::Result<Vec<u8>, CodecError> {
    let mut compressor =
        Compressor::with_prepared_dictionary(encoder).map_err(CodecError::CompressionFailed)?;
    let compressed = compressor
        .compress(data)
        .map_err(CodecError::CompressionFailed)?;
    wrap(dict_id, schema, data.len(), compressed)
}

fn compress_raw(
    schema: &str,
    data: &[u8],
    level: Level,
) -> std::result::Result<Vec<u8>, CodecError> {
    let compressed =
        zstd::stream::encode_all(data, level.get()).map_err(CodecError::CompressionFailed)?;
    wrap(DictId::from(0), schema, data.len(), compressed)
}

pub fn compress(
    data: &[u8],
    column: &ColumnKey,
    level: Level,
) -> std::result::Result<Vec<u8>, CodecError> {
    let id = CURRENT_DICT_IDS.lock().get(column).copied();

    if let Some(id) = id {
        if id.get() != 0 {
            if let Some(encoder) = get_encoder_cached(id) {
                return compress_with_encoder(id, column.schema(), data, &encoder);
            }
        }
    }

    compress_raw(column.schema(), data, level)
}

pub fn sqlite_compress(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) -> Result<()> {
    let data = api::value_blob(&values[0]);
    let schema = api::value_text(&values[1])?;
    let table = api::value_text(&values[2])?;
    let column = api::value_text(&values[3])?;
    let key = ColumnKey::new(schema, table, column);

    let id = CURRENT_DICT_IDS.lock().get(&key).copied();

    let mut conn = SqliteConn::from_context(context);

    let compressed = match id.filter(|id| id.get() != 0) {
        Some(id) => match get_encoder(key.schema(), id, &mut conn, DEFAULT_LEVEL) {
            Ok(encoder) => compress_with_encoder(id, key.schema(), data, &encoder)?,
            Err(DictError::NotReady) => compress_raw(key.schema(), data, DEFAULT_LEVEL)?,
            Err(e) => return Err(CodecError::DictError(e).into()),
        },
        None => compress_raw(key.schema(), data, DEFAULT_LEVEL)?,
    };

    api::result_blob(context, &compressed);
    Ok(())
}
