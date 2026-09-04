/// Quotes a schema-qualified SQLite identifier.
///
/// Both components are quoted independently so an embedded identifier cannot
/// change the qualified-name structure.
pub fn quote_qualified(schema: &str, name: &str) -> String {
    format!("\"{schema}\".\"{name}\"")
}

/// Quotes one SQLite identifier, escaping embedded double quotes.
pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

/// Quotes a string value for the SQL fragments built by this crate.
///
/// Values are escaped rather than treated as identifiers; callers should use
/// [`quote_identifier`] for schema, table, or column names.
pub fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
