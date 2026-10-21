use std::ffi::{c_char, c_int};

use libsqlite3_sys::{SQLITE_OK, sqlite3, sqlite3_api_routines, sqlite3_auto_extension};

use crate::SqliteError;

pub type ExtensionInit =
    unsafe extern "C" fn(*mut sqlite3, *mut *mut c_char, *mut sqlite3_api_routines) -> c_int;

pub fn register_auto_extension(init: ExtensionInit) -> Result<(), SqliteError> {
    #[allow(clippy::missing_transmute_annotations)]
    let code = unsafe { sqlite3_auto_extension(Some(std::mem::transmute(init))) };

    match code {
        SQLITE_OK => Ok(()),
        _ => Err(SqliteError::Sqlite {
            operation: "load extension",
            code,
            message: "can't load extension".into(),
        }),
    }
}
