use std::collections::HashSet;

use sqlite_ffi::{Connection, SqlValue, column_texts, first_value};

use crate::{
    ExtensionState, SetupConfig, SetupError, SetupTable,
    setup::{
        SqlIdent,
        config::{SchemaName, TableName},
    },
    utils::{quote_identifier, quote_literal, quote_qualified},
};

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

fn ensure_trigger_name_is_not_conflicting(
    connection: &Connection,
    table: &SetupTable,
) -> Result<(), SetupError> {
    let trigger_name = table.insert_trigger_name();

    let rows = connection
        .query(
            &format!(
                "SELECT type FROM {schema}.sqlite_master WHERE name = ?1",
                schema = table.schema
            ),
            &[SqlValue::Text(trigger_name.clone())],
        )
        .map_err(SetupError::from_conn)?;

    if let Some(existing_type) = first_value(&rows).and_then(SqlValue::as_text)
        && existing_type != "trigger"
    {
        return Err(SetupError::NameConflict {
            name: trigger_name,
            existing_type: existing_type.to_string(),
        });
    }

    Ok(())
}

fn build_insert_values_list(
    table_columns: &[String],
    compressed: &HashSet<&str>,
    table: &SetupTable,
) -> String {
    let schema = quote_literal(table.schema.as_str());
    let table_name = quote_literal(table.name.as_str());

    table_columns
        .iter()
        .map(|col| {
            let quoted = quote_identifier(col);
            if compressed.contains(col.as_str()) {
                format!(
                    "compress(NEW.{quoted}, {schema}, {table_name}, {})",
                    quote_literal(col)
                )
            } else {
                format!("NEW.{quoted}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn ensure_insert_trigger_exists(
    connection: &Connection,
    table: &SetupTable,
) -> Result<(), SetupError> {
    ensure_trigger_name_is_not_conflicting(connection, table)?;

    let table_columns = table_column_names(connection, &table.schema, &table.name)?;
    let compressed = table.compressed_column_names();
    let columns = table_columns
        .iter()
        .map(|c| quote_identifier(c))
        .collect::<Vec<_>>()
        .join(", ");
    let values = build_insert_values_list(&table_columns, &compressed, table);

    connection
        .batch_execute(&format!(
            "CREATE TRIGGER IF NOT EXISTS {trigger} \
             INSTEAD OF INSERT ON {view} \
             BEGIN \
               INSERT INTO {base_table} ({columns}) VALUES ({values}); \
             END",
            trigger = quote_qualified(table.schema.as_str(), &table.insert_trigger_name()),
            view = quote_identifier(&table.decoded_view_name()),
            base_table = table.as_qualified_table_name(),
        ))
        .map_err(SetupError::from_conn)?;

    Ok(())
}

/// Create `INSTEAD OF INSERT` triggers for tables marked writable.
///
/// Must run after [`super::views::init_view`]: the trigger targets the decoded view.
pub(super) fn init_trigger(state: &ExtensionState, config: &SetupConfig) -> Result<(), SetupError> {
    let connection = state.as_ref();

    for table in &config.tables {
        if !table.has_trigger {
            continue;
        }

        if table.columns.is_empty() {
            return Err(SetupError::InvalidConfig(
                "writable view requires at least one compressed column",
            ));
        }

        ensure_insert_trigger_exists(connection, table)?;
    }

    Ok(())
}
