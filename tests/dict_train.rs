use rusqlite::{params, Connection};
use sqlite_compress::{setup, train, SetupConfig, SetupTable, DEFAULT_LEVEL};

mod common;

#[test]
fn train_persists_a_new_dictionary() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    )
    .unwrap();

    for i in 0..64 {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute(
            "INSERT INTO raw.requests_raw (data) VALUES (?1)",
            params![sample.as_bytes()],
        )
        .unwrap();
    }

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: "requests_raw".to_string(),
            schema: "raw".to_string(),
            columns: vec!["data".to_string()],
        }],
        compression_level: DEFAULT_LEVEL,
    };
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let dict_id = train(&mut wrapper, &config, 1024, 10, 1).unwrap();

    let (stored_id, dict_size): (u32, usize) = conn
        .query_row("SELECT id, length(dict) FROM raw._zstd_dicts", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();

    assert_eq!(dict_id, 1);
    assert_eq!(stored_id, dict_id);
    assert!(dict_size > 0);
}

fn sample_config() -> SetupConfig {
    SetupConfig {
        tables: vec![SetupTable {
            name: "requests_raw".to_string(),
            schema: "raw".to_string(),
            columns: vec!["data".to_string()],
        }],
        compression_level: DEFAULT_LEVEL,
    }
}
fn setup_raw_table(conn: &Connection) {
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    )
    .unwrap();
}
fn insert_samples(conn: &Connection, count: usize) {
    for i in 0..count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute(
            "INSERT INTO raw.requests_raw (data) VALUES (?1)",
            params![sample.as_bytes()],
        )
        .unwrap();
    }
}

#[test]
fn train_rejects_empty_samples() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    )
    .unwrap();

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: "requests_raw".to_string(),
            schema: "raw".to_string(),
            columns: vec!["data".to_string()],
        }],
        compression_level: DEFAULT_LEVEL,
    };
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let error = train(&mut wrapper, &config, 1024, 10, 1).unwrap_err();
    assert!(error.to_string().contains("not enough samples"));
}

#[test]
fn train_stores_row_count() {
    let conn = Connection::open_in_memory().unwrap();
    setup_raw_table(&conn);
    insert_samples(&conn, 64);
    let config = sample_config();
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();
    let dict_id = train(&mut wrapper, &config, 1024, 10, 1).unwrap();
    let (stored_id, row_count): (u32, i64) = conn
        .query_row(
            "SELECT id, row_count FROM raw._zstd_dicts WHERE id = ?1",
            [dict_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored_id, dict_id);
    assert_eq!(row_count, 64);
}

#[test]
fn train_skips_when_growth_below_threshold() {
    let conn = Connection::open_in_memory().unwrap();
    setup_raw_table(&conn);
    insert_samples(&conn, 64);
    let config = sample_config();
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();
    let first = train(&mut wrapper, &config, 1024, 10, 1).unwrap();
    assert_eq!(first, 1);
    // +100 samples << RETRAIN_GROWTH (5000)
    insert_samples(&conn, 100);
    let err = train(&mut wrapper, &config, 1024, 10, 1).unwrap_err();
    assert!(
        err.to_string().contains("retrain skipped"),
        "unexpected error: {err}"
    );
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM raw._zstd_dicts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn train_retrains_when_growth_reaches_threshold() {
    const RETRAIN_GROWTH: i64 = 5000;
    let conn = Connection::open_in_memory().unwrap();
    setup_raw_table(&conn);
    let prior_row_count = 100_i64;
    let sample_count = (prior_row_count + RETRAIN_GROWTH) as usize;
    insert_samples(&conn, sample_count);
    let config = sample_config();
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();
    // Simulate a previous train with low row_count
    conn.execute(
        "INSERT INTO raw._zstd_dicts (id, dict, trained_at, row_count)
         VALUES (1, X'00', strftime('%s','now'), ?1)",
        [prior_row_count],
    )
    .unwrap();
    let dict_id = train(&mut wrapper, &config, 1024, 10, 1).unwrap();
    assert_eq!(dict_id, 2);
    let row_count: i64 = conn
        .query_row(
            "SELECT row_count FROM raw._zstd_dicts WHERE id = ?1",
            [dict_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(row_count, sample_count as i64);
}
