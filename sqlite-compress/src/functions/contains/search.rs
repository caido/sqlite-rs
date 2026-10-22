use sqlite_ffi::SqliteBlob;
use sqlite_ffi::SqliteError;
use std::io::{self, BufReader};
use zstd::stream::read::Decoder;

use crate::{
    DictError, DictId,
    cache::get_decoder_in_cache,
    functions::contains::utils::to_sqlite,
    functions::{ParsedBlob, contains::matcher::Matcher, header::Header},
    state::ExtensionState,
};

pub(crate) fn search_blob(
    state: &ExtensionState,
    schema: &str,
    blob: &SqliteBlob<'_>,
    matcher: &Matcher,
    offset: usize,
) -> Result<bool, SqliteError> {
    let total = blob.len();
    if total == 0 {
        return matcher.find_in(io::empty(), offset).map_err(to_sqlite);
    }

    // Peek just enough to parse the header (no full materialization).
    let peek_len = Header::PREFIX_SIZE.min(total);
    let mut peek = vec![0u8; peek_len];
    blob.read_at(0, &mut peek)?;

    match Header::parse_blob(&peek) {
        Ok(ParsedBlob::Plain) | Err(_) if peek_len < Header::PREFIX_SIZE => {
            matcher.find_in(blob.reader(0), offset).map_err(to_sqlite)
        }
        Ok(ParsedBlob::Plain) => matcher.find_in(blob.reader(0), offset).map_err(to_sqlite),
        Ok(ParsedBlob::Framed(header, _)) => {
            let header_len = header.header_len as usize;

            let dict_id = DictId::from(header.dict_id.get());
            let prepared = if dict_id.get() != 0 {
                match get_decoder_in_cache(state, schema, dict_id) {
                    Ok(d) => Some(d),
                    Err(DictError::NotReady) => None,
                    Err(e) => return Err(SqliteError::Message(e.to_string())),
                }
            } else {
                None
            };

            let reader = BufReader::with_capacity(64 * 1024, blob.reader(header_len));
            let decoder = match prepared.as_deref() {
                Some(dict) => Decoder::with_prepared_dictionary(reader, dict),
                None => Decoder::with_dictionary(reader, &[]),
            }
            .map_err(to_sqlite)?;

            matcher.find_in(decoder, offset).map_err(to_sqlite)
        }
        Err(e) => Err(SqliteError::Message(e.to_string())),
    }
}
