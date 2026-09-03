use crate::{
    dict::DICT_TABLE_NAME,
    utils::{quote_literal, quote_qualified},
    ColumnKey, DictError, DictId, SetupConnection,
};

pub(crate) fn read_current_id<C: SetupConnection>(
    conn: &mut C,
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

    let dict_id = u32::try_from(latest)
        .map(DictId::new)
        .map_err(|_| DictError::NotFound(DictId::new(0)))?;

    Ok(Some(dict_id))
}
