# sqlite-compress

SQLite extension (Rust) for **zstd dictionary compression** on blob columns. It trains per-column dictionaries from your data, stores them in SQLite, caches encoders/decoders in memory, and exposes `compress` / `decompress` SQL functions plus decode views.

## Features

- **Setup** — per schema, create `__compress_dicts` and `__compress_decoded_*` views, then warm the dict cache
- **Train** — `train_all` / `train_by_column`: sample configured columns, build a zstd dictionary, persist it, update the in-memory caches
- **SQL functions** — `compress(blob, schema, table, column)` and `decompress(blob, schema)`, registered by `sqlite3_compress_init`
- **Connection handle** — `SetupConnection::sqlite_handle` so the host passes the `*mut sqlite3` it already owns

## Quick start

Load the extension on the connection before `setup` or `train_all`. The init entry point stores the extension state those functions read back from the handle.

With the default `bundled` feature, call `sqlite3_compress_init` from the host and pass a null API-routines pointer. A `.load` build enables `loadable_extension` and lets SQLite call that entry point.

### What `setup` does

1. Validates each column policy (`retrain_growth`, `min_samples`, `max_samples`)
2. Ensures each configured table and column exists
3. Creates `__compress_decoded_{table}` views (idempotent; compressed columns go through `decompress`)
4. Creates `__compress_dicts` in each schema (`id`, `dict`, `trained_at`, `table_name`, `column_name`, `row_count`)
5. Warms encoder/decoder caches from the latest dictionary per column, if one is already stored

### What `train` does

1. Validates the dictionary capacity and that the table has at least one column
2. Skips the column unless the non-empty sample count reaches `min_samples`, and unless the count has grown by `retrain_growth` since the latest dictionary
3. Builds a dictionary from up to `max_samples` values
4. Inserts it into `__compress_dicts` (the latest row by `id` is current) and updates the caches

`train_all` returns one `DictId` per column that was trained. Skipped columns are omitted.

## Usage

The host implements `SetupConnection` for the connection it already owns. See the [setup and training sketch](docs/examples/setup_and_train.rs).

```rust
use sqlite_compress::{
    setup, train_all, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupTable,
    TableName, DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH,
};

struct AppConnection {
    db: *mut libsqlite3_sys::sqlite3,
}

impl SetupConnection for AppConnection {
    unsafe fn sqlite_handle(&self) -> *mut libsqlite3_sys::sqlite3 {
        self.db
    }
}

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

// `sqlite3_compress_init` has already run on `app.db`.
setup(&app, &config)?;
train_all(&app, &config, 112_640)?;
```

`112_640` is the dictionary capacity in bytes. The third argument of `SetupColumn::new` is `min_samples`, and the fourth is `max_samples`.

After training, store a compressed BLOB:

```sql
INSERT INTO requests (body)
VALUES (compress(:body, 'main', 'requests', 'body'));
```

For ordinary reads, query the decoded view created by setup. Its name is `__compress_decoded_<table>`:

```sql
SELECT id, body
FROM __compress_decoded_requests;
```

decompress takes the compressed blob and the schema that holds its dictionary:

```sql
SELECT decompress(body, 'main') AS body
FROM requests;
```
