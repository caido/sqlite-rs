/// Quote a schema-qualified name as `"schema"."name"`.
pub fn quote_qualified(schema: &str, name: &str) -> String {
    format!("\"{schema}\".\"{name}\"")
}

/// Quote a SQL identifier, escaping embedded `"` by doubling them.
pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
