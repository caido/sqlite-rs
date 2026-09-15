mod dataset;

#[path = "utils.rs"]
mod utils;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use sqlite_compress::{compress, decompress, Header, CURRENT_DICT_IDS, DEFAULT_LEVEL};

fn bench_decompress(c: &mut Criterion) {
    let mut group = c.benchmark_group("decompress");

    for kind in utils::PAYLOAD_KINDS {
        for size in utils::SIZE_BUCKETS {
            let sample = utils::sample(kind, size);
            let label = utils::sample_label(kind, size);

            let column_key = utils::column_key();

            CURRENT_DICT_IDS.lock().remove(&column_key);
            let zstd: Vec<u8> =
                compress(&sample, &column_key, DEFAULT_LEVEL).expect("compress zstd");
            let roundtrip = decompress(&zstd, Some(column_key.schema())).expect("decompress zstd");

            assert_eq!(roundtrip, sample);
            utils::report_size("zstd", &label, sample.len(), zstd.len());

            group.throughput(Throughput::Bytes(sample.len() as u64));

            // Raw decompress using zstd with no dictionary
            group.bench_function(BenchmarkId::new("decompress_zstd_no_dict", &label), |b| {
                b.iter(|| {
                    let out = decompress(black_box(zstd.as_slice()), Some(column_key.schema()))
                        .expect("decompress");
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

            let roundtrip =
                decompress(&zstd_dict, Some(column_key.schema())).expect("decompress zstd_dict");
            assert_eq!(roundtrip, sample);
            utils::report_size("zstd_dict", &label, sample.len(), zstd_dict.len());

            // Compress using zstd with dictionary
            group.bench_function(BenchmarkId::new("decompress_zstd_with_dict", &label), |b| {
                CURRENT_DICT_IDS.lock().insert(column_key.clone(), *dict_id);
                b.iter(|| {
                    let out =
                        decompress(black_box(zstd_dict.as_slice()), Some(column_key.schema()))
                            .expect("decompress");
                    black_box(out);
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_decompress);
criterion_main!(benches);
