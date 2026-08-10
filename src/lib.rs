mod conn;
mod dict;
mod functions;
mod setup;
mod utils;

pub const DEFAULT_RETRAIN_GROWTH: usize = 5000;

pub use dict::{errors::DictError, get_decoder, get_encoder, train, DictId, LATEST_DICT_ID};
pub use functions::{compress, decompress, DEFAULT_LEVEL};
pub use setup::{
    setup, ColumnName, DictStore, SchemaName, SetupConfig, SetupConnection, SetupError, SetupTable,
    TableName,
};
use sqlite_loadable::{define_scalar_function, prelude::*, Result};

#[sqlite_entrypoint]
pub fn sqlite3_compress_init(db: *mut sqlite3) -> Result<()> {
    define_scalar_function(
        db,
        "compress",
        1,
        functions::sqlite_compress,
        FunctionFlags::DETERMINISTIC,
    )?;

    define_scalar_function(
        db,
        "decompress",
        1,
        functions::sqlite_decompress,
        FunctionFlags::DETERMINISTIC,
    )?;

    Ok(())
}
