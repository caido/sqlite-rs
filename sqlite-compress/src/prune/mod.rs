mod errors;
mod queries;
mod types;

use std::time::{Duration, Instant};

pub use crate::prune::types::PruneStatus;
use crate::{
    dict::DICT_TABLE_NAME,
    functions::{compress, decompress},
    prune::{errors::PruneError, queries::Query},
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

    let current_ids = Query::select_current_ids(&state, &dict_table)?;

    for row in &current_ids {
        let table_name = row[0].as_text().expect("table_name should be a text");
        let column_name = row[1].as_text().expect("column_name should be a text");
        let current_id = row[2].as_i64().expect("id should be an integer");

        let current_id = DictId::new(current_id as u32);

        let quote_column = quote_identifier(column_name);

        let target = quote_qualified(schema, table_name);

        let mut after_rowid =
            Query::select_last_rowid(&state, &prune_table, table_name, column_name)?;

        loop {
            let rows = Query::select_rows_to_prune(
                &state,
                &quote_column,
                &target,
                after_rowid,
                BATCH_SIZE,
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

                let tx = state.connection.transaction()?;

                Query::insert_or_update_last_rowid(
                    &tx,
                    &prune_table,
                    table_name,
                    column_name,
                    current_id,
                    row_id,
                )?;

                Query::update_last_rowid(&tx, &target, &quote_column, compressed, row_id)?;

                tx.commit()?;
            }

            if start_timer.elapsed() > timeout {
                Query::insert_or_update_last_rowid(
                    &state,
                    &prune_table,
                    table_name,
                    column_name,
                    current_id,
                    after_rowid,
                )?;

                state.connection.wal_checkpoint(schema)?;

                return Ok(PruneStatus::Partial);
            }
        }

        let tx = state.connection.transaction()?;

        Query::delete_prune_row(&tx, &prune_table, table_name, column_name)?;

        Query::delete_older_dicts(&tx, &dict_table, current_id, table_name, column_name)?;

        tx.commit()?;

        state.connection.wal_checkpoint(schema)?;
    }

    Ok(PruneStatus::Done)
}
