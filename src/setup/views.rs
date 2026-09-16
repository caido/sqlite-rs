use crate::{
    conn::Connection,
    setup::{
        config::{ColumnName, SchemaName, TableName},
        SetupConfig, SetupError, SqlIdent,
    },
    utils::{quote_identifier, quote_literal},
    SetupTable,
};

/// Doing the check if the table exists in the database.
/// Based on the schema and the table name.
fn ensure_table_exists(
    conn: &Connection,
    schema: &SchemaName,
    table_name: &TableName,
) -> Result<(), SetupError> {
    let count = conn
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
fn ensure_column_exist(
    conn: &Connection,
    table: &TableName,
    schema: &SchemaName,
    column: &ColumnName,
) -> Result<(), SetupError> {
    let col_exists = conn
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
    conn: &Connection,
    schema: &SchemaName,
    table: &TableName,
) -> Result<Vec<String>, SetupError> {
    conn.query_strings(&format!(
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

fn ensure_table_view_exists(conn: &Connection, table: &SetupTable) -> Result<(), SetupError> {
    let view_name = table.name.decoded_view_name();
    let schema_qualified = table.as_qualified_schema_name();
    let count = conn
        .query_i64(&format!(
            "SELECT COUNT(*) FROM {schema_qualified}.sqlite_master \
             WHERE type = 'view' AND name = {}",
            quote_literal(&view_name)
        ))
        .map_err(SetupError::from_conn)?;

    if count > 0 {
        return Ok(());
    }

    let table_columns = table_column_names(conn, &table.schema, &table.name)?;
    let compressed = table.compressed_column_names();
    let select_list = build_decoded_select_list(&table_columns, &compressed, table.schema.as_str());
    let qualified_view = table.as_qualified_decoded_view_name();

    conn.batch_execute(&format!(
        "CREATE VIEW IF NOT EXISTS {qualified_view} AS \
         SELECT {select_list} FROM {}",
        table.as_qualified_table_name()
    ));

    Ok(())
}

/// Init goes through several checks
/// 1. Ensure the table exists in the database.
/// 2. Ensure the columns exist in the database.
/// 3. Ensure the views exist in the database.
///    A view is created for each column that targets the table in [`SetupConfig`].
pub(super) fn init_view(conn: &Connection, config: &SetupConfig) -> Result<(), SetupError> {
    for table in config.tables.iter() {
        ensure_table_exists(&conn, &table.schema, &table.name)?;

        for column in &table.columns {
            ensure_column_exist(&conn, &table.name, &table.schema, &column.name)?;
        }

        if !table.columns.is_empty() {
            ensure_table_view_exists(&conn, table)?;
        }
    }

    Ok(())
}
