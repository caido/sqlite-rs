use sqlite_loadable::{api, prelude::*, Result};
use zstd::bulk::Compressor;

use crate::{
    cache::get_encoder_in_cache,
    client_data,
    conn::SqliteConn,
    dict::{errors::DictError, ColumnKey, DictId},
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

pub fn sqlite_compress(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) -> Result<()> {
    let data = api::value_blob(&values[0]);
    let schema = api::value_text(&values[1])?;
    let table = api::value_text(&values[2])?;
    let column = api::value_text(&values[3])?;
    let key = ColumnKey::new(schema, table, column);

    let db = api::context_db_handle(context);

    let mut conn = SqliteConn::from_context(context);

    let id = client_data::with_cache(db, |cache| cache.lock().current_id(&key))
        .expect("connection state must exist");

    let compressed = match id.filter(|id| id.get() != 0) {
        Some(id) => {
            let encoder = client_data::with_cache(db, |cache| {
                get_encoder_in_cache(cache, &mut conn, key.schema(), id, DEFAULT_LEVEL)
            })
            .expect("connection state must exist");

            match encoder {
                Ok(encoder) => compress_with_encoder(id, data, &encoder)?,
                Err(DictError::NotReady) => compress_raw(data, DEFAULT_LEVEL)?,
                Err(e) => return Err(CodecError::DictError(e).into()),
            }
        }
        None => compress_raw(data, DEFAULT_LEVEL)?,
    };

    api::result_blob(context, &compressed);
    Ok(())
}
