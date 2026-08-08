use crate::setup::config::{ColumnName, SchemaName, TableName};
use crate::setup::{SetupConfig, SetupConnection, SetupError, SqlIdent};
use crate::SetupTable;

/// Doing the check if the table exists in the database. 
/// Based on the schema and the table name.
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

/// For each column in the table, check if the column exists in the database.
/// Based on the table name and the schema.
fn ensure_columns_exist<C>(
    conn: &mut C,
    table: &TableName,
    schema: &SchemaName,
    columns: &Vec<ColumnName>,
) -> Result<bool, SetupError>
where
    C: SetupConnection,
{
    for column in columns {
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
    }
    Ok(true)
}

/// Since only one view is created by schema, check if the view exists in the database.
fn ensure_view_exists<C>(conn: &mut C, table: &SetupTable) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    let schema = table.as_qualified_schema_name();
    let bare = table.view_name();
    let qualified = table.as_qualified_view_name();

    let count = conn
        .query_i64(&format!(
            "SELECT COUNT(*) FROM {schema}.sqlite_master \
     WHERE type = 'view' AND name = '{bare}'"
        ))
        .map_err(SetupError::from_conn)?;

    if count > 0 {
        return Ok(());
    }

    conn.batch_execute(&format!(
        "CREATE VIEW IF NOT EXISTS {qualified} AS SELECT * FROM {}",
        table.as_qualified_name()
    ))
    .map_err(SetupError::from_conn)?;

    Ok(())
}

/// Create decode views for [`SetupConfig`] tables.
pub fn init_view<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    for table in config.tables.iter() {
        ensure_table_exists(conn, &table.schema, &table.name)?;
        ensure_columns_exist(conn, &table.name, &table.schema, &table.columns)?;

        ensure_view_exists(conn, table)?;
    }

    Ok(())
}
