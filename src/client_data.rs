use std::{
    ffi::{c_void, CStr},
    sync::atomic::{AtomicPtr, Ordering},
};

use parking_lot::Mutex;
use sqlite_loadable::prelude::{sqlite3, sqlite3_api_routines};

use crate::cache::ConnectionCache;

pub type Destructor = sqlite_clientdata_sys::Destructor;

pub(crate) struct State {
    pub(crate) cache: Mutex<ConnectionCache>,
}

pub(crate) const NAME: &CStr = c"sqlite-compress";

static API: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

pub unsafe fn initialize(api: *const sqlite3_api_routines) {
    API.store(api.cast_mut().cast(), Ordering::Release);
}

pub unsafe fn set(
    api: *const sqlite3_api_routines,
    db: *mut sqlite3,
    name: &CStr,
    data: *mut c_void,
    destructor: Option<Destructor>,
) -> i32 {
    unsafe {
        sqlite_clientdata_sys::sqlite_clientdata_set(
            api.cast(),
            db.cast(),
            name.as_ptr(),
            data,
            destructor,
        )
    }
}

pub unsafe fn get(db: *mut sqlite3, name: &CStr) -> *mut c_void {
    let api = API.load(Ordering::Acquire);

    unsafe { sqlite_clientdata_sys::sqlite_clientdata_get(api, db.cast(), name.as_ptr()) }
}

pub(crate) fn with_cache<R>(
    db: *mut sqlite3,
    f: impl FnOnce(&Mutex<ConnectionCache>) -> R,
) -> Option<R> {
    let state_ptr = unsafe { get(db, NAME) };

    let state = unsafe { state_ptr.cast::<State>().as_ref() }?;

    Some(f(&state.cache))
}
