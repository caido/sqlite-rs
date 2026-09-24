use memchr::memmem::Finder;
use sqlite_ffi::{Context, SqliteBlob, SqliteError, Value};
use zstd::stream::raw::{Decoder, Operation};

use crate::{
    cache::get_decoder_in_cache, functions::header::Header, DictError, DictId, ExtensionState,
};

const CHUNK: usize = 64 * 1024;

struct Progress {
    matched: bool,
    frame_done: bool,
}

pub fn sqlite_clike(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    let schema = values[0].to_text()?;
    let table = values[1].to_text()?;
    let column = values[2].to_text()?;
    let rowid = values[3].to_i64();
    let needle = values[4].to_text()?.as_bytes();

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

    let matched = if let Some(dict) = &prepared {
        let mut decoder = Decoder::with_prepared_dictionary(dict.as_ref())
            .map_err(|e| SqliteError::Message(e.to_string()))?;
        scan(&mut decoder, &blob, &mut compressed, header_len, needle)?
    } else {
        let mut decoder = Decoder::new().map_err(|e| SqliteError::Message(e.to_string()))?;
        scan(&mut decoder, &blob, &mut compressed, header_len, needle)?
    };

    context.result_int64(if matched { 1 } else { 0 });

    Ok(())
}

fn scan(
    decoder: &mut Decoder<'_>,
    blob: &SqliteBlob,
    compressed: &mut [u8],
    header_len: usize,
    needle: &[u8],
) -> Result<bool, SqliteError> {
    let finder = Finder::new(needle);

    let mut plain = vec![0u8; CHUNK];
    let mut tail = Vec::with_capacity(needle.len() - 1);

    let total = blob.len();
    let mut pos = 0;

    while pos < total {
        let n = CHUNK.min(total - pos);
        let start = if pos == 0 {
            header_len
        } else {
            blob.read_at(pos, &mut compressed[..n])?;
            0
        };

        pos += n;

        let step = feed(
            decoder,
            &compressed[start..n],
            &mut plain,
            &mut tail,
            &finder,
        )?;

        if step.matched || step.frame_done {
            return Ok(step.matched);
        }
    }

    Ok(false)
}

fn feed(
    decoder: &mut Decoder<'_>,
    mut input: &[u8],
    plain: &mut [u8],
    tail: &mut Vec<u8>,
    finder: &Finder<'_>,
) -> Result<Progress, SqliteError> {
    loop {
        let status = decoder
            .run_on_buffers(input, plain)
            .map_err(|e| SqliteError::Message(e.to_string()))?;

        if status.bytes_written > 0 && take_match(finder, tail, &plain[..status.bytes_written]) {
            return Ok(Progress {
                matched: true,
                frame_done: status.remaining == 0,
            });
        }

        input = &input[status.bytes_read..];

        if status.remaining == 0 && status.bytes_written < plain.len() {
            return Ok(Progress {
                matched: false,
                frame_done: true,
            });
        }

        if input.is_empty() && status.bytes_written < plain.len() {
            return Ok(Progress {
                matched: false,
                frame_done: false,
            });
        }

        if status.bytes_read == 0 && status.bytes_written == 0 {
            return Err(SqliteError::Message("zstd decoder made no progress".into()));
        }
    }
}

fn take_match(finder: &Finder<'_>, tail: &mut Vec<u8>, chunk: &[u8]) -> bool {
    let k = finder.needle().len() - 1;

    if !tail.is_empty() {
        let head = &chunk[..k.min(chunk.len())];
        let base = tail.len();
        tail.extend_from_slice(head);
        if finder.find(tail).is_some() {
            return true;
        }
        tail.truncate(base);
    }

    if finder.find(chunk).is_some() {
        return true;
    }

    if chunk.len() >= k {
        tail.clear();
        tail.extend_from_slice(&chunk[chunk.len() - k..]);
    } else {
        tail.extend_from_slice(chunk);
        let excess = tail.len().saturating_sub(k);
        tail.drain(..excess);
    }
    false
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use libsqlite3_sys::{sqlite3_close, sqlite3_open, SQLITE_OK};
    use sqlite_ffi::{Connection, SqlValue};
    use zstd::stream::raw::Decoder;

    use super::*;
    use crate::{functions::header::wrap, DictId};

    fn search(plain: &[u8], needle: &[u8]) -> bool {
        let payload = zstd::stream::encode_all(plain, 1).unwrap();
        let stored = wrap(DictId::from(0), plain.len(), payload).unwrap();

        let mut db = ptr::null_mut();
        let rc = unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) };
        assert_eq!(rc, SQLITE_OK);
        let conn = Connection::from_raw(db);
        conn.batch_execute("CREATE TABLE t (id INTEGER PRIMARY KEY, data BLOB)")
            .unwrap();

        let rowid = conn
            .execute(
                "INSERT INTO t (data) VALUES (?) RETURNING id",
                &[SqlValue::Blob(stored)],
            )
            .unwrap();

        let found = {
            let blob = conn.open_blob("main", "t", "data", rowid, false).unwrap();
            let mut compressed = vec![0u8; CHUNK.min(blob.len())];
            blob.read_at(0, &mut compressed).unwrap();
            let mut decoder = Decoder::new().unwrap();
            scan(
                &mut decoder,
                &blob,
                &mut compressed,
                Header::PREFIX_SIZE,
                needle,
            )
            .unwrap()
        };
        unsafe { sqlite3_close(db) };
        found
    }

    #[test]
    fn finds_needle_in_first_block() {
        let plain = b"alpha bravo needle charlie";
        assert!(search(plain, b"needle"));
    }

    #[test]
    fn misses_absent_needle() {
        assert!(!search(b"alpha bravo charlie", b"needle"));
    }

    #[test]
    fn finds_needle_across_plain_chunks() {
        let mut plain = vec![b'a'; CHUNK + 8];
        let needle = b"xyz";
        let at = CHUNK - 1;
        plain[at..at + needle.len()].copy_from_slice(needle);
        assert!(search(&plain, needle));
    }

    #[test]
    fn finds_needle_after_first_compressed_read() {
        let mut plain = vec![0u8; CHUNK + 4096];
        for (i, byte) in plain.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }
        let needle = b"NEEDLE!!";
        let at = plain.len() - needle.len();
        plain[at..].copy_from_slice(needle);
        assert!(search(&plain, needle));
    }

    #[test]
    fn finds_needle_between_two_windows() {
        let needle = b"abcdef";
        let finder = Finder::new(needle);
        let mut tail = Vec::new();

        let mut first = vec![b'q'; 32];
        first[29..].copy_from_slice(b"abc");

        let mut second = vec![b'z'; 16];
        second[..3].copy_from_slice(b"def");

        assert!(!take_match(&finder, &mut tail, &first));
        assert!(take_match(&finder, &mut tail, &second));
    }
}
