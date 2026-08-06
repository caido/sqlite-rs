mod dataset;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use dataset::{generate_sample, PayloadKind, SizeBucket};
use rand::rngs::StdRng;
use rand::SeedableRng;
use rusqlite::{Connection, LoadExtensionGuard};
use sqlite_compress::{
    compress, setup, train, DictStore, SetupConfig, SetupConnection, SetupTable, LATEST_DICT_ID,
    DEFAULT_LEVEL,
};

const DICT_TRAIN_SAMPLES: u64 = 128;
const DICT_CAPACITY: usize = 8 * 1024;

fn json_sample() -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(42);
    generate_sample(&mut rng, 0, PayloadKind::Json, SizeBucket::Small)
}

fn open_table() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");
    conn.execute_batch(
        "CREATE TABLE requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        );",
    )
    .expect("create table");
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

fn open_table_with_extension() -> Connection {
    let conn = open_table();
    let path = compress_extension_path();
    unsafe {
        let _guard = LoadExtensionGuard::new(&conn).expect("enable load_extension");
        conn.load_extension(&path, Some("sqlite3_compress_init"))
            .unwrap_or_else(|e| panic!("load extension {}: {e}", path.display()));
    }
    // Smoke-check the SQL entrypoint is registered.
    conn.query_row("SELECT typeof(compress(X'00'))", [], |row| {
        row.get::<_, String>(0)
    })
    .expect("compress() SQL function missing after load_extension");
    conn
}

fn bench_config() -> SetupConfig {
    SetupConfig {
        tables: vec![SetupTable {
            name: "requests_raw".to_string(),
            schema: "raw".to_string(),
            columns: vec!["data".to_string()],
        }],
        compression_level: DEFAULT_LEVEL,
    }
}

struct RusqliteConn<'a>(&'a Connection);

impl DictStore for RusqliteConn<'_> {
    type Error = rusqlite::Error;

    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error> {
        let mut stmt = self.0.prepare(sql)?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }
}

impl SetupConnection for RusqliteConn<'_> {
    fn batch_execute(&mut self, sql: &str) -> Result<(), Self::Error> {
        self.0.execute_batch(sql)
    }

    fn query_i64(&mut self, sql: &str) -> Result<i64, Self::Error> {
        self.0.query_row(sql, [], |row| row.get(0))
    }

    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<(), Self::Error> {
        self.0.execute(sql, [blob])?;
        Ok(())
    }
}

/// Seed a table with JSON samples, then train via the public `train` API (outside the timed path).
/// Returns the dictionary id (also cached in `LATEST_DICT_ID`).
fn setup_trained_dict() -> u32 {
    let conn = Connection::open_in_memory().expect("open dict db");
    conn.execute_batch(
        "
        ATTACH DATABASE ':memory:' AS raw;
        CREATE TABLE raw.requests_raw (
            id INTEGER PRIMARY KEY,
            data BLOB NOT NULL
        );
        ",
    )
    .expect("create training table");

    let mut insert = conn
        .prepare("INSERT INTO raw.requests_raw (data) VALUES (?1)")
        .expect("prepare insert");
    for i in 0..DICT_TRAIN_SAMPLES {
        let mut rng = StdRng::seed_from_u64(1_000 + i);
        let sample = generate_sample(&mut rng, i, PayloadKind::Json, SizeBucket::Small);
        insert.execute([sample.as_slice()]).expect("seed sample");
    }
    drop(insert);

    let config = bench_config();
    let mut wrapper = RusqliteConn(&conn);
    setup(&mut wrapper, &config).expect("setup");
    let dict_id = train(
        &mut wrapper,
        &config,
        DICT_CAPACITY,
        DICT_TRAIN_SAMPLES as usize,
        DICT_TRAIN_SAMPLES as usize,
    )
    .expect("train dictionary");

    assert_eq!(LATEST_DICT_ID.load(Ordering::Relaxed), dict_id);
    dict_id
}

fn bench_insert(c: &mut Criterion) {
    let sample = json_sample();

    // Raw path must not see a dictionary from a previous Criterion run in-process.
    LATEST_DICT_ID.store(0, Ordering::Relaxed);
    let raw = compress(&sample, DEFAULT_LEVEL).expect("compress raw");
    assert_eq!(u32::from_le_bytes(raw[..4].try_into().unwrap()), 0);

    let dict_id = setup_trained_dict();
    let with_dict = compress(&sample, DEFAULT_LEVEL).expect("compress with dict");
    let dict_header = u32::from_le_bytes(with_dict[..4].try_into().unwrap());
    assert_eq!(dict_header, dict_id, "expected dict-backed compress after train");

    eprintln!(
        "sample: kind=json bucket=small raw_bytes={} dict_id={dict_id}",
        sample.len()
    );
    eprintln!(
        "ratios:  raw={} ({:.2}) dict={} ({:.2})",
        raw.len(),
        raw.len() as f64 / sample.len() as f64,
        with_dict.len(),
        with_dict.len() as f64 / sample.len() as f64
    );

    let mut group = c.benchmark_group("insert");

    group.bench_function(BenchmarkId::new("sql", "json_small"), |b| {
        b.iter_batched(
            open_table,
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

    group.bench_function(BenchmarkId::new("compress", "json_small"), |b| {
        // Force raw zstd for this variant even if a dict was trained later in setup.
        LATEST_DICT_ID.store(0, Ordering::Relaxed);
        b.iter_batched(
            open_table,
            |conn| {
                let compressed =
                    compress(black_box(sample.as_slice()), DEFAULT_LEVEL).expect("compress");
                conn.execute(
                    "INSERT INTO requests_raw (data) VALUES (?1)",
                    [black_box(compressed.as_slice())],
                )
                .expect("insert");
            },
            criterion::BatchSize::SmallInput,
        );
    });

    group.bench_function(BenchmarkId::new("compress_dict", "json_small"), |b| {
        LATEST_DICT_ID.store(dict_id, Ordering::Relaxed);
        b.iter_batched(
            open_table,
            |conn| {
                let compressed =
                    compress(black_box(sample.as_slice()), DEFAULT_LEVEL).expect("compress");
                debug_assert_eq!(
                    u32::from_le_bytes(compressed[..4].try_into().unwrap()),
                    dict_id
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

    // SQL compress() via loaded cdylib (separate address space from the rlib API,
    // so this path is raw zstd unless the extension warms its own dict cache).
    group.bench_function(BenchmarkId::new("compress_ext", "json_small"), |b| {
        let conn = open_table_with_extension();
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

    group.finish();
}

criterion_group!(benches, bench_insert);
criterion_main!(benches);
