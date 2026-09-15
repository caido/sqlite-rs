use thiserror::Error;

#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("SQLite error (code {code}): {message}")]
    Sqlite { code: i32, message: String },

    #[error("Invalid string: {0}")]
    InvalidString(#[from] std::ffi::NulError),

    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    #[error("Invalid client name: {0}")]
    InvalidClientName(String),
}
