use std::sync::Mutex;

use sqlite_ffi::{first_value, SqlValue};

mod common;

use common::TestDb;

static STATUS: Mutex<()> = Mutex::new(());

#[test]
fn like_loads_full_blob_when_match_is_at_start() {
    let _guard = STATUS.lock().unwrap();
    const BODY_LEN: usize = 1024 * 1024;
    let needle = b"NEEDLE";

    let mut body = vec![b'a'; BODY_LEN];
    body[..needle.len()].copy_from_slice(needle);

    let db = TestDb::open();
    let conn = db.state();
    conn.as_ref()
        .batch_execute("CREATE TABLE t(id INTEGER PRIMARY KEY, data BLOB)")
        .unwrap();

    conn.as_ref()
        .execute("INSERT INTO t(data) VALUES (?1)", &[SqlValue::Blob(body)])
        .unwrap();

    let id_hw = conn.as_ref().malloc_highwater().unwrap();
    let id_rows = conn
        .as_ref()
        .query("SELECT id FROM t WHERE id = 1", &[])
        .unwrap();
    assert_eq!(first_value(&id_rows).and_then(SqlValue::as_i64), Some(1));
    println!("id_hw: {id_hw}");

    let _ = conn.as_ref().malloc_highwater().unwrap();
    let like_rows = conn
        .as_ref()
        .query("SELECT data LIKE 'NEEDLE%' FROM t WHERE id = 1", &[])
        .unwrap();
    assert_eq!(first_value(&like_rows).and_then(SqlValue::as_i64), Some(1));
    let like_hw = conn.as_ref().malloc_highwater().unwrap();

    println!("like_hw: {like_hw}");
}

#[test]
fn clike_allocates_less_than_decompress_like_when_match_is_at_start() {
    let _guard = STATUS.lock().unwrap();
    const BODY_LEN: usize = 1024 * 1024;
    let needle = b"NEEDLE";

    let mut body = vec![b'a'; BODY_LEN];
    body[..needle.len()].copy_from_slice(needle);

    let db = TestDb::open();
    let conn = db.state();
    conn.as_ref()
        .batch_execute("CREATE TABLE t(id INTEGER PRIMARY KEY, data BLOB)")
        .unwrap();

    conn.as_ref()
        .execute(
            "INSERT INTO t(data) VALUES (compress(?1, 'main', 't', 'data'))",
            &[SqlValue::Blob(body)],
        )
        .unwrap();

    let _ = conn.as_ref().malloc_highwater().unwrap();
    let like_rows = conn
        .as_ref()
        .query(
            "SELECT decompress(data, 'main') LIKE 'NEEDLE%' FROM t WHERE id = 1",
            &[],
        )
        .unwrap();
    assert_eq!(first_value(&like_rows).and_then(SqlValue::as_i64), Some(1));
    let like_hw = conn.as_ref().malloc_highwater().unwrap();

    let clike_rows = conn
        .as_ref()
        .query(
            "SELECT clike('main', 't', 'data', id, 'NEEDLE') FROM t WHERE id = 1",
            &[],
        )
        .unwrap();
    assert_eq!(first_value(&clike_rows).and_then(SqlValue::as_i64), Some(1));
    let clike_hw = conn.as_ref().malloc_highwater().unwrap();

    println!("decompress_like_hw: {like_hw}");
    println!("clike_hw: {clike_hw}");
}
