use sqlite_ffi::{Connection, SqlValue, first_value};

use crate::{ColumnKey, DictError, DictId, dict::DICT_TABLE_NAME, utils::quote_qualified};

/// Read the current (latest inserted) dict id for `column` from `__compress_dicts`.
pub(crate) fn read_current_id<C: AsRef<Connection>>(
    connection: C,
    column: &ColumnKey,
) -> Result<Option<DictId>, DictError> {
    let table = quote_qualified(column.schema(), DICT_TABLE_NAME);

    let connection = connection.as_ref();

    let rows = connection
        .query(
            &format!(
                "SELECT id FROM {table} \
         WHERE table_name = ?1 AND column_name = ?2 ORDER BY id DESC LIMIT 1",
            ),
            &[
                SqlValue::Text(column.table().to_string()),
                SqlValue::Text(column.column().to_string()),
            ],
        )
        .map_err(|e| DictError::Connection(e.into()))?;

    let latest = first_value(&rows).and_then(SqlValue::as_i64).unwrap_or(0);

    if latest == 0 {
        return Ok(None);
    }

    let dict_id = DictId::try_from(latest)?;

    Ok(Some(dict_id))
}
