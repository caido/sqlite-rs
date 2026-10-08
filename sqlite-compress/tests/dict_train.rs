use sqlite_compress::{
    DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH, ExtensionState, Header, SchemaName, SetupColumn,
    SetupConfig, SetupTable, TableName, TrainOptions, TrainProgress, setup, train_all,
    train_by_column,
};

mod common;
use common::DEFAULT_MAX_SAMPLES;
use sqlite_ffi::{SqlValue, first_value};

pub fn compress_blob(state: &ExtensionState, data: &[u8]) -> Vec<u8> {
    let rows = state
        .as_ref()
        .query(
            "SELECT compress(?1, 'raw', 'requests_raw', 'data')",
            &[SqlValue::Blob(data.to_vec())],
        )
        .unwrap();
    first_value(&rows).unwrap().as_blob().unwrap().to_vec()
}

pub fn decompress_blob(state: &ExtensionState, blob: &[u8]) -> Vec<u8> {
    let rows = state
        .as_ref()
        .query(
            "SELECT decompress(?1, 'raw')",
            &[SqlValue::Blob(blob.to_vec())],
        )
        .unwrap();
    first_value(&rows).unwrap().as_blob().unwrap().to_vec()
}

pub fn header_dict_id(blob: &[u8]) -> u32 {
    Header::parse(blob).unwrap().0.dict_id.get()
}

#[test]
fn train_persists_a_new_dictionary() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
        )
        .unwrap();

    for i in 0..sample_count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        state
            .as_ref()
            .execute(
                "INSERT INTO raw.requests_raw (data) VALUES (?1) RETURNING id",
                &[SqlValue::Text(sample)],
            )
            .unwrap();
    }

    let config = SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                sample_count,
                DEFAULT_MAX_SAMPLES,
            )],
        )],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();

    let rows = state
        .as_ref()
        .query("SELECT id FROM raw.__compress_dicts", &[])
        .unwrap();

    let stored_id = first_value(&rows).unwrap().as_i64().unwrap();

    let dict_size = state
        .as_ref()
        .query("SELECT length(dict) FROM raw.__compress_dicts", &[])
        .unwrap();

    let dict_size = first_value(&dict_size).unwrap().as_i64().unwrap();

    assert_eq!(dict_ids.len(), 1);
    assert_eq!(stored_id, dict_ids[0].get() as i64);
    assert!(dict_size > 0);
}

#[test]
fn train_persists_dictionary_per_column() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
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

        state
            .as_ref()
            .execute(
                "INSERT INTO raw.requests_raw (data, headers) VALUES (?1, ?2)",
                &[SqlValue::Text(data), SqlValue::Text(headers)],
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
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![column("data"), column("headers")],
        )],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert_eq!(dict_ids.len(), 2);

    let columns = state
        .as_ref()
        .query(
            "SELECT column_name FROM raw.__compress_dicts \
         WHERE table_name = 'requests_raw' ORDER BY column_name",
            &[],
        )
        .unwrap();

    assert_eq!(
        columns,
        vec![
            vec![SqlValue::Text("data".into())],
            vec![SqlValue::Text("headers".into())],
        ]
    );

    for id in dict_ids {
        let rows = state
            .as_ref()
            .query(
                "SELECT COUNT(*) FROM raw.__compress_dicts WHERE id = ?1",
                &[SqlValue::Integer(id.get() as i64)],
            )
            .unwrap();

        let count = first_value(&rows).unwrap().as_i64().unwrap();

        assert_eq!(count, 1);
    }
}

