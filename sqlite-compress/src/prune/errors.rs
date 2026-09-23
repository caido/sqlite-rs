use thiserror::Error;

use crate::CodecError;

#[derive(Error, Debug)]
pub enum PruneError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] sqlite_ffi::SqliteError),
    #[error("Codec error: {0}")]
    Codec(#[from] CodecError),
    #[error("{column}: expected {expected}")]
    BadColumn {
        column: String,
        expected: &'static str,
    },
}
