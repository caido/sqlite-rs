//! A minimal `rusqlite` adapter and setup/training workflow.
//!
//! In an application, implement these traits for the connection wrapper you
//! already use. Load the compiled SQLite extension separately before issuing
//! SQL calls to `compress` or `decompress`.

use rusqlite::{params, Connection};
use sqlite_compress::{
    setup, train_all, CacheKeySource, DbKey, DictStore, SchemaName, SetupColumn, SetupConfig,
    SetupConnection, SetupTable, TableName, DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH,
};

struct RusqliteConnection<'a> {
    connection: &'a Connection,
    key: DbKey,
}

impl<'a> RusqliteConnection<'a> {
    fn new(connection: &'a Connection) -> Self {
        Self {
            connection,
            key: DbKey::new(),
        }
    }
}

impl CacheKeySource for RusqliteConnection<'_> {
    fn db_key(&self) -> DbKey {
        self.key
    }
}

impl DictStore for RusqliteConnection<'_> {
    type Error = rusqlite::Error;

    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error> {
        let mut statement = self.connection.prepare(sql)?;
        let values = statement.query_map([], |row| row.get(0))?.collect();
        values
    }
}

impl SetupConnection for RusqliteConnection<'_> {
    fn query_strings(&mut self, sql: &str) -> Result<Vec<String>, Self::Error> {
        let mut statement = self.connection.prepare(sql)?;
        let values = statement.query_map([], |row| row.get(0))?.collect();
        values
    }

    fn batch_execute(&mut self, sql: &str) -> Result<(), Self::Error> {
        self.connection.execute_batch(sql)
    }

    fn query_i64(&mut self, sql: &str) -> Result<i64, Self::Error> {
        self.connection.query_row(sql, [], |row| row.get(0))
    }

    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<i64, Self::Error> {
        self.connection.query_row(sql, [blob], |row| row.get(0))
    }

    fn for_each_blob<F>(&mut self, sql: &str, mut visit: F) -> Result<(), Self::Error>
    where
        F: FnMut(&[u8]) -> Result<(), Self::Error>,
    {
        let mut statement = self.connection.prepare(sql)?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            visit(row.get_ref(0)?.as_blob()?)?;
        }
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let connection = Connection::open_in_memory()?;
    connection
        .execute_batch("CREATE TABLE requests (id INTEGER PRIMARY KEY, body BLOB NOT NULL);")?;

    for id in 0..64 {
        let body = format!("GET /api/users/{id} HTTP/1.1\\r\\nHost: example.com\\r\\n\\r\\n");
        connection.execute(
            "INSERT INTO requests (body) VALUES (?1)",
            params![body.as_bytes()],
        )?;
    }

    let config = SetupConfig {
        tables: vec![SetupTable {
            schema: SchemaName::new("main"),
            name: TableName::new("requests"),
            columns: vec![SetupColumn::new("body", DEFAULT_RETRAIN_GROWTH, 64, 1_000)],
        }],
        compression_level: DEFAULT_LEVEL,
    };

    let mut managed_connection = RusqliteConnection::new(&connection);
    setup(&mut managed_connection, &config)?;
    let dictionary_ids = train_all(&mut managed_connection, &config, 1_024)?;

    println!("Created {} dictionary/dictionaries.", dictionary_ids.len());
    Ok(())
}
