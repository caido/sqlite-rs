use super::{SetupConfig, SetupConnection, SetupError};
use crate::dict::{warm_cache, DICT_TABLE_NAME};

fn ensure_table_exists<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    let mut schemas_done = std::collections::HashSet::new();

    for table in &config.tables {
        if !schemas_done.insert(&table.schema) {
            continue;
        }

        let schema = table.as_qualified_schema_name();

        let count = conn
            .query_i64(&format!(
                "SELECT COUNT(*) FROM {schema}.sqlite_master \
         WHERE type = 'table' AND name = '{DICT_TABLE_NAME}'"
            ))
            .map_err(SetupError::from_conn)?;

        if count == 1 {
            continue;
        }

        conn.batch_execute(&format!(
            "
        CREATE TABLE IF NOT EXISTS {schema}.{DICT_TABLE_NAME} (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            dict BLOB NOT NULL,
            trained_at INTEGER NOT NULL,
            row_count INTEGER NOT NULL
        );
    "
        ))
        .map_err(SetupError::from_conn)?;
    }

    Ok(())
}

pub fn init_dict<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    ensure_table_exists(conn, config)?;
    warm_cache(conn, config.compression_level).map_err(|e| SetupError::DictTrain(e.to_string()))?;
    Ok(())
}
