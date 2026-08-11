mod dataset;

#[path = "utils.rs"]
mod utils;

use std::sync::atomic::Ordering;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sqlite_compress::{compress, decompress, DEFAULT_LEVEL, LATEST_DICT_ID};

fn bench_decompress(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompress");

    for kind in utils::PAYLOAD_KINDS {
        for size in utils::SIZE_BUCKETS {
            let sample = utils::sample(kind, size);
            let label = utils::sample_label(kind, size);

            LATEST_DICT_ID.store(0, Ordering::Relaxed);
            let zstd: Vec<u8> = compress(&sample, DEFAULT_LEVEL).expect("compress zstd");
            let roundtrip = decompress(&zstd).expect("decompress zstd");
            assert_eq!(roundtrip, sample);
            utils::report_size("zstd", &label, sample.len(), zstd.len());

            group.throughput(Throughput::Bytes(sample.len() as u64));
            group.bench_function(BenchmarkId::new("zstd", &label), |b| {
                b.iter(|| {
                    let out = decompress(black_box(zstd.as_slice())).expect("decompress");
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
            let roundtrip = decompress(&zstd_dict).expect("decompress zstd_dict");
            assert_eq!(roundtrip, sample);
            utils::report_size("zstd_dict", &label, sample.len(), zstd_dict.len());

            group.bench_function(BenchmarkId::new("zstd_dict", &label), |b| {
                LATEST_DICT_ID.store(dict_id.get(), Ordering::Relaxed);
                b.iter(|| {
                    let out = decompress(black_box(zstd_dict.as_slice())).expect("decompress");
                    black_box(out);
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_decompress);
criterion_main!(benches);
