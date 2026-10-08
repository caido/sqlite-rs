use std::error::Error as StdError;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum SetupError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] sqlite_ffi::SqliteError),
    #[error("connection error: {0}")]
    Connection(#[source] Box<dyn StdError + Send + Sync>),
    #[error("table not found: {0}")]
    TableNotFound(String),
    #[error("column {column} not found in {table}")]
    ColumnNotFound { table: String, column: String },
    #[error("invalid config: {0}")]
    InvalidConfig(&'static str),
    #[error("dict train error: {0}")]
    DictTrain(String),
    #[error("connection state missing")]
    MissingConnectionState,
    #[error("dict train cancelled")]
    Cancelled,
    #[error("name conflict: {name} already exists as {existing_type}")]
    NameConflict { name: String, existing_type: String },
}

impl SetupError {
    pub fn from_conn(err: impl StdError + Send + Sync + 'static) -> Self {
        Self::Connection(Box::new(err))
    }
}
