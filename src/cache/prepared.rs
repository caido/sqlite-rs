use std::sync::Arc;

use zstd::dict::{DecoderDictionary, EncoderDictionary};

use crate::{
    cache::registry::{with_conn, CacheKeySource, REGISTRY},
    dict::{load_raw_dict, read_current_id, DictKey},
    functions::Level,
    ColumnKey, DictError, DictId, DictStore, SetupConnection,
};

/// Returns the prepared encoder for `dict_id`, loading its bytes on a cache miss.
///
/// The cache is checked again after the SQLite read because another caller may
/// have populated the connection-scoped cache during that read.
pub(crate) fn get_encoder<C>(
    conn: &mut C,
    schema: &str,
    dict_id: DictId,
    level: Level,
) -> Result<Arc<EncoderDictionary<'static>>, DictError>
where
    C: DictStore + CacheKeySource,
{
    let key = DictKey::new(schema, dict_id);
    let db_key = conn.db_key();

    if let Some(dict) = REGISTRY.lock().peek_encoder(db_key, &key) {
        return Ok(dict);
    }

    let raw = load_raw_dict(&key, conn)?;
    let encoder = Arc::new(EncoderDictionary::copy(&raw, level.get()));
    let mut registry = REGISTRY.lock();
    let cache = registry.cache_for(db_key);
    let mut encoders = cache.encoders.lock();

    if let Some(dict) = encoders.get(&key) {
        return Ok(dict);
    }

    encoders.insert(key, encoder.clone());
    Ok(encoder)
}

/// Returns the prepared decoder for `dict_id`, loading its bytes on a cache miss.
///
/// See [`get_encoder`] for the second cache check performed after loading.
pub(crate) fn get_decoder<C>(
    conn: &mut C,
    schema: &str,
    dict_id: DictId,
) -> Result<Arc<DecoderDictionary<'static>>, DictError>
where
    C: DictStore + CacheKeySource,
{
    let key = DictKey::new(schema, dict_id);
    let db_key = conn.db_key();

    if let Some(dict) = REGISTRY.lock().peek_decoder(db_key, &key) {
        return Ok(dict);
    }

    let raw = load_raw_dict(&key, conn)?;
    let decoder = Arc::new(DecoderDictionary::copy(&raw));
    let mut registry = REGISTRY.lock();
    let cache = registry.cache_for(db_key);

    let mut decoders = cache.decoders.lock();
    if let Some(dict) = decoders.get(&key) {
        return Ok(dict);
    }

    decoders.insert(key, decoder.clone());
    Ok(decoder)
}

/// Synchronizes a column's current dictionary and prepared codecs with SQLite.
///
/// If no dictionary is current, the cached column mapping is removed so later
/// compression falls back to raw Zstandard data.
pub(crate) fn warm_cache<C>(conn: &mut C, column: &ColumnKey, level: Level) -> Result<(), DictError>
where
    C: SetupConnection + CacheKeySource,
{
    let dict_id = match read_current_id(conn, column)? {
        Some(id) => id,
        None => {
            with_conn(conn, |cache| cache.clear_current_id(column));
            return Ok(());
        }
    };

    with_conn(conn, |cache| cache.set_current_id(column.clone(), dict_id));
    get_encoder(conn, column.schema(), dict_id, level)?;
    get_decoder(conn, column.schema(), dict_id)?;

    Ok(())
}

