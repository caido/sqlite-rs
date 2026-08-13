use crate::{
    setup::{
        config::{ColumnName, SchemaName, TableName},
        SetupConfig, SetupConnection, SetupError, SqlIdent,
    },
    SetupTable,
};

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
fn ensure_column_exist<C>(
    conn: &mut C,
    table: &TableName,
    schema: &SchemaName,
    column: &ColumnName,
) -> Result<bool, SetupError>
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

    Ok(true)
}

/// A view is created for each column that targets the table in [`SetupConfig`].
fn ensure_view_exist<C>(
    conn: &mut C,
    schema_qualified_name: &str,
    table: &SetupTable,
    column: &ColumnName,
) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    let column_view_name = column.view_name(&table.name);
    let qualified = table.column_as_qualified_view_name(&column);

    let count = conn
        .query_i64(&format!(
            "SELECT COUNT(*) FROM {schema_qualified_name}.sqlite_master \
     WHERE type = 'view' AND name = '{column_view_name}'"
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

/// Init go over several checks
/// 1. Ensure the table exists in the database.
/// 2. Ensure the columns exist in the database.
/// 3. Ensure the views exist in the database.
/// A view is created for each column that targets the table in [`SetupConfig`].
pub fn init_view<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    for table in config.tables.iter() {
        ensure_table_exists(conn, &table.schema, &table.name)?;
        let schema = table.as_qualified_schema_name();

        for column in &table.columns {
            ensure_column_exist(conn, &table.name, &table.schema, &column)?;
            ensure_view_exist(conn, &schema, &table, &column)?;
        }
    }

    Ok(())
}
