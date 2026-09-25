use std::{
    ffi::{c_char, CString},
    ptr,
};

use libsqlite3_sys::{sqlite3_malloc, SQLITE_ERROR};
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
                copy_cstr(bytes, ptr);
                *pz_err_msg = ptr;
            }
        }
        code
    }
}

unsafe fn copy_cstr(src: &[u8], dst: *mut c_char) {
    unsafe { ptr::copy_nonoverlapping(src.as_ptr().cast(), dst, src.len()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_null_out_pointer_returns_code() {
        let err = SqliteError::Message("x".into());
        let code = unsafe { err.report(std::ptr::null_mut()) };
        assert_eq!(code, err.code());
    }

    #[test]
    fn copy_cstr_writes_bytes_and_nul() {
        let src = b"hello\0";
        let mut dst = vec![0u8; src.len()];
        unsafe { copy_cstr(src, dst.as_mut_ptr().cast()) };
        assert_eq!(dst, src);
    }
}
