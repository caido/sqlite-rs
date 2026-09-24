use std::{hint::black_box, ptr, sync::Once};

use criterion::{criterion_group, criterion_main, Criterion};
use libsqlite3_sys::{sqlite3, sqlite3_auto_extension, sqlite3_close, sqlite3_open, SQLITE_OK};
use sqlite_compress::{sqlite3_compress_init, ExtensionState};
use sqlite_ffi::{first_value, SqlValue};

struct BenchDb(*mut sqlite3);

impl BenchDb {
    fn open() -> Self {
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite3_compress_init as *const (),
            )));
        });
        let mut db = ptr::null_mut();
        assert_eq!(
            unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) },
            SQLITE_OK
        );
        Self(db)
    }

    fn state(&self) -> ExtensionState {
        ExtensionState::from_db(self.0).unwrap()
    }
}

impl Drop for BenchDb {
    fn drop(&mut self) {
        unsafe { sqlite3_close(self.0) };
    }
}

fn insert(conn: &ExtensionState, kind: &str, at: Option<usize>) {
    const BODY_LEN: usize = 8 * 1024 * 1024;
    let mut body = vec![b'a'; BODY_LEN];
    if let Some(at) = at {
        body[at..at + 6].copy_from_slice(b"NEEDLE");
    }
    conn.as_ref()
        .execute(
            "INSERT INTO t(data, kind) VALUES (compress(?1, 'main', 't', 'data'), ?2)",
            &[SqlValue::Blob(body), SqlValue::Text(kind.to_string())],
        )
        .unwrap();
}

fn hit(conn: &ExtensionState, sql: &str) -> i64 {
    let rows = conn.as_ref().query(sql, &[]).unwrap();
    first_value(&rows).and_then(SqlValue::as_i64).unwrap()
}

fn bench_scan(c: &mut Criterion) {
    let db = BenchDb::open();
    let conn = db.state();
    conn.as_ref()
        .batch_execute("CREATE TABLE t(id INTEGER PRIMARY KEY, data BLOB, kind TEXT)")
        .unwrap();
    insert(&conn, "start", Some(0));
    insert(&conn, "middle", Some(4 * 1024 * 1024));
    insert(&conn, "end", Some(8 * 1024 * 1024 - 6));
    insert(&conn, "absent", None);

    let mut group = c.benchmark_group("clike_vs_like");
    group.sample_size(10);

    for (kind, like_pat, clike_pat, expect) in [
        ("start", "NEEDLE%", "NEEDLE", 1),
        ("middle", "%NEEDLE%", "NEEDLE", 1),
        ("end", "%NEEDLE", "NEEDLE", 1),
        ("absent", "%ABSENT%", "ABSENT", 0),
    ] {
        let like_sql = format!(
            "SELECT decompress(data, 'main') LIKE '{like_pat}' FROM t WHERE kind = '{kind}'"
        );
        let clike_sql = format!(
            "SELECT clike('main', 't', 'data', id, '{clike_pat}') FROM t WHERE kind = '{kind}'"
        );
        assert_eq!(hit(&conn, &like_sql), expect);
        assert_eq!(hit(&conn, &clike_sql), expect);

        group.bench_function(format!("decompress-like-{kind}"), |b| {
            b.iter(|| black_box(hit(&conn, &like_sql)))
        });
        group.bench_function(format!("clike-{kind}"), |b| {
            b.iter(|| black_box(hit(&conn, &clike_sql)))
        });
    }

    group.finish();
}

criterion_group!(benches, bench_scan);
criterion_main!(benches);
