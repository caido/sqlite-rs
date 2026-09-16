use thiserror::Error;

#[derive(Debug, Error)]
pub enum SqliteError {
    #[error("SQLite {operation} failed (code {code}): {message}")]
    Sqlite {
        operation: &'static str,
        code: i32,
        message: String,
    },

    #[error("Invalid string: {0}")]
    InvalidString(#[from] std::ffi::NulError),

    #[error("Invalid UTF-8: {0}")]
    InvalidUtf8(#[from] std::str::Utf8Error),

    #[error("Pointer not valid: {0}")]
    PointerNotValid(String),
}
