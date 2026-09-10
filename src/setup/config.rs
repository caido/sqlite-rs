use std::fmt::Display;

use sqlite_loadable::ext::sqlite3;

use crate::{
    functions::Level,
    utils::{quote_identifier, quote_qualified},
    SetupError,
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

pub trait SqlIdent {
    fn as_str(&self) -> &str;

    fn quote(&self) -> String {
        quote_identifier(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub tables: Vec<SetupTable>,
    pub compression_level: Level,
}

impl SetupConfig {
    pub fn iter_columns(&self) -> impl Iterator<Item = (&SetupTable, &SetupColumn)> {
        self.tables
            .iter()
            .flat_map(|table| table.columns.iter().map(move |column| (table, column)))
    }

    pub fn validate(&self) -> Result<(), SetupError> {
        for table in &self.tables {
            for column in &table.columns {
                column.validate()?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct SetupTable {
    pub name: TableName,
    pub schema: SchemaName,
    pub columns: Vec<SetupColumn>,
}

impl SetupTable {
    pub fn as_qualified_decoded_view_name(&self) -> String {
        quote_qualified(self.schema.as_str(), &self.name.decoded_view_name())
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
}

pub trait DictStore {
    type Error: std::error::Error + Send + Sync + 'static;
    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error>;
}

pub trait SetupConnection: DictStore {
    fn sqlite_handle(&self) -> *mut sqlite3;
    fn query_strings(&mut self, sql: &str) -> Result<Vec<String>, Self::Error>;
    fn batch_execute(&mut self, sql: &str) -> Result<(), Self::Error>;
    fn query_i64(&mut self, sql: &str) -> Result<i64, Self::Error>;
    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<i64, Self::Error>;

    /// For each blob in the table, execute the function.
    /// Be able to stream the blobs to the function.
    fn for_each_blob<F>(&mut self, sql: &str, f: F) -> Result<(), Self::Error>
    where
        F: FnMut(&[u8]) -> Result<(), Self::Error>;
}
