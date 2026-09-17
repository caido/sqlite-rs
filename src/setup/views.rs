use sqlite_ffi::Connection;

use crate::{
    setup::{
        config::{ColumnName, SchemaName, TableName},
        SetupConfig, SetupError, SqlIdent,
    },
    state::ExtensionState,
    utils::{quote_identifier, quote_literal},
    SetupTable,
};

/// Doing the check if the table exists in the database.
/// Based on the schema and the table name.
fn ensure_table_exists<C: AsRef<Connection>>(
    connection: C,
    schema: &SchemaName,
    table_name: &TableName,
) -> Result<(), SetupError> {
    let connection = connection.as_ref();

    let count = connection
        .query_i64(&format!(
            "SELECT COUNT(*) FROM {schema}.sqlite_master WHERE type = 'table' AND name = {table_name}"
        ))
        .map_err(SetupError::from_conn)?;

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
/// Based on the table name and the schema.
fn ensure_column_exist<C: AsRef<Connection>>(
    connection: &C,
    table: &TableName,
    schema: &SchemaName,
    column: &ColumnName,
) -> Result<(), SetupError> {
    let connection = connection.as_ref();

    let col_exists = connection
        .query_i64(&format!(
            "SELECT COUNT(*) FROM pragma_table_info({table}, {schema}) WHERE name = {column}"
        ))
        .map_err(SetupError::from_conn)?;

    if col_exists == 0 {
        return Err(SetupError::ColumnNotFound {
            table: table.as_str().to_string(),
            column: column.as_str().to_string(),
        });
    }

    Ok(())
}

fn table_column_names(
    db: &Connection,
    schema: &SchemaName,
    table: &TableName,
) -> Result<Vec<String>, SetupError> {
    db.query_strings(&format!(
        "SELECT name AS value FROM pragma_table_info({table}, {schema}) ORDER BY cid"
    ))
    .map_err(SetupError::from_conn)
}

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

fn ensure_table_view_exists<C: AsRef<Connection>>(
    connection: &C,
    table: &SetupTable,
) -> Result<(), SetupError> {
    let connection = connection.as_ref();
    let view_name = table.name.decoded_view_name();
    let schema_qualified = table.as_qualified_schema_name();
    let count = connection
        .query_i64(&format!(
            "SELECT COUNT(*) FROM {schema_qualified}.sqlite_master \
             WHERE type = 'view' AND name = {}",
            quote_literal(&view_name)
        ))
        .map_err(SetupError::from_conn)?;

    if count > 0 {
        return Ok(());
    }

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
