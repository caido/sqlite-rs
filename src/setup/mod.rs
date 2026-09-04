mod config;
mod dict;
mod errors;
mod views;

pub use config::{
    ColumnName, DictStore, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupTable,
    SqlIdent, TableName,
};
use dict::init_dict;
pub use errors::SetupError;
use views::init_view;

use crate::cache::CacheKeySource;

/// Prepares the database objects required by [`SetupConfig`].
///
/// Setup validates source tables and columns, creates decoded views and
/// dictionary stores, then warms caches from the dictionaries already stored.
///
/// # Errors
///
/// Returns an error when configuration is invalid, a configured object is
/// missing, or SQLite cannot create or query the required objects.
pub fn setup<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection + CacheKeySource,
{
    config.validate()?;

    init_view(conn, config)?;
    init_dict(conn, config)?;

    Ok(())
}
