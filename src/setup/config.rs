#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub tables: Vec<SetupTable>,
    pub compression_level: i32,
}

#[derive(Debug, Clone)]
pub struct SetupTable {
    pub name: String,
    pub schema: String,
    pub columns: Vec<String>,
}

pub trait DictStore {
    type Error: std::error::Error + Send + Sync + 'static;
    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error>;
}

pub trait SetupConnection: DictStore {
    fn batch_execute(&mut self, sql: &str) -> Result<(), Self::Error>;
    fn query_i64(&mut self, sql: &str) -> Result<i64, Self::Error>;
    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<(), Self::Error>;
}
