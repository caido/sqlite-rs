mod cache;
mod dict;
mod functions;
mod setup;
mod state;
mod utils;

pub const DEFAULT_RETRAIN_GROWTH: usize = 5000;

pub use dict::{errors::DictError, train_all, train_by_column, ColumnKey, DictId};
pub use functions::{CodecError, Header, DEFAULT_LEVEL};
pub use setup::{
    setup, ColumnName, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupError,
    SetupTable, TableName,
};
use sqlite_ffi::{Connection, SqliteError};
use sqlite_ffi_macros::sqlite_entrypoint;

pub use crate::state::ExtensionState;

#[sqlite_entrypoint(sqlite3_compress_init)]
fn sqlite3_compress_init_entrypoint(connection: Connection) {
    if let Err(SqliteError::Sqlite { code, .. }) = ExtensionState::attach(connection) {
        panic!("Failed to attach connection: {}", code);
    }

    if let Err(SqliteError::Sqlite { code, .. }) =
        connection.create_function_v2("compress", 4, functions::sqlite_compress)
    {
        panic!("Failed to create compress function: {}", code);
    }

    if let Err(SqliteError::Sqlite { code, .. }) =
        connection.create_function_v2("decompress", 1, functions::sqlite_decompress)
    {
        panic!("Failed to create decompress function: {}", code);
    }

    if let Err(SqliteError::Sqlite { code, .. }) =
        connection.create_function_v2("decompress", 2, functions::sqlite_decompress)
    {
        panic!("Failed to create decompress function: {}", code);
    }
}
