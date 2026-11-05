# sqlite-compress

SQLite extension (Rust) for **zstd dictionary compression** on blob columns. It trains per-column dictionaries from your data, stores them in SQLite, caches encoders/decoders in memory, and exposes `compress` / `decompress` SQL functions plus optionally configured decoded views.

## Features

- **Setup** — per schema, create `__compress_dicts`; create decoded views only for tables configured with `SetupTable::with_view_name`, then warm the dict cache
- **Train** — `train_all` / `train_by_column`: sample configured columns, build a zstd dictionary, persist it, update the in-memory caches
- **SQL functions** — `compress(blob, schema, table, column)` and `decompress(blob, schema)`, registered by `sqlite3_compress_init`
- **Connection handle** — `SetupConnection::sqlite_handle` so the host passes the `*mut sqlite3` it already owns

## Quick start

Load the extension on the connection before `setup` or `train_all`. The init entry point stores the extension state those functions read back from the handle.

With the default `bundled` feature, call `sqlite3_compress_init` from the host and pass a null API-routines pointer. A `.load` build enables `loadable_extension` and lets SQLite call that entry point.

### What `setup` does

1. Validates each column policy (`retrain_policy`, `min_samples`, `max_samples`)
2. Ensures each configured table and column exists
3. Creates a decoded view only when a table has a configured view name. The configured name is prefixed with `__compress_decoded_`; no view is created when `with_view_name` is omitted.
4. Creates `__compress_dicts` in each schema (`id`, `dict`, `trained_at`, `table_name`, `column_name`, `row_count`)
5. Warms encoder/decoder caches from the latest dictionary per column, if one is already stored

### What `train` does

1. Validates the dictionary capacity and that the table has at least one column
2. Skips the column unless the non-empty sample count reaches `min_samples`, and unless growth since the latest dictionary satisfies `retrain_policy` (`Percent` scales with the last trained row count; `Absolute` uses a fixed delta)
3. Builds a dictionary from up to `max_samples` values
4. Inserts it into `__compress_dicts` (the latest row by `id` is current) and updates the caches

`train_all` returns one `DictId` per column that was trained. Skipped columns are omitted.

## Usage

The host implements `SetupConnection` for the connection it already owns. See the [setup and training sketch](docs/examples/setup_and_train.rs).

```rust
use sqlite_compress::{
    setup, train_all, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupTable,
    TableName, ViewName,DEFAULT_LEVEL, DEFAULT_RETRAIN_POLICY,
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
    tables: vec![SetupTable::new(
        SchemaName::new("main"),
        TableName::new("requests"),
        vec![SetupColumn::new(
            "body",
            DEFAULT_RETRAIN_POLICY,
            1_000,
            10_000,
        )],
    ).with_view_name(ViewName::new("requests")),],
    compression_level: DEFAULT_LEVEL,
};

// `sqlite3_compress_init` has already run on `app.db`.
setup(&app, &config)?;
train_all(&app, &config, 112_640)?;
```

`112_640` is the dictionary capacity in bytes. The second argument of `SetupColumn::new` is the [`RetrainPolicy`](src/setup/config.rs) (`Percent` or `Absolute`); the third is `min_samples`, and the fourth is `max_samples`.

After training, store a compressed BLOB:

```sql
INSERT INTO requests (body)
VALUES (compress(:body, 'main', 'requests', 'body'));
```

+For ordinary reads, query the decoded view created by setup. `with_view_name` supplies the base name; setup prefixes it with `__compress_decoded_`. For example, `ViewName::new("requests_decoded")` creates `__compress_decoded_requests_decoded`. If `with_view_name` is omitted, setup creates no view

```sql
SELECT id, body
FROM __compress_decoded_requests_decoded;
```

decompress takes the compressed blob and the schema that holds its dictionary:

```sql
SELECT decompress(body, 'main') AS body
FROM requests;
```
