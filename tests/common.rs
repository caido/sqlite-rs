use std::{ptr, sync::Once};

use libsqlite3_sys::{sqlite3, sqlite3_auto_extension, sqlite3_close, sqlite3_open, SQLITE_OK};
use sqlite_compress::{Connection, SetupConnection, sqlite3_compress_init};

#[allow(dead_code)]
pub const DEFAULT_MIN_SAMPLES: usize = 1000;
#[allow(dead_code)]
pub const DEFAULT_MAX_SAMPLES: usize = 10000;

pub struct TestDb {
    db: *mut sqlite3,
}

impl TestDb {
    pub fn open() -> Self {
        static REGISTER: Once = Once::new();
        REGISTER.call_once(|| unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite3_compress_init as *const (),
            )));
        });

        let mut db = ptr::null_mut();
        let rc = unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) };
        assert_eq!(rc, SQLITE_OK);

        Self { db }
    }

    pub fn conn(&self) -> &'static Connection {
        Connection::from_db(self.db).unwrap()
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        unsafe { sqlite3_close(self.db) };
    }
}

impl SetupConnection for TestDb {
    fn sqlite_handle(&self) -> *mut sqlite3 {
        self.db
    }
}
