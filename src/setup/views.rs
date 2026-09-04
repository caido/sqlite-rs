use crate::{
    setup::{
        config::{ColumnName, SchemaName, TableName},
        SetupConfig, SetupConnection, SetupError, SqlIdent,
    },
    utils::{quote_identifier, quote_literal},
    SetupTable,
};

/// Verifies that setup will not create a view over a missing source table.
fn ensure_table_exists<C>(
    conn: &mut C,
    schema: &SchemaName,
    table_name: &TableName,
) -> Result<(), SetupError>
where
    C: SetupConnection,
{
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

/// Verifies that each configured column belongs to its configured table.
fn ensure_column_exist<C>(
    conn: &mut C,
    table: &TableName,
    schema: &SchemaName,
    column: &ColumnName,
) -> Result<(), SetupError>
where
    C: SetupConnection,
{
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

fn table_column_names<C>(
    conn: &mut C,
    schema: &SchemaName,
    table: &TableName,
) -> Result<Vec<String>, SetupError>
where
    C: SetupConnection,
{
    conn.query_strings(&format!(
        "SELECT name AS value FROM pragma_table_info({table}, {schema}) ORDER BY cid"
    ))
    .map_err(SetupError::from_conn)
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

fn ensure_table_view_exists<C>(conn: &mut C, table: &SetupTable) -> Result<(), SetupError>
where
    C: SetupConnection,
{
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
    ))
    .map_err(SetupError::from_conn)?;

    Ok(())
}

/// Validates configured sources before creating their decoded views.
///
/// Views are created only after all configured columns for a table have been
/// confirmed, avoiding a partially initialized table configuration.
pub(super) fn init_view<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    for table in config.tables.iter() {
        ensure_table_exists(conn, &table.schema, &table.name)?;

        for column in &table.columns {
            ensure_column_exist(conn, &table.name, &table.schema, &column.name)?;
        }

        if !table.columns.is_empty() {
            ensure_table_view_exists(conn, table)?;
        }
    }

    Ok(())
}
