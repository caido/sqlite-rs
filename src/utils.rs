pub fn quote_qualified(schema: &str, name: &str) -> String {
    format!("\"{schema}\".\"{name}\"")
}

pub fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