#[test]
fn train_skips_column_without_enough_samples() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
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
        state
            .as_ref()
            .execute(
                "INSERT INTO raw.requests_raw (data) VALUES (?1) RETURNING id",
                &[SqlValue::Text(data)],
            )
            .unwrap();
    }

    let config = SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![
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
        )],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert_eq!(dict_ids.len(), 1);

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.__compress_dicts \
         WHERE table_name = 'requests_raw' AND column_name = 'data'",
            &[],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn train_by_column_persists_one_column() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
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
        state
            .as_ref()
            .execute(
                "INSERT INTO raw.requests_raw (data, headers) VALUES (?1, ?2)",
                &[SqlValue::Text(data), SqlValue::Text(headers)],
            )
            .unwrap();
    }

    let table = SetupTable::new(
        SchemaName::new("raw"),
        TableName::new("requests_raw"),
        vec![
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
    );

    let config = SetupConfig {
        tables: vec![table.clone()],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let mut options = TrainOptions::default();

    let dict_id = train_by_column(
        &db,
        &config.tables[0],
        &config.tables[0].columns[0],
        config.compression_level,
        1024,
        &mut options,
    )
    .unwrap();

    assert!(dict_id.is_some());
    assert_eq!(dict_id.unwrap().get(), 1);

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*) FROM raw.__compress_dicts \
         WHERE table_name = 'requests_raw' AND column_name = 'data'",
            &[],
        )
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

fn sample_config(min_samples: usize) -> SetupConfig {
    SetupConfig {
        tables: vec![SetupTable::new(
            SchemaName::new("raw"),
            TableName::new("requests_raw"),
            vec![SetupColumn::new(
                "data",
                DEFAULT_RETRAIN_GROWTH,
                min_samples,
                DEFAULT_MAX_SAMPLES,
            )],
        )],
        compression_level: DEFAULT_LEVEL,
    }
}

fn setup_raw_table(state: &ExtensionState) {
    state
        .as_ref()
        .batch_execute(
            "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
        )
        .unwrap();
}

fn insert_samples(state: &ExtensionState, count: usize) {
    for i in 0..count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        state
            .as_ref()
            .execute(
                "INSERT INTO raw.requests_raw (data) VALUES (?1) RETURNING id",
                &[SqlValue::Text(sample)],
            )
            .unwrap();
    }
}

#[test]
fn train_stores_row_count() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let state = db.state();

    setup_raw_table(&state);
    insert_samples(&state, sample_count);

    let config = sample_config(sample_count);

    setup(&db, &config).unwrap();

    let dict_ids = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();

    assert_eq!(dict_ids.len(), 1);

    let dict_id = dict_ids[0];

    let rows = state
        .as_ref()
        .query(
            "SELECT id FROM raw.__compress_dicts WHERE id = ?1",
            &[SqlValue::Integer(dict_id.get() as i64)],
        )
        .unwrap();

    let stored_id = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(stored_id, dict_id.get() as i64);

    let rows = state
        .as_ref()
        .query(
            "SELECT row_count FROM raw.__compress_dicts WHERE id = ?1",
            &[SqlValue::Integer(dict_id.get() as i64)],
        )
        .unwrap();

    let row_count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(row_count, sample_count as i64);
}

#[test]
fn train_skips_when_growth_below_threshold() {
    let sample_count = 64;

    let db = common::TestDb::open();
    let state = db.state();

    setup_raw_table(&state);
    insert_samples(&state, sample_count);

    let config = sample_config(sample_count);
    setup(&db, &config).unwrap();

    let first = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].get(), 1);

    // +100 samples << RETRAIN_GROWTH (5000)
    insert_samples(&state, 100);

    let second = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert!(second.is_empty());

    let rows = state
        .as_ref()
        .query("SELECT COUNT(*) FROM raw.__compress_dicts", &[])
        .unwrap();

    let count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(count, 1);
}

