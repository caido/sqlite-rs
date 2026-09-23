use std::ffi::c_int;

use libsqlite3_sys::{SQLITE_OK, sqlite3_int64, sqlite3_status64};

use crate::{Connection, error::SqliteError};

#[repr(i32)]
enum SqliteStatus {
    MallocSize = 5,
}

impl Connection {
    pub fn malloc_highwater(&self) -> Result<i64, SqliteError> {
        let mut current = 0;
        let mut highwater = 0;
        let rc = unsafe {
            status_raw(
                SqliteStatus::MallocSize as c_int,
                &mut current,
                &mut highwater,
                1,
            )
        };

        if rc != SQLITE_OK {
            return Err(SqliteError::Sqlite {
                operation: "sqlite3_status64",
                code: rc,
                message: "malloc highwater".into(),
            });
        }

        Ok(highwater)
    }
}

unsafe fn status_raw(
    op: c_int,
    current: *mut sqlite3_int64,
    highwater: *mut sqlite3_int64,
    reset_flag: c_int,
) -> c_int {
    unsafe { sqlite3_status64(op, current, highwater, reset_flag) }
}
