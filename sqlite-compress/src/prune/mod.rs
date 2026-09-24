mod errors;
mod types;

use std::time::{Duration, Instant};

use sqlite_ffi::{first_value, SqlValue};

pub use crate::prune::types::PruneStatus;

use crate::{
    dict::DICT_TABLE_NAME,
    functions::{compress, decompress},
    prune::errors::PruneError,
    utils::{quote_identifier, quote_qualified},
    DictId, ExtensionState, Header, SetupConnection,
};

pub(crate) static PRUNE_TABLE_NAME: &str = "__compress_prune";

const BATCH_SIZE: i64 = 100;

pub fn prune<C: SetupConnection>(
    connection: &C,
    schema: &str,
    timeout: Option<Duration>,
) -> Result<PruneStatus, PruneError> {
    let timeout = timeout.unwrap_or(Duration::from_hours(1));

    let start_timer = Instant::now();

    let state = ExtensionState::from_db(unsafe { connection.sqlite_handle() })?;

    let dict_table = quote_qualified(schema, DICT_TABLE_NAME);
    let prune_table = quote_qualified(schema, PRUNE_TABLE_NAME);

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

        let rows = state.connection.query(
            &format!(
                "SELECT last_rowid FROM {prune_table} WHERE table_name = ?1 AND column_name = ?2 "
            ),
            &[
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
            ],
        )?;

        let mut after_rowid = first_value(&rows).and_then(SqlValue::as_i64).unwrap_or(0);

        loop {
            let rows = state.connection.query(
                &format!(
                    "SELECT {quote_column}, rowid FROM {target} \
                 WHERE rowid > ?1 ORDER BY rowid LIMIT ?2"
                ),
                &[
                    SqlValue::Integer(after_rowid),
                    SqlValue::Integer(BATCH_SIZE),
                ],
            )?;

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

                after_rowid = row_id;

                let (header, _) = Header::parse(data)?;

                if header.dict_id.get() >= current_id.get() {
                    continue;
                }

                let decompressed = decompress(&state, data, schema)?;
                let compressed = compress(&state, schema, current_id, &decompressed)?;

                state.connection.execute(
                    &format!("INSERT INTO {prune_table} (table_name, column_name, target_dict_id, last_rowid) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(table_name, column_name) DO UPDATE SET  last_rowid = excluded.last_rowid,target_dict_id = excluded.target_dict_id"),
                    &[SqlValue::Text(table_name.to_string()), SqlValue::Text(column_name.to_string()), SqlValue::Integer(current_id.get().into()), SqlValue::Integer(row_id)],
                )?;

                state.connection.execute(
                    &format!("UPDATE {target} SET {quote_column} = ?1 WHERE rowid = ?2"),
                    &[SqlValue::Blob(compressed), SqlValue::Integer(row_id)],
                )?;
            }

            if start_timer.elapsed() > timeout {
                state.connection.execute(
                    &format!(
                        "INSERT INTO {prune_table} \
             (table_name, column_name, target_dict_id, last_rowid) \
             VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(table_name, column_name) DO UPDATE SET \
             last_rowid = excluded.last_rowid, \
             target_dict_id = excluded.target_dict_id"
                    ),
                    &[
                        SqlValue::Text(table_name.to_string()),
                        SqlValue::Text(column_name.to_string()),
                        SqlValue::Integer(current_id.get().into()),
                        SqlValue::Integer(after_rowid),
                    ],
                )?;
                return Ok(PruneStatus::Partial);
            }
        }

        state.connection.execute(
            &format!("DELETE FROM {prune_table} WHERE table_name = ?1 AND column_name = ?2"),
            &[
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
            ],
        )?;

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

    Ok(PruneStatus::Done)
}
