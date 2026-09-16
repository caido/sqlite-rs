mod cache;
mod conn;
mod dict;
mod functions;
mod setup;
mod utils;

pub const DEFAULT_RETRAIN_GROWTH: usize = 5000;

use std::ffi::{c_char, c_int};

pub use dict::{errors::DictError, train_all, train_by_column, ColumnKey, DictId};
pub use functions::{CodecError, Header, DEFAULT_LEVEL};
use libsqlite3_sys::{sqlite3, sqlite3_api_routines, SQLITE_OK};
pub use setup::{
    setup, ColumnName, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupError,
    SetupTable, TableName,
};
use sqlite_ffi::{Database, SqliteError};

pub use crate::conn::Connection;

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn sqlite3_compress_init(
    db: *mut sqlite3,
    _pz_err_msg: *mut *mut c_char,
    #[cfg_attr(not(feature = "loadable_extension"), allow(unused_variables))]
    p_api: *mut sqlite3_api_routines,
) -> c_int {
    let database = Database::from_raw(db);

    #[cfg(feature = "loadable_extension")]
    if let Err(SqliteError::Sqlite { code, .. }) = Database::init_extension(p_api) {
        return code;
    }

    if let Err(SqliteError::Sqlite { code, .. }) = Connection::attach(database) {
        return code;
    }

    if let Err(SqliteError::Sqlite { code, .. }) = database.create_function_v2(
        "compress",
        4,
        sqlite_ffi::TextRep::UTF8,
        functions::sqlite_compress,
    ) {
        return code;
    }

    if let Err(SqliteError::Sqlite { code, .. }) = database.create_function_v2(
        "decompress",
        1,
        sqlite_ffi::TextRep::UTF8,
        functions::sqlite_decompress,
    ) {
        return code;
    }

    if let Err(SqliteError::Sqlite { code, .. }) = database.create_function_v2(
        "decompress",
        2,
        sqlite_ffi::TextRep::UTF8,
        functions::sqlite_decompress,
    ) {
        return code;
    }

    SQLITE_OK
}
