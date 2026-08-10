mod compress;
mod decompress;
mod errors;
mod header;

pub const DEFAULT_LEVEL: i32 = 3;

pub use compress::{compress, sqlite_compress};
pub use decompress::{decompress, sqlite_decompress};
pub use header::Header;
