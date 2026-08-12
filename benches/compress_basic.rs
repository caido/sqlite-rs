mod dataset;

#[path = "utils.rs"]
mod utils;

use std::sync::atomic::Ordering;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sqlite_compress::{compress, DEFAULT_LEVEL, LATEST_DICT_ID};

fn bench_compress(c: &mut Criterion) {
    let mut group = c.benchmark_group("compress");

    for kind in utils::PAYLOAD_KINDS {
        for size in utils::SIZE_BUCKETS {
            let sample = utils::sample(kind, size);
            let label = utils::sample_label(kind, size);

            LATEST_DICT_ID.store(0, Ordering::Relaxed);
            let zstd = compress(&sample, DEFAULT_LEVEL).expect("compress zstd");
            assert_eq!(u32::from_le_bytes(zstd[..4].try_into().unwrap()), 0);
            utils::report_size("zstd", &label, sample.len(), zstd.len());

            group.throughput(Throughput::Bytes(sample.len() as u64));

            // Raw compress using zstd with no dictionary
            group.bench_function(BenchmarkId::new("compress_zstd", &label), |b| {
                LATEST_DICT_ID.store(0, Ordering::Relaxed);
                b.iter(|| {
                    let out =
                        compress(black_box(sample.as_slice()), DEFAULT_LEVEL).expect("compress");
                    black_box(out);
                });
            });

            let dict_id = utils::setup_trained_dict(kind, size);
            LATEST_DICT_ID.store(dict_id.get(), Ordering::Relaxed);
            let zstd_dict = compress(&sample, DEFAULT_LEVEL).expect("compress zstd_dict");
            debug_assert_eq!(
                u32::from_le_bytes(zstd_dict[..4].try_into().unwrap()),
                dict_id.get()
            );
            utils::report_size("zstd_dict", &label, sample.len(), zstd_dict.len());

            // Compress using zstd with dictionary
            group.bench_function(BenchmarkId::new("compress_zstd_dict", &label), |b| {
                LATEST_DICT_ID.store(dict_id.get(), Ordering::Relaxed);
                b.iter(|| {
                    let out =
                        compress(black_box(sample.as_slice()), DEFAULT_LEVEL).expect("compress");
                    black_box(out);
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_compress);
criterion_main!(benches);
