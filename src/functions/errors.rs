use thiserror::Error;

use crate::dict::errors::DictError;

/// Describes a failure while encoding or decoding an extension payload.
#[derive(Error, Debug)]
pub enum CodecError {
    #[error("compression requires one argument")]
    CompressionRequiresOneArgument,
    #[error("compression failed: {0}")]
    CompressionFailed(std::io::Error),
    #[error("decompression failed: {0}")]
    DecompressionFailed(std::io::Error),
    #[error("decompression requires one argument")]
    DecompressionRequiresOneArgument,
    #[error("malformed header: expected at least 4 bytes")]
    MalformedHeader,
    #[error("dict not found")]
    DictNotFound,
    #[error("dict not ready")]
    DictNotReady,
    #[error("dict error: {0}")]
    DictError(DictError),
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("unknown codec: {0}")]
    UnknownCodec(u8),
    #[error("schema required")]
    SchemaRequired,
    #[error("unknown version: {0}")]
    UnknownVersion(u8),
}

impl From<CodecError> for sqlite_loadable::Error {
    fn from(err: CodecError) -> Self {
        sqlite_loadable::Error::new_message(err.to_string())
    }
}
