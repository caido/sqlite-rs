use std::slice;

use libsqlite3_sys::{sqlite3_value, sqlite3_value_blob, sqlite3_value_bytes, sqlite3_value_text};

use crate::SqliteError;

pub fn value_blob<'a>(value: &*mut sqlite3_value) -> &'a [u8] {
    unsafe {
        let ptr = sqlite3_value_blob(*value);
        let len = sqlite3_value_bytes(*value) as usize;
        if ptr.is_null() {
            &[]
        } else {
            slice::from_raw_parts(ptr.cast(), len)
        }
    }
}

pub fn value_text<'a>(value: &*mut sqlite3_value) -> Result<&'a str, SqliteError> {
    unsafe {
        let ptr = sqlite3_value_text(*value);
        let len = sqlite3_value_bytes(*value) as usize;
        if ptr.is_null() {
            return Ok("");
        }
        let bytes = slice::from_raw_parts(ptr.cast(), len);
        std::str::from_utf8(bytes).map_err(Into::into)
    }
}
