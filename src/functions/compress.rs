use sqlite_ffi::{Context, Value};
use zstd::bulk::Compressor;

use crate::{
    cache::get_encoder_in_cache,
    conn::Connection,
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

pub fn sqlite_compress(context: Context, values: &[Value]) {
    let data = values[0].to_blob();
    let schema = values[1].to_text().unwrap();
    let table = values[2].to_text().unwrap();
    let column = values[3].to_text().unwrap();

    let key = ColumnKey::new(schema, table, column);

    let conn = Connection::from_context(&context).unwrap();

    let id = conn
        .cache
        .lock()
        .current_id(&key)
        .ok_or(CodecError::MissingConnectionState)
        .unwrap();

    let compressed = if id.get() != 0 {
        match get_encoder_in_cache(&conn, schema, id, DEFAULT_LEVEL) {
            Ok(encoder) => compress_with_encoder(id, data, &encoder).unwrap(),
            Err(DictError::NotReady) => compress_raw(data, DEFAULT_LEVEL).unwrap(),
            Err(_) => return, //Err(CodecError::DictError(e).into()).unwrap(),
        }
    } else {
        compress_raw(data, DEFAULT_LEVEL).unwrap()
    };

    context.result_blob(&compressed);
}
