# sqlite-ffi-macros

Procedural macros for SQLite extensions built on [`sqlite-ffi`](../sqlite-ffi). The crate exports one attribute, `sqlite_entrypoint`, which turns a Rust init function into the C symbol SQLite calls when an extension is loaded.

## Usage

The annotated function takes a `sqlite_ffi::Connection` and returns `Result<(), SqliteError>`. The exported symbol keeps that function's name.

```rust
use sqlite_ffi::{Connection, SqliteError};
use sqlite_ffi_macros::sqlite_entrypoint;

#[sqlite_entrypoint]
pub fn sqlite3_example_init(connection: Connection) -> Result<(), SqliteError> {
    connection.create_function("add_one", 1, add_one)?;
    Ok(())
}
```

Build the extension crate as a cdylib and pass the symbol name to SQLite:

```sql
.load ./target/release/libexample sqlite3_example_init
```

Attribute arguments are ignored. The macro parses the item as a function and generates the entry point from its name and body.
