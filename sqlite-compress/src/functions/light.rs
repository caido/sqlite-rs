use memchr::memmem::Finder;
use sqlite_ffi::SqliteBlob;
use sqlite_ffi::{Context, SqliteError, Value};
use std::io::{self, BufReader, Read};
use zstd::dict::DecoderDictionary;
use zstd::stream::read::Decoder;

use crate::{
    DictError, DictId,
    cache::get_decoder_in_cache,
    functions::{ParsedBlob, errors::CodecError, header::Header},
    state::ExtensionState,
};

const STREAM_CHUNK: usize = 16 * 1024;

/// `decompress_cont(schema, table, column, rowid, needle, offset) -> 0|1`
pub fn sqlite_decompress_cont(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    if values.len() < 6 {
        return Err(SqliteError::Message(
            "decompress_cont expects 6 arguments: schema, table, column, rowid, needle, offset"
                .into(),
        ));
    }

    let schema = values[0].to_text()?;
    let table = values[1].to_text()?;
    let column = values[2].to_text()?;
    let rowid = values[3].to_i64();
    let needle = values[4].to_blob();
    let offset = values[5].to_i64().max(0) as usize;

    let state = ExtensionState::from_context(&context)?;
    let blob = state
        .connection
        .open_blob(schema, table, column, rowid, false)?;

    let total = blob.len();
    if total == 0 {
        context.result_bool(contains_bytes(&[], needle, offset));
        return Ok(());
    }

    // Peek just enough to parse the header (no full materialization).
    let peek_len = Header::PREFIX_SIZE.min(total);
    let mut peek = vec![0u8; peek_len];
    blob.read_at(0, &mut peek)?;

    let found = match Header::parse_blob(&peek) {
        Ok(ParsedBlob::Plain) | Err(_) if peek_len < Header::PREFIX_SIZE => {
            contains_streaming(BlobReader::new(&blob, 0), needle, offset)
                .map_err(|e| SqliteError::Message(e.to_string()))?
        }
        Ok(ParsedBlob::Plain) => contains_streaming(BlobReader::new(&blob, 0), needle, offset)
            .map_err(|e| SqliteError::Message(e.to_string()))?,
        Ok(ParsedBlob::Framed(header, _)) => {
            let header_len = header.header_len as usize;
            if total < header_len {
                return Err(SqliteError::Message(
                    "compressed blob is shorter than the header".into(),
                ));
            }

            if header_len > peek.len() {
                peek.resize(header_len, 0);
                blob.read_at(0, &mut peek)?;
            }

            let dict_id = DictId::from(header.dict_id.get());
            let prepared = if dict_id.get() != 0 {
                match get_decoder_in_cache(&state, schema, dict_id) {
                    Ok(d) => Some(d),
                    Err(DictError::NotReady) => None,
                    Err(e) => return Err(SqliteError::Message(e.to_string())),
                }
            } else {
                None
            };

            let reader = BufReader::new(BlobReader::new(&blob, header_len));
            let decoder = match prepared.as_deref() {
                Some(dict) => Decoder::with_prepared_dictionary(reader, dict),
                None => Decoder::with_dictionary(reader, &[]),
            }
            .map_err(|e| SqliteError::Message(e.to_string()))?;

            contains_streaming(decoder, needle, offset)
                .map_err(|e| SqliteError::Message(e.to_string()))?
        }
        Err(e) => return Err(SqliteError::Message(e.to_string())),
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

struct BlobReader<'a> {
    blob: &'a SqliteBlob<'a>,
    pos: usize,
    end: usize,
}

impl<'a> BlobReader<'a> {
    fn new(blob: &'a SqliteBlob<'a>, start: usize) -> Self {
        let end = blob.len();
        Self {
            blob,
            pos: start.min(end),
            end,
        }
    }

    fn from_range(blob: &'a SqliteBlob<'a>, start: usize, end: usize) -> Self {
        let end = end.min(blob.len());
        Self {
            blob,
            pos: start.min(end),
            end,
        }
    }
}

impl Read for BlobReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.end.saturating_sub(self.pos);
        if remaining == 0 || buf.is_empty() {
            return Ok(0);
        }
        let n = buf.len().min(remaining);
        self.blob
            .read_at(self.pos, &mut buf[..n])
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        self.pos += n;
        Ok(n)
    }
}
