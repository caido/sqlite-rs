use std::ffi::CString;

use sqlite_loadable::{
    ext::{
        sqlite3ext_column_value, sqlite3ext_finalize, sqlite3ext_prepare_v2, sqlite3ext_step,
        sqlite3ext_value_blob, sqlite3ext_value_bytes,
    },
    prelude::*,
};

use crate::{
    cache::{CacheKeySource, DbKey},
    setup::DictStore,
};

const SQLITE_OK: i32 = 0;
const SQLITE_ROW: i32 = 100;
const SQLITE_DONE: i32 = 101;

/// Adapter that exposes the current SQLite connection through setup traits.
pub struct SqliteConn {
    db: *mut sqlite3,
    key: DbKey,
}

impl SqliteConn {
    /// Recovers the SQLite connection that invoked an extension function.
    pub fn from_context(context: *mut sqlite3_context) -> Self {
        let db = sqlite_loadable::api::context_db_handle(context);
        Self {
            db,
            key: DbKey::for_handle(db as *mut std::ffi::c_void),
        }
    }
}

impl CacheKeySource for SqliteConn {
    fn db_key(&self) -> DbKey {
        self.key
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

impl DictStore for SqliteConn {
    type Error = ConnError;

    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error> {
        let mut stmt = std::ptr::null_mut();
        let c_sql = CString::new(sql).map_err(|e| ConnError(e.to_string()))?;

        // SAFETY: `self.db` comes from SQLite's invocation context and `c_sql`
        // remains alive for the entire prepare call.
        let rc = unsafe {
            sqlite3ext_prepare_v2(self.db, c_sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut())
        };
        if rc != SQLITE_OK {
            return Err(ConnError(format!("prepare failed: {rc}")));
        }

        let mut out = Vec::new();
        // SAFETY: SQLite owns `stmt` after successful preparation. Each BLOB is
        // copied before the next step or finalization invalidates its pointer.
        unsafe {
            loop {
                match sqlite3ext_step(stmt) {
                    SQLITE_ROW => {
                        let value = sqlite3ext_column_value(stmt, 0);
                        let len = sqlite3ext_value_bytes(value) as usize;
                        let ptr = sqlite3ext_value_blob(value) as *const u8;
                        if ptr.is_null() {
                            out.push(Vec::new());
                        } else {
                            out.push(std::slice::from_raw_parts(ptr, len).to_vec());
                        }
                    }
                    SQLITE_DONE => break,
                    code => {
                        sqlite3ext_finalize(stmt);
                        return Err(ConnError(format!("step failed: {code}")));
                    }
                }
            }
            sqlite3ext_finalize(stmt);
        }
        Ok(out)
    }
}
