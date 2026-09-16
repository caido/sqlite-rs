use std::{ffi::c_void, ptr::NonNull};

use libsqlite3_sys::{sqlite3_context, sqlite3_result_blob};

pub struct Context {
    pub(crate) ctx: NonNull<sqlite3_context>,
}

impl Context {
    pub fn from_raw(ctx: *mut sqlite3_context) -> Self {
        Self {
            ctx: NonNull::new(ctx).unwrap(),
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
}

unsafe fn result_blob_raw(context: *mut sqlite3_context, blob: *const c_void, size: i32) {
    unsafe {
        sqlite3_result_blob(context, blob, size, libsqlite3_sys::SQLITE_TRANSIENT());
    }
}
