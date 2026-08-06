mod conn;
mod dict;
mod functions;
mod setup;
mod utils;

pub use dict::errors::DictError;
pub use dict::{get_decoder, get_encoder, train, LATEST_DICT_ID};
pub use functions::{compress, decompress, DEFAULT_LEVEL};
pub use setup::{setup, DictStore, SetupConfig, SetupConnection, SetupError, SetupTable};

use sqlite_loadable::prelude::*;
use sqlite_loadable::{define_scalar_function, Result};

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
