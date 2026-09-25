//! `clike(schema, table, column, rowid, pattern)`: `LIKE` on a compressed
//! value, without materializing the plaintext.
//!
//! The blob is read through SQLite incremental I/O and decompressed in fixed
//! chunks (`scan`). Each plaintext chunk goes to a `LikeMatcher`
//! (`matcher`), which reassembles UTF-8 characters (`utf8`) and runs the
//! compiled pattern (`pattern`) as an NFA (`nfa`). Decompression stops as
//! soon as the matcher knows the answer.

mod matcher;
mod nfa;
mod pattern;
mod scan;
mod utf8;

use matcher::{LikeMatcher, Verdict};
use scan::{scan, CHUNK};
use sqlite_ffi::{Context, SqliteError, Value};
use zstd::stream::raw::Decoder;

use crate::{
    cache::get_decoder_in_cache, functions::header::Header, DictError, DictId, ExtensionState,
};

pub fn sqlite_clike(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    let schema = values[0].to_text()?;
    let table = values[1].to_text()?;
    let column = values[2].to_text()?;
    let rowid = values[3].to_i64();
    let mut matcher = LikeMatcher::new(values[4].to_text()?.as_bytes());

    if matcher.verdict() == Verdict::Match {
        context.result_int64(1);
        return Ok(());
    }

    let state = ExtensionState::from_context(&context)?;
    let blob = state
        .connection
        .open_blob(schema, table, column, rowid, false)?;

    let total = blob.len();
    if total < Header::PREFIX_SIZE {
        return Err(SqliteError::Message(
            "compressed blob is shorter than the header".into(),
        ));
    }

    let mut compressed = vec![0u8; CHUNK.min(total)];
    blob.read_at(0, &mut compressed)?;

    let (header, _) =
        Header::parse(&compressed).map_err(|e| SqliteError::Message(e.to_string()))?;
    let dict_id = DictId::from(header.dict_id.get());
    let header_len = header.header_len as usize;

    let prepared = if dict_id.get() != 0 {
        match get_decoder_in_cache(&state, schema, dict_id) {
            Ok(dict) => Some(dict),
            Err(DictError::NotReady) => None,
            Err(e) => return Err(SqliteError::Message(e.to_string())),
        }
    } else {
        None
    };

    let mut decoder = if let Some(dict) = &prepared {
        Decoder::with_prepared_dictionary(dict.as_ref())
    } else {
        Decoder::new()
    }
    .map_err(|e| SqliteError::Message(e.to_string()))?;

    let matched = scan(
        &mut decoder,
        &blob,
        &mut compressed,
        header_len,
        &mut matcher,
    )?;

    context.result_int64(i64::from(matched));

    Ok(())
}

#[cfg(test)]
mod tests {

    use std::ptr;

    use libsqlite3_sys::{sqlite3_close, sqlite3_open, SQLITE_OK};

    use crate::Connection;

    #[test]
    fn lone_percent_matches_without_opening_the_blob() {
        let mut db = ptr::null_mut();
        let rc = unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) };
        assert_eq!(rc, SQLITE_OK);

        let rc = unsafe { crate::sqlite3_compress_init(db, ptr::null_mut(), ptr::null_mut()) };
        assert_eq!(rc, SQLITE_OK);

        let conn = Connection::from_raw(db);
        let rows = conn
            .query("SELECT clike('main', 't', 'data', 1, '%')", &[])
            .unwrap();
        assert_eq!(rows[0][0].as_i64(), Some(1));

        unsafe { sqlite3_close(db) };
    }
}
