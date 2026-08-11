mod dataset;

use std::{path::PathBuf, time::Instant};

use clap::Parser;
use dataset::{generate_stream, PayloadKind, SizeBucket};
use rusqlite::Connection;

#[derive(Parser, Debug)]
#[command(
    name = "gen_dataset",
    about = "Generate HTTP payload dataset for benches"
)]
struct Args {
    /// Number of rows to generate
    #[arg(long, default_value_t = 1_000_000)]
    rows: u64,

    /// Output SQLite database path
    #[arg(long, default_value = "target/bench-dataset.db")]
    out: PathBuf,

    /// RNG seed for reproducibility
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// Rows committed per transaction
    #[arg(long, default_value_t = 10_000)]
    batch_size: u64,

    /// Replace existing output file
    #[arg(long, default_value_t = false)]
    force: bool,
}

fn main() {
    let args = Args::parse();
    if let Err(err) = run(args) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    if args.rows == 0 {
        return Err("rows must be > 0".into());
    }
    if args.batch_size == 0 {
        return Err("batch-size must be > 0".into());
    }

    if args.out.exists() {
        if args.force {
            std::fs::remove_file(&args.out)?;
        } else {
            return Err(format!(
                "output {} already exists (pass --force to overwrite)",
                args.out.display()
            )
            .into());
        }
    }

    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let started = Instant::now();
    let mut conn = Connection::open(&args.out)?;
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = OFF;
        PRAGMA temp_store = MEMORY;
        CREATE TABLE requests_raw (
            id INTEGER PRIMARY KEY,
            kind TEXT NOT NULL,
            size_bucket TEXT NOT NULL,
            data BLOB NOT NULL
        );
        ",
    )?;

    let mut kind_counts = [0u64; 3];
    let mut bucket_counts = [0u64; 3];
    let mut total_bytes: u64 = 0;

    let mut tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO requests_raw (id, kind, size_bucket, data) VALUES (?1, ?2, ?3, ?4)",
        )?;

        for (i, sample) in generate_stream(args.seed, args.rows).enumerate() {
            let id = (i as u64) + 1;
            match sample.kind {
                PayloadKind::Json => kind_counts[0] += 1,
                PayloadKind::Html => kind_counts[1] += 1,
                PayloadKind::Form => kind_counts[2] += 1,
            }
            match sample.size_bucket {
                SizeBucket::Tiny => bucket_counts[0] += 1,
                SizeBucket::Small => bucket_counts[1] += 1,
                SizeBucket::Medium => bucket_counts[2] += 1,
            }
            total_bytes += sample.bytes.len() as u64;

            stmt.execute((
                id,
                sample.kind.as_str(),
                sample.size_bucket.as_str(),
                sample.bytes.as_slice(),
            ))?;

            if id.is_multiple_of(args.batch_size) {
                drop(stmt);
                tx.commit()?;
                tx = conn.transaction()?;
                stmt = tx.prepare(
                    "INSERT INTO requests_raw (id, kind, size_bucket, data) VALUES (?1, ?2, ?3, ?4)",
                )?;
                eprint!(
                    "\rgenerated {id}/{} ({:.1}%)",
                    args.rows,
                    (id as f64) * 100.0 / (args.rows as f64)
                );
            }
        }
    }
    tx.commit()?;
    eprintln!();

    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;

    let elapsed = started.elapsed();
    let avg = if args.rows > 0 {
        total_bytes as f64 / args.rows as f64
    } else {
        0.0
    };

    println!("dataset written to {}", args.out.display());
    println!("rows:          {}", args.rows);
    println!("seed:          {}", args.seed);
    println!("total_bytes:   {total_bytes}");
    println!("avg_row_bytes: {avg:.1}");
    println!(
        "kinds:         json={} html={} form={}",
        kind_counts[0], kind_counts[1], kind_counts[2]
    );
    println!(
        "buckets:       tiny={} small={} medium={}",
        bucket_counts[0], bucket_counts[1], bucket_counts[2]
    );
    println!("elapsed:       {:.2}s", elapsed.as_secs_f64());
    println!(
        "throughput:    {:.0} rows/s",
        args.rows as f64 / elapsed.as_secs_f64().max(1e-9)
    );

    Ok(())
}
