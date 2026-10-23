use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MatcherError {
    #[error("invalid LIKE pattern: {0}")]
    Pattern(String),

    #[error(transparent)]
    Io(#[from] io::Error),
}
