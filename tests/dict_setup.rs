use rusqlite::Connection;
use sqlite_compress::{
    setup, SchemaName, SetupColumn, SetupConfig, SetupTable, TableName, DEFAULT_LEVEL,
    DEFAULT_RETRAIN_GROWTH,
};
mod common;
use common::{DEFAULT_MAX_SAMPLES, DEFAULT_MIN_SAMPLES};

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
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='__compress_dicts'",
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
            CREATE TABLE raw.__compress_dicts (
                id INTEGER PRIMARY KEY,
                dict BLOB NOT NULL,
                trained_at INTEGER NOT NULL,
                table_name TEXT NOT NULL,
                column_name TEXT NOT NULL,
                row_count INTEGER NOT NULL,
                is_current INTEGER NOT NULL DEFAULT 0
            );
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
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='__compress_dicts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn setup_creates_dict_table_per_schema() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        ATTACH DATABASE ':memory:' AS archive;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        CREATE TABLE archive.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    )
    .unwrap();

    let column = || {
        SetupColumn::new(
            "data",
            DEFAULT_RETRAIN_GROWTH,
            DEFAULT_MIN_SAMPLES,
            DEFAULT_MAX_SAMPLES,
        )
    };

    let config = SetupConfig {
        tables: vec![
            SetupTable {
                name: TableName::new("requests_raw"),
                schema: SchemaName::new("raw"),
                columns: vec![column()],
            },
            SetupTable {
                name: TableName::new("requests_raw"),
                schema: SchemaName::new("archive"),
                columns: vec![column()],
            },
        ],
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    for schema in ["raw", "archive"] {
        let count: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {schema}.sqlite_master \
                     WHERE type = 'table' AND name = '__compress_dicts'"
                ),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "__compress_dicts missing in schema {schema}");
    }
}
