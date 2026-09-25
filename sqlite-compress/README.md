# sqlite-compress

SQLite extension (Rust) for **zstd dictionary compression** on blob columns. It trains per-column dictionaries from your data, stores them in SQLite, caches encoders/decoders in memory, and exposes `compress` / `decompress` SQL functions plus decode views.

## Features

- **Setup** — per schema, create `__compress_dicts` and `__compress_decoded_*` views, then warm the dict cache
- **Train** — `train_all` / `train_by_column`: sample configured columns, build a zstd dictionary, persist it, update LRU caches
- **SQL functions** — `compress(blob, schema, table, column)` and `decompress(blob[, schema])` registered via `sqlite3_compress_init`
- **Connection trait** — `SetupConnection` so callers can wrap any SQLite connection handle

## Quick start

### What `setup` does

1. Ensures each configured table/column exists
2. Creates `__compress_decoded_{table}` views (idempotent; compressed columns go through `decompress`)
3. Creates `__compress_dicts` in each schema (`id`, `dict`, `trained_at`, `table_name`, `column_name`, `row_count`)
4. Warms encoder/decoder caches from the latest dictionary per column (if any)

### What `train` does

1. Validates config (capacity, tables, columns)
2. Checks there are enough samples and enough growth since the last train (`min_samples`, `retrain_growth`)
3. Builds a dictionary from sampled blobs
4. Inserts it into `__compress_dicts` (latest row by `id` is current) and syncs LRU caches