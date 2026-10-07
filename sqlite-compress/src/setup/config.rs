use std::{collections::HashSet, fmt::Display};

use libsqlite3_sys::sqlite3;

use crate::{
    SetupError,
    functions::Level,
    utils::{quote_identifier, quote_qualified},
};

const VIEW_SUFFIX: &str = "__compress_decoded";

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SchemaName(String);

impl SqlIdent for SchemaName {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SchemaName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.quote())
    }
}

impl SchemaName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }
}

/// A view name paired with a schema by [`SetupTable`].
#[derive(Debug, Clone)]
pub struct ViewName(String);

impl ViewName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A table name paired with a schema by [`SetupTable`].
#[derive(Debug, Clone)]
pub struct TableName(String);

impl TableName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    pub fn decoded_view_name(&self) -> String {
        format!("{}_{}", VIEW_SUFFIX, self.as_str())
    }
}

impl SqlIdent for TableName {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for TableName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.quote())
    }
}

#[derive(Debug, Clone)]
pub struct SetupColumn {
    pub name: ColumnName,
    pub retrain_growth: usize,
    pub min_samples: usize,
    pub max_samples: usize,
}

impl SetupColumn {
    pub fn new(name: &str, retrain_growth: usize, min_samples: usize, max_samples: usize) -> Self {
        Self {
            name: ColumnName::new(name),
            retrain_growth,
            min_samples,
            max_samples,
        }
    }

    /// Validates that the sampling and retraining thresholds can be satisfied.
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::InvalidConfig`] when a threshold is zero or the
    /// sampling limit is smaller than the initial-training threshold.
    pub fn validate(&self) -> Result<(), SetupError> {
        if self.retrain_growth == 0 {
            return Err(SetupError::InvalidConfig(
                "retrain growth must be greater than zero",
            ));
        }

        if self.min_samples == 0 {
            return Err(SetupError::InvalidConfig(
                "min samples must be greater than zero",
            ));
        }

        if self.max_samples == 0 {
            return Err(SetupError::InvalidConfig(
                "max samples must be greater than zero",
            ));
        }

        if self.max_samples < self.min_samples {
            return Err(SetupError::InvalidConfig(
                "max samples must be greater than or equal to min samples",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ColumnName(String);

impl ColumnName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    /// Returns the column-specific name `__compress_decoded_<table>_<column>`;
    /// setup creates the table-level `__compress_decoded_<table>` view instead.
    pub fn view_name(&self, table_name: &TableName) -> String {
        format!("{}_{}_{}", VIEW_SUFFIX, table_name.as_str(), self.as_str())
    }
}

impl SqlIdent for ColumnName {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ColumnName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.quote())
    }
}

/// Converts a configuration name into a quoted SQL identifier.
pub trait SqlIdent {
    fn as_str(&self) -> &str;

    fn quote(&self) -> String {
        quote_identifier(self.as_str())
    }
}

/// Describes the tables and columns managed by the extension.
#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub tables: Vec<SetupTable>,
    pub compression_level: Level,
}

impl SetupConfig {
    /// Iterates over every configured table and column pair.
    ///
    /// Training uses this flattened order to apply the same policy to
    /// every configured column.
    pub fn iter_columns(&self) -> impl Iterator<Item = (&SetupTable, &SetupColumn)> {
        self.tables
            .iter()
            .flat_map(|table| table.columns.iter().map(move |column| (table, column)))
    }

    /// Validates every column policy before setup or training changes the database.
    /// Also validates that the decoded view name is not duplicated in the same schema.
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::InvalidConfig`] when a sample or retraining bound
    /// cannot produce a valid dictionary.
    pub fn validate(&self) -> Result<(), SetupError> {
        let mut seen = HashSet::new();

        for table in &self.tables {
            if table.columns.is_empty() {
                continue;
            }

            let key = (
                table.schema.as_str().to_ascii_lowercase(),
                table.decoded_view_name().to_ascii_lowercase(),
            );

            if !seen.insert(key) {
                return Err(SetupError::InvalidConfig(
                    "duplicate decoded view name in the same schema",
                ));
            }

            for column in &table.columns {
                column.validate()?;
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SetupTable {
    pub(crate) name: TableName,
    pub(crate) schema: SchemaName,
    pub(crate) columns: Vec<SetupColumn>,
    view: Option<ViewName>,
}

impl SetupTable {
    pub fn new(schema: SchemaName, name: TableName, columns: Vec<SetupColumn>) -> Self {
        Self {
            schema,
            name,
            columns,
            view: None,
        }
    }

    pub fn compressed_column_names(&self) -> std::collections::HashSet<&str> {
        self.columns.iter().map(|c| c.name.as_str()).collect()
    }

    pub fn as_qualified_name(&self) -> String {
        quote_qualified(self.schema.as_str(), self.name.as_str())
    }

    pub fn as_qualified_schema_name(&self) -> String {
        self.schema.quote()
    }

    pub fn as_qualified_table_name(&self) -> String {
        self.name.quote()
    }

    pub fn column_as_qualified_view_name(&self, column_name: &ColumnName) -> String {
        quote_qualified(
            self.schema.as_str(),
            column_name.view_name(&self.name).as_str(),
        )
    }

    pub(crate) fn decoded_view_name(&self) -> String {
        match &self.view {
            Some(view) => view.as_str().to_string(),
            None => self.name.decoded_view_name(),
        }
    }

    pub fn as_qualified_decoded_view_name(&self) -> String {
        quote_qualified(self.schema.as_str(), &self.decoded_view_name())
    }

    pub fn with_view(mut self, view: ViewName) -> Self {
        self.view = Some(view);
        self
    }
}

pub trait SetupConnection {
    /// # Safety
    ///
    /// This method must be called with a valid SQLite connection handle.
    unsafe fn sqlite_handle(&self) -> *mut sqlite3;
}
