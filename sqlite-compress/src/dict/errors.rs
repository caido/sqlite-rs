use std::error::Error as StdError;

use thiserror::Error;

use crate::dict::types::DictId;

#[derive(Error, Debug)]
pub enum DictError {
    #[error("dictionary not ready")]
    NotReady,

    #[error("dict connection error: {0}")]
    Connection(Box<dyn StdError + Send + Sync>),

    #[error("dictionary id {0} not found")]
    NotFound(DictId),
}
