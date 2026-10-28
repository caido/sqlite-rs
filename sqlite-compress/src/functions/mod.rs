mod compress;
mod compressed_dict_id;
mod decompress;
mod errors;
mod header;
mod latest_dict_id;
#[cfg(feature = "like")]
mod like;
mod types;

/// Compression level used when configuration does not select another level.
pub const DEFAULT_LEVEL: types::Level = types::Level::new(3);

pub use compress::sqlite_compress;
pub use compressed_dict_id::sqlite_compressed_dict_id;
pub use decompress::sqlite_decompress;
pub(crate) use decompress::{decompress_raw, decompress_with_decoder};
pub use errors::CodecError;
pub use header::Header;
pub(crate) use header::ParsedBlob;
pub use latest_dict_id::sqlite_latest_dict_id;
#[cfg(feature = "like")]
pub use like::sqlite_decompress_like;
pub use types::Level;
