use rusqlite::Connection;
use sqlite_compress::{
    setup, ColumnName, SchemaName, SetupConfig, SetupError, SetupTable, TableName,
};
mod common;

#[test]
fn setup_skips_existing_view() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
         ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        CREATE VIEW raw.requests_raw_decoded AS SELECT * FROM requests_raw;
        "#,
    )
    .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![ColumnName::new("data")],
        }],
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();
}

#[test]
fn setup_creates_view() {
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
            columns: vec![ColumnName::new("data")],
        }],
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let view_name = "requests_raw_zstd_decoded";
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view' AND name = ?1",
            [view_name],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_rejects_missing_column() {
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
            columns: vec![ColumnName::new("unvalid_column")],
        }],
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    let err = setup(&mut wrapper, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::ColumnNotFound { table, column }
            if table == "requests_raw" && column == "unvalid_column"
    ));
}

#[test]
fn setup_rejects_missing_table() {
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
            name: TableName::new("missing_table"),
            schema: SchemaName::new("raw"),
            columns: vec![],
        }],
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    let err = setup(&mut wrapper, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::TableNotFound(table)
            if table == "raw.missing_table"
    ));
}
