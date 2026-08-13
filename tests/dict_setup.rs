/* use rusqlite::Connection;
use sqlite_compress::{
    setup, SchemaName, SetupColumn, SetupConfig, SetupTable, TableName, DEFAULT_LEVEL,
    DEFAULT_RETRAIN_GROWTH,
};
mod common;

const DEFAULT_MIN_SAMPLES: usize = 1000;
const DEFAULT_MAX_SAMPLES: usize = 10000;

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
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                DEFAULT_MIN_SAMPLES,
                DEFAULT_MAX_SAMPLES,
            )],
        }],
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='__zstd_dicts'",
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
        CREATE TABLE raw.__zstd_dicts (id INTEGER PRIMARY KEY, dict BLOB NOT NULL, trained_at INTEGER NOT NULL);
        "#,
    )
    .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                DEFAULT_MIN_SAMPLES,
                DEFAULT_MAX_SAMPLES,
            )],
        }],
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='__zstd_dicts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}
 */
