use libsqlite3_sys::{sqlite3, sqlite3_context, sqlite3_context_db_handle};

use crate::SqliteError;

pub fn get_context_db_handle(context: *mut sqlite3_context) -> Result<*mut sqlite3, SqliteError> {
    if context.is_null() {
        return Err(SqliteError::Sqlite {
            code: -1,
            message: "null sqlite3_context".into(),
        });
    }

    let db = unsafe { get_context_db_handle_raw(context) };
    if db.is_null() {
        return Err(SqliteError::Sqlite {
            code: -1,
            message: "null db handle from context".into(),
        });
    }
    Ok(db)
}

unsafe fn get_context_db_handle_raw(context: *mut sqlite3_context) -> *mut sqlite3 {
    unsafe { sqlite3_context_db_handle(context) }
}
