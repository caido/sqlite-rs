use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DictId(u32);

impl From<u32> for DictId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl DictId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
    pub fn get(self) -> u32 {
        self.0
    }
}

impl Display for DictId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ColumnKey {
    schema: String,
    table: String,
    column: String,
}

impl ColumnKey {
    pub fn new(schema: &str, table: &str, column: &str) -> Self {
        Self {
            schema: schema.to_string(),
            table: table.to_string(),
            column: column.to_string(),
        }
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn table(&self) -> &str {
        &self.table
    }

    pub fn column(&self) -> &str {
        &self.column
    }
}
