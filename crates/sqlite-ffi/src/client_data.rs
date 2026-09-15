use std::ffi::{CString, c_char, c_void};

use libsqlite3_sys::{SQLITE_OK, sqlite3, sqlite3_get_clientdata, sqlite3_set_clientdata};

use crate::error::SqliteError;

type XDestroy = Option<unsafe extern "C" fn(*mut c_void)>;

pub fn get_client_data<T>(db: *mut sqlite3, name: &str) -> Result<&T, SqliteError> {
    let c_name = CString::new(name);

    let client = unsafe { get_client_data_raw(db, c_name.unwrap().as_ptr()) };

    if client.is_null() {
        return Err(SqliteError::InvalidClientName(name.to_string()));
    }

    let client = client.cast::<T>();

    let client = unsafe { client.as_ref() };

    Ok(client.expect("can't cast"))
}

pub fn set_client_data<T>(db: *mut sqlite3, name: &str, p: *mut T) -> Result<(), SqliteError> {
    let c_name = CString::new(name);

    let (p, x_destroy) = unsafe { to_sqlite_destroy(p) };

    let rc = unsafe { set_client_data_raw(db, c_name.unwrap().as_ptr(), p, x_destroy) };

    if rc != SQLITE_OK {
        return Err(SqliteError::Sqlite {
            code: rc,
            message: "cant't set client data".into(),
        });
    }

    Ok(())
}

unsafe fn to_sqlite_destroy<T>(p: *mut T) -> (*mut c_void, XDestroy) {
    unsafe extern "C" fn destroy<T>(p: *mut c_void) {
        if !p.is_null() {
            unsafe {
                drop(Box::from_raw(p.cast::<T>()));
            }
        }
    }

    (p.cast(), Some(destroy::<T>))
}

unsafe fn get_client_data_raw(db: *mut sqlite3, name: *const c_char) -> *mut c_void {
    unsafe { sqlite3_get_clientdata(db, name) }
}

unsafe fn set_client_data_raw(
    db: *mut sqlite3,
    name: *const c_char,
    p: *mut c_void,
    x_destroy: XDestroy,
) -> i32 {
    unsafe { sqlite3_set_clientdata(db, name, p, x_destroy) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libsqlite3_sys::{sqlite3_close, sqlite3_open};
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};

    struct Counter {
        value: u32,
        dropped: &'static AtomicBool,
    }

    impl Drop for Counter {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn set_then_get_returns_same_value() {
        static DROPPED: AtomicBool = AtomicBool::new(false);

        let mut db = ptr::null_mut();
        assert_eq!(unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) }, 0);

        let p = Box::into_raw(Box::new(Counter {
            value: 42,
            dropped: &DROPPED,
        }));
        set_client_data(db, "demo", p).unwrap();

        let got = get_client_data::<Counter>(db, "demo").unwrap();
        assert_eq!(got.value, 42);

        assert!(!DROPPED.load(Ordering::SeqCst));

        unsafe { sqlite3_close(db) };

        assert!(DROPPED.load(Ordering::SeqCst));
    }

    #[test]
    fn get_missing_name_is_error() {
        let mut db = ptr::null_mut();
        assert_eq!(unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) }, 0);

        assert!(get_client_data::<Counter>(db, "missing").is_err());

        unsafe { sqlite3_close(db) };
    }
}
