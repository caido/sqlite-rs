# sqlite-ffi

A small Rust layer over [`libsqlite3-sys`](https://docs.rs/libsqlite3-sys) for code that already holds a SQLite connection pointer. It covers scalar SQL functions, bound queries, and per-connection client data. It does not open or close databases, and it does not replace a full SQLite toolkit such as rusqlite.

`sqlite-ffi` is the FFI crate used by [`sqlite-compress`](../../sqlite-compress). The companion crate [`sqlite-ffi-macros`](../sqlite-ffi-macros) generates the C entry point of a loadable extension.

## Features

- Wrap an existing `*mut sqlite3` in `Connection` without taking ownership
- Register scalar functions with a Rust closure
- Read function arguments as `Value` and write a blob or an error on `Context`
- Run bound queries (`query`, `execute`) and multi-statement SQL (`batch_execute`)
- Store Rust state on a connection with `sqlite3_set_clientdata`
- Initialize the SQLite API table when built as a loadable extension

## Crate features

| Feature              | Default | Effect                                                                    |
| -------------------- | ------- | ------------------------------------------------------------------------- |
| `bundled`            | yes     | Compiles SQLite via `libsqlite3-sys/bundled`                              |
| `loadable_extension` | no      | Uses the host SQLite through `sqlite3_api_routines` instead of linking it |

Disable the default features when the host application already provides SQLite:

```toml
sqlite-ffi = { version = "0.1.0", path = "../crates/sqlite-ffi", default-features = false }
```

Enable loadable_extension on both sqlite-ffi and libsqlite3-sys when the crate is loaded with .load.

### Scalar functions

Connection does not own the database. Open it yourself, and close it when you are done. from_raw panics if the pointer is null.

```rust
use sqlite_ffi::{Connection, Context, SqliteError, Value};
fn add_one(context: Context, args: &[Value]) -> Result<(), SqliteError> {
    let n = args[0].to_i64();
    context.result_blob(&(n + 1).to_le_bytes());
    Ok(())
}
// `db` is a live `*mut libsqlite3_sys::sqlite3`.
let connection = Connection::from_raw(db);
connection.create_function("add_one", 1, add_one)?;
```

create_function registers a scalar function (sqlite3_create_function_v2 with SQLITE_UTF8). The argument count is the SQLite nArg value: a fixed count, or -1 for any number of arguments.

Inside the callback:

- `Value::to_i64`, `to_text`, and `to_blob` read the current argument. The returned borrows are only valid for that call.
- `Context::result_blob` copies the bytes into SQLite (SQLITE_TRANSIENT).
- Returning `Err` reports the error text to SQLite. A successful callback that never sets a result yields SQL NULL.
- Only scalar functions are supported. Aggregates and window functions are not.

### Queries

SqlValue is an owned SQLite value: Null, Integer(i64), Float(f64), Text(String), or Blob(Vec<u8>). Parameters are bound in order, starting at ?1. Text is passed as a C string, so an interior NUL byte returns SqliteError::InvalidString. Blobs and text are copied into SQLite.

```rust
use sqlite_ffi::{first_value, SqlValue};
connection.batch_execute(
    "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT, payload BLOB)",
)?;
let id = connection.execute(
    "INSERT INTO items (name, payload) VALUES (?1, ?2) RETURNING id",
    &[
        SqlValue::Text("a".into()),
        SqlValue::Blob(b"raw".to_vec()),
    ],
)?;
let rows = connection.query(
    "SELECT name, payload FROM items WHERE id = ?1",
    &[SqlValue::Integer(id)],
)?;
let name = first_value(&rows).and_then(SqlValue::as_text);
```

- `query` runs one statement and returns every row. Each row is a `Vec<SqlValue>`.
- `execute` runs one statement. If it produces a row, the return value is column 0 of the first row as an integer (the RETURNING id pattern above). Otherwise it returns 0. It is not a changes count.
- `batch_execute` runs one or more statements through sqlite3_exec, with no parameters. On success it returns SQLITE_OK (0).

### Client data

Extension state can live on the connection. SQLite owns the allocation and drops it when the name is overwritten or the connection closes.

```rust
connection.set_client_data("my-extension", MyState { generation: 1 })?;
let state = connection.get_client_data::<MyState>("my-extension")?;
assert_eq!(state.generation, 1);
```

- `get_client_data` returns an error when the name is missing. The type parameter must be the same type that was stored. SQLite does not check it; a mismatched T is undefined behavior.

- `Connection::from_context` rebuilds a wrapper from a function `Context`, which is how a callback reaches client data stored at init time.

### Errors

SqliteError covers a failed SQLite call (Sqlite { operation, code, message }), an interior NUL, invalid UTF-8, a missing pointer, or a free-form Message. code() returns the SQLite result code, or SQLITE_ERROR for the non-SQLite variants.
