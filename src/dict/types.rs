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
pub struct ColumnKey(String);

impl ColumnKey {
    pub fn new(schema: &str, table_name: &str, column_name: &str) -> Self {
        Self(format!(
            "{}:{}|{}:{}|{}:{}",
            schema.len(),
            schema,
            table_name.len(),
            table_name,
            column_name.len(),
            column_name
        ))
    }
}
