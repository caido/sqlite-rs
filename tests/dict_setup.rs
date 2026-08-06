use rusqlite::Connection;
use sqlite_compress::{setup, SetupConfig, SetupTable};
mod common;

#[test]
fn setup_table_does_not_exist() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
         ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    )
    .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: "requests_raw".to_string(),
            schema: "raw".to_string(),
            columns: vec!["data".to_string()],
        }],
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='_zstd_dicts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn setup_table_already_exists() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
         ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        CREATE TABLE raw._zstd_dicts (id INTEGER PRIMARY KEY, dict BLOB NOT NULL, trained_at INTEGER NOT NULL);
        "#,
    )
    .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: "requests_raw".to_string(),
            schema: "raw".to_string(),
            columns: vec!["data".to_string()],
        }],
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='_zstd_dicts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
