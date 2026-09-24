use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    ptr::NonNull,
};

use libsqlite3_sys::{
    SQLITE_OK, sqlite3, sqlite3_blob, sqlite3_blob_bytes, sqlite3_blob_close, sqlite3_blob_open,
    sqlite3_blob_read, sqlite3_errmsg,
};

use crate::{Connection, error::SqliteError};

pub struct SqliteBlob {
    db: NonNull<sqlite3>,
    handle: NonNull<sqlite3_blob>,
}

impl SqliteBlob {
    pub fn len(&self) -> usize {
        unsafe { blob_bytes_raw(self.handle.as_ptr()) as usize }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> Result<(), SqliteError> {
        let n = i32::try_from(buf.len())
            .map_err(|_| SqliteError::Message("read length exceeds i32".into()))?;

        let offset = i32::try_from(offset)
            .map_err(|_| SqliteError::Message("read offset exceeds i32".into()))?;

        let rc = unsafe { blob_read_raw(self.handle.as_ptr(), buf.as_mut_ptr().cast(), n, offset) };

        if rc != SQLITE_OK {
            let message = unsafe {
                CStr::from_ptr(sqlite3_errmsg(self.db.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(SqliteError::Sqlite {
                operation: "sqlite3_blob_read",
                code: rc,
                message,
            });
        }

        Ok(())
    }
}

impl Connection {
    pub fn open_blob(
        &self,
        schema: &str,
        table: &str,
        column: &str,
        rowid: i64,
        write: bool,
    ) -> Result<SqliteBlob, SqliteError> {
        let schema = CString::new(schema)?;
        let table = CString::new(table)?;
        let column = CString::new(column)?;
        let mut blob = std::ptr::null_mut();

        let rc = unsafe {
            blob_open_raw(
                self.as_ptr(),
                schema.as_ptr(),
                table.as_ptr(),
                column.as_ptr(),
                rowid,
                if write { 1 } else { 0 },
                &mut blob,
            )
        };

        if rc != SQLITE_OK {
            let message = unsafe {
                CStr::from_ptr(sqlite3_errmsg(self.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(SqliteError::Sqlite {
                operation: "sqlite3_blob_open",
                code: rc,
                message,
            });
        }

        Ok(SqliteBlob {
            db: NonNull::new(self.as_ptr()).expect("database pointer is not valid"),
            handle: NonNull::new(blob).expect("sqlite3_blob_open returned a null handle"),
        })
    }
}

impl Drop for SqliteBlob {
    fn drop(&mut self) {
        unsafe { sqlite3_blob_close(self.handle.as_ptr()) };
    }
}

unsafe fn blob_open_raw(
    db: *mut libsqlite3_sys::sqlite3,
    schema: *const c_char,
    table: *const c_char,
    column: *const c_char,
    rowid: i64,
    flags: c_int,
    blob: *mut *mut sqlite3_blob,
) -> c_int {
    unsafe { sqlite3_blob_open(db, schema, table, column, rowid, flags, blob) }
}

unsafe fn blob_read_raw(
    blob: *mut sqlite3_blob,
    buf: *mut c_void,
    n: c_int,
    offset: c_int,
) -> c_int {
    unsafe { sqlite3_blob_read(blob, buf, n, offset) }
}

unsafe fn blob_bytes_raw(blob: *mut sqlite3_blob) -> c_int {
    unsafe { sqlite3_blob_bytes(blob) }
}
