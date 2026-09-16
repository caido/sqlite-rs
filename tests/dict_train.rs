use sqlite_compress::{
    setup, train_all, train_by_column, Connection, SchemaName, SetupColumn, SetupConfig,
    SetupTable, TableName, DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH,
};

mod common;
use common::DEFAULT_MAX_SAMPLES;

#[test]
fn train_persists_a_new_dictionary() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let conn = db.conn();

    conn.batch_execute(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    );

    for i in 0..sample_count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute_blob(
            "INSERT INTO raw.requests_raw (data) VALUES (?1) RETURNING id",
            sample.as_bytes(),
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

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024).unwrap();

    let stored_id = conn
        .query_i64("SELECT id FROM raw.__compress_dicts")
        .unwrap() as u32;

    let dict_size = conn
        .query_i64("SELECT length(dict) FROM raw.__compress_dicts")
        .unwrap() as usize;

    assert_eq!(dict_ids.len(), 1);
    assert_eq!(stored_id, dict_ids[0].get());
    assert!(dict_size > 0);
}

#[test]
fn train_persists_dictionary_per_column() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let conn = db.conn();

    conn.batch_execute(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB,
            headers BLOB
        );
        ",
    );

    for i in 0..sample_count {
        let data = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        let headers = format!("content-type: application/json\r\nx-request-id: {i}\r\n");

        conn.execute_blobs(
            "INSERT INTO raw.requests_raw (data, headers) VALUES (?1, ?2)",
            &[data.as_bytes(), headers.as_bytes()],
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

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 2);

    let columns = conn
        .query_strings(
            "SELECT column_name FROM raw.__compress_dicts \
         WHERE table_name = 'requests_raw' ORDER BY column_name",
        )
        .unwrap();

    assert_eq!(columns, ["data", "headers"]);
    for id in dict_ids {
        assert_eq!(
            conn.query_i64(&format!(
                "SELECT COUNT(*) FROM raw.__compress_dicts WHERE id = {}",
                id.get()
            ))
            .unwrap(),
            1
        );
    }
}

#[test]
fn train_skips_column_without_enough_samples() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let conn = db.conn();

    conn.batch_execute(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB,
            headers BLOB
        );
        ",
    );

    // `data` has enough samples; `headers` stays NULL → not enough
    for i in 0..sample_count {
        let data = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute_blob(
            "INSERT INTO raw.requests_raw (data) VALUES (?1)",
            data.as_bytes(),
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

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 1);

    let count = conn
        .query_i64(
            "SELECT COUNT(*) FROM raw.__compress_dicts \
         WHERE table_name = 'requests_raw' AND column_name = 'data'",
        )
        .unwrap();

    assert_eq!(count, 1);
}

#[test]
fn train_by_column_persists_one_column() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let conn = db.conn();

    conn.batch_execute(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB,
            headers BLOB
        );
        ",
    );

    for i in 0..sample_count {
        let data = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        let headers = format!("content-type: application/json\r\nx-request-id: {i}\r\n");
        conn.execute_blobs(
            "INSERT INTO raw.requests_raw (data, headers) VALUES (?1, ?2)",
            &[data.as_bytes(), headers.as_bytes()],
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

    setup(&db, &config).unwrap();

    let dict_id = train_by_column(
        &db,
        &config.tables[0],
        &config.tables[0].columns[0],
        config.compression_level,
        1024,
    )
    .unwrap();

    assert!(dict_id.is_some());
    assert_eq!(dict_id.unwrap().get(), 1);

    let count = conn
        .query_i64(
            "SELECT COUNT(*) FROM raw.__compress_dicts \
         WHERE table_name = 'requests_raw' AND column_name = 'data'",
        )
        .unwrap();

    assert_eq!(count, 1);
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
    conn.batch_execute(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    );
}

fn insert_samples(conn: &Connection, count: usize) {
    for i in 0..count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute_blob(
            "INSERT INTO raw.requests_raw (data) VALUES (?1) RETURNING id",
            sample.as_bytes(),
        )
        .unwrap();
    }
}

#[test]
fn train_stores_row_count() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let conn = db.conn();

    setup_raw_table(&conn);
    insert_samples(&conn, sample_count);

    let config = sample_config(sample_count);

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024).unwrap();

    assert_eq!(dict_ids.len(), 1);

    let dict_id = dict_ids[0];

    let stored_id = conn
        .query_i64(&format!(
            "SELECT id FROM raw.__compress_dicts WHERE id = {}",
            dict_id.get()
        ))
        .unwrap();

    assert_eq!(stored_id, dict_id.get() as i64);

    let row_count = conn
        .query_i64(&format!(
            "SELECT row_count FROM raw.__compress_dicts WHERE id = {}",
            dict_id.get()
        ))
        .unwrap();

    assert_eq!(row_count, sample_count as i64);
}

#[test]
fn train_skips_when_growth_below_threshold() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let conn = db.conn();

    setup_raw_table(&conn);
    insert_samples(&conn, sample_count);

    let config = sample_config(sample_count);
    setup(&db, &config).unwrap();

    let first = train_all(&db, &config, 1024).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].get(), 1);

    // +100 samples << RETRAIN_GROWTH (5000)
    insert_samples(&conn, 100);

    let second = train_all(&db, &config, 1024).unwrap();
    assert!(second.is_empty());

    let count: i64 = conn
        .query_i64("SELECT COUNT(*) FROM raw.__compress_dicts")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn train_retrains_when_growth_reaches_threshold() {
    let prior_row_count = 100_i64;
    let sample_count = (prior_row_count as usize) + DEFAULT_RETRAIN_GROWTH;

    let db = common::TestDb::open();
    let conn = db.conn();

    setup_raw_table(&conn);
    insert_samples(&conn, sample_count);

    let config = sample_config(sample_count);
    setup(&db, &config).unwrap();

    // Simulate a previous train with low row_count for this column
    assert_eq!(
        conn.batch_execute(&format!(
            "INSERT INTO raw.__compress_dicts \
         (id, dict, trained_at, table_name, column_name, row_count) \
         VALUES (1, X'00', strftime('%s','now'), \
         'requests_raw', 'data', {prior_row_count})"
        )),
        0
    );

    let dict_ids = train_all(&db, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 1);
    assert_eq!(dict_ids[0].get(), 2);

    let row_count = conn
        .query_i64(&format!(
            "SELECT row_count FROM raw.__compress_dicts WHERE id = {}",
            dict_ids[0].get()
        ))
        .unwrap();

    assert_eq!(row_count, sample_count as i64);
}

#[test]
fn train_all_marks_dict_as_current() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let conn = db.conn();

    conn.batch_execute(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
    );

    for i in 0..sample_count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        conn.execute_blob(
            "INSERT INTO raw.requests_raw (data) VALUES (?1) RETURNING id",
            sample.as_bytes(),
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

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024).unwrap();
    assert_eq!(dict_ids.len(), 1);

    let current = conn
        .query_i64(
            "SELECT id FROM raw.__compress_dicts \
         WHERE table_name = 'requests_raw' \
         AND column_name = 'data' AND is_current = 1",
        )
        .unwrap();

    assert_eq!(current as u32, dict_ids[0].get());
}
