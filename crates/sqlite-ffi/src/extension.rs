use libsqlite3_sys::{InitError, SQLITE_ERROR, rusqlite_extension_init2, sqlite3_api_routines};

use crate::{Database, error::SqliteError};

impl Database {
    /// # Safety
    /// `p_api` doit être un pointeur valide fourni par SQLite.
    pub unsafe fn init_extension(p_api: *mut sqlite3_api_routines) -> Result<(), SqliteError> {
        if p_api.is_null() {
            return Err(SqliteError::Sqlite {
                operation: "init extension",
                code: SQLITE_ERROR,
                message: "null sqlite3_api_routines".into(),
            });
        }

        let res = unsafe { init_extension_raw(p_api) };

        res.map_err(|e| SqliteError::Sqlite {
            operation: "init extension",
            code: SQLITE_ERROR,
            message: e.to_string(),
        })
    }
}

unsafe fn init_extension_raw(p_api: *mut sqlite3_api_routines) -> Result<(), InitError> {
    unsafe { rusqlite_extension_init2(p_api) }
}
