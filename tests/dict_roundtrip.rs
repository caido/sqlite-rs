use rusqlite::Connection;
use sqlite_compress::{get_decoder, get_encoder, DictId, DEFAULT_LEVEL};
use zstd::bulk::{Compressor, Decompressor};

mod common;

use crate::common::{expect_decoder, expect_encoder};

fn insert_trained_dict(conn: &Connection, id: DictId) -> Vec<u8> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS __zstd_dicts (
            id INTEGER PRIMARY KEY,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL
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
        "INSERT OR REPLACE INTO __zstd_dicts (id, dict, trained_at) VALUES (?1, ?2, strftime('%s','now'))",
        rusqlite::params![id.get(), dict],
    )
    .unwrap();

    dict
}

#[test]
fn insert_dict_encode_decode_equals_input() {
    let conn = Connection::open_in_memory().unwrap();
    let id = DictId::new(2_001);
    let _dict_bytes = insert_trained_dict(&conn, id);

    let mut wrapper = common::RusqliteConn::new(&conn);
    let encoder = expect_encoder(get_encoder(id, &mut wrapper, DEFAULT_LEVEL));
    let decoder = expect_decoder(get_decoder(id, &mut wrapper));

    let original = b"GET /api/users/42 HTTP/1.1\r\nHost: example.com\r\n\r\n";

    let mut compressor = Compressor::with_prepared_dictionary(&encoder).unwrap();
    let compressed = compressor.compress(original).unwrap();
    assert_ne!(compressed, original);

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
    insert_trained_dict(&conn, id);

    let mut wrapper = common::RusqliteConn::new(&conn);
    let encoder = expect_encoder(get_encoder(id, &mut wrapper, DEFAULT_LEVEL));
    let decoder = expect_decoder(get_decoder(id, &mut wrapper));

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
