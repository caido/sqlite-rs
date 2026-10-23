use matcher::Matcher;
use sqlite_ffi::{Context, SqliteError, Value};

use crate::{
    functions::like::search::{search_blob, to_sqlite},
    state::ExtensionState,
};

const PATTERN_ARG: i32 = 4;

mod search;

/// `decompress_like(schema, table, column, rowid, pattern, offset) -> 0|1`
///
/// `pattern` uses `LIKE` wildcards (`%`, `_`) and matches anywhere in the
/// value, ASCII case-insensitively.
pub fn sqlite_decompress_like(mut context: Context, values: &[Value]) -> Result<(), SqliteError> {
    if values.len() < 6 {
        return Err(SqliteError::Message(
            "decompress_like expects 6 arguments: schema, table, column, rowid, pattern, offset"
                .into(),
        ));
    }

    let schema = values[0].to_text()?;
    let table = values[1].to_text()?;
    let column = values[2].to_text()?;
    let rowid = values[3].to_i64();
    let pattern = values[PATTERN_ARG as usize].to_blob();
    let offset = values[5].to_i64().max(0) as usize;

    let state = ExtensionState::from_context(&context)?;
    let blob = state
        .connection
        .open_blob(schema, table, column, rowid, false)?;

    let aux = context.aux::<Matcher>(PATTERN_ARG);
    let found = match aux.get() {
        Ok(matcher) => search_blob(&state, schema, &blob, matcher, offset)?,
        Err(_) => {
            let matcher = Matcher::compile(pattern).map_err(to_sqlite)?;
            let found = search_blob(&state, schema, &blob, &matcher, offset)?;
            aux.set(matcher);
            found
        }
    };

    context.result_bool(found);
    Ok(())
}
