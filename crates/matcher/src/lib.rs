mod errors;
mod matcher;
mod utils;

pub use errors::MatcherError;
pub use matcher::Matcher;
pub use utils::{like_literal_segments, like_to_regex};
