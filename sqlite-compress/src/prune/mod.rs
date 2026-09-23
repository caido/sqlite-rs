mod errors;

use sqlite_ffi::SqlValue;

use crate::{
    dict::DICT_TABLE_NAME,
    functions::{compress, decompress},
    prune::errors::PruneError,
    utils::{quote_identifier, quote_qualified},
    DictId, ExtensionState, Header, SetupConnection,
};

const BATCH_SIZE: i64 = 100;

pub fn prune<C: SetupConnection>(connection: &C, schema: &str) -> Result<(), PruneError> {
    let state = ExtensionState::from_db(unsafe { connection.sqlite_handle() })?;

    let dict_table = quote_qualified(schema, DICT_TABLE_NAME);

    let current_ids = state.connection.query(
        &format!("SELECT table_name, column_name, MAX(id) AS id FROM {dict_table} GROUP BY table_name, column_name"),
        &[],
    )?;

    for row in &current_ids {
        let table_name = row[0].as_text().expect("table_name should be a text");
        let column_name = row[1].as_text().expect("column_name should be a text");
        let current_id = row[2].as_i64().expect("id should be an integer");

        let current_id = DictId::new(current_id as u32);

        let quote_column = quote_identifier(column_name);

        let target = quote_qualified(schema, table_name);

        let mut after_rowid: Option<i64> = None;

        loop {
            let rows = match after_rowid {
                None => state.connection.query(
                    &format!(
                        "SELECT {quote_column}, rowid FROM {target} \
                 ORDER BY rowid LIMIT ?1"
                    ),
                    &[SqlValue::Integer(BATCH_SIZE)],
                )?,
                Some(after_rowid) => state.connection.query(
                    &format!(
                        "SELECT {quote_column}, rowid FROM {target} \
                 WHERE rowid > ?1 ORDER BY rowid LIMIT ?2"
                    ),
                    &[
                        SqlValue::Integer(after_rowid),
                        SqlValue::Integer(BATCH_SIZE),
                    ],
                )?,
            };

            if rows.is_empty() {
                break;
            }

            for row in &rows {
                let data = row[0].as_blob().ok_or_else(|| PruneError::BadColumn {
                    column: column_name.to_string(),
                    expected: "blob",
                })?;

                let row_id = row[1].as_i64().ok_or_else(|| PruneError::BadColumn {
                    column: column_name.to_string(),
                    expected: "integer rowid",
                })?;

                after_rowid = Some(row_id);

                let (header, _) = Header::parse(data)?;

                if header.dict_id.get() == current_id.get() {
                    continue;
                }

                let decompressed = decompress(&state, data, schema)?;
                let compressed = compress(&state, schema, current_id, &decompressed)?;

                state.connection.execute(
                    &format!("UPDATE {target} SET {quote_column} = ?1 WHERE rowid = ?2"),
                    &[SqlValue::Blob(compressed), SqlValue::Integer(row_id)],
                )?;
            }
        }

        state.connection.execute(
            &format!(
                "DELETE FROM {dict_table} WHERE id < ?1 AND table_name = ?2 AND column_name = ?3"
            ),
            &[
                SqlValue::Integer(i64::from(current_id.get())),
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
            ],
        )?;
    }
    Ok(())
}
