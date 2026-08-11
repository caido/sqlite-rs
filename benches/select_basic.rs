mod dataset;

#[path = "utils.rs"]
mod utils;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use rusqlite::Connection;
use sqlite_compress::{compress, decompress, DEFAULT_LEVEL};

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
    conn.query_row("SELECT data FROM requests_raw WHERE id = 1", [], |row| {
        row.get(0)
    })
    .expect("select")
}

fn select_decompressed_sql(conn: &Connection) -> Vec<u8> {
    conn.query_row(
        "SELECT decompress(data) FROM requests_raw WHERE id = 1",
        [],
        |row| row.get(0),
    )
    .expect("select decompress()")
}

fn bench_select(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    for kind in utils::PAYLOAD_KINDS {
        for size in utils::SIZE_BUCKETS {
            let sample = utils::sample(kind, size);
            let label = utils::sample_label(kind, size);

            let compressed = compress(&sample, DEFAULT_LEVEL).expect("compress sample");
            utils::report_size("zstd", &label, sample.len(), compressed.len());

            let roundtrip = decompress(&compressed).expect("decompress sample");
            assert_eq!(roundtrip, sample);

            //Selection without decompression raw sqlite
            group.bench_function(BenchmarkId::new("raw_sqlite_select", &label), |b| {
                let conn = open_filled(&sample);
                b.iter(|| {
                    let blob = select_blob(&conn);
                    black_box(blob);
                });
            });

            //Selection with decompression rust sqlite_compress
            group.bench_function(BenchmarkId::new("rust_decompress_select", &label), |b| {
                let conn = open_filled(&compressed);
                b.iter(|| {
                    let blob = select_blob(&conn);
                    let out = decompress(black_box(&blob)).expect("decompress");
                    black_box(out);
                });
            });

            //Selection with decompression rust sqlite_compress with extension
            group.bench_function(
                BenchmarkId::new("rust_decompress_ext_select", &label),
                |b| {
                    let conn = utils::open_table_with_extension();
                    let ext_roundtrip = select_decompressed_sql(&conn);
                    assert_eq!(ext_roundtrip, sample);

                    b.iter(|| {
                        let out = select_decompressed_sql(&conn);
                        black_box(out);
                    });
                },
            );
        }
    }
    group.finish();
}

criterion_group!(benches, bench_select);
criterion_main!(benches);
