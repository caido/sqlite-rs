use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MatcherError {
    #[error(transparent)]
    Io(#[from] io::Error),
}
