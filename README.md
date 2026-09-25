# sqlite-compress

Rust workspace for a SQLite extension that compresses blob columns with Zstandard dictionaries, and for the FFI layer that extension is built on.

The host keeps ownership of the SQLite connection. These crates wrap an existing `*mut sqlite3`: they do not open or close databases.

## Crates

| Crate                                                     | Path                       | Role                                                                                                                                                                     |
| --------------------------------------------------------- | -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [`sqlite-compress`](sqlite-compress/README.md)            | `sqlite-compress`          | Loadable extension (`cdylib` + `rlib`). Trains a per-column zstd dictionary, stores it in SQLite, caches encoders and decoders, and registers `compress` / `decompress`. |
| [`sqlite-ffi`](crates/sqlite-ffi/README.md)               | `crates/sqlite-ffi`        | Thin wrapper over `libsqlite3-sys`: scalar functions, bound queries, and per-connection client data.                                                                     |
| [`sqlite-ffi-macros`](crates/sqlite-ffi-macros/README.md) | `crates/sqlite-ffi-macros` | Procedural macro `sqlite_entrypoint`, which turns a Rust init function into the C symbol SQLite calls on `.load`.                                                        |

`sqlite-compress` depends on the other two. `sqlite-ffi` can be used on its own by any code that already holds a connection pointer.

## Requirements

[mise](https://mise.jdx.dev/) installs the toolchain from [`.mise/config.toml`](.mise/config.toml)
