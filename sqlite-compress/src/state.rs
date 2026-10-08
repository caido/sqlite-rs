use std::sync::{Arc, LazyLock};

use libsqlite3_sys::sqlite3;
use parking_lot::Mutex;
use sqlite_ffi::{Connection, Context, SqliteError};

use crate::cache::DictCache;

// Shared cache for all connections.
static SHARED: LazyLock<Cache> = LazyLock::new(|| Cache(Arc::new(Mutex::new(DictCache::new()))));

/// Per-connection handle: this SQLite connection plus a clone of the process-wide dictionary cache.
///
/// `sqlite3_compress_init` calls [`ExtensionState::attach`], which stores a clone of the shared
/// [`Cache`] in the connection's client data (not a fresh empty cache).
/// `setup`, `train_all`, `compress`, and `decompress` read it back with [`ExtensionState::from_db`]
/// or [`ExtensionState::from_context`].
/// All connections therefore share the same `current_ids`; prepared encoders/decoders live in the
/// separate process-wide pool.
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
        connection.set_client_data(Self::NAME, SHARED.clone())
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
