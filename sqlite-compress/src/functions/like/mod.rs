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

/// `decompress_like(schema, table, column, rowid, pattern, offset) -> 0|1`
///
/// Uses `LIKE` wildcards (`%`, `_`) and matches anywhere in the value.
/// Matching is case-sensitive.
pub fn sqlite_decompress_like(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    sqlite_decompress_like_with_case(context, values, true)
}

/// `decompress_ilike(schema, table, column, rowid, pattern, offset) -> 0|1`
///
/// Uses `LIKE` wildcards (`%`, `_`) and matches anywhere in the value.
/// Matching is case-insensitive.
pub fn sqlite_decompress_ilike(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    sqlite_decompress_like_with_case(context, values, false)
}

fn sqlite_decompress_like_with_case(
    mut context: Context,
    values: &[Value],
    case_sensitive: bool,
) -> Result<(), SqliteError> {
    if values.len() != 6 {
        return Err(SqliteError::Message(
            "decompress_like and decompress_ilike expect 6 arguments: schema, table, column, rowid, pattern, offset"
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
