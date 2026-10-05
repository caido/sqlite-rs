mod config;
mod dict;
mod errors;
mod trigger;
mod views;

pub use config::{
    ColumnName, SchemaName, SetupColumn, SetupConfig, SetupConnection, SetupTable, SqlIdent,
    TableName, ViewName,
};
use dict::init_dict;
pub use errors::SetupError;
use trigger::init_trigger;
use views::init_view;

use crate::state::ExtensionState;

/// Create decode views and the dictionary store for [`SetupConfig`] tables.
///
/// The dictionary store is warmed to ensure the dictionary is ready to be used.
pub fn setup<C: SetupConnection>(conn: &C, config: &SetupConfig) -> Result<(), SetupError> {
    let state = ExtensionState::from_db(unsafe { conn.sqlite_handle() })?;

    config.validate()?;

    init_view(&state, config)?;
    init_trigger(&state, config)?;
    init_dict(&state, config)?;

    Ok(())
}
