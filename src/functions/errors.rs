use crate::dict::errors::DictError;
use sqlite_loadable::Error;

#[derive(Debug)]
pub enum CodecError {
    CompressionRequiresOneArgument,
    CompressionFailed(std::io::Error),
    DecompressionFailed(std::io::Error),
    DecompressionRequiresOneArgument,
    MalformedHeader,
    DictNotFound,
    DictNotReady,
    DictError(DictError),
}

impl std::error::Error for CodecError {} //TODO

impl From<CodecError> for Error {
    fn from(err: CodecError) -> Self {
        Error::new_message(err.to_string())
    }
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DecompressionRequiresOneArgument => {
                write!(f, "decompression requires one argument")
            }
            Self::MalformedHeader => write!(f, "malformed header: expected at least 4 bytes"),
            Self::CompressionRequiresOneArgument => write!(f, "compression requires one argument"),
            Self::CompressionFailed(e) => write!(f, "compression failed: {e}"),
            Self::DecompressionFailed(e) => write!(f, "decompression failed: {e}"),
            Self::DictNotFound => write!(f, "dict not found"),
            Self::DictNotReady => write!(f, "dict not ready"),
            Self::DictError(e) => write!(f, "dict error: {e}"),
        }
    }
}
