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
        let connection = Box::new(Self {
            db,
            cache: Mutex::new(ConnectionCache::new()),
        });

        db.set_client_data(Self::NAME, connection)
    }

    pub fn database(&self) -> &Database {
        &self.db
    }

    pub fn from_db(db: *mut sqlite3) -> Result<&'static Self, SqliteError> {
        let db = Database::from_raw(db);
        db.get_client_data(Self::NAME)
    }

    pub fn from_context(context: &Context) -> Result<&'static Self, SqliteError> {
        let db = Database::from_context(context)?;
        db.get_client_data(Self::NAME)
    }
}
