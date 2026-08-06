mod config;
mod dict;
mod errors;
mod views;

pub use config::{DictStore, SetupConfig, SetupConnection, SetupTable};
use dict::init_dict;
pub use errors::SetupError;
use views::init_view;

pub fn setup<C>(conn: &mut C, config: &SetupConfig) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    init_view(conn, config)?;
    init_dict(conn, config)?;

    Ok(())
}
