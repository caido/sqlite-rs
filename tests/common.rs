use rusqlite::Connection;
use sqlite_compress::DictStore;
use sqlite_compress::SetupConnection;
use std::sync::Arc;

use sqlite_compress::DictError;
use zstd::dict::{DecoderDictionary, EncoderDictionary};

#[allow(dead_code)]
pub fn expect_encoder(
    result: Result<Arc<EncoderDictionary<'static>>, DictError>,
) -> Arc<EncoderDictionary<'static>> {
    match result {
        Ok(v) => v,
        Err(e) => panic!("expected encoder, got {e}"),
    }
}

#[allow(dead_code)]
pub fn expect_decoder(
    result: Result<Arc<DecoderDictionary<'static>>, DictError>,
) -> Arc<DecoderDictionary<'static>> {
    match result {
        Ok(v) => v,
        Err(e) => panic!("expected decoder, got {e}"),
    }
}

pub struct RusqliteConn<'a>(&'a Connection);

impl<'a> RusqliteConn<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self(conn)
    }
}

impl DictStore for RusqliteConn<'_> {
    type Error = rusqlite::Error;
    fn query_blobs(&mut self, sql: &str) -> Result<Vec<Vec<u8>>, Self::Error> {
        let mut stmt = self.0.prepare(sql)?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }
}

impl SetupConnection for RusqliteConn<'_> {
    fn batch_execute(&mut self, sql: &str) -> std::result::Result<(), Self::Error> {
        self.0.execute_batch(sql)
    }

    fn query_i64(&mut self, sql: &str) -> Result<i64, Self::Error> {
        self.0.query_row(sql, [], |row| row.get(0))
    }

    fn execute_blob(&mut self, sql: &str, blob: &[u8]) -> Result<(), Self::Error> {
        self.0.execute(sql, [blob])?;
        Ok(())
    }
}
