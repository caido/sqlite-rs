mod dataset;

#[path = "utils.rs"]
mod utils;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sqlite_compress::{compress, Header, CURRENT_DICT_IDS, DEFAULT_LEVEL};

fn bench_compress(c: &mut Criterion) {
    let mut group = c.benchmark_group("compress");

    for kind in utils::PAYLOAD_KINDS {
        for size in utils::SIZE_BUCKETS {
            let sample = utils::sample(kind, size);
            let label = utils::sample_label(kind, size);

            let column_key = utils::column_key();

            CURRENT_DICT_IDS.lock().remove(&column_key);

            let zstd = compress(&sample, &column_key, DEFAULT_LEVEL).expect("compress zstd");
            let (header, _) = Header::parse(&zstd).expect("parse header");
            assert_eq!(header.dict_id.get(), 0);

            utils::report_size("zstd", &label, sample.len(), zstd.len());

            group.throughput(Throughput::Bytes(sample.len() as u64));

            // Raw compress using zstd with no dictionary
            group.bench_function(BenchmarkId::new("compress_zstd_no_dict", &label), |b| {
                CURRENT_DICT_IDS.lock().remove(&column_key);
                b.iter(|| {
                    let out = compress(black_box(sample.as_slice()), &column_key, DEFAULT_LEVEL)
                        .expect("compress");
                    black_box(out);
                });
            });

            let dict_ids = utils::setup_trained_dict(kind, size);
            let dict_id = dict_ids.first().expect("train dictionary");

            CURRENT_DICT_IDS.lock().insert(column_key.clone(), *dict_id);

            let zstd_dict =
                compress(&sample, &column_key, DEFAULT_LEVEL).expect("compress zstd_dict");

            let (header, _) = Header::parse(&zstd_dict).expect("parse header");
            debug_assert_eq!(header.dict_id.get(), dict_id.get());
            utils::report_size("zstd_dict", &label, sample.len(), zstd_dict.len());

            // Compress using zstd with dictionary
            group.bench_function(BenchmarkId::new("compress_zstd_with_dict", &label), |b| {
                CURRENT_DICT_IDS.lock().insert(column_key.clone(), *dict_id);
                b.iter(|| {
                    let out = compress(black_box(sample.as_slice()), &column_key, DEFAULT_LEVEL)
                        .expect("compress");
                    black_box(out);
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_compress);
criterion_main!(benches);
