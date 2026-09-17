use sqlite_ffi::{Context, Value};
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
) -> std::io::Result<Vec<u8>> {
    let mut decompressor = Decompressor::with_prepared_dictionary(decoder)?;
    decompressor.decompress(data, uncompressed_len)
}

pub(crate) fn decompress_raw(
    payload: &[u8],
    uncompressed_len: usize,
) -> std::result::Result<Vec<u8>, CodecError> {
    zstd::bulk::decompress(payload, uncompressed_len).map_err(CodecError::DecompressionFailed)
}

pub fn sqlite_decompress(context: Context, values: &[Value]) {
    let blob = values[0].to_blob();

    let schema = values
        .get(1)
        .map(|v| v.to_text().expect("schema is not a text"));

    let (header, payload) = Header::parse(blob).expect("invalid header");

    let dict_id = DictId::from(header.dict_id.get());
    let len = header.uncompressed_len.get() as usize;

    let decompressed = if dict_id.get() != 0 {
        let state = ExtensionState::from_context(&context).expect("missing connection state");

        let schema = schema
            .ok_or(CodecError::SchemaRequired)
            .expect("schema is required");
        let decoder = get_decoder_in_cache(state, schema, dict_id);

        match decoder {
            Ok(decoder) => {
                decompress_with_decoder(payload, &decoder, len).expect("decompression failed")
            }
            Err(DictError::NotReady) => decompress_raw(payload, len).expect("decompression failed"),
            Err(e) => {
                context.result_error(&e.to_string());
                return;
            }
        }
    } else {
        decompress_raw(payload, len).expect("decompression failed")
    };

    context.result_blob(&decompressed);
}
