use std::{
    ffi::{CStr, CString, c_char},
    ptr,
};

use libsqlite3_sys::{SQLITE_ERROR, sqlite3, sqlite3_errmsg, sqlite3_malloc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("SQLite {operation} failed (code {code}): {message}")]
    Sqlite {
        operation: &'static str,
        code: i32,
        message: String,
    },

    #[error("Invalid string: {0}")]
    InvalidString(#[from] std::ffi::NulError),

    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    #[error("Pointer not valid: {0}")]
    PointerNotValid(String),

    #[error("{0}")]
    Message(String),
}

impl SqliteError {
    pub fn code(&self) -> i32 {
        match self {
            Self::Sqlite { code, .. } => *code,
            _ => SQLITE_ERROR,
        }
    }

    /// # Safety
    #[cold]
    pub(crate) unsafe fn from_db(db: *mut sqlite3, operation: &'static str, code: i32) -> Self {
        let message = unsafe {
            CStr::from_ptr(sqlite3_errmsg(db))
                .to_string_lossy()
                .into_owned()
        };
        Self::Sqlite {
            operation,
            code,
            message,
        }
    }

    /// # Safety
    /// `pz_err_msg` is the out-pointer passed by SQLite into the C entrypoint
    pub unsafe fn report(&self, pz_err_msg: *mut *mut c_char) -> i32 {
        let code = self.code();
        if pz_err_msg.is_null() {
            return code;
        }

        let Ok(msg) = CString::new(self.to_string()) else {
            return code;
        };

        let bytes = msg.as_bytes_with_nul();
        let ptr = unsafe { sqlite3_malloc(bytes.len() as i32).cast::<c_char>() };
        if !ptr.is_null() {
            unsafe {
                ptr::copy_nonoverlapping(bytes.as_ptr().cast(), ptr, bytes.len());
                *pz_err_msg = ptr;
            }
        }
        code
    }
}
