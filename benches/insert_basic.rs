mod dataset;

use std::sync::atomic::Ordering;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use sqlite_compress::{compress, DEFAULT_LEVEL, LATEST_DICT_ID};

#[path = "utils.rs"]
mod utils;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    for kind in utils::PAYLOAD_KINDS {
        for size in utils::SIZE_BUCKETS {
            let sample = utils::sample(kind, size);

            LATEST_DICT_ID.store(0, Ordering::Relaxed);
            let raw = compress(&sample, DEFAULT_LEVEL).expect("compress raw");
            assert_eq!(u32::from_le_bytes(raw[..4].try_into().unwrap()), 0);

            let dict_id = utils::setup_trained_dict(kind, size);

            let label = utils::sample_label(kind, size);

            // Default sqlite insertion with no compression
            group.bench_function(BenchmarkId::new("raw_sqlite_insert", &label), |b| {
                b.iter_batched(
                    utils::open_table,
                    |conn| {
                        conn.execute(
                            "INSERT INTO requests_raw (data) VALUES (?1)",
                            [black_box(sample.as_slice())],
                        )
                        .expect("insert");
                    },
                    criterion::BatchSize::SmallInput,
                );
            });

            // Insertion with rust sqlite_compress, no dictionary
            group.bench_function(BenchmarkId::new("rust_compress_and_insert", &label), |b| {
                LATEST_DICT_ID.store(0, Ordering::Relaxed);
                b.iter_batched(
                    utils::open_table,
                    |conn| {
                        let compressed = compress(black_box(sample.as_slice()), DEFAULT_LEVEL)
                            .expect("compress");
                        conn.execute(
                            "INSERT INTO requests_raw (data) VALUES (?1)",
                            [black_box(compressed.as_slice())],
                        )
                        .expect("insert");
                    },
                    criterion::BatchSize::SmallInput,
                );
            });

            // Insertion with rust sqlite_compress + dictionary
            group.bench_function(BenchmarkId::new("rust_compress_dict_insert", &label), |b| {
                LATEST_DICT_ID.store(dict_id.get(), Ordering::Relaxed);
                b.iter_batched(
                    utils::open_table,
                    |conn| {
                        let compressed = compress(black_box(sample.as_slice()), DEFAULT_LEVEL)
                            .expect("compress");
                        debug_assert_eq!(
                            u32::from_le_bytes(compressed[..4].try_into().unwrap()),
                            dict_id.get()
                        );
                        conn.execute(
                            "INSERT INTO requests_raw (data) VALUES (?1)",
                            [black_box(compressed.as_slice())],
                        )
                        .expect("insert");
                    },
                    criterion::BatchSize::SmallInput,
                );
            });

            // Insertion using compress() SQL function via loaded cdylib
            group.bench_function(BenchmarkId::new("compress_ext_insert", &label), |b| {
                let conn = utils::open_table_with_extension();
                b.iter_batched(
                    || {
                        conn.execute("DELETE FROM requests_raw", [])
                            .expect("reset table");
                    },
                    |()| {
                        conn.execute(
                            "INSERT INTO requests_raw (data) VALUES (compress(?1))",
                            [black_box(sample.as_slice())],
                        )
                        .expect("insert via extension");
                    },
                    criterion::BatchSize::SmallInput,
                );
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench_insert);
criterion_main!(benches);
