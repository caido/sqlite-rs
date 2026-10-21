use sqlite_ffi::{Context, SqliteError, Value, ValueType};

use crate::{error::RegexpError, wrapper::Regex};

/// Use as `regexp(pattern, col)` or `col REGEXP pattern`
pub fn regexp(mut ctx: Context, values: &[Value]) -> Result<(), SqliteError> {
    match_regexp(&mut ctx, values).map_err(|e| SqliteError::Message(e.to_string()))
}

fn match_regexp(ctx: &mut Context, values: &[Value]) -> Result<(), RegexpError> {
    // Get content or return early if null
    let content_value = values.get(1).ok_or(RegexpError::MissingContent)?;
    let content = match content_value.value_type() {
        ValueType::Null => {
            ctx.result_bool(false);
            return Ok(());
        }
        ValueType::Blob | ValueType::Text => content_value.to_blob(),
        _ => return Err(RegexpError::InvalidContentType),
    };

    let aux = ctx.aux::<Regex>(0);
    let matched = match aux.get() {
        Ok(r) => r.is_match(content),
        Err(_) => {
            let pattern = values
                .first()
                .ok_or(RegexpError::MissingPattern)?
                .to_text()?;

            let r = Regex::new(pattern)?;
            let matched = r.is_match(content);
            aux.set(r);
            matched
        }
    };

    ctx.result_bool(matched);
    Ok(())
}
