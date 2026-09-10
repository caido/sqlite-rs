mod cache;
mod client_data;
mod conn;
mod dict;
mod functions;
mod setup;
mod utils;

pub const DEFAULT_RETRAIN_GROWTH: usize = 5000;

use std::ffi::c_void;

pub use dict::{errors::DictError, train_all, train_by_column, ColumnKey, DictId};
pub use functions::{CodecError, Header, DEFAULT_LEVEL};
use parking_lot::Mutex;
pub use setup::{
    setup, ColumnName, DictStore, SchemaName, SetupColumn, SetupConfig, SetupConnection,
    SetupError, SetupTable, TableName,
};
use sqlite_loadable::{define_scalar_function, prelude::*, Result, SQLITE_OK};

use crate::{cache::ConnectionCache, client_data::State};

unsafe extern "C" fn drop_state(ptr: *mut c_void) {
    if !ptr.is_null() {
        unsafe {
            drop(Box::from_raw(ptr.cast::<State>()));
        }
    }
}

#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn sqlite3_compress_init(
    db: *mut sqlite3,
    pz_err_msg: *mut *mut c_char,
    p_api: *const sqlite3_api_routines,
) -> c_int {
    unsafe {
        client_data::initialize(p_api);
    }

    let raw_state = Box::into_raw(Box::new(State {
        cache: Mutex::new(ConnectionCache::new()),
    }))
    .cast::<c_void>();

    let rc = unsafe {
        client_data::set(
            p_api,
            db,
            c"sqlite-compress.poc",
            raw_state,
            Some(drop_state),
        )
    };

    if rc != SQLITE_OK {
        return rc;
    }

    register_entrypoint(db, pz_err_msg, p_api, sqlite3_compress_init_impl)
}

pub fn sqlite3_compress_init_impl(db: *mut sqlite3) -> Result<()> {
    define_scalar_function(
        db,
        "compress",
        4,
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

    define_scalar_function(
        db,
        "decompress",
        2,
        functions::sqlite_decompress,
        FunctionFlags::DETERMINISTIC,
    )?;

    Ok(())
}
