mod cache;
mod dict;
mod functions;
mod setup;
mod state;
mod utils;

pub const DEFAULT_RETRAIN_GROWTH: usize = 5000;

pub use dict::{ColumnKey, DictId, errors::DictError, train_all, train_by_column};
pub use functions::{CodecError, DEFAULT_LEVEL, Header};
pub use setup::{
    ColumnName, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupError, SetupTable,
    TableName, setup,
};
use sqlite_ffi::{Connection, SqliteError};
use sqlite_ffi_macros::sqlite_entrypoint;

pub use crate::state::{Cache, ExtensionState};

#[sqlite_entrypoint]
pub fn sqlite3_compress_init(connection: Connection) -> Result<(), SqliteError> {
    ExtensionState::attach(&connection)?;

    connection.create_function("compress", 4, functions::sqlite_compress)?;

    connection.create_function("decompress", 1, functions::sqlite_decompress)?;

    connection.create_function("decompress", 2, functions::sqlite_decompress)?;

    Ok(())
}
