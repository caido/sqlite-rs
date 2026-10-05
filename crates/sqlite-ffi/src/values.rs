use std::{ptr::NonNull, slice};

use libsqlite3_sys::{
    sqlite3_value, sqlite3_value_blob, sqlite3_value_bytes, sqlite3_value_int64,
    sqlite3_value_text, sqlite3_value_type,
};

use crate::SqliteError;

pub struct Value {
    value: NonNull<sqlite3_value>,
}

pub enum ValueType {
    Integer,
    Float,
    Text,
    Blob,
    Null,
}

impl Value {
    pub fn from_raw(value: *mut sqlite3_value) -> Value {
        Self {
            value: NonNull::new(value).expect("value pointer is not valid"),
        }
    }

    pub fn to_blob(&self) -> &[u8] {
        unsafe {
            let ptr = sqlite3_value_blob(self.value.as_ptr());
            let len = sqlite3_value_bytes(self.value.as_ptr()) as usize;
            bytes_from_raw(ptr.cast(), len)
        }
    }

    pub fn to_i64(&self) -> i64 {
        unsafe { sqlite3_value_int64(self.value.as_ptr()) }
    }

    pub fn to_text(&self) -> Result<&str, SqliteError> {
        unsafe {
            let ptr = sqlite3_value_text(self.value.as_ptr());
            let len = sqlite3_value_bytes(self.value.as_ptr()) as usize;
            if ptr.is_null() {
                return Ok("");
            }
            let bytes = bytes_from_raw(ptr.cast(), len);
            std::str::from_utf8(bytes).map_err(Into::into)
        }
    }

    pub fn value_type(&self) -> ValueType {
        match unsafe { sqlite3_value_type(self.value.as_ptr()) } {
            libsqlite3_sys::SQLITE_INTEGER => ValueType::Integer,
            libsqlite3_sys::SQLITE_FLOAT => ValueType::Float,
            libsqlite3_sys::SQLITE_TEXT => ValueType::Text,
            libsqlite3_sys::SQLITE_BLOB => ValueType::Blob,
            libsqlite3_sys::SQLITE_NULL => ValueType::Null,
            other => unreachable!("sqlite3_value_type returned {other}"),
        }
    }
}

pub(super) unsafe fn bytes_from_raw<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() {
        &[]
    } else {
        unsafe { slice::from_raw_parts(ptr, len) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_from_raw_reads_buffer() {
        let buf = b"abc";
        let bytes = unsafe { bytes_from_raw(buf.as_ptr(), buf.len()) };
        assert_eq!(bytes, b"abc");
    }

    #[test]
    fn bytes_from_raw_null_is_empty() {
        let bytes = unsafe { bytes_from_raw(std::ptr::null(), 0) };
        assert!(bytes.is_empty());
    }
}
