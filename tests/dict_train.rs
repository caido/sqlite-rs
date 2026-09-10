use rusqlite::{params, Connection};
use sqlite_compress::{
    setup, train_all, train_by_column, SchemaName, SetupColumn, SetupConfig, SetupTable, TableName,
    DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH,
};

mod common;
use common::DEFAULT_MAX_SAMPLES;

#[test]
fn train_persists_a_new_dictionary() {
    let sample_count = 64;
    let conn = common::open_connection();
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    )
    .unwrap();

    for i in 0..sample_count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute(
            "INSERT INTO raw.requests_raw (data) VALUES (?1)",
            params![sample.as_bytes()],
        )
        .unwrap();
    }

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                sample_count,
                DEFAULT_MAX_SAMPLES,
            )],
        }],
        compression_level: DEFAULT_LEVEL,
    };
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let dict_ids = train_all(&mut wrapper, &config, 1024).unwrap();

    let (stored_id, dict_size): (u32, usize) = conn
        .query_row(
            "SELECT id, length(dict) FROM raw.__compress_dicts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(dict_ids.len(), 1);
    assert_eq!(stored_id, dict_ids[0].get());
    assert!(dict_size > 0);
}

#[test]
fn train_persists_dictionary_per_column() {
    let sample_count = 64;
    let conn = common::open_connection();
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB,
            headers BLOB
        );
        ",
    )
    .unwrap();

    for i in 0..sample_count {
        let data = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        let headers = format!("content-type: application/json\r\nx-request-id: {i}\r\n");
        conn.execute(
            "INSERT INTO raw.requests_raw (data, headers) VALUES (?1, ?2)",
            params![data.as_bytes(), headers.as_bytes()],
        )
        .unwrap();
    }

    let column = |name: &str| {
        SetupColumn::new(
            name,
            DEFAULT_RETRAIN_GROWTH,
            sample_count,
            DEFAULT_MAX_SAMPLES,
        )
    };

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![column("data"), column("headers")],
        }],
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let dict_ids = train_all(&mut wrapper, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 2);

    let rows: Vec<(String, String, u32)> = conn
        .prepare(
            "SELECT table_name, column_name, id FROM raw.__compress_dicts \
             ORDER BY column_name",
        )
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "requests_raw");
    assert_eq!(rows[0].1, "data");
    assert_eq!(rows[1].0, "requests_raw");
    assert_eq!(rows[1].1, "headers");

    let stored_ids: Vec<u32> = rows.iter().map(|(_, _, id)| *id).collect();
    assert!(stored_ids.contains(&dict_ids[0].get()));
    assert!(stored_ids.contains(&dict_ids[1].get()));
}

#[test]
fn train_skips_column_without_enough_samples() {
    let sample_count = 64;
    let conn = common::open_connection();
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB,
            headers BLOB
        );
        ",
    )
    .unwrap();

    // `data` has enough samples; `headers` stays NULL → not enough
    for i in 0..sample_count {
        let data = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute(
            "INSERT INTO raw.requests_raw (data) VALUES (?1)",
            params![data.as_bytes()],
        )
        .unwrap();
    }

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![
                SetupColumn::new(
                    "data",
                    DEFAULT_RETRAIN_GROWTH,
                    sample_count,
                    DEFAULT_MAX_SAMPLES,
                ),
                SetupColumn::new(
                    "headers",
                    DEFAULT_RETRAIN_GROWTH,
                    sample_count,
                    DEFAULT_MAX_SAMPLES,
                ),
            ],
        }],
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let dict_ids = train_all(&mut wrapper, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 1);

    let rows: Vec<(String, String)> = conn
        .prepare("SELECT table_name, column_name FROM raw.__compress_dicts")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    assert_eq!(rows, vec![("requests_raw".into(), "data".into())]);
}

