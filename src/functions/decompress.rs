use libsqlite3_sys::{sqlite3_context, sqlite3_value};
use zstd::bulk::Decompressor;

use crate::{
    cache::get_decoder_in_cache,
    conn::Connection,
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

pub fn sqlite_decompress(context: *mut sqlite3_context, values: &[*mut sqlite3_value]) {
    let blob = match values.first() {
        Some(v) => sqlite_ffi::value_blob(v),
        None => return, //return Err(CodecError::DecompressionRequiresOneArgument.into()),
    };

    let schema = match values.get(1) {
        Some(v) => Some(sqlite_ffi::value_text(v).unwrap()),
        None => None,
    };

    let (header, payload) = Header::parse(blob).unwrap();

    let dict_id = DictId::from(header.dict_id.get());
    let len = header.uncompressed_len.get() as usize;

    let decompressed = if dict_id.get() != 0 {
        let conn = Connection::from_context(context).unwrap();
        let schema = schema.ok_or(CodecError::SchemaRequired).unwrap();
        let decoder = get_decoder_in_cache(&conn, schema, dict_id);

        match decoder {
            Ok(decoder) => decompress_with_decoder(payload, &decoder, len).unwrap(),
            Err(DictError::NotReady) => decompress_raw(payload, len).unwrap(),
            Err(_) => return, //Err(CodecError::DictError(e).into()),
        }
    } else {
        decompress_raw(payload, len).unwrap()
    };

    sqlite_ffi::result_blob(context, &decompressed);
}
