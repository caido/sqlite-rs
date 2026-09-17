use libsqlite3_sys::sqlite3;
use parking_lot::Mutex;
use sqlite_ffi::{Connection, Context, SqliteError};

use crate::cache::DictCache;

pub struct ExtensionState {
    pub(crate) connection: Connection,
    pub(crate) cache: Mutex<DictCache>,
}

impl AsRef<Connection> for ExtensionState {
    fn as_ref(&self) -> &Connection {
        &self.connection
    }
}

impl ExtensionState {
    const NAME: &str = "sqlite-compress";

    pub fn attach(connection: &Connection) -> Result<(), SqliteError> {
        let state = Self {
            connection: Connection::from_raw(connection.as_ptr()),
            cache: Mutex::new(DictCache::new()),
        };

        connection.set_client_data(Self::NAME, state)
    }

    pub fn from_db<'a>(db: *mut sqlite3) -> Result<&'a Self, SqliteError> {
        let db = Connection::from_raw(db);
        db.get_client_data(Self::NAME)
    }

    pub fn from_context(context: &Context) -> Result<&Self, SqliteError> {
        let db = Connection::from_context(context)?;
        db.get_client_data(Self::NAME)
    }
}
