use sqlite_compress::{
    DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH, SchemaName, SetupColumn, SetupConfig, SetupError,
    SetupTable, TableName, ViewName, setup,
};
mod common;
use common::{DEFAULT_MAX_SAMPLES, DEFAULT_MIN_SAMPLES};
use sqlite_ffi::{SqlValue, first_value};

fn invalid_config(column: SetupColumn) -> SetupConfig {
    SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![column],
        )],
        compression_level: DEFAULT_LEVEL,
    }
}

#[test]
fn setup_skips_existing_view() {
    let db = common::TestDb::open();
    let state = db.state();

    let view_name = "__compress_decoded_requests_raw";

    state
        .as_ref()
        .batch_execute(&format!(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        CREATE VIEW raw."{view_name}" AS SELECT id FROM raw.requests_raw;
        "#
        ))
        .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                DEFAULT_MIN_SAMPLES,
                DEFAULT_MAX_SAMPLES,
            )],
        )],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view'",
            &[],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_creates_view() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
        )
        .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                DEFAULT_MIN_SAMPLES,
                DEFAULT_MAX_SAMPLES,
            )],
        )],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let view_name = "__compress_decoded_requests_raw";
    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view' AND name = ?1",
            &[SqlValue::Text(view_name.to_string())],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_creates_one_view_per_table() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
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
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![
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
        )],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let rows = state
        .as_ref()
        .query(
            "SELECT name FROM raw.sqlite_master \
         WHERE type = 'view' ORDER BY name",
            &[],
        )
        .unwrap();

    assert_eq!(
        rows,
        vec![vec![SqlValue::Text(
            "__compress_decoded_requests_raw".into()
        )]]
    );
}

#[test]
fn setup_rejects_missing_column() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
    ATTACH DATABASE ':memory:' AS raw;
    CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
    "#,
        )
        .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![SetupColumn::new(
                "invalid_column",
                DEFAULT_RETRAIN_GROWTH,
                DEFAULT_MIN_SAMPLES,
                DEFAULT_MAX_SAMPLES,
            )],
        )],
        compression_level: DEFAULT_LEVEL,
    };

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::ColumnNotFound { table, column }
            if table == "requests_raw" && column == "invalid_column"
    ));
}

#[test]
fn setup_rejects_missing_table() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
    ATTACH DATABASE ':memory:' AS raw;
    CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
    "#,
        )
        .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("missing_table"),
            vec![],
        )],
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
    let state = db.state();

    state
        .as_ref()
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
    let state = db.state();

    state
        .as_ref()
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
    let state = db.state();

    state
        .as_ref()
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
    let state = db.state();

    state
        .as_ref()
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

#[test]
fn setup_rejects_view_equal_to_table_name() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
        )
        .unwrap();

    let config = SetupConfig {
        tables: vec![
            SetupTable::new(
                SchemaName::new("raw"),
                TableName::new("requests_raw"),
                vec![SetupColumn::new(
                    "data",
                    DEFAULT_RETRAIN_GROWTH,
                    DEFAULT_MIN_SAMPLES,
                    DEFAULT_MAX_SAMPLES,
                )],
            )
            .with_view(ViewName::new("requests_raw")),
        ],
        compression_level: DEFAULT_LEVEL,
    };

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::NameConflict { name, existing_type }
            if name == "requests_raw" && existing_type == "table"
    ));

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view'",
            &[],
        )
        .unwrap();
    let count = first_value(&rows).unwrap().as_i64().unwrap();
    assert_eq!(count, 0);
}

#[test]
fn setup_rejects_table_occupying_view_name() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_data (id INTEGER PRIMARY KEY, data BLOB);
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
        )
        .unwrap();

    let config = SetupConfig {
        tables: vec![
            SetupTable::new(
                SchemaName::new("raw"),
                TableName::new("requests_data"),
                vec![SetupColumn::new(
                    "data",
                    DEFAULT_RETRAIN_GROWTH,
                    DEFAULT_MIN_SAMPLES,
                    DEFAULT_MAX_SAMPLES,
                )],
            )
            .with_view(ViewName::new("requests_raw")),
        ],
        compression_level: DEFAULT_LEVEL,
    };

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::NameConflict { name, existing_type }
            if name == "requests_raw" && existing_type == "table"
    ));
}

#[test]
fn setup_creates_custom_view_name() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
        )
        .unwrap();

    let custom_view = "requests_decoded";

    let config = SetupConfig {
        tables: vec![
            SetupTable::new(
                SchemaName::new("raw"),
                TableName::new("requests_raw"),
                vec![SetupColumn::new(
                    "data",
                    DEFAULT_RETRAIN_GROWTH,
                    DEFAULT_MIN_SAMPLES,
                    DEFAULT_MAX_SAMPLES,
                )],
            )
            .with_view(ViewName::new(custom_view)),
        ],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let rows = state
        .as_ref()
        .query(
            "SELECT name FROM raw.sqlite_master \
         WHERE type = 'view' ORDER BY name",
            &[],
        )
        .unwrap();

    assert_eq!(rows, vec![vec![SqlValue::Text(custom_view.into())]]);

    let default_name = "__compress_decoded_requests_raw";
    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view' AND name = ?1",
            &[SqlValue::Text(default_name.to_string())],
        )
        .unwrap();
    let count = first_value(&rows).unwrap().as_i64().unwrap();
    assert_eq!(count, 0);
}

#[test]
fn setup_rejects_duplicate_custom_view_names() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        CREATE TABLE raw.responses_raw (id INTEGER PRIMARY KEY, data BLOB);
    "#,
        )
        .unwrap();

    let config = SetupConfig {
        tables: vec![
            SetupTable::new(
                SchemaName::new("raw"),
                TableName::new("requests_raw"),
                vec![SetupColumn::new(
                    "data",
                    DEFAULT_RETRAIN_GROWTH,
                    DEFAULT_MIN_SAMPLES,
                    DEFAULT_MAX_SAMPLES,
                )],
            )
            .with_view(ViewName::new("decoded")),
            SetupTable::new(
                SchemaName::new("raw"),
                TableName::new("responses_raw"),
                vec![SetupColumn::new(
                    "data",
                    DEFAULT_RETRAIN_GROWTH,
                    DEFAULT_MIN_SAMPLES,
                    DEFAULT_MAX_SAMPLES,
                )],
            )
            .with_view(ViewName::new("Decoded")),
        ],
        compression_level: DEFAULT_LEVEL,
    };

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(err, SetupError::InvalidConfig(_)));

    let count = first_value(
        &state
            .as_ref()
            .query(
                "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'view'",
                &[],
            )
            .unwrap(),
    )
    .unwrap()
    .as_i64()
    .unwrap();
    assert_eq!(count, 0);
}
