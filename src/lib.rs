mod cache;
mod conn;
mod dict;
mod functions;
mod setup;
mod utils;

pub const DEFAULT_RETRAIN_GROWTH: usize = 5000;

pub use crate::conn::Connection;
pub use dict::{errors::DictError, train_all, train_by_column, ColumnKey, DictId};
pub use functions::{CodecError, Header, DEFAULT_LEVEL};
pub use setup::{
    setup, ColumnName, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupError,
    SetupTable, TableName,
};
use sqlite_ffi::{Database, SqliteError};
use sqlite_ffi_macros::sqlite_entrypoint;

#[sqlite_entrypoint(sqlite3_compress_init)]
fn sqlite3_compress_init_entrypoint(database: Database) {
    if let Err(SqliteError::Sqlite { code, .. }) = Connection::attach(database) {
        panic!("Failed to attach connection: {}", code);
    }

    if let Err(SqliteError::Sqlite { code, .. }) = database.create_function_v2(
        "compress",
        4,
        sqlite_ffi::TextRep::UTF8,
        functions::sqlite_compress,
    ) {
        panic!("Failed to create compress function: {}", code);
    }

    if let Err(SqliteError::Sqlite { code, .. }) = database.create_function_v2(
        "decompress",
        1,
        sqlite_ffi::TextRep::UTF8,
        functions::sqlite_decompress,
    ) {
        panic!("Failed to create decompress function: {}", code);
    }

    if let Err(SqliteError::Sqlite { code, .. }) = database.create_function_v2(
        "decompress",
        2,
        sqlite_ffi::TextRep::UTF8,
        functions::sqlite_decompress,
    ) {
        panic!("Failed to create decompress function: {}", code);
    }
}
