use std::sync::Arc;

use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{
    Cache, ColumnKey, DictError, DictId,
    cache::pool,
    dict::{DictKey, get_raw_dict, read_current_id},
    functions::Level,
    state::ExtensionState,
};

/// Return the prepared zstd encoder for `(schema, dict_id)`, loading it from `__compress_dicts` and caching it on a miss.
pub(crate) fn get_encoder_in_cache(
    state: &ExtensionState,
    schema: &str,
    dict_id: DictId,
    level: Level,
) -> Result<Arc<EncoderDictionary<'static>>, DictError> {
    let key = DictKey::new(schema, dict_id);

    pool::get_encoder(&key, level, || get_raw_dict(&key, state))
}

/// Return the prepared zstd decoder for `(schema, dict_id)`, loading it from `__compress_dicts` and caching it on a miss.
pub(crate) fn get_decoder_in_cache(
    state: &ExtensionState,
    schema: &str,
    dict_id: DictId,
) -> Result<Arc<DecoderDictionary<'static>>, DictError> {
    let key = DictKey::new(schema, dict_id);

    pool::get_decoder(&key, || get_raw_dict(&key, state))
}

/// Load this column's current dict id, encoder, and decoder. Call it as early as possible so later `compress` and `decompress` hit the cache.
pub(crate) fn warm_cache(
    state: &ExtensionState,
    column: &ColumnKey,
    level: Level,
) -> Result<(), DictError> {
    let dict_id = match read_current_id(state, column)? {
        Some(id) => id,
        None => {
            state.cache.lock().clear_current_id(column);
            return Ok(());
        }
    };

    state.cache.lock().set_current_id(column.clone(), dict_id);

    get_encoder_in_cache(state, column.schema(), dict_id, level)?;
    get_decoder_in_cache(state, column.schema(), dict_id)?;

    Ok(())
}

pub(crate) fn insert_into_caches(
    cache: &Cache,
    column: &ColumnKey,
    dict_id: DictId,
    dictionary: &[u8],
    level: Level,
) {
    cache.lock().set_current_id(column.clone(), dict_id);
    pool::insert_prepared(DictKey::new(column.schema(), dict_id), dictionary, level);
}

#[cfg(test)]
mod tests {
    use std::{ptr, sync::Arc};

    use libsqlite3_sys::{SQLITE_OK, sqlite3, sqlite3_auto_extension, sqlite3_close, sqlite3_open};
    use parking_lot::Once;
    use sqlite_ffi::{SqlValue, SqliteError};
    use zstd::{
        bulk::{Compressor, Decompressor},
        dict::{DecoderDictionary, EncoderDictionary},
    };

    use super::{get_decoder_in_cache, get_encoder_in_cache};
    use crate::{
        DEFAULT_LEVEL, ExtensionState, SetupConnection, dict::DictId, sqlite3_compress_init,
    };

    const SCHEMA: &str = "main";

    pub struct TestDb {
        db: *mut sqlite3,
    }

    impl TestDb {
        pub fn open() -> Self {
            static REGISTER: Once = Once::new();
            REGISTER.call_once(|| unsafe {
                #[allow(clippy::missing_transmute_annotations)]
                sqlite3_auto_extension(Some(std::mem::transmute(
                    sqlite3_compress_init as *const (),
                )));
            });

            let mut db = ptr::null_mut();
            let rc = unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) };
            assert_eq!(rc, SQLITE_OK);

