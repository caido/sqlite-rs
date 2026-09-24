mod config;
mod dict;
mod errors;
mod prune;
mod views;

pub use config::{
    ColumnName, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupTable, SqlIdent,
    TableName,
};
use dict::init_dict;
pub use errors::SetupError;
use views::init_view;

use crate::{setup::prune::init_table, state::ExtensionState};

/// Create decode views and the dictionary store for [`SetupConfig`] tables, then warm the dict cache.
pub fn setup<C: SetupConnection>(conn: &C, config: &SetupConfig) -> Result<(), SetupError> {
    let state = ExtensionState::from_db(unsafe { conn.sqlite_handle() })?;

    config.validate()?;

    init_view(&state, config)?;
    init_dict(&state, config)?;
    init_table(&state, config)?;

    Ok(())
}
