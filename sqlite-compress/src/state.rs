use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use libsqlite3_sys::sqlite3;
use parking_lot::Mutex;
use sqlite_ffi::{Connection, Context, SqliteError};

use crate::cache::DictCache;

static CACHES: LazyLock<Mutex<HashMap<usize, Cache>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
    pub fn attach(connection: &Connection) -> Result<(), SqliteError> {
        let cache = Cache(Arc::new(Mutex::new(DictCache::new())));
        CACHES.lock().insert(connection.as_ptr() as usize, cache);

        Ok(())
    }

    pub fn detach(connection: &Connection) {
        CACHES.lock().remove(&(connection.as_ptr() as usize));
    }

    fn lookup(db: *mut sqlite3) -> Result<Self, SqliteError> {
        let cache = CACHES
            .lock()
            .get(&(db as usize))
            .cloned()
            .ok_or_else(|| SqliteError::PointerNotValid("compress cache".into()))?;

        Ok(Self {
            connection: Connection::from_raw(db),
            cache,
        })
    }

    pub fn from_db(db: *mut sqlite3) -> Result<Self, SqliteError> {
        Self::lookup(db)
    }

    pub fn from_context(context: &Context) -> Result<Self, SqliteError> {
        let connection = Connection::from_context(context)?;
        Self::lookup(connection.as_ptr())
    }
}
