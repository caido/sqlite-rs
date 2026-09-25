//! Drives zstd over the blob and feeds each plaintext chunk to the matcher.
//! Neither the compressed value nor the plaintext is held in full.

use sqlite_ffi::{SqliteBlob, SqliteError};
use zstd::stream::raw::{Decoder, Operation};

use super::matcher::{LikeMatcher, Verdict};

pub(super) const CHUNK: usize = 64 * 1024;

enum Step {
    Done(bool),
    NeedInput,
}

pub(super) fn scan(
    decoder: &mut Decoder<'_>,
    blob: &SqliteBlob,
    compressed: &mut [u8],
    header_len: usize,
    matcher: &mut LikeMatcher,
) -> Result<bool, SqliteError> {
    let mut plain = vec![0u8; CHUNK];
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
        if let Step::Done(matched) = feed(decoder, &compressed[start..n], &mut plain, matcher)? {
            return Ok(matched);
        }
    }
    Ok(false)
}

fn feed(
    decoder: &mut Decoder<'_>,
    mut input: &[u8],
    plain: &mut [u8],
    matcher: &mut LikeMatcher,
) -> Result<Step, SqliteError> {
    loop {
        let status = decoder
            .run_on_buffers(input, plain)
            .map_err(|e| SqliteError::Message(e.to_string()))?;

        match matcher.push(&plain[..status.bytes_written]) {
            Verdict::Match => return Ok(Step::Done(true)),
            Verdict::NoMatch => return Ok(Step::Done(false)),
            Verdict::NeedMore => {}
        }

        input = &input[status.bytes_read..];

        if status.remaining == 0 {
            return Ok(Step::Done(matcher.finish()));
        }

        if input.is_empty() && status.bytes_written < plain.len() {
            return Ok(Step::NeedInput);
        }

        if status.bytes_read == 0 && status.bytes_written == 0 {
            return Err(SqliteError::Message("compressed frame is truncated".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use libsqlite3_sys::{sqlite3_close, sqlite3_open, SQLITE_OK};
    use sqlite_ffi::{Connection, SqlValue};
    use zstd::stream::raw::Decoder;

    use super::*;
    use crate::{functions::header::wrap, DictId, Header};

    fn search(plain: &[u8], pattern: &str) -> bool {
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
            let mut matcher = LikeMatcher::new(pattern.as_bytes());

            scan(
                &mut decoder,
                &blob,
                &mut compressed,
                Header::PREFIX_SIZE,
                &mut matcher,
            )
            .unwrap()
        };
        unsafe { sqlite3_close(db) };
        found
    }

    const TEXT: &[u8] = b"alpha bravo needle charlie";

    #[test]
    fn literal_pattern_is_whole_value_equality() {
        assert!(search(TEXT, "alpha bravo needle charlie"));
        assert!(!search(TEXT, "needle"));
        assert!(!search(TEXT, "alpha bravo needle charli"));
    }

    #[test]
    fn percent_anchors_prefix_suffix_and_contains() {
        assert!(search(TEXT, "alpha%"));
        assert!(search(TEXT, "%charlie"));
        assert!(search(TEXT, "%needle%"));
        assert!(!search(TEXT, "%needle"));
        assert!(!search(TEXT, "needle%"));
        assert!(!search(TEXT, "%absent%"));
    }

    #[test]
    fn underscore_matches_exactly_one_character() {
        assert!(search(TEXT, "alph_ bravo%"));
        assert!(search(TEXT, "%n__dle%"));
        assert!(!search(TEXT, "%n_dle%"));
        assert!(search("aéc".as_bytes(), "a_c"));
        assert!(!search("aéc".as_bytes(), "a__c"));
    }

    #[test]
    fn percent_backtracks_to_a_later_occurrence() {
        assert!(search(b"aaab", "%a%ab"));
        assert!(search(b"axxbyyc", "a%b%c"));
        assert!(!search(b"axxbyy", "a%b%c"));
    }

    #[test]
    fn empty_pattern_only_matches_empty_value() {
        assert!(!search(TEXT, ""));
    }

    #[test]
    fn finds_literal_across_plain_chunks() {
        let mut plain = vec![b'a'; CHUNK + 8];
        let at = CHUNK - 1;
        plain[at..at + 3].copy_from_slice(b"xyz");
        assert!(search(&plain, "%xyz%"));
    }

    #[test]
    fn underscore_matches_char_split_across_plain_chunks() {
        let mut plain = vec![b'z'; CHUNK + 8];
        plain[CHUNK - 2] = b'a';
        plain[CHUNK - 1..CHUNK + 1].copy_from_slice("é".as_bytes());
        plain[CHUNK + 1] = b'b';
        assert!(search(&plain, "%a_b%"));
        assert!(!search(&plain, "%a__b%"));
    }

    #[test]
    fn finds_suffix_after_first_compressed_read() {
        let mut plain = vec![0u8; CHUNK + 4096];
        for (i, byte) in plain.iter_mut().enumerate() {
            *byte = (i % 251) as u8;
        }
        let needle = b"NEEDLE!!";
        let at = plain.len() - needle.len();
        plain[at..].copy_from_slice(needle);
        assert!(search(&plain, "%NEEDLE!!"));
        assert!(!search(&plain, "%NEEDLE!"));
    }

    #[test]
    fn finds_suffix_when_frame_ends_on_chunk_boundary() {
        let mut plain = vec![b'a'; 2 * CHUNK];
        let at = plain.len() - 3;
        plain[at..].copy_from_slice(b"xyz");
        assert!(search(&plain, "%xyz"));
    }
}
