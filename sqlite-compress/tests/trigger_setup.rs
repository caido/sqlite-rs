use sqlite_compress::{
    DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH, Header, SchemaName, SetupColumn, SetupConfig,
    SetupError, SetupTable, TableName, setup,
};
mod common;
use common::{DEFAULT_MAX_SAMPLES, DEFAULT_MIN_SAMPLES};
use sqlite_ffi::{SqlValue, first_value};

fn writable_config() -> SetupConfig {
    SetupConfig {
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
            .with_trigger(),
        ],
        compression_level: DEFAULT_LEVEL,
    }
}

fn open_raw_table(sql: &str) -> (common::TestDb, sqlite_compress::ExtensionState) {
    let db = common::TestDb::open();
    let state = db.state();
    state.as_ref().batch_execute(sql).unwrap();
    (db, state)
}

#[test]
fn setup_creates_insert_trigger() {
    let (db, state) = open_raw_table(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    );

    setup(&db, &writable_config()).unwrap();

    let name = "__compress_decoded_requests_raw_insert";
    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'trigger' AND name = ?1",
            &[SqlValue::Text(name.into())],
        )
        .unwrap();
    assert_eq!(first_value(&rows).unwrap().as_i64().unwrap(), 1);
}

#[test]
fn insert_through_view_compresses_and_round_trips() {
    let (db, state) = open_raw_table(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    );
    setup(&db, &writable_config()).unwrap();

    let payload = b"GET /api/users/1 HTTP/1.1\r\nHost: example.com\r\n\r\n";
    state
        .as_ref()
        .execute(
            "INSERT INTO raw.__compress_decoded_requests_raw (data) VALUES (?1)",
            &[SqlValue::Blob(payload.to_vec())],
        )
        .unwrap();

    let stored = state
        .as_ref()
        .query("SELECT data FROM raw.requests_raw", &[])
        .unwrap();
    let blob = first_value(&stored).unwrap().as_blob().unwrap();
    assert_ne!(blob, payload);
    assert_eq!(Header::parse(blob).unwrap().0.dict_id.get(), 0);

    let decoded = state
        .as_ref()
        .query("SELECT data FROM raw.__compress_decoded_requests_raw", &[])
        .unwrap();
    assert_eq!(first_value(&decoded).unwrap().as_blob().unwrap(), payload);
}

#[test]
fn insert_through_view_preserves_null_compressed_column() {
    let (db, state) = open_raw_table(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    );
    setup(&db, &writable_config()).unwrap();

    state
        .as_ref()
        .execute(
            "INSERT INTO raw.__compress_decoded_requests_raw (data) VALUES (NULL)",
            &[],
        )
        .unwrap();

    let rows = state
        .as_ref()
        .query("SELECT data IS NULL FROM raw.requests_raw", &[])
        .unwrap();
    assert_eq!(first_value(&rows).unwrap().as_i64().unwrap(), 1);
}

#[test]
fn setup_insert_trigger_is_idempotent() {
    let (db, state) = open_raw_table(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    );
    let config = writable_config();
    setup(&db, &config).unwrap();
    setup(&db, &config).unwrap();

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.sqlite_master WHERE type = 'trigger'",
            &[],
        )
        .unwrap();
    assert_eq!(first_value(&rows).unwrap().as_i64().unwrap(), 1);
}

#[test]
fn setup_rejects_table_occupying_trigger_name() {
    let (db, _state) = open_raw_table(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        CREATE TABLE raw."__compress_decoded_requests_raw_insert" (id INTEGER);
        "#,
    );

    let err = setup(&db, &writable_config()).unwrap_err();
    assert!(matches!(
        err,
        SetupError::NameConflict { name, existing_type }
            if name == "__compress_decoded_requests_raw_insert" && existing_type == "table"
    ));
}

#[test]
fn setup_rejects_writable_view_without_columns() {
    let (db, _state) = open_raw_table(
        r#"
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        "#,
    );

    let config = SetupConfig {
        tables: vec![
            SetupTable::new(
                SchemaName::new("raw"),
                TableName::new("requests_raw"),
                vec![],
            )
            .with_trigger(),
        ],
        compression_level: DEFAULT_LEVEL,
    };

    let err = setup(&db, &config).unwrap_err();
    assert!(matches!(
        err,
        SetupError::InvalidConfig("writable view requires at least one compressed column")
    ));
}