            Self { db }
        }

        pub fn state(&self) -> ExtensionState {
            ExtensionState::from_db(self.db).unwrap()
        }
    }

    impl Drop for TestDb {
        fn drop(&mut self) {
            unsafe { sqlite3_close(self.db) };
        }
    }

    impl SetupConnection for TestDb {
        unsafe fn sqlite_handle(&self) -> *mut sqlite3 {
            self.db
        }
    }

    fn ensure_dicts_table(state: &ExtensionState) -> Result<(), SqliteError> {
        state.connection.batch_execute(
            r#"
            CREATE TABLE IF NOT EXISTS __compress_dicts (
                id INTEGER PRIMARY KEY,
                dict BLOB NOT NULL,
                trained_at INTEGER NOT NULL,
                table_name TEXT NOT NULL,
                column_name TEXT NOT NULL,
                row_count INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )?;

        Ok(())
    }

    fn insert_trained_dict(
        state: &ExtensionState,
        id: DictId,
        table_name: &str,
        column_name: &str,
    ) -> Result<(), SqliteError> {
        ensure_dicts_table(state)?;

        let samples: Vec<Vec<u8>> = (0..32)
            .map(|i| {
                format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n").into_bytes()
            })
            .collect();
        let sample_refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
        let dict = zstd::dict::from_samples(&sample_refs, 1024).unwrap();

        state
            .connection
            .execute(
                "INSERT OR REPLACE INTO __compress_dicts \
         (id, dict, trained_at, table_name, column_name, row_count) \
         VALUES (?1, ?2, strftime('%s','now'), ?3, ?4, 32) \
         RETURNING id",
                &[
                    SqlValue::Integer(id.get() as i64),
                    SqlValue::Blob(dict),
                    SqlValue::Text(table_name.to_string()),
                    SqlValue::Text(column_name.to_string()),
                ],
            )
            .unwrap();

        Ok(())
    }

    fn expect_encoder(
        r: Result<Arc<EncoderDictionary<'static>>, crate::DictError>,
    ) -> Arc<EncoderDictionary<'static>> {
        r.expect("encoder")
    }

    fn expect_decoder(
        r: Result<Arc<DecoderDictionary<'static>>, crate::DictError>,
    ) -> Arc<DecoderDictionary<'static>> {
        r.expect("decoder")
    }

    #[test]
    fn insert_dict_encode_decode_equals_input() {
        let db = TestDb::open();
        let state = db.state();

        let id = DictId::new(2_001);
        insert_trained_dict(&state, id, "requests_raw", "data").unwrap();

        let encoder = expect_encoder(get_encoder_in_cache(&state, SCHEMA, id, DEFAULT_LEVEL));
        let decoder = expect_decoder(get_decoder_in_cache(&state, SCHEMA, id));

        let original = b"GET /api/users/42 HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let mut compressor = Compressor::with_prepared_dictionary(&encoder).unwrap();
        let compressed = compressor.compress(original).unwrap();
        assert_ne!(compressed.as_slice(), original);

        let mut decompressor = Decompressor::with_prepared_dictionary(&decoder).unwrap();
        let decompressed = decompressor
            .decompress(&compressed, original.len())
            .unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn roundtrip_multiple_payloads_with_same_dict() {
        let db = TestDb::open();
        let state = db.state();

        let id = DictId::new(2_002);
        insert_trained_dict(&state, id, "requests_raw", "data").unwrap();

        let encoder = expect_encoder(get_encoder_in_cache(&state, SCHEMA, id, DEFAULT_LEVEL));
        let decoder = expect_decoder(get_decoder_in_cache(&state, SCHEMA, id));

        let payloads: [&[u8]; 3] = [
            b"GET /api/users/1 HTTP/1.1\r\nHost: example.com\r\n\r\n",
            b"GET /api/users/2 HTTP/1.1\r\nHost: example.com\r\n\r\n",
            b"POST /api/users HTTP/1.1\r\nHost: example.com\r\n\r\n{\"ok\":true}",
        ];

        for original in payloads {
            let mut compressor = Compressor::with_prepared_dictionary(&encoder).unwrap();
            let compressed = compressor.compress(original).unwrap();
            let mut decompressor = Decompressor::with_prepared_dictionary(&decoder).unwrap();
            let decompressed = decompressor
                .decompress(&compressed, original.len().max(compressed.len()) * 4)
                .unwrap();
            assert_eq!(decompressed, original);
        }
    }

    #[test]
    fn get_decoder_not_ready_when_table_empty() {
        let db = TestDb::open();
        let state = db.state();

        ensure_dicts_table(&state).unwrap();

        assert!(matches!(
            get_decoder_in_cache(&state, SCHEMA, DictId::new(90_002)),
            Err(crate::DictError::NotReady)
        ));
    }

    #[test]
    fn get_encoder_not_ready_when_table_empty() {
        let db = TestDb::open();
        let state = db.state();

        ensure_dicts_table(&state).unwrap();

        assert!(matches!(
            get_encoder_in_cache(&state, SCHEMA, DictId::new(90_001), DEFAULT_LEVEL),
            Err(crate::DictError::NotReady)
        ));
    }

    #[test]
    fn get_encoder_loads_and_caches() {
        let db = TestDb::open();
        let state = db.state();

        let id = DictId::new(1_001);
        insert_trained_dict(&state, id, "requests_raw", "data").unwrap();

        let first = expect_encoder(get_encoder_in_cache(&state, SCHEMA, id, DEFAULT_LEVEL));
        let second = expect_encoder(get_encoder_in_cache(&state, SCHEMA, id, DEFAULT_LEVEL));
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn get_decoder_loads_and_caches() {
        let db = TestDb::open();
        let state = db.state();

        let id = DictId::new(1_002);
        insert_trained_dict(&state, id, "requests_raw", "data").unwrap();

        let first = expect_decoder(get_decoder_in_cache(&state, SCHEMA, id));
        let second = expect_decoder(get_decoder_in_cache(&state, SCHEMA, id));

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn get_encoder_and_decoder_roundtrip() {
        let db = TestDb::open();
        let state = db.state();

        let id = DictId::new(1_003);
        insert_trained_dict(&state, id, "requests_raw", "data").unwrap();

        let encoder = expect_encoder(get_encoder_in_cache(&state, SCHEMA, id, DEFAULT_LEVEL));
        let decoder = expect_decoder(get_decoder_in_cache(&state, SCHEMA, id));

        let original = b"GET /api/users/42 HTTP/1.1\r\nHost: example.com\r\n\r\n";
        let mut compressor = Compressor::with_prepared_dictionary(&encoder).unwrap();
        let compressed = compressor.compress(original).unwrap();

        let mut decompressor = Decompressor::with_prepared_dictionary(&decoder).unwrap();
        let decompressed = decompressor
            .decompress(&compressed, original.len())
            .unwrap();

        assert_eq!(decompressed, original);
    }

    fn seed_dict_into(
        state: &ExtensionState,
        schema: &str,
        id: DictId,
        table_name: &str,
        column_name: &str,
    ) {
        let samples: Vec<Vec<u8>> = (0..32)
            .map(|i| {
                format!(
                    "{schema} sample {i} GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n"
                )
                .into_bytes()
            })
            .collect();
        let sample_refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
        let dict = zstd::dict::from_samples(&sample_refs, 1024).unwrap();

        state
            .connection
            .execute(
                &format!(
                    "INSERT OR REPLACE INTO \"{schema}\".\"__compress_dicts\" \
         (id, dict, trained_at, table_name, column_name, row_count) \
         VALUES (?1, ?2, strftime('%s','now'), ?3, ?4, 32) \
         RETURNING id"
                ),
                &[
                    SqlValue::Integer(id.get() as i64),
                    SqlValue::Blob(dict),
                    SqlValue::Text(table_name.to_string()),
                    SqlValue::Text(column_name.to_string()),
                ],
            )
            .unwrap();
    }

    #[test]
    fn get_encoder_and_decoder_isolate_same_id_across_schemas() {
        let db = TestDb::open();
        let state = db.state();

        state
            .connection
            .batch_execute(
                r#"
        ATTACH DATABASE ':memory:' AS raw;
        ATTACH DATABASE ':memory:' AS archive;

        CREATE TABLE raw.__compress_dicts (
            id INTEGER PRIMARY KEY,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_count INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE archive.__compress_dicts (
            id INTEGER PRIMARY KEY,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_count INTEGER NOT NULL DEFAULT 0
        );
        "#,
            )
            .unwrap();

        let id = DictId::new(1);
        let archive_only = DictId::new(2);

        seed_dict_into(&state, "raw", id, "requests_raw", "data");
        seed_dict_into(&state, "archive", id, "requests_raw", "data");
        seed_dict_into(&state, "archive", archive_only, "requests_raw", "data");

        let raw_enc = expect_encoder(get_encoder_in_cache(&state, "raw", id, DEFAULT_LEVEL));
        let archive_enc =
            expect_encoder(get_encoder_in_cache(&state, "archive", id, DEFAULT_LEVEL));
        assert!(!Arc::ptr_eq(&raw_enc, &archive_enc));

        let raw_enc_again = expect_encoder(get_encoder_in_cache(&state, "raw", id, DEFAULT_LEVEL));
        assert!(Arc::ptr_eq(&raw_enc, &raw_enc_again));

        expect_decoder(get_decoder_in_cache(&state, "archive", archive_only));
        assert!(matches!(
            get_encoder_in_cache(&state, "raw", archive_only, DEFAULT_LEVEL),
            Err(crate::DictError::NotReady)
        ));
        assert!(matches!(
            get_decoder_in_cache(&state, "raw", archive_only),
            Err(crate::DictError::NotReady)
        ));
    }
}