/// Publishes a newly persisted dictionary to the connection-scoped cache.
///
/// Encoder and decoder entries are installed together so subsequent reads use
/// the same dictionary version in either direction.
pub(crate) fn insert_into_caches<C: CacheKeySource>(
    conn: &C,
    column: &ColumnKey,
    dict_id: DictId,
    dictionary: &[u8],
    level: Level,
) {
    with_conn(conn, |cache| {
        cache.set_current_id(column.clone(), dict_id);

        let key = DictKey::new(column.schema(), dict_id);
        cache.encoders.lock().insert(
            key.clone(),
            Arc::new(EncoderDictionary::copy(dictionary, level.get())),
        );
        cache
            .decoders
            .lock()
            .insert(key, Arc::new(DecoderDictionary::copy(dictionary)));
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusqlite::Connection;
    use zstd::{
        bulk::{Compressor, Decompressor},
        dict::{DecoderDictionary, EncoderDictionary},
    };

    use super::{get_decoder, get_encoder};
    use crate::{
        cache::{CacheKeySource, DbKey},
        dict::DictId,
        setup::DictStore,
        DEFAULT_LEVEL,
    };

    const SCHEMA: &str = "main";

    struct TestConn<'a> {
        conn: &'a Connection,
        key: DbKey,
    }

    impl<'a> TestConn<'a> {
        fn new(conn: &'a Connection) -> Self {
            Self {
                conn,
                key: DbKey::new(),
            }
        }
    }

    impl CacheKeySource for TestConn<'_> {
        fn db_key(&self) -> DbKey {
            self.key
        }
    }

    impl DictStore for TestConn<'_> {
        type Error = rusqlite::Error;
        fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error> {
            let mut stmt = self.conn.prepare(sql)?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            rows.collect()
        }
    }

    fn ensure_dicts_table(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS __compress_dicts (
                id INTEGER PRIMARY KEY,
                dict BLOB NOT NULL,
                trained_at INTEGER NOT NULL,
                table_name TEXT NOT NULL,
                column_name TEXT NOT NULL,
                row_count INTEGER NOT NULL DEFAULT 0,
                is_current INTEGER NOT NULL DEFAULT 0
            );
            "#,
        )
        .unwrap();
    }

    fn insert_trained_dict(conn: &Connection, id: DictId, table_name: &str, column_name: &str) {
        ensure_dicts_table(conn);

        let samples: Vec<Vec<u8>> = (0..32)
            .map(|i| {
                format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n").into_bytes()
            })
            .collect();
        let sample_refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
        let dict = zstd::dict::from_samples(&sample_refs, 1024).unwrap();

        conn.execute(
            "INSERT OR REPLACE INTO __compress_dicts \
             (id, dict, trained_at, table_name, column_name, row_count, is_current) \
             VALUES (?1, ?2, strftime('%s','now'), ?3, ?4, 32, 1)",
            rusqlite::params![id.get(), dict, table_name, column_name],
        )
        .unwrap();
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
        let conn = Connection::open_in_memory().unwrap();
        let id = DictId::new(2_001);
        insert_trained_dict(&conn, id, "requests_raw", "data");

        let mut wrapper = TestConn::new(&conn);
        let encoder = expect_encoder(get_encoder(&mut wrapper, SCHEMA, id, DEFAULT_LEVEL));
        let decoder = expect_decoder(get_decoder(&mut wrapper, SCHEMA, id));

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
        let conn = Connection::open_in_memory().unwrap();
        let id = DictId::new(2_002);
        insert_trained_dict(&conn, id, "requests_raw", "data");

        let mut wrapper = TestConn::new(&conn);
        let encoder = expect_encoder(get_encoder(&mut wrapper, SCHEMA, id, DEFAULT_LEVEL));
        let decoder = expect_decoder(get_decoder(&mut wrapper, SCHEMA, id));

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
        let conn = Connection::open_in_memory().unwrap();
        ensure_dicts_table(&conn);
        let mut wrapper = TestConn::new(&conn);

        assert!(matches!(
            get_decoder(&mut wrapper, SCHEMA, DictId::new(90_002)),
            Err(crate::DictError::NotReady)
        ));
    }

    #[test]
    fn get_encoder_not_ready_when_table_empty() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_dicts_table(&conn);
        let mut wrapper = TestConn::new(&conn);
        assert!(matches!(
            get_encoder(&mut wrapper, SCHEMA, DictId::new(90_001), DEFAULT_LEVEL),
            Err(crate::DictError::NotReady)
        ));
    }

    #[test]
    fn get_encoder_loads_and_caches() {
        let conn = Connection::open_in_memory().unwrap();
        let id = DictId::new(1_001);
        insert_trained_dict(&conn, id, "requests_raw", "data");
        let mut wrapper = TestConn::new(&conn);

        let first = expect_encoder(get_encoder(&mut wrapper, SCHEMA, id, DEFAULT_LEVEL));
        let second = expect_encoder(get_encoder(&mut wrapper, SCHEMA, id, DEFAULT_LEVEL));
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn get_decoder_loads_and_caches() {
        let conn = Connection::open_in_memory().unwrap();
        let id = DictId::new(1_002);
        insert_trained_dict(&conn, id, "requests_raw", "data");
        let mut wrapper = TestConn::new(&conn);

        let first = expect_decoder(get_decoder(&mut wrapper, SCHEMA, id));
        let second = expect_decoder(get_decoder(&mut wrapper, SCHEMA, id));

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn get_encoder_and_decoder_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        let id = DictId::new(1_003);
        insert_trained_dict(&conn, id, "requests_raw", "data");
        let mut wrapper = TestConn::new(&conn);

        let encoder = expect_encoder(get_encoder(&mut wrapper, SCHEMA, id, DEFAULT_LEVEL));
        let decoder = expect_decoder(get_decoder(&mut wrapper, SCHEMA, id));

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
        conn: &Connection,
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

        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO \"{schema}\".\"__compress_dicts\" \
         (id, dict, trained_at, table_name, column_name, row_count, is_current) \
         VALUES (?1, ?2, strftime('%s','now'), ?3, ?4, 32, 1)"
            ),
            rusqlite::params![id.get(), dict, table_name, column_name],
        )
        .unwrap();
    }

    #[test]
    fn get_encoder_and_decoder_isolate_same_id_across_schemas() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        ATTACH DATABASE ':memory:' AS archive;

        CREATE TABLE raw.__compress_dicts (
            id INTEGER PRIMARY KEY,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_count INTEGER NOT NULL DEFAULT 0,
            is_current INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE archive.__compress_dicts (
            id INTEGER PRIMARY KEY,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_count INTEGER NOT NULL DEFAULT 0,
            is_current INTEGER NOT NULL DEFAULT 0
        );
        "#,
        )
        .unwrap();

        let id = DictId::new(1);
        let archive_only = DictId::new(2);

        seed_dict_into(&conn, "raw", id, "requests_raw", "data");
        seed_dict_into(&conn, "archive", id, "requests_raw", "data");
        seed_dict_into(&conn, "archive", archive_only, "requests_raw", "data");

        let mut wrapper = TestConn::new(&conn);

        let raw_enc = expect_encoder(get_encoder(&mut wrapper, "raw", id, DEFAULT_LEVEL));
        let archive_enc = expect_encoder(get_encoder(&mut wrapper, "archive", id, DEFAULT_LEVEL));
        assert!(!Arc::ptr_eq(&raw_enc, &archive_enc));

        let raw_enc_again = expect_encoder(get_encoder(&mut wrapper, "raw", id, DEFAULT_LEVEL));
        assert!(Arc::ptr_eq(&raw_enc, &raw_enc_again));

        expect_decoder(get_decoder(&mut wrapper, "archive", archive_only));
        assert!(matches!(
            get_encoder(&mut wrapper, "raw", archive_only, DEFAULT_LEVEL),
            Err(crate::DictError::NotReady)
        ));
        assert!(matches!(
            get_decoder(&mut wrapper, "raw", archive_only),
            Err(crate::DictError::NotReady)
        ));
    }
}
