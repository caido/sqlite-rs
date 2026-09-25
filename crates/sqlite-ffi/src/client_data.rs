use std::ffi::{c_char, c_void, CString};

use libsqlite3_sys::{sqlite3, sqlite3_get_clientdata, sqlite3_set_clientdata, SQLITE_OK};

use crate::{
    error::SqliteError,
    utils::{ptr_as_ref, to_sqlite_destroy, XDestroy},
    Connection,
};

impl Connection {
    /// # Panics
    ///
    /// Panics if the client data pointer cannot be cast to `&T`
    pub fn get_client_data<'a, T>(&'a self, name: &str) -> Result<&'a T, SqliteError> {
        let c_name = CString::new(name)?;

        let client = unsafe { get_client_data_raw(self.conn.as_ptr(), c_name.as_ptr()) };

        if client.is_null() {
            return Err(SqliteError::PointerNotValid(format!(
                "client data for name: {}",
                name
            )));
        }

        let client = unsafe { ptr_as_ref::<T>(client) };

        Ok(client.expect("can't cast"))
    }

    pub fn set_client_data<T>(&self, name: &str, value: T) -> Result<(), SqliteError> {
        let c_name = CString::new(name)?;

        let pointer = Box::into_raw(Box::new(value));

        let (p, x_destroy) = unsafe { to_sqlite_destroy(pointer) };

        let rc = unsafe { set_client_data_raw(self.conn.as_ptr(), c_name.as_ptr(), p, x_destroy) };

        if rc != SQLITE_OK {
            return Err(SqliteError::Sqlite {
                operation: "set client data",
                code: rc,
                message: format!("cant't set client data for name: {}", name),
            });
        }

        Ok(())
    }
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
    use std::{
        ptr,
        sync::atomic::{AtomicBool, Ordering},
    };

    use libsqlite3_sys::{sqlite3_close, sqlite3_open};

    use super::*;

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
    fn destroy_drops_client_data_once() {
        static DROPPED: AtomicBool = AtomicBool::new(false);

        let (ptr, destroy) = unsafe {
            to_sqlite_destroy(Box::into_raw(Box::new(Counter {
                value: 7,
                dropped: &DROPPED,
            })))
        };

        assert!(!DROPPED.load(Ordering::SeqCst));
        unsafe { destroy.unwrap()(ptr) };
        assert!(DROPPED.load(Ordering::SeqCst));
    }

    #[test]
    fn destroy_ignores_null_pointer() {
        let (_ptr, destroy) = unsafe { to_sqlite_destroy::<u32>(ptr::null_mut()) };
        unsafe { destroy.unwrap()(ptr::null_mut()) };
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn set_then_get_returns_same_value() {
        static DROPPED: AtomicBool = AtomicBool::new(false);

        let mut db = ptr::null_mut();
        assert_eq!(unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) }, 0);

        let db = Connection::from_raw(db);

        let p = Box::into_raw(Box::new(Counter {
            value: 42,
            dropped: &DROPPED,
        }));
        db.set_client_data("demo", p).unwrap();

        let got = db.get_client_data::<Counter>("demo").unwrap();
        assert_eq!(got.value, 42);

        assert!(!DROPPED.load(Ordering::SeqCst));

        unsafe { sqlite3_close(db.conn.as_ptr()) };

        assert!(DROPPED.load(Ordering::SeqCst));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn get_missing_name_is_error() {
        let mut db = ptr::null_mut();
        assert_eq!(unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) }, 0);

        let db = Connection::from_raw(db);

        assert!(db.get_client_data::<Counter>("missing").is_err());

        unsafe { sqlite3_close(db.conn.as_ptr()) };
    }
}
