use std::{ffi::c_void, marker::PhantomData, ptr::NonNull};

use libsqlite3_sys::{sqlite3_context, sqlite3_get_auxdata, sqlite3_set_auxdata};

use crate::{
    SqliteError,
    utils::{XDestroy, ptr_as_ref, to_sqlite_destroy},
};

pub struct Aux<'a, T> {
    pub ctx: NonNull<sqlite3_context>,
    pub arg: i32,
    pub _lifetime: PhantomData<&'a mut T>,
}

impl<'a, T> Aux<'a, T> {
    pub fn get(&self) -> Result<&T, SqliteError> {
        let aux = unsafe { get_auxdata_raw(self.ctx.as_ptr(), self.arg) };

        if aux.is_null() {
            return Err(SqliteError::PointerNotValid(format!(
                "get auxdata for arg: {}",
                self.arg
            )));
        }

        let aux = unsafe { ptr_as_ref::<T>(aux) };

        Ok(aux.expect("can't cast"))
    }

    pub fn set(&self, value: T) {
        let pointer = Box::into_raw(Box::new(value));

        unsafe {
            let (p, x_destroy) = to_sqlite_destroy(pointer);

            set_auxdata_raw(self.ctx.as_ptr(), self.arg, p, x_destroy);
        }
    }
}

unsafe fn get_auxdata_raw(context: *mut sqlite3_context, arg: i32) -> *mut c_void {
    unsafe { sqlite3_get_auxdata(context, arg) }
}

unsafe fn set_auxdata_raw(
    context: *mut sqlite3_context,
    arg: i32,
    ptr: *mut c_void,
    x_destroy: XDestroy,
) {
    unsafe { sqlite3_set_auxdata(context, arg, ptr, x_destroy) }
}
