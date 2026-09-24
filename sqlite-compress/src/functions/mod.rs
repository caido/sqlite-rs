mod clike;
mod compress;
mod decompress;
mod errors;
mod header;
mod like;
mod types;

pub const DEFAULT_LEVEL: types::Level = types::Level::new(3);

pub use clike::sqlite_clike;
pub use compress::sqlite_compress;
pub use decompress::sqlite_decompress;
pub(crate) use decompress::{decompress_raw, decompress_with_decoder};
pub use errors::CodecError;
pub use header::Header;
pub use types::Level;
