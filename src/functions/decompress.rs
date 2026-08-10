use sqlite_loadable::{api, prelude::*, Result};
use zstd::bulk::Decompressor;

use crate::{
    conn::SqliteConn,
    dict::{get_decoder, get_decoder_cached},
    functions::{
        errors::CodecError::{self},
        header::Header,
    },
    DictError,
};

fn decompress_with_decoder(
    data: &[u8],
    decoder: &zstd::dict::DecoderDictionary<'static>,
    uncompressed_len: usize,
) -> std::io::Result<Vec<u8>> {
    let mut decompressor = Decompressor::with_prepared_dictionary(decoder)?;
    decompressor.decompress(data, uncompressed_len)
}

fn decompress_raw(
    payload: &[u8],
    uncompressed_len: usize,
) -> std::result::Result<Vec<u8>, CodecError> {
    zstd::bulk::decompress(payload, uncompressed_len).map_err(CodecError::DecompressionFailed)
}

pub fn decompress(blob: &[u8]) -> std::result::Result<Vec<u8>, CodecError> {
    let (header, payload) = Header::parse(blob)?;
    if header.dict_id.get() != 0 {
        if let Some(decoder) = get_decoder_cached(header.dict_id) {
            return decompress_with_decoder(payload, &decoder, header.uncompressed_len as usize)
                .map_err(CodecError::DecompressionFailed);
        }
    }
    decompress_raw(payload, header.uncompressed_len as usize)
}

pub fn sqlite_decompress(
    context: *mut sqlite3_context,
    values: &[*mut sqlite3_value],
) -> Result<()> {
    let value = values
        .first()
        .ok_or(CodecError::DecompressionRequiresOneArgument)?;

    let blob = api::value_blob(value);
    let (header, payload) = Header::parse(blob)?;
    let mut conn = SqliteConn::from_context(context);
    let decompressed = if header.dict_id.get() != 0 {
        match get_decoder(header.dict_id, &mut conn) {
            Ok(decoder) => {
                decompress_with_decoder(payload, &decoder, header.uncompressed_len as usize)
                    .map_err(CodecError::DecompressionFailed)?
            }
            Err(DictError::NotReady) => decompress_raw(payload, header.uncompressed_len as usize)?,
            Err(e) => return Err(CodecError::DictError(e).into()),
        }
    } else {
        decompress_raw(payload, header.uncompressed_len as usize)?
    };

    api::result_blob(context, &decompressed);
    Ok(())
}
