//! Select benchmarks categorized by operation.
//!
//! ```text
//! cargo bench --bench select_basic
//! ```

mod dataset;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use dataset::{generate_sample, PayloadKind, SizeBucket};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusqlite::Connection;
use sqlite_compress::{compress, decompress, DEFAULT_LEVEL};

fn json_sample() -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(42);
    generate_sample(&mut rng, 0, PayloadKind::Json, SizeBucket::Small)
}

fn open_filled(data: &[u8]) -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    conn.execute_batch(
        "CREATE TABLE requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        );",
    )
    .expect("create table");
    conn.execute("INSERT INTO requests_raw (id, data) VALUES (1, ?1)", [data])
        .expect("seed row");
    conn
}

fn select_blob(conn: &Connection) -> Vec<u8> {
    conn.query_row(
        "SELECT data FROM requests_raw WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .expect("select")
}

fn bench_select(c: &mut Criterion) {
    let sample = json_sample();
    let compressed = compress(&sample, DEFAULT_LEVEL).expect("compress sample");
    eprintln!(
        "sample: kind=json bucket=small raw={} compressed={} ratio={:.2}",
        sample.len(),
        compressed.len(),
        compressed.len() as f64 / sample.len() as f64
    );

    let roundtrip = decompress(&compressed).expect("decompress sample");
    assert_eq!(roundtrip, sample);

    let mut group = c.benchmark_group("select");

    group.bench_function(BenchmarkId::new("sql", "json_small"), |b| {
        let conn = open_filled(&sample);
        b.iter(|| {
            let blob = select_blob(&conn);
            black_box(blob);
        });
    });

    group.bench_function(BenchmarkId::new("decompress", "json_small"), |b| {
        let conn = open_filled(&compressed);
        b.iter(|| {
            let blob = select_blob(&conn);
            let out = decompress(black_box(&blob)).expect("decompress");
            black_box(out);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_select);
criterion_main!(benches);
