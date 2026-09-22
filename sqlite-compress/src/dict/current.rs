use sqlite_ffi::{first_value, Connection, SqlValue};

use crate::{
    dict::DICT_TABLE_NAME,
    utils::{quote_literal, quote_qualified},
    ColumnKey, DictError, DictId,
};

pub(crate) fn read_current_id<C: AsRef<Connection>>(
    connection: C,
    column: &ColumnKey,
) -> Result<Option<DictId>, DictError> {
    let table = quote_qualified(column.schema(), DICT_TABLE_NAME);

    let connection = connection.as_ref();

    let rows = connection
        .query(
            &format!(
                "SELECT COALESCE(MAX(id), 0) FROM {table} \
         WHERE table_name = ?1 AND column_name = ?2 AND is_current = 1",
            ),
            &[
                SqlValue::Text(quote_literal(column.table())),
                SqlValue::Text(quote_literal(column.column())),
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
