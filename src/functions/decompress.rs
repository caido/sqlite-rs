use sqlite_loadable::{api, prelude::*, Result};
use zstd::bulk::Decompressor;

use crate::{
    cache::get_decoder,
    conn::SqliteConn,
    functions::{
        errors::CodecError::{self},
        header::Header,
    },
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

pub fn sqlite_decompress(
    context: *mut sqlite3_context,
    values: &[*mut sqlite3_value],
) -> Result<()> {
    let blob = match values.first() {
        Some(v) => api::value_blob(v),
        None => return Err(CodecError::DecompressionRequiresOneArgument.into()),
    };

    let schema = match values.get(1) {
        Some(v) => Some(api::value_text(v)?),
        None => None,
    };

    let (header, payload) = Header::parse(blob)?;

    let dict_id = DictId::from(header.dict_id.get());
    let len = header.uncompressed_len.get() as usize;

    let decompressed = if dict_id.get() != 0 {
        let schema = schema.ok_or(CodecError::SchemaRequired)?;
        let mut conn = SqliteConn::from_context(context);
        match get_decoder(&mut conn, schema, dict_id) {
            Ok(decoder) => decompress_with_decoder(payload, &decoder, len)
                .map_err(CodecError::DecompressionFailed)?,
            Err(DictError::NotReady) => decompress_raw(payload, len)?,
            Err(e) => return Err(CodecError::DictError(e).into()),
        }
    } else {
        decompress_raw(payload, len)?
    };

    api::result_blob(context, &decompressed);
    Ok(())
}
