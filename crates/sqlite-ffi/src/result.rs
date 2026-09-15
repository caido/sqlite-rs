use libsqlite3_sys::{sqlite3_context, sqlite3_result_blob};
use std::ffi::c_void;

pub fn result_blob(context: *mut sqlite3_context, blob: &[u8]) {
    unsafe {
        result_blob_raw(context, blob.as_ptr().cast::<c_void>(), blob.len() as i32);
    }
}

unsafe fn result_blob_raw(context: *mut sqlite3_context, blob: *const c_void, size: i32) {
    unsafe {
        sqlite3_result_blob(context, blob, size, libsqlite3_sys::SQLITE_TRANSIENT());
    }
}
