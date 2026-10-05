mod error;
mod operator;
mod wrapper;

use sqlite_ffi::Connection;
use sqlite_ffi_macros::sqlite_entrypoint;

#[sqlite_entrypoint]
pub fn sqlite3_reglite_init(connection: Connection) -> Result<(), sqlite_ffi::SqliteError> {
    connection.create_function("regexp", 2, operator::regexp)?;

    Ok(())
}
