use matcher::Matcher;
use sqlite_ffi::{Context, SqliteError, Value};

use crate::{functions::like::search::search_blob, state::ExtensionState};

mod search;

const PATTERN_ARG: i32 = 4;

struct CachedMatcher {
    pattern: Vec<u8>,
    case_sensitive: bool,
    matcher: Matcher,
}

/// `decompress_like(schema, table, column, rowid, pattern, offset[, case_sensitive]) -> 0|1`
///
/// `pattern` uses `LIKE` wildcards (`%`, `_`) and matches anywhere in the
/// value. `case_sensitive` is optional and defaults to 0.
pub fn sqlite_decompress_like(mut context: Context, values: &[Value]) -> Result<(), SqliteError> {
    if values.len() < 6 && values.len() != 7 {
        return Err(SqliteError::Message(
                    "decompress_like expects 6 or 7 arguments: schema, table, column, rowid, pattern, offset[, case_sensitive]"
                        .into(),
                ));
    }

    let schema = values[0].to_text()?;
    let table = values[1].to_text()?;
    let column = values[2].to_text()?;
    let rowid = values[3].to_i64();
    let pattern = values[PATTERN_ARG as usize].to_blob();
    let offset = values[5].to_i64().max(0) as usize;
    let case_sensitive = values.get(6).is_some_and(|value| value.to_i64() != 0);

    let state = ExtensionState::from_context(&context)?;
    let blob = state
        .connection
        .open_blob(schema, table, column, rowid, false)?;

    let aux = context.aux::<CachedMatcher>(PATTERN_ARG);
    let cached = aux.get().ok().filter(|cached| {
        cached.pattern.as_slice() == pattern && cached.case_sensitive == case_sensitive
    });

    let found = if let Some(cached) = cached {
        search_blob(&state, schema, &blob, &cached.matcher, offset)?
    } else {
        let matcher = Matcher::compile(pattern, case_sensitive);
        let found = search_blob(&state, schema, &blob, &matcher, offset)?;

        aux.set(CachedMatcher {
            pattern: pattern.to_vec(),
            case_sensitive,
            matcher,
        });

        found
    };

    context.result_bool(found);
    Ok(())
}
