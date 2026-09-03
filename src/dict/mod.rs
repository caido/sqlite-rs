use crate::{dict::errors::DictError, setup::DictStore, utils::quote_qualified};

mod current;
pub mod errors;
mod train;
mod types;

pub(crate) use current::read_current_id;
pub use train::{train_all, train_by_column};
pub(crate) use types::DictKey;
pub use types::{ColumnKey, DictId};

pub static DICT_TABLE_NAME: &str = "__compress_dicts";

pub(crate) fn load_raw_dict<C: DictStore>(
    dict_key: &DictKey,
    conn: &mut C,
) -> Result<Vec<u8>, DictError> {
    let table = quote_qualified(&dict_key.schema, DICT_TABLE_NAME);
    let rows = conn
        .query_blobs(&format!(
            "SELECT dict AS value FROM {table} WHERE id = {}",
            dict_key.id.get()
        ))
        .map_err(|e| DictError::Connection(e.into()))?;

    if rows.is_empty() {
        return Err(DictError::NotReady);
    }

    rows.into_iter()
        .next()
        .ok_or(DictError::NotFound(dict_key.id))
}
