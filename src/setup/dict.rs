use std::collections::HashSet;

use super::{SetupConfig, SetupConnection, SetupError};
use crate::{
    cache::warm_cache,
    conn::Connection,
    dict::{ColumnKey, DICT_TABLE_NAME},
    setup::SqlIdent,
};

/// For each schema in the [`SetupConfig`] checks if the dictionary table exists.
/// If not, create it.
fn ensure_table_exists<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    let schemas: HashSet<_> = config.tables.iter().map(|table| &table.schema).collect();

    for schema in schemas {
        let schema = schema.quote();

        conn.batch_execute(&format!(
            "
        CREATE TABLE IF NOT EXISTS {schema}.{DICT_TABLE_NAME} (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_count INTEGER NOT NULL,
            is_current INTEGER NOT NULL DEFAULT 0
        );
    "
        ))
        .map_err(SetupError::from_conn)?;
    }

    Ok(())
}

/// Create the dictionary store for [`SetupConfig`] tables, then warm the dict cache.
/// The warm cache is mandatory to ensure the dictionary is ready to be used.
/// By [get_decoder_cached](crate::dict::get_decoder_cached), [get_encoder_cached](crate::dict::get_encoder_cached),
/// the dictionary is cached in memory.
pub(super) fn init_dict<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    ensure_table_exists(conn, config)?;

    for table in &config.tables {
        for column in &table.columns {
            let key = ColumnKey::new(
                table.schema.as_str(),
                table.name.as_str(),
                column.name.as_str(),
            );

            let client = Connection::from_db(conn.sqlite_handle()).unwrap();

            warm_cache(&client.cache, conn, &key, config.compression_level)
                .map_err(|e| SetupError::DictTrain(e.to_string()))?;
        }
    }

    Ok(())
}
