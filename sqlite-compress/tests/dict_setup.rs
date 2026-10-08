use sqlite_compress::{
    DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH, SchemaName, SetupColumn, SetupConfig, SetupTable,
    TableName, setup,
};
mod common;
use common::{DEFAULT_MAX_SAMPLES, DEFAULT_MIN_SAMPLES};
use sqlite_ffi::first_value;

#[test]
fn setup_table_does_not_exist() {
    let db = common::TestDb::open();
    let state = db.state();

    let res = state.as_ref().batch_execute(
        r#"
            ATTACH DATABASE ':memory:' AS raw;
            CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
            "#,
    );

    assert_eq!(res.unwrap(), 0);

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
            "SELECT COUNT(*) FROM raw.sqlite_master \
             WHERE type='table' AND name='__compress_dicts'",
            &[],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_table_already_exists() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
            ATTACH DATABASE ':memory:' AS raw;
            CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
            CREATE TABLE raw.__compress_dicts (
                id INTEGER PRIMARY KEY,
                dict BLOB NOT NULL,
                trained_at INTEGER NOT NULL,
                table_name TEXT NOT NULL,
                column_name TEXT NOT NULL,
                row_count INTEGER NOT NULL
            );
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
        compression_level: sqlite_compress::DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='__compress_dicts'",
            &[],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_creates_dict_table_per_schema() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
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
            SetupTable::new(
                SchemaName::new("raw"),
                TableName::new("requests_raw"),
                vec![column()],
            ),
            SetupTable::new(
                SchemaName::new("archive"),
                TableName::new("requests_raw"),
                vec![column()],
            ),
        ],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    for schema in ["raw", "archive"] {
        let rows = state
            .as_ref()
            .query(
                &format!(
                    "SELECT COUNT(*) FROM {schema}.sqlite_master \
                     WHERE type = 'table' AND name = '__compress_dicts'"
                ),
                &[],
            )
            .unwrap();
        let count = first_value(&rows).unwrap().as_i64().unwrap();
        assert_eq!(count, 1, "__compress_dicts missing in schema {schema}");
    }
}
