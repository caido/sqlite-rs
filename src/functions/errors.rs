use thiserror::Error;

use crate::dict::errors::DictError;

#[derive(Error, Debug)]
pub enum CodecError {
    #[error("dictionary error: {0}")]
    DictError(#[from] DictError),

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

    #[error("payload too large")]
    PayloadTooLarge,
    #[error("unknown codec: {0}")]
    UnknownCodec(u8),
    #[error("schema required")]
    SchemaRequired,
    #[error("unknown version: {0}")]
    UnknownVersion(u8),
    #[error("connection state missing")]
    MissingConnectionState,
}
