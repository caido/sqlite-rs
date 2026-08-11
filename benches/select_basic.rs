//! Select benchmarks categorized by operation.
//!
//! ```text
//! cargo bench --bench select_basic
//! ```

mod dataset;

use std::path::PathBuf;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use dataset::{generate_sample, PayloadKind, SizeBucket};
use rand::{rngs::StdRng, SeedableRng};
use rusqlite::{Connection, LoadExtensionGuard};
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

fn compress_extension_path() -> PathBuf {
    let target_dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"));

    let lib_name = if cfg!(target_os = "windows") {
        "sqlite_compress.dll"
    } else if cfg!(target_os = "macos") {
        "libsqlite_compress.dylib"
    } else {
        "libsqlite_compress.so"
    };

    for profile in ["release", "bench", "debug"] {
        let candidate = target_dir.join(profile).join(lib_name);
        if candidate.exists() {
            return candidate;
        }
    }

    panic!(
        "sqlite-compress extension not found under {} (build with cargo bench / cargo build --release)",
        target_dir.display()
    );
}

fn open_filled_with_extension(data: &[u8]) -> Connection {
    let conn = open_filled(data);
    let path = compress_extension_path();
    unsafe {
        let _guard = LoadExtensionGuard::new(&conn).expect("enable load_extension");
        conn.load_extension(&path, Some("sqlite3_compress_init"))
            .unwrap_or_else(|e| panic!("load extension {}: {e}", path.display()));
    }
    conn.query_row("SELECT typeof(decompress(compress(X'00')))", [], |row| {
        row.get::<_, String>(0)
    })
    .expect("decompress() SQL function missing after load_extension");
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

    group.bench_function(BenchmarkId::new("decompress_ext", "json_small"), |b| {
        let conn = open_filled_with_extension(&compressed);
        let ext_roundtrip = select_decompressed_sql(&conn);
        assert_eq!(ext_roundtrip, sample);

        b.iter(|| {
            let out = select_decompressed_sql(&conn);
            black_box(out);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_select);
criterion_main!(benches);
