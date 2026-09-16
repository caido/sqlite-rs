use std::{ptr::NonNull, slice};

use libsqlite3_sys::{sqlite3_value, sqlite3_value_blob, sqlite3_value_bytes, sqlite3_value_int64, sqlite3_value_text};

use crate::SqliteError;

pub struct Value {
    value: NonNull<sqlite3_value>,
}

impl Value {
    pub fn from_raw(value: *mut sqlite3_value) -> Value {
        Self {
            value: NonNull::new(value).unwrap(),
        }
    }

    pub fn to_blob(&self) -> &[u8] {
        unsafe {
            let ptr = sqlite3_value_blob(self.value.as_ptr());
            let len = sqlite3_value_bytes(self.value.as_ptr()) as usize;
            if ptr.is_null() {
                &[]
            } else {
                slice::from_raw_parts(ptr.cast(), len)
            }
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
            let bytes = slice::from_raw_parts(ptr.cast(), len);
            std::str::from_utf8(bytes).map_err(Into::into)
        }
    }
}
