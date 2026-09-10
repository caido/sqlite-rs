use std::ffi::{c_char, c_int, c_void};

pub type Destructor = unsafe extern "C" fn(*mut c_void);

unsafe extern "C" {
    pub fn sqlite_clientdata_set(
        api: *const c_void,
        db: *mut c_void,
        name: *const c_char,
        data: *mut c_void,
        destructor: Option<Destructor>,
    ) -> c_int;

    pub fn sqlite_clientdata_get(
        api: *const c_void,
        db: *mut c_void,
        name: *const c_char,
    ) -> *mut c_void;
}
