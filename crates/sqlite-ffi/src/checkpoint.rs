use std::ffi::{CString, c_char};

use libsqlite3_sys::{
    SQLITE_BUSY, SQLITE_CHECKPOINT_TRUNCATE, SQLITE_OK, sqlite3, sqlite3_wal_checkpoint_v2,
};

use crate::{Connection, SqliteError};

impl Connection {
    pub fn wal_checkpoint(&self, schema: &str) -> Result<(), SqliteError> {
        let c_schema = CString::new(schema)?;
        let mut log = 0;
        let mut ckpt = 0;

        let rc = unsafe {
            wal_checkpoint_raw(
                self.as_ptr(),
                c_schema.as_ptr(),
                SQLITE_CHECKPOINT_TRUNCATE,
                &mut log,
                &mut ckpt,
            )
        };

        if rc == SQLITE_OK {
            return Ok(());
        }

        Err(SqliteError::Sqlite {
            operation: "wal checkpoint",
            code: rc,
            message: if rc == SQLITE_BUSY {
                "checkpoint blocked".into()
            } else {
                "checkpoint failed".into()
            },
        })
    }
}

unsafe fn wal_checkpoint_raw(
    conn: *mut sqlite3,
    schema: *const c_char,
    mode: i32,
    log: *mut i32,
    ckpt: *mut i32,
) -> i32 {
    unsafe { sqlite3_wal_checkpoint_v2(conn, schema, mode, log, ckpt) }
}