#[test]
fn train_retrains_when_growth_reaches_threshold() {
    let prior_row_count = 100_i64;
    let sample_count = (prior_row_count as usize) + DEFAULT_RETRAIN_GROWTH;

    let db = common::TestDb::open();
    let state = db.state();

    setup_raw_table(&state);
    insert_samples(&state, sample_count);

    let config = sample_config(sample_count);
    setup(&db, &config).unwrap();

    // Simulate a previous train with low row_count for this column
    let res = state.as_ref().batch_execute(&format!(
        "INSERT INTO raw.__compress_dicts \
         (id, dict, trained_at, table_name, column_name, row_count) \
         VALUES (1, X'00', strftime('%s','now'), \
         'requests_raw', 'data', {prior_row_count})"
    ));

    assert_eq!(res.unwrap(), 0);

    let dict_ids = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert_eq!(dict_ids.len(), 1);
    assert_eq!(dict_ids[0].get(), 2);

    let rows = state
        .as_ref()
        .query(
            "SELECT row_count FROM raw.__compress_dicts WHERE id = ?1",
            &[SqlValue::Integer(dict_ids[0].get() as i64)],
        )
        .unwrap();

    let row_count = first_value(&rows).unwrap().as_i64().unwrap();

    assert_eq!(row_count, sample_count as i64);
}

#[test]
fn train_all_marks_dict_as_current() {
    let prior_row_count = 100_i64;
    let sample_count = (prior_row_count as usize) + DEFAULT_RETRAIN_GROWTH;

    let db = common::TestDb::open();
    let state = db.state();

    setup_raw_table(&state);
    insert_samples(&state, sample_count);

    let config = sample_config(sample_count);
    setup(&db, &config).unwrap();

    state
        .as_ref()
        .batch_execute(&format!(
            "INSERT INTO raw.__compress_dicts \
             (id, dict, trained_at, table_name, column_name, row_count) \
             VALUES (1, X'00', strftime('%s','now'), \
             'requests_raw', 'data', {prior_row_count})"
        ))
        .unwrap();

    let dict_ids = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert_eq!(dict_ids.len(), 1);
    let new_id = dict_ids[0].get();
    assert_eq!(new_id, 2);

    let rows = state
        .as_ref()
        .query(
            "SELECT COUNT(*), MAX(id) FROM raw.__compress_dicts \
             WHERE table_name = 'requests_raw' AND column_name = 'data'",
            &[],
        )
        .unwrap();
    let count = rows[0][0].as_i64().unwrap();
    let max_id = rows[0][1].as_i64().unwrap();
    assert_eq!(count, 2);
    assert_eq!(max_id as u32, new_id);

    let compressed = compress_blob(
        &state,
        b"GET /api/users/0 HTTP/1.1\r\nHost: example.com\r\n\r\n",
    );
    assert_eq!(header_dict_id(&compressed), new_id);
}

#[test]
fn decompress_old_dict_after_retrain() {
    let sample_count = 64;
    let db = common::TestDb::open();
    let state = db.state();

    setup_raw_table(&state);
    insert_samples(&state, sample_count);

    let config = sample_config(sample_count);
    setup(&db, &config).unwrap();

    let first = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert_eq!(first[0].get(), 1);

    let original = b"GET /api/users/42 HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let blob_v1 = compress_blob(&state, original);
    assert_eq!(header_dict_id(&blob_v1), 1);

    state
        .as_ref()
        .batch_execute("UPDATE raw.__compress_dicts SET row_count = 1 WHERE id = 1")
        .unwrap();
    insert_samples(&state, DEFAULT_RETRAIN_GROWTH);

    let second = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    assert_eq!(second[0].get(), 2);

    let blob_v2 = compress_blob(&state, original);
    assert_eq!(header_dict_id(&blob_v2), 2);

    assert_eq!(decompress_blob(&state, &blob_v1), original);
    assert_eq!(decompress_blob(&state, &blob_v2), original);
}

