use std::{ffi::c_void, ptr::NonNull};

use libsqlite3_sys::{sqlite3_context, sqlite3_result_blob, sqlite3_result_error, sqlite3_result_int64};

pub struct Context {
    pub(crate) ctx: NonNull<sqlite3_context>,
}

impl Context {
    pub fn from_raw(ctx: *mut sqlite3_context) -> Self {
        Self {
            ctx: NonNull::new(ctx).expect("context pointer is not valid"),
        }
    }

    pub fn result_blob(&self, blob: &[u8]) {
        unsafe {
            result_blob_raw(
                self.ctx.as_ptr(),
                blob.as_ptr().cast::<c_void>(),
                blob.len() as i32,
            );
        }
    }

    pub fn result_error(&self, error: &str) {
        unsafe {
            sqlite3_result_error(self.ctx.as_ptr(), error.as_ptr().cast(), error.len() as i32);
        }
    }

    pub fn result_int64(&self, value: i64) {
        unsafe { result_int64_raw(self.ctx.as_ptr(), value) };
    }
}

unsafe fn result_blob_raw(context: *mut sqlite3_context, blob: *const c_void, size: i32) {
    unsafe {
        sqlite3_result_blob(context, blob, size, libsqlite3_sys::SQLITE_TRANSIENT());
    }
}

unsafe fn result_int64_raw(context: *mut sqlite3_context, value: i64) {
    unsafe { sqlite3_result_int64(context, value) };
}
