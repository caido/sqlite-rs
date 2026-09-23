use std::result::Result;

use sqlite_ffi::{Context, SqliteError, Value};
use zstd::bulk::Decompressor;

use crate::{
    cache::get_decoder_in_cache,
    functions::{
        errors::CodecError::{self},
        header::Header,
    },
    state::ExtensionState,
    DictError, DictId,
};

pub(crate) fn decompress_with_decoder(
    data: &[u8],
    decoder: &zstd::dict::DecoderDictionary<'static>,
    uncompressed_len: usize,
) -> Result<Vec<u8>, CodecError> {
    let mut decompressor =
        Decompressor::with_prepared_dictionary(decoder).map_err(CodecError::DecompressionFailed)?;
    decompressor
        .decompress(data, uncompressed_len)
        .map_err(CodecError::DecompressionFailed)
}

pub(crate) fn decompress_raw(
    payload: &[u8],
    uncompressed_len: usize,
) -> Result<Vec<u8>, CodecError> {
    zstd::bulk::decompress(payload, uncompressed_len).map_err(CodecError::DecompressionFailed)
}

pub(crate) fn decompress(
    state: &ExtensionState,
    blob: &[u8],
    schema: &str,
) -> Result<Vec<u8>, SqliteError> {
    let (header, payload) = Header::parse(blob).map_err(|e| SqliteError::Message(e.to_string()))?;

    let dict_id = DictId::from(header.dict_id.get());
    let len = header.uncompressed_len.get() as usize;

    let decompressed = if dict_id.get() != 0 {
        let decoder = get_decoder_in_cache(state, schema, dict_id);

        match decoder {
            Ok(decoder) => decompress_with_decoder(payload, &decoder, len)
                .map_err(|e| SqliteError::Message(e.to_string()))?,
            Err(DictError::NotReady) => {
                decompress_raw(payload, len).map_err(|e| SqliteError::Message(e.to_string()))?
            }
            Err(e) => return Err(SqliteError::Message(e.to_string())),
        }
    } else {
        decompress_raw(payload, len).map_err(|e| SqliteError::Message(e.to_string()))?
    };

    Ok(decompressed)
}

pub fn sqlite_decompress(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    let blob = values[0].to_blob();
    let schema = values[1].to_text()?;

    let state = ExtensionState::from_context(&context)?;

    let decompressed = decompress(&state, blob, schema)?;

    context.result_blob(&decompressed);

    Ok(())
}
