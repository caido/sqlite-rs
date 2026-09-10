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

/// Create decode views and the dictionary store for [`SetupConfig`] tables, then warm the dict cache.
pub fn setup<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection + CacheKeySource,
{
    config.validate()?;

    init_view(conn, config)?;
    init_dict(conn, config)?;

    Ok(())
}
