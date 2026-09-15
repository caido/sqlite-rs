#![allow(dead_code)]

use std::path::PathBuf;

use rand::{rngs::StdRng, SeedableRng};
use rusqlite::{Connection, LoadExtensionGuard};
use sqlite_compress::{
    setup, train_all, ColumnKey, DictId, DictStore, SchemaName, SetupColumn, SetupConfig,
    SetupConnection, SetupTable, TableName, DEFAULT_LEVEL,
};

use crate::dataset::{generate_sample, PayloadKind, SizeBucket};

pub fn column_key() -> ColumnKey {
    ColumnKey::new("raw", "requests_raw", "data")
}

pub const SIZE_BUCKETS: [SizeBucket; 4] = [
    SizeBucket::Tiny,
    SizeBucket::Small,
    SizeBucket::Medium,
    SizeBucket::Large,
];

pub const PAYLOAD_KINDS: [PayloadKind; 3] =
    [PayloadKind::Json, PayloadKind::Html, PayloadKind::Form];

const DICT_TRAIN_SAMPLES: u64 = 128;
const DICT_CAPACITY: usize = 8 * 1024;

pub fn json_sample(size: SizeBucket) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(42);
    generate_sample(&mut rng, 0, PayloadKind::Json, size)
}

pub fn sample(kind: PayloadKind, size: SizeBucket) -> Vec<u8> {
    let mut rng = StdRng::seed_from_u64(42);
    generate_sample(&mut rng, 0, kind, size)
}

pub fn sample_label(kind: PayloadKind, size: SizeBucket) -> String {
    format!("{}_{}", kind.as_str(), size.as_str())
}

pub fn report_size(algo: &str, label: &str, raw_len: usize, compressed_len: usize) {
    let ratio = compressed_len as f64 / raw_len as f64;
    let saved = 100.0 * (1.0 - ratio);
    eprintln!(
        "{algo:>12} {label:<12} raw={raw_len:>6} compressed={compressed_len:>6} ratio={ratio:.3} saved={saved:>5.1}%"
    );
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

    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<i64, Self::Error> {
        self.0.query_row(sql, [blob], |row| row.get(0))
    }

    fn for_each_blob<F>(&mut self, sql: &str, mut f: F) -> Result<(), Self::Error>
    where
        F: FnMut(&[u8]) -> Result<(), Self::Error>,
    {
        let mut stmt = self.0.prepare(sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let blob: &[u8] = row.get_ref(0)?.as_blob()?;
            f(blob)?;
        }
        Ok(())
    }
    fn query_strings(&mut self, sql: &str) -> Result<Vec<String>, Self::Error> {
        let mut stmt = self.0.prepare(sql)?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }
}

fn bench_config() -> SetupConfig {
    SetupConfig {
        tables: vec![SetupTable {
            name: TableName::new("requests_raw"),
            schema: SchemaName::new("raw"),
            columns: vec![SetupColumn::new(
                "data",
                5000,
                DICT_TRAIN_SAMPLES as usize,
                DICT_TRAIN_SAMPLES as usize,
            )],
        }],
        compression_level: DEFAULT_LEVEL,
    }
}

/// Train a dictionary on samples of the given kind/size and warm the caches.
pub fn setup_trained_dict(kind: PayloadKind, size: SizeBucket) -> Vec<DictId> {
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
        let sample = generate_sample(&mut rng, i, kind, size);
        insert.execute([sample.as_slice()]).expect("seed sample");
    }
    drop(insert);

    let config = bench_config();
    let mut wrapper = RusqliteConn(&conn);
    setup(&mut wrapper, &config).expect("setup");
    train_all(&mut wrapper, &config, DICT_CAPACITY).expect("train dictionary")
}

pub fn open_table() -> Connection {
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

pub fn open_table_with_extension() -> Connection {
    let conn = open_table();
    let path = compress_extension_path();
    unsafe {
        let _guard = LoadExtensionGuard::new(&conn).expect("enable load_extension");
        conn.load_extension(&path, Some("sqlite3_compress_init"))
            .unwrap_or_else(|e| panic!("load extension {}: {e}", path.display()));
    }
    conn.query_row(
        "SELECT typeof(compress(X'00','raw', 'requests_raw', 'data'))",
        [],
        |row| row.get::<_, String>(0),
    )
    .expect("compress() SQL function missing after load_extension");
    conn
}
