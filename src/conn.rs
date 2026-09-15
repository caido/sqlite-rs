use libsqlite3_sys::{sqlite3, sqlite3_context};
use parking_lot::Mutex;
use sqlite_ffi::{set_client_data, SqliteError};

use crate::cache::ConnectionCache;

pub struct Connection {
    db: *mut sqlite3,
    pub(crate) cache: Mutex<ConnectionCache>,
}

impl Connection {
    const NAME: &str = "sqlite-compress";

    pub fn attach(db: *mut sqlite3) -> Result<(), SqliteError> {
        let raw = Box::into_raw(Box::new(Self {
            db,
            cache: Mutex::new(ConnectionCache::new()),
        }));

        match set_client_data(db, Self::NAME, raw) {
            Ok(()) => Ok(()),
            Err(e) => {
                drop(unsafe { Box::from_raw(raw) });
                Err(e)
            }
        }
    }

    pub fn query_blobs(&self, sql: &str) -> Result<Vec<Vec<u8>>, SqliteError> {
        sqlite_ffi::query_blobs(self.db, sql)
    }

    pub fn from_db(db: *mut sqlite3) -> Result<&'static Self, SqliteError> {
        sqlite_ffi::get_client_data(db, Self::NAME)
    }

    pub fn from_context(context: *mut sqlite3_context) -> Result<&'static Self, SqliteError> {
        let db = sqlite_ffi::get_context_db_handle(context)?;
        Self::from_db(db)
    }
}

#[derive(Debug)]
pub struct ConnError(String);

impl std::fmt::Display for ConnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ConnError {}
