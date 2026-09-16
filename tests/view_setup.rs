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
    let db = common::TestDb::open();
    let conn = db.conn();

    let view_name = "__compress_decoded_requests_raw";

    conn.database()
        .batch_execute(&format!(
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

    setup(&db, &config).unwrap();

    let count = conn
        .database()
        .query_i64("SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view'")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn setup_creates_view() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
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

    setup(&db, &config).unwrap();

    let view_name = "__compress_decoded_requests_raw";
    let count = conn
        .database()
        .query_i64(&format!(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view' AND name = '{}'",
            view_name
        ))
        .unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_creates_one_view_per_table() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
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

    setup(&db, &config).unwrap();

    let names = conn
        .database()
        .query_strings(
            "SELECT name FROM raw.sqlite_master \
         WHERE type = 'view' ORDER BY name",
        )
        .unwrap();

    assert_eq!(names, vec!["__compress_decoded_requests_raw"]);
}

#[test]
fn setup_rejects_missing_column() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
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

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::ColumnNotFound { table, column }
            if table == "requests_raw" && column == "unvalid_column"
    ));
}

#[test]
fn setup_rejects_missing_table() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
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

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::TableNotFound(table)
            if table == "raw.missing_table"
    ));
}

#[test]
fn setup_rejects_zero_retrain_growth() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
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

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("retrain growth must be greater than zero")
    ));
}

#[test]
fn setup_rejects_zero_min_samples() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
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

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("min samples must be greater than zero")
    ));
}

#[test]
fn setup_rejects_zero_max_samples() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
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

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("max samples must be greater than zero")
    ));
}

#[test]
fn setup_rejects_max_samples_below_min() {
    let db = common::TestDb::open();
    let conn = db.conn();

    conn.database()
        .batch_execute(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
        )
        .unwrap();

    let config = invalid_config(SetupColumn::new("data", DEFAULT_RETRAIN_GROWTH, 2000, 1000));

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("max samples must be greater than or equal to min samples")
    ));
}
