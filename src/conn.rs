use libsqlite3_sys::sqlite3;
use parking_lot::Mutex;
use sqlite_ffi::{Context, Database, SqliteError};

use crate::cache::ConnectionCache;

pub struct Connection {
    pub(crate) db: Database,
    pub(crate) cache: Mutex<ConnectionCache>,
}

impl Connection {
    const NAME: &str = "sqlite-compress";

    pub fn attach(db: Database) -> Result<(), SqliteError> {
        let raw = Box::into_raw(Box::new(Self {
            db,
            cache: Mutex::new(ConnectionCache::new()),
        }));

        match db.set_client_data(Self::NAME, raw) {
            Ok(()) => Ok(()),
            Err(e) => {
                drop(unsafe { Box::from_raw(raw) });
                Err(e)
            }
        }
    }

    pub fn from_db(db: *mut sqlite3) -> Result<&'static Self, SqliteError> {
        let db = Database::from_raw(db);
        db.get_client_data(Self::NAME)
    }

    pub fn from_context(context: &Context) -> Result<&'static Self, SqliteError> {
        let db = Database::from_context(&context)?;
        db.get_client_data(Self::NAME)
    }
}
