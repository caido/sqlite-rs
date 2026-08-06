#[derive(Debug)]
pub enum DictError {
    NotReady,
    Connection(Box<dyn std::error::Error + Send + Sync>),
    NotFound(u32),
}

impl std::fmt::Display for DictError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady => write!(f, "dictionary not ready"),
            Self::Connection(e) => write!(f, "dict connection error: {e}"),
            Self::NotFound(id) => write!(f, "dictionary id {id} not found"),
        }
    }
}
impl std::error::Error for DictError {}
