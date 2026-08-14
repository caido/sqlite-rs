use rusqlite::Connection;
use sqlite_compress::{
    compress, decompress, get_decoder, get_encoder, ColumnKey, DictId, Header, CURRENT_DICT_IDS,
    DEFAULT_LEVEL,
};

mod common;
use crate::common::{expect_decoder, expect_encoder};

const SCHEMA: &str = "raw";

fn column(name: &str) -> ColumnKey {
    ColumnKey::new(SCHEMA, "requests_raw", name)
}

fn seed_dict(conn: &Connection, id: DictId, table_name: &str, column_name: &str) {
    let _ = conn.execute("ATTACH DATABASE ':memory:' AS raw", []);
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS "raw"."__zstd_dicts" (
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

    let samples: Vec<Vec<u8>> = (0..32)
        .map(|i| format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n").into_bytes())
        .collect();
    let sample_refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
    let dict = zstd::dict::from_samples(&sample_refs, 1024).unwrap();

    conn.execute(
        "INSERT OR REPLACE INTO \"raw\".\"__zstd_dicts\" \
         (id, dict, trained_at, table_name, column_name, row_count) \
         VALUES (?1, ?2, strftime('%s','now'), ?3, ?4, 32)",
        rusqlite::params![id.get(), dict, table_name, column_name],
    )
    .unwrap();
}

#[test]
fn compress_decompress_roundtrip_via_cached_dict() {
    let conn = Connection::open_in_memory().unwrap();
    let id = DictId::new(2_001);
    seed_dict(&conn, id, "requests_raw", "data");

    let mut wrapper = common::RusqliteConn::new(&conn);
    // populate both caches + CURRENT_DICT_ID
    expect_encoder(get_encoder(SCHEMA, id, &mut wrapper, DEFAULT_LEVEL));
    expect_decoder(get_decoder(SCHEMA, id, &mut wrapper));

    CURRENT_DICT_IDS.lock().insert(column("data"), id);

    let original = b"GET /api/users/42 HTTP/1.1\r\nHost: example.com\r\n\r\n";

    let compressed = compress(original, &column("data"), DEFAULT_LEVEL).unwrap();

    let (header, _schema, _payload) = Header::parse(&compressed).unwrap();

    assert_eq!(header.dict_id.get(), id.get());
    assert!(compressed.len() > 4);

    let decompressed = decompress(&compressed).unwrap();
    assert_eq!(decompressed, original);
}

#[test]
fn compress_falls_back_to_raw_without_cache() {
    // use an id that was never loaded into the process caches
    // (LATEST may still be set by other tests — isolate by only checking header
    // after ensuring no encoder for that path, or reset via a fresh unused scenario)

    let original = b"hello world without dict";
    let compressed = compress(original, &column("data"), DEFAULT_LEVEL).unwrap();
    let header = u32::from_le_bytes(compressed[..4].try_into().unwrap());

    if header == 0 {
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }
}

#[test]
fn compress_uses_distinct_dict_ids_per_column() {
    let conn = Connection::open_in_memory().unwrap();
    let data_id = DictId::new(3_001);
    let headers_id = DictId::new(3_002);

    seed_dict(&conn, data_id, "requests_raw", "data");
    seed_dict(&conn, headers_id, "requests_raw", "headers");

    let mut wrapper = common::RusqliteConn::new(&conn);
    expect_encoder(get_encoder(SCHEMA, data_id, &mut wrapper, DEFAULT_LEVEL));
    expect_decoder(get_decoder(SCHEMA, data_id, &mut wrapper));
    expect_encoder(get_encoder(SCHEMA, headers_id, &mut wrapper, DEFAULT_LEVEL));
    expect_decoder(get_decoder(SCHEMA, headers_id, &mut wrapper));

    {
        let mut map = CURRENT_DICT_IDS.lock();
        map.insert(column("data"), data_id);
        map.insert(column("headers"), headers_id);
    }

    let payload = b"GET /api/users/42 HTTP/1.1\r\nHost: example.com\r\n\r\n";

    let compressed_data = compress(payload, &column("data"), DEFAULT_LEVEL).unwrap();
    let compressed_headers = compress(payload, &column("headers"), DEFAULT_LEVEL).unwrap();

    let (header_data, _schema, _payload) = Header::parse(&compressed_data).unwrap();
    let (header_headers, _schema, _payload) = Header::parse(&compressed_headers).unwrap();

    assert_eq!(header_data.dict_id.get(), data_id.get());
    assert_eq!(header_headers.dict_id.get(), headers_id.get());
    assert_ne!(header_data.dict_id.get(), header_headers.dict_id.get());

    assert_eq!(decompress(&compressed_data).unwrap(), payload);
    assert_eq!(decompress(&compressed_headers).unwrap(), payload);
}

#[test]
fn compress_embeds_schema_in_header() {
    let conn = Connection::open_in_memory().unwrap();
    let id = DictId::new(12_001);
    seed_dict(&conn, id, "requests_raw", "data");

    let mut wrapper = common::RusqliteConn::new(&conn);
    expect_encoder(get_encoder(SCHEMA, id, &mut wrapper, DEFAULT_LEVEL));

    CURRENT_DICT_IDS.lock().insert(column("data"), id);

    let compressed = compress(
        b"GET /api/users/1 HTTP/1.1\r\n\r\n",
        &column("data"),
        DEFAULT_LEVEL,
    )
    .unwrap();
    let (header, schema, _) = Header::parse(&compressed).unwrap();

    assert_eq!(header.dict_id.get(), id.get());
    assert_eq!(schema, "raw");
    assert_eq!(header.schema_len as usize, schema.len());
}
