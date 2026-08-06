use std::error::Error as StdError;
use std::fmt;

#[derive(Debug)]
pub enum SetupError {
    Connection(Box<dyn StdError + Send + Sync>),
    TableNotFound(String),
    ColumnNotFound { table: String, column: String },
    InvalidConfig(&'static str),
    DictTrain(String),
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connection(e) => write!(f, "connection error: {e}"),
            Self::TableNotFound(t) => write!(f, "table not found: {t}"),
            Self::ColumnNotFound { table, column } => {
                write!(f, "column {column} not found in {table}")
            }
            Self::InvalidConfig(msg) => write!(f, "invalid config: {msg}"),
            Self::DictTrain(msg) => write!(f, "dict train error: {msg}"),
        }
    }
}

impl StdError for SetupError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Connection(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl SetupError {
    pub fn from_conn(err: impl StdError + Send + Sync + 'static) -> Self {
        Self::Connection(Box::new(err))
    }
}
