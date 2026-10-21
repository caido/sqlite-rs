use thiserror::Error;

#[derive(Debug, Error)]
pub enum RegexpError {
    #[error("expected argument 1 as content")]
    MissingContent,
    #[error("expected argument 1 as content of type blob or text")]
    InvalidContentType,
    #[error("expected argument 0 as pattern")]
    MissingPattern,
    #[error("pattern not valid regex: {0}")]
    InvalidPattern(#[from] regex::Error),
    #[error(transparent)]
    Sqlite(#[from] sqlite_ffi::SqliteError),
}
