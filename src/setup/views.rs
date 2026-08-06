use crate::setup::{SetupConfig, SetupConnection, SetupError};
use crate::utils::q;

fn ensure_table_exists<C>(conn: &mut C, schema: &str, table_name: &str) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    let count = conn
        .query_i64(&format!(
            "SELECT COUNT(*) FROM \"{schema}\".sqlite_master WHERE type = 'table' AND name = '{table_name}'"
        ))
        .map_err(SetupError::from_conn)?;

    if count == 0 {
        return Err(SetupError::TableNotFound(format!("{schema}.{table_name}")));
    }

    Ok(())
}

fn ensure_columns_exist<C>(
    conn: &mut C,
    table_name: &str,
    schema: &str,
    columns: &[String],
) -> Result<bool, SetupError>
where
    C: SetupConnection,
{
    for column in columns {
        let col_exists = conn
            .query_i64(&format!(
    "SELECT COUNT(*) FROM pragma_table_info('{table_name}', '{schema}') WHERE name = '{column}'"
))
            .map_err(SetupError::from_conn)?;

        if col_exists == 0 {
            return Err(SetupError::ColumnNotFound {
                table: table_name.to_string(),
                column: column.to_string(),
            });
        }
    }
    Ok(true)
}

fn ensure_view_exists<C>(conn: &mut C, schema: &str, table_name: &str) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    let view_name = format!("{}_zstd_decoded", table_name);

    let view_q = q(schema, &view_name);

    let count = conn
        .query_i64(&format!(
    "SELECT COUNT(*) FROM \"{schema}\".sqlite_master WHERE type = 'view' AND name = '{view_name}'"
))
        .map_err(SetupError::from_conn)?;

    if count > 0 {
        return Ok(());
    }

    conn.batch_execute(&format!(
        "CREATE VIEW IF NOT EXISTS {view_q} AS SELECT * FROM {table_name}"
    ))
    .map_err(SetupError::from_conn)?;

    Ok(())
}

pub fn init_view<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    for table in config.tables.iter() {
        ensure_table_exists(conn, &table.schema, &table.name)?;
        ensure_columns_exist(conn, &table.name, &table.schema, &table.columns)?;

        ensure_view_exists(conn, &table.schema, &table.name)?;
    }

    Ok(())
}
