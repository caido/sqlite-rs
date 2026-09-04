# sqlite-compress

`sqlite-compress` is a SQLite extension and Rust library for compressing BLOB
columns with Zstandard. It can train a dictionary from representative values,
store that dictionary alongside the data, and transparently expose decoded
views for normal reads.

## Features

- SQLite scalar functions: `compress` and `decompress`
- Zstandard compression, with a raw-compression fallback before a dictionary is available
- Per-column dictionary training from existing BLOB values
- Configurable sample and retraining thresholds
- One dictionary store per SQLite schema
- Generated decoded views for configured tables
- In-memory encoder and decoder caches

## Installation

Build the extension from source:

```sh
cargo build --release
```

Load the generated dynamic library in SQLite, passing the extension entry point
explicitly. The exact library filename differs by platform:

```sql
.load ./target/release/libsqlite_compress sqlite3_compress_init
```

> TODO: publish pre-built releases and document the recommended installation
> channel.

## Workflow

Today, applications prepare and train dictionaries through the Rust API:

1. Configure the tables and BLOB columns to manage.
2. Run `setup` to create dictionary storage and decoded views.
3. Run `train_all` once enough representative rows are available, then rerun it
   after the configured data-growth threshold.
4. Write compressed values with `compress` and read them through the decoded
   view.

`setup` and `train_all` are intentionally explicit for now. Automating these
steps inside the library is planned for a future release.

## Usage

The Rust host supplies a connection adapter, then configures and prepares its
tables. See the complete, runnable [setup and training example](examples/setup_and_train.rs).

```rust
use sqlite_compress::{
    setup, train_all, SchemaName, SetupColumn, SetupConfig, SetupTable, TableName,
    DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH,
};

let config = SetupConfig {
    tables: vec![SetupTable {
        schema: SchemaName::new("main"),
        name: TableName::new("requests"),
        columns: vec![SetupColumn::new(
            "body",
            DEFAULT_RETRAIN_GROWTH,
            1_000,
            10_000,
        )],
    }],
    compression_level: DEFAULT_LEVEL,
};

setup(&mut connection, &config)?;
train_all(&mut connection, &config, 112_640)?;
```

After loading the extension and training a dictionary, store a compressed BLOB:

```sql
INSERT INTO requests (body)
VALUES (compress(:body, 'main', 'requests', 'body'));
```

For ordinary reads, query the decoded view created by `setup`. Its name is
`__compress_decoded_<table>`:

```sql
SELECT id, body
FROM __compress_decoded_requests;
```

`decompress` is also available when a view is not appropriate. Dictionary-backed
payloads need the schema name; raw Zstandard payloads do not:

```sql
SELECT decompress(body, 'main') AS body
FROM requests;
```

## Example

Run the complete setup and training example with:

```sh
cargo run --example setup_and_train
```
