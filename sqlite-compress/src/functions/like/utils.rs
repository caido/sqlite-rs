use std::fmt::Write as _;

use sqlite_ffi::SqliteError;

pub(crate) fn to_sqlite(e: impl std::fmt::Display) -> SqliteError {
    SqliteError::Message(e.to_string())
}

/// Translates a `LIKE` pattern into a byte-oriented regex.
///
/// Literals are emitted as `\xHH` so ASCII folds case and other bytes match
/// verbatim. `_` is one UTF-8 character, or one byte that cannot start a
/// valid sequence so invalid UTF-8 still advances.
pub(crate) fn like_to_regex(pattern: &[u8]) -> String {
    let mut re = String::with_capacity(8 + pattern.len() * 4);
    re.push_str("(?is-u)");
    for &b in pattern {
        match b {
            b'%' => re.push_str(".*"),
            b'_' => re.push_str(r"(?:(?u-i:.)|[\x80-\xBF\xC0\xC1\xF5-\xFF])"),
            _ => {
                let _ = write!(re, r"\x{b:02X}");
            }
        }
    }
    re
}
