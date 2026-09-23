use sqlite_compress::{
    prune, setup, train_by_column, ExtensionState, SchemaName, SetupColumn, SetupConfig,
    SetupTable, TableName, DEFAULT_LEVEL,
};
use sqlite_ffi::{first_value, SqlValue};

use crate::common::{compress_blob, decompress_blob, header_dict_id};

mod common;

const PAGE_SIZE: usize = 100;
const GROUPS: usize = 5;
const PER_GROUP: usize = PAGE_SIZE / GROUPS + 1;

struct PruneFixture {
    db: common::TestDb,
    plaintexts: Vec<Vec<u8>>,
    dict_ids: Vec<u32>,
}

fn init() -> PruneFixture {
    let db = common::TestDb::open();
    let state = db.state();

    state
        .as_ref()
        .batch_execute(
            "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB
        );
        ",
        )
        .unwrap();

    let table = SetupTable {
        name: TableName::new("requests_raw"),
        schema: SchemaName::new("raw"),
        columns: vec![SetupColumn::new(
            "data",
            PER_GROUP,
            PER_GROUP,
            common::DEFAULT_MAX_SAMPLES,
        )],
    };
    let column = table.columns[0].clone();
    let config = SetupConfig {
        tables: vec![table.clone()],
        compression_level: DEFAULT_LEVEL,
    };

    setup(&db, &config).unwrap();

    let mut plaintexts = Vec::with_capacity(GROUPS * PER_GROUP);
    let mut dict_ids = Vec::with_capacity(GROUPS);

    for group in 0..GROUPS {
        let mut pending = Vec::with_capacity(PER_GROUP);

        for index in 0..PER_GROUP {
            let plaintext = request_payload(group, index);
            let id = state
                .as_ref()
                .execute(
                    "INSERT INTO raw.requests_raw (data) VALUES (?1) RETURNING id",
                    &[SqlValue::Blob(plaintext.clone())],
                )
                .unwrap();
            pending.push((id, plaintext));
        }

        let dict_id = train_by_column(&db, &table, &column, DEFAULT_LEVEL, 256)
            .unwrap()
            .expect("retrain should persist a new dictionary")
            .get();
        dict_ids.push(dict_id);

        for (id, plaintext) in pending {
            let compressed = compress_blob(&state, &plaintext);
            assert_eq!(header_dict_id(&compressed), dict_id);

            state
                .as_ref()
                .execute(
                    "UPDATE raw.requests_raw SET data = ?1 WHERE id = ?2",
                    &[SqlValue::Blob(compressed), SqlValue::Integer(id)],
                )
                .unwrap();
            plaintexts.push(plaintext);
        }
    }

    let fixture = PruneFixture {
        db,
        plaintexts,
        dict_ids,
    };
    assert_groups_use_distinct_dicts(&fixture);
    fixture
}

fn request_payload(group: usize, index: usize) -> Vec<u8> {
    format!(
        "GET /api/users/{group}/{index} HTTP/1.1\r\n\
         Host: example.com\r\n\
         User-Agent: sqlite-compress-prune\r\n\
         Accept: application/json\r\n\r\n\
         {{\"group\":{group},\"index\":{index}}}"
    )
    .into_bytes()
}

fn assert_groups_use_distinct_dicts(fixture: &PruneFixture) {
    let state = fixture.db.state();
    let rows = stored_data(&state);

    assert!(rows.len() > PAGE_SIZE);
    assert_eq!(rows.len(), GROUPS * PER_GROUP);
    assert_eq!(fixture.dict_ids.len(), GROUPS);
    assert_eq!(dict_row_count(&state), GROUPS as i64);

    for (group, dict_id) in fixture.dict_ids.iter().enumerate() {
        let start = group * PER_GROUP;
        for blob in &rows[start..start + PER_GROUP] {
            assert_eq!(header_dict_id(blob), *dict_id);
        }
    }
}

fn stored_data(state: &ExtensionState) -> Vec<Vec<u8>> {
    state
        .as_ref()
        .query("SELECT data FROM raw.requests_raw ORDER BY id", &[])
        .unwrap()
        .into_iter()
        .map(|row| row[0].as_blob().unwrap().to_vec())
        .collect()
}

fn dict_row_count(state: &ExtensionState) -> i64 {
    let rows = state
        .as_ref()
        .query("SELECT COUNT(*) FROM raw.__compress_dicts", &[])
        .unwrap();
    first_value(&rows).unwrap().as_i64().unwrap()
}

fn assert_pruned_to_latest_dict(fixture: &PruneFixture) {
    let state = fixture.db.state();
    let latest = *fixture.dict_ids.last().unwrap();

    assert_eq!(dict_row_count(&state), 1);

    let rows = state
        .as_ref()
        .query("SELECT id FROM raw.__compress_dicts", &[])
        .unwrap();
    let remaining = first_value(&rows).unwrap().as_i64().unwrap();
    assert_eq!(remaining, latest as i64);

    let stored = stored_data(&state);
    assert_eq!(stored.len(), fixture.plaintexts.len());

    for (blob, plaintext) in stored.iter().zip(&fixture.plaintexts) {
        assert_eq!(header_dict_id(blob), latest);
        assert_eq!(decompress_blob(&state, blob), *plaintext);
    }
}

#[test]
fn prune_all_keeps_one_dict_and_rewrites_every_row() {
    let fixture = init();

    prune(&fixture.db, "raw").unwrap();

    assert_pruned_to_latest_dict(&fixture);
}
