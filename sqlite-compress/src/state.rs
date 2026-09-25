use std::sync::Arc;

use libsqlite3_sys::sqlite3;
use parking_lot::Mutex;
use sqlite_ffi::{Connection, Context, SqliteError};

use crate::cache::DictCache;

/// Per-connection state for the extension: the SQLite connection and its dictionary cache.
///
/// `sqlite3_compress_init` calls [`ExtensionState::attach`], which stores an empty [`Cache`] in the connection's client data.
/// `setup`, `train_all`, `compress`, and `decompress` read that same cache back with [`ExtensionState::from_db`] or [`ExtensionState::from_context`].
/// The cache is an `Arc`, so every lookup shares the encoders, decoders, and current dict ids for this connection.
pub struct ExtensionState {
    pub(crate) connection: Connection,
    pub(crate) cache: Cache,
}

#[derive(Clone)]
pub struct Cache(Arc<Mutex<DictCache>>);

impl Cache {
    pub(crate) fn lock(&self) -> parking_lot::MutexGuard<'_, DictCache> {
        self.0.lock()
    }
}

impl AsRef<Connection> for ExtensionState {
    fn as_ref(&self) -> &Connection {
        &self.connection
    }
}

impl ExtensionState {
    const NAME: &str = "sqlite-compress";

    pub fn attach(connection: &Connection) -> Result<(), SqliteError> {
        let cache = Cache(Arc::new(Mutex::new(DictCache::new())));

        connection.set_client_data(Self::NAME, cache)
    }

    pub fn from_db(db: *mut sqlite3) -> Result<Self, SqliteError> {
        let connection = Connection::from_raw(db);
        let cache = connection.get_client_data::<Cache>(Self::NAME)?.clone();

        Ok(Self { cache, connection })
    }

    pub fn from_context(context: &Context) -> Result<Self, SqliteError> {
        let connection = Connection::from_context(context)?;
        let cache = connection.get_client_data::<Cache>(Self::NAME)?.clone();

        Ok(Self { cache, connection })
    }
}
