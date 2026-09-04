use std::collections::HashSet;

use super::{SetupConfig, SetupConnection, SetupError};
use crate::{
    cache::{warm_cache, CacheKeySource},
    dict::{ColumnKey, DICT_TABLE_NAME},
    setup::SqlIdent,
};

/// Creates one dictionary store per configured schema when it is absent.
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

/// Prepares dictionary storage and synchronizes cached current dictionaries.
///
/// Warming ensures [`crate::functions::sqlite_compress`] can select existing
/// dictionaries before the first compression call.
pub(super) fn init_dict<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection + CacheKeySource,
{
    ensure_table_exists(conn, config)?;

    for table in &config.tables {
        for column in &table.columns {
            let key = ColumnKey::new(
                table.schema.as_str(),
                table.name.as_str(),
                column.name.as_str(),
            );
            warm_cache(conn, &key, config.compression_level)
                .map_err(|e| SetupError::DictTrain(e.to_string()))?;
        }
    }

    Ok(())
}