#[test]
fn train_by_column_persists_one_column() {
    let sample_count = 64;
    let conn = common::open_connection();
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB,
            headers BLOB
        );
        ",
    )
    .unwrap();

    for i in 0..sample_count {
        let data = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        let headers = format!("content-type: application/json\r\nx-request-id: {i}\r\n");
        conn.execute(
            "INSERT INTO raw.requests_raw (data, headers) VALUES (?1, ?2)",
            params![data.as_bytes(), headers.as_bytes()],
        )
        .unwrap();
    }

    let table = SetupTable {
        name: TableName::new("requests_raw"),
        schema: SchemaName::new("raw"),
        columns: vec![
            SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                sample_count,
                DEFAULT_MAX_SAMPLES,
            ),
            SetupColumn::new(
                "headers",
                DEFAULT_RETRAIN_GROWTH,
                sample_count,
                DEFAULT_MAX_SAMPLES,
            ),
        ],
    };

    let config = SetupConfig {
        tables: vec![table.clone()],
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let dict_id = train_by_column(
        &mut wrapper,
        &config.tables[0],
        &config.tables[0].columns[0],
        config.compression_level,
        1024,
    )
    .unwrap();

    assert!(dict_id.is_some());
    assert_eq!(dict_id.unwrap().get(), 1);

    let rows: Vec<(String, String)> = conn
        .prepare("SELECT table_name, column_name FROM raw.__compress_dicts")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();

    // only the targeted column was trained
    assert_eq!(rows, vec![("requests_raw".into(), "data".into())]);
}

fn sample_config(min_samples: usize) -> SetupConfig {
    SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                min_samples,
                DEFAULT_MAX_SAMPLES,
            )],
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
fn train_stores_row_count() {
    let sample_count = 64;
    let conn = common::open_connection();

    setup_raw_table(&conn);
    insert_samples(&conn, sample_count);

    let config = sample_config(sample_count);
    let mut wrapper = common::RusqliteConn::new(&conn);

    setup(&mut wrapper, &config).unwrap();

    let dict_ids = train_all(&mut wrapper, &config, 1024).unwrap();

    assert_eq!(dict_ids.len(), 1);

    let dict_id = dict_ids[0];
    let (stored_id, row_count): (u32, i64) = conn
        .query_row(
            "SELECT id, row_count FROM raw.__compress_dicts WHERE id = ?1",
            [dict_id.get()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(stored_id, dict_id.get());
    assert_eq!(row_count, sample_count as i64);
}

#[test]
fn train_skips_when_growth_below_threshold() {
    let sample_count = 64;
    let conn = common::open_connection();

    setup_raw_table(&conn);
    insert_samples(&conn, sample_count);

    let config = sample_config(sample_count);
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let first = train_all(&mut wrapper, &config, 1024).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].get(), 1);

    // +100 samples << RETRAIN_GROWTH (5000)
    insert_samples(&conn, 100);

    let second = train_all(&mut wrapper, &config, 1024).unwrap();
    assert!(second.is_empty());

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM raw.__compress_dicts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn train_retrains_when_growth_reaches_threshold() {
    let prior_row_count = 100_i64;
    let sample_count = (prior_row_count as usize) + DEFAULT_RETRAIN_GROWTH;

    let conn = common::open_connection();
    setup_raw_table(&conn);
    insert_samples(&conn, sample_count);

    let config = sample_config(sample_count);
    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    // Simulate a previous train with low row_count for this column
    conn.execute(
        "INSERT INTO raw.__compress_dicts \
         (id, dict, trained_at, table_name, column_name, row_count) \
         VALUES (1, X'00', strftime('%s','now'), 'requests_raw', 'data', ?1)",
        [prior_row_count],
    )
    .unwrap();

    let dict_ids = train_all(&mut wrapper, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 1);
    assert_eq!(dict_ids[0].get(), 2);

    let row_count: i64 = conn
        .query_row(
            "SELECT row_count FROM raw.__compress_dicts WHERE id = ?1",
            [dict_ids[0].get()],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(row_count, sample_count as i64);
}

#[test]
fn train_all_marks_dict_as_current() {
    let sample_count = 64;
    let conn = common::open_connection();
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    )
    .unwrap();

    for i in 0..sample_count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute(
            "INSERT INTO raw.requests_raw (data) VALUES (?1)",
            params![sample.as_bytes()],
        )
        .unwrap();
    }

    let config = SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                sample_count,
                DEFAULT_MAX_SAMPLES,
            )],
        }],
        compression_level: DEFAULT_LEVEL,
    };

    let mut wrapper = common::RusqliteConn::new(&conn);
    setup(&mut wrapper, &config).unwrap();

    let dict_ids = train_all(&mut wrapper, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 1);

    let current: i64 = conn
        .query_row(
            "SELECT id FROM raw.__compress_dicts \
             WHERE table_name = 'requests_raw' AND column_name = 'data' AND is_current = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(current as u32, dict_ids[0].get());
}
