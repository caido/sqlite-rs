use sqlite_ffi::{Context, SqliteError, Value};
use zstd::bulk::Compressor;

use crate::{
    cache::get_encoder_in_cache,
    dict::{errors::DictError, ColumnKey, DictId},
    functions::{errors::CodecError, header::wrap, types::Level, DEFAULT_LEVEL},
    state::ExtensionState,
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

pub(crate) fn compress(
    state: &ExtensionState,
    schema: &str,
    dict_id: DictId,
    data: &[u8],
) -> Result<Vec<u8>, SqliteError> {
    let compressed = if dict_id.get() != 0 {
        match get_encoder_in_cache(state, schema, dict_id, DEFAULT_LEVEL) {
            Ok(encoder) => compress_with_encoder(dict_id, data, &encoder)
                .map_err(|e| SqliteError::Message(e.to_string()))?,
            Err(DictError::NotReady) => compress_raw(data, DEFAULT_LEVEL)
                .map_err(|e| SqliteError::Message(e.to_string()))?,

            Err(e) => return Err(SqliteError::Message(e.to_string())),
        }
    } else {
        compress_raw(data, DEFAULT_LEVEL).map_err(|e| SqliteError::Message(e.to_string()))?
    };

    Ok(compressed)
}

pub fn sqlite_compress(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    let data = values[0].to_blob();
    let schema = values[1].to_text()?;
    let table = values[2].to_text()?;
    let column = values[3].to_text()?;

    let column = ColumnKey::new(schema, table, column);

    let state = ExtensionState::from_context(&context)?;

    let dict_id = state
        .cache
        .lock()
        .current_id(&column)
        .unwrap_or(DictId::from(0));

    let compressed = compress(&state, column.schema(), dict_id, data)?;

    context.result_blob(&compressed);

    Ok(())
}
