use crate::{dict::errors::DictError, utils::quote_qualified};

mod current;
pub mod errors;
mod train;
mod types;

pub(crate) use current::read_current_id;
use sqlite_ffi::{first_value, Connection, SqlValue};
pub use train::{train_all, train_by_column};
pub(crate) use types::DictKey;
pub use types::{ColumnKey, DictId};

/// Name of the per-schema table that stores trained dictionaries.
pub static DICT_TABLE_NAME: &str = "__compress_dicts";

pub(crate) fn load_raw_dict<C: AsRef<Connection>>(
    dict_key: &DictKey,
    connection: C,
) -> Result<Vec<u8>, DictError> {
    let table = quote_qualified(&dict_key.schema, DICT_TABLE_NAME);

    let connection = connection.as_ref();

    let rows = connection
        .query(
            &format!("SELECT dict AS value FROM {table} WHERE id = ?1",),
            &[SqlValue::Integer(dict_key.id.get() as i64)],
        )
        .map_err(|e| DictError::Connection(e.into()))?;

    if rows.is_empty() {
        return Err(DictError::NotReady);
    }

    first_value(&rows)
        .and_then(SqlValue::as_blob)
        .map(|b| b.to_vec())
        .ok_or(DictError::NotFound(dict_key.id))
}
