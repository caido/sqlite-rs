use rusqlite::Connection;
use sqlite_compress::{
    setup, SchemaName, SetupColumn, SetupConfig, SetupError, SetupTable, TableName, DEFAULT_LEVEL,
    DEFAULT_RETRAIN_GROWTH,
};
mod common;
use common::{DEFAULT_MAX_SAMPLES, DEFAULT_MIN_SAMPLES};

fn invalid_config(column: SetupColumn) -> SetupConfig {
    SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![column],
        }],
        compression_level: DEFAULT_LEVEL,
    }
}

#[test]
fn setup_skips_existing_view() {
    let conn = Connection::open_in_memory().unwrap();

    let view_name = "__compress_decoded_requests_raw";

    conn.execute_batch(&format!(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        CREATE VIEW raw."{view_name}" AS SELECT id FROM raw.requests_raw;
        "#
    ))
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
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
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

    let view_name = "__compress_decoded_requests_raw";
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
fn setup_creates_one_view_per_table() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB,
            headers BLOB
        );
        "#,
    )
    .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![
                SetupColumn::new(
                    "data",
                    DEFAULT_RETRAIN_GROWTH,
                    DEFAULT_MIN_SAMPLES,
                    DEFAULT_MAX_SAMPLES,
                ),
                SetupColumn::new(
                    "headers",
                    DEFAULT_RETRAIN_GROWTH,
                    DEFAULT_MIN_SAMPLES,
                    DEFAULT_MAX_SAMPLES,
                ),
            ],
        }],
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let names: Vec<String> = conn
        .prepare("SELECT name FROM raw.sqlite_master WHERE type = 'view' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(names, vec!["__compress_decoded_requests_raw".to_string()]);
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
            columns: vec![SetupColumn::new(
                "unvalid_column",
                DEFAULT_RETRAIN_GROWTH,
                DEFAULT_MIN_SAMPLES,
                DEFAULT_MAX_SAMPLES,
            )],
        }],
        compression_level: DEFAULT_LEVEL,
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
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    let err = setup(&mut wrapper, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::TableNotFound(table)
            if table == "raw.missing_table"
    ));
}

#[test]
fn setup_rejects_zero_retrain_growth() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    )
    .unwrap();

    let config = invalid_config(SetupColumn::new(
        "data",
        0,
        DEFAULT_MIN_SAMPLES,
        DEFAULT_MAX_SAMPLES,
    ));
    let mut wrapper = common::RusqliteConn::new(&conn);
    let err = setup(&mut wrapper, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("retrain growth must be greater than zero")
    ));
}

#[test]
fn setup_rejects_zero_min_samples() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    )
    .unwrap();

    let config = invalid_config(SetupColumn::new(
        "data",
        DEFAULT_RETRAIN_GROWTH,
        0,
        DEFAULT_MAX_SAMPLES,
    ));
    let mut wrapper = common::RusqliteConn::new(&conn);
    let err = setup(&mut wrapper, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("min samples must be greater than zero")
    ));
}

#[test]
fn setup_rejects_zero_max_samples() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    )
    .unwrap();

    let config = invalid_config(SetupColumn::new(
        "data",
        DEFAULT_RETRAIN_GROWTH,
        DEFAULT_MIN_SAMPLES,
        0,
    ));
    let mut wrapper = common::RusqliteConn::new(&conn);
    let err = setup(&mut wrapper, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("max samples must be greater than zero")
    ));
}

#[test]
fn setup_rejects_max_samples_below_min() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    )
    .unwrap();

    let config = invalid_config(SetupColumn::new("data", DEFAULT_RETRAIN_GROWTH, 2000, 1000));
    let mut wrapper = common::RusqliteConn::new(&conn);
    let err = setup(&mut wrapper, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("max samples must be greater than or equal to min samples")
    ));
}
