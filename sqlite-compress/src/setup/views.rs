use sqlite_ffi::{Connection, SqlValue, column_texts, first_value};

use crate::{
    SetupTable,
    setup::{
        SetupConfig, SetupError, SqlIdent,
        config::{ColumnName, SchemaName, TableName},
    },
    state::ExtensionState,
    utils::{quote_identifier, quote_literal},
};

/// Doing the check if the table exists in the database.
/// Based on the schema and the table name.
fn ensure_table_exists<C: AsRef<Connection>>(
    connection: C,
    schema: &SchemaName,
    table_name: &TableName,
) -> Result<(), SetupError> {
    let connection = connection.as_ref();

    let rows = connection
        .query(
            &format!(
                "SELECT COUNT(*) FROM {schema}.sqlite_master \
                 WHERE type = 'table' AND name = ?1"
            ),
            &[SqlValue::Text(table_name.as_str().to_string())],
        )
        .map_err(SetupError::from_conn)?;

    let count = first_value(&rows).and_then(SqlValue::as_i64).unwrap_or(0);

    if count == 0 {
        return Err(SetupError::TableNotFound(format!(
            "{}.{}",
            schema.as_str(),
            table_name.as_str()
        )));
    }

    Ok(())
}

/// For each column in the table, check if the column exists in the database.
fn ensure_column_exist<C: AsRef<Connection>>(
    connection: &C,
    table: &TableName,
    schema: &SchemaName,
    column: &ColumnName,
) -> Result<(), SetupError> {
    let connection = connection.as_ref();

    let rows = connection
        .query(
            &format!("SELECT COUNT(*) FROM pragma_table_info({table}, {schema}) WHERE name = ?1"),
            &[SqlValue::Text(column.as_str().to_string())],
        )
        .map_err(SetupError::from_conn)?;

    let count = first_value(&rows).and_then(SqlValue::as_i64).unwrap_or(0);

    if count == 0 {
        return Err(SetupError::ColumnNotFound {
            table: table.as_str().to_string(),
            column: column.as_str().to_string(),
        });
    }

    Ok(())
}

/// Ensure the view name is not conflicting with the table name.
fn ensure_name_is_not_conflicting<C: AsRef<Connection>>(
    connection: &C,
    table: &SetupTable,
) -> Result<(), SetupError> {
    let connection = connection.as_ref();

    let view_name = table.decoded_view_name();
    if view_name == table.name.as_str() {
        return Err(SetupError::NameConflict {
            name: view_name,
            existing_type: "table".to_string(),
        });
    }

    let rows = connection
        .query(
            &format!(
                "SELECT type FROM {schema}.sqlite_master WHERE name = ?1 COLLATE NOCASE AND type IN ('table', 'index')",
                schema = table.schema
            ),
            &[SqlValue::Text(view_name.clone())],
        )
        .map_err(SetupError::from_conn)?;

    if let Some(existing_type) = first_value(&rows).and_then(SqlValue::as_text) {
        return Err(SetupError::NameConflict {
            name: view_name,
            existing_type: existing_type.to_string(),
        });
    }

    Ok(())
}

fn table_column_names(
    db: &Connection,
    schema: &SchemaName,
    table: &TableName,
) -> Result<Vec<String>, SetupError> {
    let rows = db
        .query(
            &format!("SELECT name AS value FROM pragma_table_info({table}, {schema}) ORDER BY cid"),
            &[],
        )
        .map_err(SetupError::from_conn)?;

    Ok(column_texts(rows))
}

/// Produces the projection for a decoded view.
///
/// Only configured columns are passed to `decompress`; all others retain their
/// original value and order from SQLite's table metadata.
fn build_decoded_select_list(
    table_columns: &[String],
    compressed: &std::collections::HashSet<&str>,
    schema: &str,
) -> String {
    let schema = quote_literal(schema);
    table_columns
        .iter()
        .map(|col| {
            let quoted = quote_identifier(col);
            if compressed.contains(col.as_str()) {
                format!("decompress({quoted}, {schema}) AS {quoted}")
            } else {
                quoted
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Ensure the view exists in the database.
/// If not, create it.
fn ensure_table_view_exists<C: AsRef<Connection>>(
    connection: &C,
    table: &SetupTable,
) -> Result<(), SetupError> {
    ensure_name_is_not_conflicting(connection, table)?;

    let connection = connection.as_ref();

    let table_columns = table_column_names(connection, &table.schema, &table.name)?;
    let compressed = table.compressed_column_names();
    let select_list = build_decoded_select_list(&table_columns, &compressed, table.schema.as_str());
    let qualified_view = table.as_qualified_decoded_view_name();

    connection
        .batch_execute(&format!(
            "CREATE VIEW IF NOT EXISTS {qualified_view} AS \
         SELECT {select_list} FROM {}",
            table.as_qualified_table_name()
        ))
        .map_err(SetupError::from_conn)?;

    Ok(())
}

/// Init goes through several checks
/// 1. Ensure the table exists in the database.
/// 2. Ensure the columns exist in the database.
/// 3. Ensure the views exist in the database.
///    A view is created for each column that targets the table in [`SetupConfig`].
pub(super) fn init_view(state: &ExtensionState, config: &SetupConfig) -> Result<(), SetupError> {
    for table in config.tables.iter() {
        ensure_table_exists(state, &table.schema, &table.name)?;

        for column in &table.columns {
            ensure_column_exist(state, &table.name, &table.schema, &column.name)?;
        }

        if !table.columns.is_empty() {
            ensure_table_view_exists(state, table)?;
        }
    }

    Ok(())
}
