use sqlite_loadable::prelude::*;
use sqlite_loadable::{api, Result};

use crate::conn::SqliteConn;
use crate::dict::{get_decoder, get_decoder_cached};
use crate::functions::errors::CodecError::{self};
use crate::DictError;
use zstd::bulk::Decompressor;

fn decompress_with_decoder(
    data: &[u8],
    decoder: &zstd::dict::DecoderDictionary<'static>,
) -> std::io::Result<Vec<u8>> {
    let mut decompressor = Decompressor::with_prepared_dictionary(decoder)?;
    decompressor.decompress(data, data.len().saturating_mul(4).max(1024))
}

fn decompress_raw(payload: &[u8]) -> std::result::Result<Vec<u8>, CodecError> {
    zstd::stream::decode_all(payload).map_err(CodecError::DecompressionFailed)
}

fn split_blob(blob: &[u8]) -> std::result::Result<(u32, &[u8]), CodecError> {
    let header: [u8; 4] = blob
        .get(..4)
        .and_then(|h| h.try_into().ok())
        .ok_or(CodecError::MalformedHeader)?;
    Ok((u32::from_le_bytes(header), &blob[4..]))
}

pub fn decompress(blob: &[u8]) -> std::result::Result<Vec<u8>, CodecError> {
    let (dict_id, payload) = split_blob(blob)?;
    if dict_id != 0 {
        if let Some(decoder) = get_decoder_cached(dict_id) {
            return decompress_with_decoder(payload, &decoder)
                .map_err(CodecError::DecompressionFailed);
        }
    }
    decompress_raw(payload)
}

pub fn sqlite_decompress(
    context: *mut sqlite3_context,
    values: &[*mut sqlite3_value],
) -> Result<()> {
    let value = values
        .first()
        .ok_or(CodecError::DecompressionRequiresOneArgument)?;

    let blob = api::value_blob(value);
    let (dict_id, payload) = split_blob(blob)?;
    let mut conn = SqliteConn::from_context(context);
    let decompressed = if dict_id != 0 {
        match get_decoder(dict_id, &mut conn) {
            Ok(decoder) => decompress_with_decoder(payload, &decoder)
                .map_err(CodecError::DecompressionFailed)?,
            Err(DictError::NotReady) => decompress_raw(payload)?,
            Err(e) => return Err(CodecError::DictError(e).into()),
        }
    } else {
        decompress_raw(payload)?
    };

    api::result_blob(context, &decompressed);
    Ok(())
}
