mod compress;
mod decompress;
mod errors;
mod header;
mod types;

pub const DEFAULT_LEVEL: types::Level = types::Level::new(3);

pub(crate) use compress::compress;
pub use compress::sqlite_compress;
pub use decompress::sqlite_decompress;
pub(crate) use decompress::{decompress, decompress_raw, decompress_with_decoder};
pub use errors::CodecError;
pub use header::Header;
pub use types::Level;
