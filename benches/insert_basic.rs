mod dataset;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use sqlite_compress::{compress, Header, CURRENT_DICT_IDS, DEFAULT_LEVEL};

#[path = "utils.rs"]
mod utils;

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("insert");

    for kind in utils::PAYLOAD_KINDS {
        for size in utils::SIZE_BUCKETS {
            let sample = utils::sample(kind, size);

            let column_key = utils::column_key();

            CURRENT_DICT_IDS.lock().remove(&column_key);

            let raw = compress(&sample, &column_key, DEFAULT_LEVEL).expect("compress raw");
            let (header, _) = Header::parse(&raw).expect("parse header");
            assert_eq!(header.dict_id.get(), 0);

            let dict_ids = utils::setup_trained_dict(kind, size);
            let dict_id = dict_ids.first().expect("train dictionary");

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
                CURRENT_DICT_IDS.lock().remove(&column_key);
                b.iter_batched(
                    utils::open_table,
                    |conn| {
                        let compressed =
                            compress(black_box(sample.as_slice()), &column_key, DEFAULT_LEVEL)
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
                CURRENT_DICT_IDS.lock().insert(column_key.clone(), *dict_id);

                b.iter_batched(
                    utils::open_table,
                    |conn| {
                        let compressed =
                            compress(black_box(sample.as_slice()), &column_key, DEFAULT_LEVEL)
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
                            "INSERT INTO requests_raw (data) VALUES (compress(?1,'raw', 'requests_raw', 'data'))",
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
