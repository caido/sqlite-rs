use std::fmt::Display;

use crate::{
    dict::DICT_TABLE_NAME,
    utils::{quote_identifier, quote_qualified},
};

const VIEW_SUFFIX: &str = "__zstd_decoded";

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

    pub fn as_zstd_schema_name(&self) -> String {
        quote_qualified(self.as_str(), DICT_TABLE_NAME)
    }
}

#[derive(Debug, Clone)]
pub struct TableName(String);

impl TableName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
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
pub struct ColumnName(String);

impl ColumnName {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
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
    pub compression_level: i32,
    pub retrain_growth: usize,
}

#[derive(Debug, Clone)]
pub struct SetupTable {
    pub name: TableName,
    pub schema: SchemaName,
    pub columns: Vec<ColumnName>,
}

impl SetupTable {
    pub fn as_qualified_name(&self) -> String {
        quote_qualified(self.schema.as_str(), self.name.as_str())
    }

    pub fn as_qualified_schema_name(&self) -> String {
        self.schema.quote()
    }

    pub fn as_qualified_table_name(&self) -> String {
        self.name.quote()
    }

    pub fn view_name(&self) -> String {
        format!("{}{}", self.name.as_str(), VIEW_SUFFIX)
    }

    pub fn as_qualified_view_name(&self) -> String {
        quote_qualified(self.schema.as_str(), &self.view_name())
    }
}

pub trait DictStore {
    type Error: std::error::Error + Send + Sync + 'static;
    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error>;
}

pub trait SetupConnection: DictStore {
    fn batch_execute(&mut self, sql: &str) -> Result<(), Self::Error>;
    fn query_i64(&mut self, sql: &str) -> Result<i64, Self::Error>;
    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<i64, Self::Error>;

    /// For each blob in the table, execute the function.
    /// Be able to stream the blobs to the function.
    fn for_each_blob<F>(&mut self, sql: &str, f: F) -> Result<(), Self::Error>
    where
        F: FnMut(&[u8]) -> Result<(), Self::Error>;
}