#[test]
fn compress_uses_current_id_in_header() {
    let sample_count = 64;
    let db = common::TestDb::open();
    let state = db.state();

    setup_raw_table(&state);
    insert_samples(&state, sample_count);

    let config = sample_config(sample_count);
    setup(&db, &config).unwrap();

    let first = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    let id1 = first[0].get();

    let payload = b"GET /api/users/7 HTTP/1.1\r\nHost: example.com\r\n\r\n";
    let blob = compress_blob(&state, payload);
    assert_eq!(header_dict_id(&blob), id1);
    assert_eq!(decompress_blob(&state, &blob), payload);

    // Retraîn → header suit le nouveau current
    state
        .as_ref()
        .batch_execute(&format!(
            "UPDATE raw.__compress_dicts SET row_count = {} WHERE id = {}",
            sample_count as i64 - DEFAULT_RETRAIN_GROWTH as i64,
            id1
        ))
        .unwrap();
    insert_samples(&state, DEFAULT_RETRAIN_GROWTH);

    let second = train_all(&db, &config, 1024, TrainOptions::default()).unwrap();
    let id2 = second[0].get();
    assert_ne!(id2, id1);

    let blob2 = compress_blob(&state, payload);
    assert_eq!(header_dict_id(&blob2), id2);
}

#[test]
fn train_by_column_reports_progress() {
    let sample_count = 64;
    let max_samples = 64;
    let progress_every = 10;

    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
        )
        .unwrap();

    for i in 0..sample_count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        state
            .as_ref()
            .execute(
                "INSERT INTO raw.requests_raw (data) VALUES (?1)",
                &[SqlValue::Text(sample)],
            )
            .unwrap();
    }

    let column = SetupColumn::new("data", DEFAULT_RETRAIN_GROWTH, sample_count, max_samples);

    let table = SetupTable::new(
        SchemaName::new("raw"),
        TableName::new("requests_raw"),
        vec![column.clone()],
    );

    let config = SetupConfig {
        tables: vec![table.clone()],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let mut reports = Vec::new();
    let mut options = TrainOptions {
        cancel: None,
        progress_every,
        on_progress: Some(|p: TrainProgress| {
            reports.push((p.samples_done, p.samples_target));
        }),
    };

    let dict_id = train_by_column(
        &db,
        &table,
        &column,
        config.compression_level,
        1024,
        &mut options,
    )
    .unwrap();

    assert!(dict_id.is_some());
    assert!(!reports.is_empty());
    assert!(reports.iter().all(|&(_, target)| target == max_samples));
    assert!(
        reports
            .iter()
            .all(|&(done, _)| done > 0 && done.is_multiple_of(progress_every))
    );
    assert!(reports.windows(2).all(|w| w[0].0 <= w[1].0));
}

#[test]
fn train_cancelled_from_another_thread() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
    };

    use sqlite_compress::SetupError;

    let sample_count = 64;
    let max_samples = 64;

    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (id INTEGER PRIMARY KEY, data BLOB);
        ",
        )
        .unwrap();

    for i in 0..sample_count {
        let sample = format!("GET /api/users/{i} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        state
            .as_ref()
            .execute(
                "INSERT INTO raw.requests_raw (data) VALUES (?1)",
                &[SqlValue::Text(sample)],
            )
            .unwrap();
    }

    let table = SetupTable::new(
        SchemaName::new("raw"),
        TableName::new("requests_raw"),
        vec![SetupColumn::new(
            "data",
            DEFAULT_RETRAIN_GROWTH,
            sample_count,
            max_samples,
        )],
    );

    let config = SetupConfig {
        tables: vec![table],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_setter = Arc::clone(&cancel);
    let (started_tx, started_rx) = std::sync::mpsc::channel();

    let handle = thread::spawn(move || {
        started_rx.recv().unwrap();
        cancel_setter.store(true, Ordering::Relaxed);
    });

    let result = train_all(
        &db,
        &config,
        1024,
        TrainOptions {
            cancel: Some(cancel),
            progress_every: 1,
            on_progress: Some(move |_| {
                let _ = started_tx.send(());
            }),
        },
    );

    handle.join().unwrap();
    assert!(matches!(result, Err(SetupError::Cancelled)));
}
