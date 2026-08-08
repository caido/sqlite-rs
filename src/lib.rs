mod dict;
mod setup;
mod utils;

pub const DEFAULT_LEVEL: i32 = 3;
pub const DEFAULT_RETRAIN_GROWTH: usize = 5000;

pub use dict::errors::DictError;
pub use dict::{get_decoder, get_encoder, train, DictId, LATEST_DICT_ID};
pub use setup::{
    setup, ColumnName, DictStore, SchemaName, SetupConfig, SetupConnection, SetupError, SetupTable,
    TableName,
};
