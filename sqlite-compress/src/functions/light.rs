use std::io::{self, Read};

use memchr::memmem::Finder;
use sqlite_ffi::{Context, SqliteError, Value};
use zstd::dict::DecoderDictionary;
use zstd::stream::read::Decoder;

use crate::{
    DictError, DictId,
    cache::get_decoder_in_cache,
    functions::{ParsedBlob, errors::CodecError, header::Header},
    state::ExtensionState,
};

const STREAM_CHUNK: usize = 16 * 1024;

/// `decompress_cont(blob, schema, needle, offset) -> 0|1`
///
/// Literal byte substring search (no LIKE wildcards / escape).
/// `offset` skips that many decompressed bytes before searching (e.g. body_position).
pub fn sqlite_decompress_cont(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    if values.len() < 4 {
        return Err(SqliteError::Message(
            "decompress_cont expects 4 arguments: blob, schema, needle, offset".into(),
        ));
    }

    let blob = values[0].to_blob();
    let schema = values[1].to_text().unwrap_or("raw");
    let needle = values[2].to_blob();
    let offset = values[3].to_i64().max(0) as usize;

    let found = match Header::parse_blob(blob).map_err(|e| SqliteError::Message(e.to_string()))? {
        ParsedBlob::Plain => contains_bytes(blob, needle, offset),
        ParsedBlob::Framed(header, payload) => {
            let dict_id = DictId::from(header.dict_id.get());

            let prepared = if dict_id.get() != 0 {
                let state = ExtensionState::from_context(&context)?;
                match get_decoder_in_cache(&state, schema, dict_id) {
                    Ok(decoder) => Some(decoder),
                    Err(DictError::NotReady) => None,
                    Err(e) => return Err(SqliteError::Message(e.to_string())),
                }
            } else {
                None
            };

            contains_stream_zstd(payload, prepared.as_deref(), needle, offset)
                .map_err(|e| SqliteError::Message(e.to_string()))?
        }
    };

    context.result_bool(found);
    Ok(())
}

/// Literal search with optional skip. Empty needle → true (like LIKE '%%').
fn contains_bytes(haystack: &[u8], needle: &[u8], offset: usize) -> bool {
    if needle.is_empty() {
        return true;
    }
    let data = match haystack.get(offset..) {
        Some(d) => d,
        None => return false,
    };
    Finder::new(needle).find(data).is_some()
}

/// Sliding-window search over a byte stream.
///
/// Keeps `needle.len() - 1` bytes of overlap so matches spanning chunks are found.
fn contains_streaming<R: Read>(mut reader: R, needle: &[u8], mut skip: usize) -> io::Result<bool> {
    if needle.is_empty() {
        return Ok(true);
    }

    let finder = Finder::new(needle);
    let overlap_len = needle.len() - 1;
    let mut overlap = Vec::with_capacity(overlap_len);
    let mut chunk = vec![0u8; STREAM_CHUNK];

    loop {
        let n = reader.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        let mut data = &chunk[..n];

        if skip > 0 {
            if skip >= data.len() {
                skip -= data.len();
                continue;
            }
            data = &data[skip..];
            skip = 0;
        }

        let mut window = std::mem::take(&mut overlap);
        window.extend_from_slice(data);

        if finder.find(&window).is_some() {
            return Ok(true);
        }

        let keep = overlap_len.min(window.len());
        overlap.clear();
        overlap.extend_from_slice(&window[window.len() - keep..]);
    }

    Ok(false)
}

/// Stream-decompress then sliding-window search.
fn contains_stream_zstd(
    payload: &[u8],
    dict: Option<&DecoderDictionary<'static>>,
    needle: &[u8],
    offset: usize,
) -> Result<bool, CodecError> {
    let decoder = match dict {
        Some(dict) => Decoder::with_prepared_dictionary(payload, dict),
        None => Decoder::with_dictionary(payload, &[]),
    }
    .map_err(CodecError::DecompressionFailed)?;

    contains_streaming(decoder, needle, offset).map_err(CodecError::DecompressionFailed)
}
