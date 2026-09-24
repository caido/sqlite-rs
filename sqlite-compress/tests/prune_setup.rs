use sqlite_compress::{
    setup, SchemaName, SetupColumn, SetupConfig, SetupTable, TableName, DEFAULT_LEVEL,
    DEFAULT_RETRAIN_GROWTH,
};
mod common;
use common::{DEFAULT_MAX_SAMPLES, DEFAULT_MIN_SAMPLES};
use sqlite_ffi::first_value;

#[test]
fn setup_prune_table_does_not_exist() {
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

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master \
             WHERE type='table' AND name='__compress_prune'",
            &[],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_prune_table_already_exists() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
            ATTACH DATABASE ':memory:' AS raw;
            CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
            CREATE TABLE raw.__compress_prune (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                table_name TEXT NOT NULL,
                column_name TEXT NOT NULL,
                target_dict_id INTEGER NOT NULL,
                last_rowid INTEGER NOT NULL,
                UNIQUE(table_name, column_name)
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

    setup(&db, &config).unwrap();

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type='table' AND name='__compress_prune'",
            &[],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn setup_creates_prune_table_per_schema() {
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

    setup(&db, &config).unwrap();

    for schema in ["raw", "archive"] {
        let rows = state
            .as_ref()
            .query(
                &format!(
                    "SELECT COUNT(*) FROM {schema}.sqlite_master \
                     WHERE type = 'table' AND name = '__compress_prune'"
                ),
                &[],
            )
            .unwrap();
        let count = first_value(&rows).unwrap().as_i64().unwrap();
        assert_eq!(count, 1, "__compress_prune missing in schema {schema}");
    }
}

#[test]
fn setup_prune_distinct_table_column_pairs_do_not_conflict() {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            r#"
            ATTACH DATABASE ':memory:' AS raw;
            CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB, body BLOB);
            CREATE TABLE raw.responses_raw (id INTEGER PRIMARY KEY, data BLOB);
            "#,
        )
        .unwrap();

    let column = |name: &str| {
        SetupColumn::new(
            name,
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
                columns: vec![column("data"), column("body")],
            },
            SetupTable {
                name: TableName::new("responses_raw"),
                schema: SchemaName::new("raw"),
                columns: vec![column("data")],
            },
        ],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    for (table_name, column_name) in [
        ("requests_raw", "data"),
        ("requests_raw", "body"),
        ("responses_raw", "data"),
    ] {
        state
            .as_ref()
            .execute(
                "INSERT INTO raw.__compress_prune \
                 (table_name, column_name, target_dict_id, last_rowid) \
                 VALUES (?1, ?2, 1, 0)",
                &[
                    sqlite_ffi::SqlValue::Text(table_name.to_string()),
                    sqlite_ffi::SqlValue::Text(column_name.to_string()),
                ],
            )
            .unwrap();
    }

    let rows = state
        .as_ref()
        .query("SELECT COUNT(*) FROM raw.__compress_prune", &[])
        .unwrap();

    assert_eq!(first_value(&rows).unwrap().as_i64().unwrap(), 3);

    let err = state
        .as_ref()
        .execute(
            "INSERT INTO raw.__compress_prune \
             (table_name, column_name, target_dict_id, last_rowid) \
             VALUES ('requests_raw', 'data', 2, 0)",
            &[],
        )
        .unwrap_err();

    assert_eq!(err.code(), libsqlite3_sys::SQLITE_CONSTRAINT);
}
