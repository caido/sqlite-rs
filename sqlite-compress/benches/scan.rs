use std::{hint::black_box, iter::repeat_n, ptr, sync::Once};

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

const BODY_LEN: usize = 8 * 1024 * 1024;

fn needle(len: usize) -> String {
    let mut s = String::from("NEEDLE");
    s.extend(repeat_n('X', len - s.len()));
    s
}

fn insert(conn: &ExtensionState, kind: &str, needle: &str, at: Option<usize>) {
    let mut body = vec![b'a'; BODY_LEN];
    if let Some(at) = at {
        body[at..at + needle.len()].copy_from_slice(needle.as_bytes());
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

    let mut group = c.benchmark_group("clike_vs_like");
    group.sample_size(10);

    for len in [6, 64, 256, 1024] {
        let needle = needle(len);
        for (place, at) in [
            ("start", Some(0)),
            ("middle", Some(BODY_LEN / 2)),
            ("end", Some(BODY_LEN - needle.len())),
            ("absent", None),
        ] {
            insert(&conn, &format!("{place}-{len}"), &needle, at);
        }

        for (place, pattern, expect) in [
            ("start", format!("{needle}%"), 1),
            ("middle", format!("%{needle}%"), 1),
            ("end", format!("%{needle}"), 1),
            ("absent", format!("%{needle}%"), 0),
        ] {
            let kind = format!("{place}-{len}");
            let like_sql = format!(
                "SELECT decompress(data, 'main') LIKE '{pattern}' FROM t WHERE kind = '{kind}'"
            );
            let clike_sql = format!(
                "SELECT clike('main', 't', 'data', id, '{pattern}') FROM t WHERE kind = '{kind}'"
            );
            assert_eq!(hit(&conn, &like_sql), expect, "{kind}");
            assert_eq!(hit(&conn, &clike_sql), expect, "{kind}");

            group.bench_function(format!("decompress-like-{place}-{len}"), |b| {
                b.iter(|| black_box(hit(&conn, &like_sql)))
            });
            group.bench_function(format!("clike-{place}-{len}"), |b| {
                b.iter(|| black_box(hit(&conn, &clike_sql)))
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_scan);
criterion_main!(benches);
