use sqlite_ffi::{Context, SqliteError, Value};

use crate::{ColumnKey, dict::read_current_id, state::ExtensionState};

pub fn sqlite_latest_dict_id(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    let schema = values[0].to_text()?;
    let table = values[1].to_text()?;
    let column = values[2].to_text()?;

    let state = ExtensionState::from_context(&context)?;
    let key = ColumnKey::new(schema, table, column);

    let latest_id =
        read_current_id(&state, &key).map_err(|error| SqliteError::Message(error.to_string()))?;

    match latest_id {
        Some(dict_id) => context.result_int64(i64::from(dict_id.get())),
        None => context.result_null(),
    }

    Ok(())
}
