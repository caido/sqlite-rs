use crate::{
    conn::Connection,
    dict::DICT_TABLE_NAME,
    utils::{quote_literal, quote_qualified},
    ColumnKey, DictError, DictId,
};

pub(crate) fn read_current_id(
    conn: &Connection,
    column: &ColumnKey,
) -> Result<Option<DictId>, DictError> {
    let table = quote_qualified(column.schema(), DICT_TABLE_NAME);

    let latest = conn
        .query_i64(&format!(
            "SELECT COALESCE(MAX(id), 0) FROM {table} \
         WHERE table_name = {} AND column_name = {} AND is_current = 1",
            quote_literal(column.table()),
            quote_literal(column.column())
        ))
        .map_err(|e| DictError::Connection(e.into()))?;

    if latest == 0 {
        return Ok(None);
    }

    let dict_id = DictId::try_from(latest)?;

    Ok(Some(dict_id))
}
