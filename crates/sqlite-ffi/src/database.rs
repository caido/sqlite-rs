use std::ptr::NonNull;

use libsqlite3_sys::{sqlite3, sqlite3_context, sqlite3_context_db_handle};

use crate::{Context, SqliteError};

#[derive(Clone, Copy)]
pub struct Database {
    pub(crate) conn: NonNull<sqlite3>,
}

impl Database {
    pub fn from_raw(db: *mut sqlite3) -> Self {
        Database {
            conn: NonNull::new(db).unwrap(),
        }
    }

    pub fn from_context(context: &Context) -> Result<Self, SqliteError> {
        let db = unsafe { get_context_db_handle_raw(context.ctx.as_ptr()) };
        let conn = NonNull::new(db).ok_or_else(|| SqliteError::Sqlite {
            code: -1,
            message: "null db handle from context".into(),
        })?;

        Ok(Self { conn })
    }
}

unsafe fn get_context_db_handle_raw(context: *mut sqlite3_context) -> *mut sqlite3 {
    unsafe { sqlite3_context_db_handle(context) }
}
