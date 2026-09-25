//! Setup and train a per-column Zstandard dictionary.
//!
//! Call `sqlite3_compress_init` on the connection before `setup`. That entry
//! point stores the extension state `setup` and `train_all` read back through
//! `SetupConnection::sqlite_handle`.
//!
//! This sketch uses the default `bundled` build, where SQLite is linked into
//! the host and the API-routines pointer can be null. A `.load` build must
//! enable `loadable_extension` and let SQLite call the entry point itself.

use std::ptr;

use libsqlite3_sys::{sqlite3, sqlite3_close, sqlite3_open, SQLITE_OK};
use sqlite_compress::{
    setup, sqlite3_compress_init, train_all, SchemaName, SetupColumn, SetupConfig, SetupConnection,
    SetupTable, TableName, DEFAULT_LEVEL, DEFAULT_RETRAIN_GROWTH,
};
use sqlite_ffi::{Connection, SqlValue};

struct AppConnection {
    db: *mut sqlite3,
}

impl SetupConnection for AppConnection {
    unsafe fn sqlite_handle(&self) -> *mut sqlite3 {
        self.db
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut db = ptr::null_mut();
    let rc = unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) };
    if rc != SQLITE_OK {
        return Err(format!("sqlite3_open failed ({rc})").into());
    }

    let app = AppConnection { db };

    let rc = unsafe { sqlite3_compress_init(app.db, ptr::null_mut(), ptr::null_mut()) };
    if rc != SQLITE_OK {
        unsafe { sqlite3_close(db) };
        return Err(format!("sqlite3_compress_init failed ({rc})").into());
    }

    let connection = Connection::from_raw(db);
    connection
        .batch_execute("CREATE TABLE requests (id INTEGER PRIMARY KEY, body BLOB NOT NULL)")?;

    for id in 0..64 {
        let body = format!("GET /api/users/{id} HTTP/1.1\r\nHost: example.com\r\n\r\n");
        connection.execute(
            "INSERT INTO requests (body) VALUES (?1)",
            &[SqlValue::Blob(body.into_bytes())],
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

    setup(&app, &config)?;
    let dictionary_ids = train_all(&app, &config, 1_024)?;

    println!("Created {} dictionary/dictionaries.", dictionary_ids.len());

    unsafe { sqlite3_close(db) };
    Ok(())
}
