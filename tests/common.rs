use rusqlite::Connection;
use sqlite_compress::{CacheKeySource, DbKey, DictStore, SetupConnection};

#[allow(dead_code)]
pub const DEFAULT_MIN_SAMPLES: usize = 1000;
pub const DEFAULT_MAX_SAMPLES: usize = 10000;

pub struct RusqliteConn<'a> {
    conn: &'a Connection,
    key: DbKey,
}

impl<'a> RusqliteConn<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self {
            conn,
            key: DbKey::new(),
        }
    }
}

impl CacheKeySource for RusqliteConn<'_> {
    fn db_key(&self) -> DbKey {
        self.key
    }
}

impl DictStore for RusqliteConn<'_> {
    type Error = rusqlite::Error;
    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }
}

impl SetupConnection for RusqliteConn<'_> {
    fn query_strings(&mut self, sql: &str) -> Result<Vec<String>, Self::Error> {
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    fn batch_execute(&mut self, sql: &str) -> std::result::Result<(), Self::Error> {
        self.conn.execute_batch(sql)
    }

    fn query_i64(&mut self, sql: &str) -> Result<i64, Self::Error> {
        self.conn.query_row(sql, [], |row| row.get(0))
    }

    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<i64, Self::Error> {
        self.conn.query_row(sql, [blob], |row| row.get(0))
    }

    fn for_each_blob<F>(&mut self, sql: &str, mut f: F) -> Result<(), rusqlite::Error>
    where
        F: FnMut(&[u8]) -> Result<(), rusqlite::Error>,
    {
        let mut stmt = self.conn.prepare(sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let blob: &[u8] = row.get_ref(0)?.as_blob()?;
            f(blob)?;
        }
        Ok(())
    }
}
