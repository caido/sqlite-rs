mod dict;
mod setup;
mod utils;

pub use dict::errors::DictError;
pub use dict::{get_decoder, get_encoder, train, LATEST_DICT_ID};
pub use setup::{setup, DictStore, SetupConfig, SetupConnection, SetupError, SetupTable};