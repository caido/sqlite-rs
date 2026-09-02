use std::sync::Arc;

use rusqlite::Connection;
use sqlite_compress::{get_decoder, get_encoder, DictError, DictId, DictStore, DEFAULT_LEVEL};
use zstd::bulk::{Compressor, Decompressor};

mod common;

use crate::common::{expect_decoder, expect_encoder};

const SCHEMA: &str = "main";

fn ensure_dicts_table(conn: &Connection) {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS __compress_dicts (
            id INTEGER PRIMARY KEY,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL
        );
        "#,
    )
    .unwrap();
}

fn seed_dict(conn: &Connection, id: DictId, table_name: &str, column_name: &str) {
    ensure_dicts_table(conn);

    let samples: Vec<Vec<u8>> = (0..32)
        .map(|i| format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n").into_bytes())
        .collect();
    let sample_refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
    let dict = zstd::dict::from_samples(&sample_refs, 1024).unwrap();

    conn.execute(
        "INSERT OR REPLACE INTO __compress_dicts (id, dict, trained_at, table_name, column_name) VALUES (?1, ?2, strftime('%s','now'), ?3, ?4)",
        rusqlite::params![id.get(), dict, table_name, column_name],
    )
    .unwrap();
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
            format!("{schema} sample {i} GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n")
                .into_bytes()
        })
        .collect();
    let sample_refs: Vec<&[u8]> = samples.iter().map(|s| s.as_slice()).collect();
    let dict = zstd::dict::from_samples(&sample_refs, 1024).unwrap();

    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO \"{schema}\".\"__compress_dicts\" \
             (id, dict, trained_at, table_name, column_name) \
             VALUES (?1, ?2, strftime('%s','now'), ?3, ?4)"
        ),
        rusqlite::params![id.get(), dict, table_name, column_name],
    )
    .unwrap();
}

#[test]
fn get_encoder_not_ready_when_table_empty() {
    let conn = Connection::open_in_memory().unwrap();
    ensure_dicts_table(&conn);
    let mut wrapper = common::RusqliteConn::new(&conn);

    match get_encoder(SCHEMA, DictId::new(90_001), &mut wrapper, DEFAULT_LEVEL) {
        Err(DictError::NotReady) => {}
        Ok(_) => panic!("expected NotReady, got Ok"),
        Err(e) => panic!("expected NotReady, got Err({e})"),
    }
}

#[test]
fn get_decoder_not_ready_when_table_empty() {
    let conn = Connection::open_in_memory().unwrap();
    ensure_dicts_table(&conn);
    let mut wrapper = common::RusqliteConn::new(&conn);

    match get_decoder(SCHEMA, DictId::new(90_002), &mut wrapper) {
        Err(DictError::NotReady) => {}
        Ok(_) => panic!("expected NotReady, got Ok"),
        Err(e) => panic!("expected NotReady, got Err({e})"),
    }
}

#[test]
fn get_encoder_loads_and_caches() {
    let conn = Connection::open_in_memory().unwrap();
    let id = DictId::new(1_001);
    seed_dict(&conn, id, "requests_raw", "data");
    let mut wrapper = common::RusqliteConn::new(&conn);

    let first = expect_encoder(get_encoder(SCHEMA, id, &mut wrapper, DEFAULT_LEVEL));
    let second = expect_encoder(get_encoder(SCHEMA, id, &mut wrapper, DEFAULT_LEVEL));

    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn get_decoder_loads_and_caches() {
    let conn = Connection::open_in_memory().unwrap();
    let id = DictId::new(1_002);
    seed_dict(&conn, id, "requests_raw", "data");
    let mut wrapper = common::RusqliteConn::new(&conn);

    let first = expect_decoder(get_decoder(SCHEMA, id, &mut wrapper));
    let second = expect_decoder(get_decoder(SCHEMA, id, &mut wrapper));

    assert!(Arc::ptr_eq(&first, &second));
}

#[test]
fn get_encoder_and_decoder_roundtrip() {
    let conn = Connection::open_in_memory().unwrap();
    let id = DictId::new(1_003);
    seed_dict(&conn, id, "requests_raw", "data");
    let mut wrapper = common::RusqliteConn::new(&conn);

    let encoder = expect_encoder(get_encoder(SCHEMA, id, &mut wrapper, DEFAULT_LEVEL));
    let decoder = expect_decoder(get_decoder(SCHEMA, id, &mut wrapper));

    let original = b"GET /api/users/42 HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let mut compressor = Compressor::with_prepared_dictionary(&encoder).unwrap();
    let compressed = compressor.compress(original).unwrap();

    let mut decompressor = Decompressor::with_prepared_dictionary(&decoder).unwrap();
    let decompressed = decompressor
        .decompress(&compressed, original.len())
        .unwrap();

    assert_eq!(decompressed, original);
}

#[test]
fn query_blobs_returns_seeded_dict() {
    let conn = Connection::open_in_memory().unwrap();
    let id = DictId::new(1_004);
    seed_dict(&conn, id, "requests_raw", "data");
    let mut wrapper = common::RusqliteConn::new(&conn);

    let rows = wrapper
        .query_blobs(&format!(
            "SELECT dict FROM __compress_dicts WHERE id = {id}"
        ))
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].is_empty());
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
            column_name TEXT NOT NULL
        );
        CREATE TABLE archive.__compress_dicts (
            id INTEGER PRIMARY KEY,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL
        );
        "#,
    )
    .unwrap();

    let id = DictId::new(1);
    let archive_only = DictId::new(2);

    seed_dict_into(&conn, "raw", id, "requests_raw", "data");
    seed_dict_into(&conn, "archive", id, "requests_raw", "data");
    seed_dict_into(&conn, "archive", archive_only, "requests_raw", "data");

    let mut wrapper = common::RusqliteConn::new(&conn);

    // warm both schemas first — same DictId must not collide
    let raw_enc = expect_encoder(get_encoder("raw", id, &mut wrapper, DEFAULT_LEVEL));
    let archive_enc = expect_encoder(get_encoder("archive", id, &mut wrapper, DEFAULT_LEVEL));
    assert!(!Arc::ptr_eq(&raw_enc, &archive_enc));

    let raw_enc_again = expect_encoder(get_encoder("raw", id, &mut wrapper, DEFAULT_LEVEL));
    assert!(Arc::ptr_eq(&raw_enc, &raw_enc_again));

    expect_decoder(get_decoder("archive", archive_only, &mut wrapper));
    match get_encoder("raw", archive_only, &mut wrapper, DEFAULT_LEVEL) {
        Err(DictError::NotReady) => {}
        Ok(_) => panic!("expected NotReady from wrong schema after cache warm, got Ok"),
        Err(e) => panic!("expected NotReady from wrong schema after cache warm, got Err({e})"),
    }
    match get_decoder("raw", archive_only, &mut wrapper) {
        Err(DictError::NotReady) => {}
        Ok(_) => panic!("expected NotReady from wrong schema after cache warm, got Ok"),
        Err(e) => panic!("expected NotReady from wrong schema after cache warm, got Err({e})"),
    }
}
