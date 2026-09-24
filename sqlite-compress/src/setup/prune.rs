use std::collections::HashSet;

use sqlite_ffi::Connection;

use crate::{
    prune::PRUNE_TABLE_NAME,
    setup::{SetupConfig, SetupError, SqlIdent},
    state::ExtensionState,
};

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
        CREATE TABLE IF NOT EXISTS {schema}.{PRUNE_TABLE_NAME} (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            table_name TEXT NOT NULL,
            column_name TEXT NOT NULL,
            target_dict_id INTEGER NOT NULL,
            last_rowid INTEGER NOT NULL,
            UNIQUE(table_name, column_name)
        );
    "
            ))
            .map_err(SetupError::from_conn)?;
    }

    Ok(())
}

pub(super) fn init_table(state: &ExtensionState, config: &SetupConfig) -> Result<(), SetupError> {
    ensure_table_exists(state, config)?;

    Ok(())
}
