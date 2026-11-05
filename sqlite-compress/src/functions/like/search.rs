use std::io::{self, BufReader};

use matcher::Matcher;
use sqlite_ffi::{SqliteBlob, SqliteError};
use zstd::{stream::read::Decoder, zstd_safe};

use crate::{
    CodecError, DictError, DictId,
    cache::get_decoder_in_cache,
    functions::{
        ParsedBlob,
        header::{CodecKind, Header},
    },
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
            matcher.find_in(blob.reader(offset), 0).map_err(to_sqlite)
        }
        Ok(ParsedBlob::Plain) => matcher.find_in(blob.reader(offset), 0).map_err(to_sqlite),
        Ok(ParsedBlob::Framed(header, _)) => {
            let header_len = header.header_len as usize;

            let codec = CodecKind::try_from(header.codec).map_err(to_sqlite)?;

            if codec == CodecKind::Plain {
                let expected = header.uncompressed_len.get() as usize;
                let actual = total.saturating_sub(header_len);

                if actual != expected {
                    return Err(to_sqlite(CodecError::PlainPayloadLengthMismatch {
                        expected,
                        actual,
                    }));
                }

                return matcher
                    .find_in(blob.reader(header_len.saturating_add(offset)), 0)
                    .map_err(to_sqlite);
            }

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

            let reader =
                BufReader::with_capacity(zstd_safe::DCtx::in_size(), blob.reader(header_len));
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

pub(crate) fn to_sqlite(e: impl std::fmt::Display) -> SqliteError {
    SqliteError::Message(e.to_string())
}
