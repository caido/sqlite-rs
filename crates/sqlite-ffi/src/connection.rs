use std::ptr::NonNull;

use libsqlite3_sys::{sqlite3, sqlite3_context, sqlite3_context_db_handle};

use crate::{Context, SqliteError};

#[derive(Clone, Copy)]
pub struct Connection {
    pub(crate) conn: NonNull<sqlite3>,
}

impl Connection {
    pub fn from_raw(db: *mut sqlite3) -> Self {
        Connection {
            conn: NonNull::new(db).expect("database pointer is not valid"),
        }
    }

    pub fn from_context(context: &Context) -> Result<Self, SqliteError> {
        let db = unsafe { get_context_db_handle_raw(context.ctx.as_ptr()) };
        let conn = NonNull::new(db).expect("null db handle from context");

        Ok(Self { conn })
    }
}

unsafe fn get_context_db_handle_raw(context: *mut sqlite3_context) -> *mut sqlite3 {
    unsafe { sqlite3_context_db_handle(context) }
}
