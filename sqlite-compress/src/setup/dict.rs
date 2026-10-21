use std::collections::HashSet;

use sqlite_ffi::Connection;

use super::{SetupConfig, SetupError};
use crate::{
    cache::warm_cache,
    dict::{ColumnKey, DICT_TABLE_NAME},
    setup::SqlIdent,
    state::ExtensionState,
};

/// For each schema in the [`SetupConfig`] checks if the dictionary table exists.
/// If not, create it.
fn ensure_table_exists<C: AsRef<Connection>>(
    connection: C,
    config: &SetupConfig,
) -> Result<(), SetupError> {
    let connection = connection.as_ref();

    let schemas: HashSet<_> = config.tables.iter().map(|table| &table.schema).collect();

    for schema in schemas {
        let schema = schema.quote();

        connection
            .batch_execute(&format!(
                "
        CREATE TABLE IF NOT EXISTS {schema}.{DICT_TABLE_NAME} (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            row_count INTEGER NOT NULL
        );
    "
            ))
            .map_err(SetupError::from_conn)?;
    }

    Ok(())
}

/// Create the dictionary store for [`SetupConfig`] tables, then warm the dict cache.
/// The warm cache is mandatory to ensure the dictionary is ready to be used.
pub(super) fn init_dict(state: &ExtensionState, config: &SetupConfig) -> Result<(), SetupError> {
    ensure_table_exists(state, config)?;

    for table in &config.tables {
        for column in &table.columns {
            let key = ColumnKey::new(
                table.schema.as_str(),
                table.name.as_str(),
                column.name.as_str(),
            );

            warm_cache(state, &key, config.compression_level)
                .map_err(|e| SetupError::DictTrain(e.to_string()))?;
        }
    }

    Ok(())
}
