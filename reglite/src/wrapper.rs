/// Regex wrapper class that allows for unified management of invalid regex patterns
#[derive(Clone)]
pub struct Regex(Option<regex::bytes::Regex>);

impl Regex {
    pub fn new(re: &str) -> Result<Regex, regex::Error> {
        let regex = match regex::bytes::Regex::new(re) {
            Ok(r) => Some(r),
            Err(_) if cfg!(feature = "ignore-invalid") => None,
            Err(e) => return Err(e),
        };
        Ok(Regex(regex))
    }

    #[inline(always)]
    pub fn is_match(&self, haystack: &[u8]) -> bool {
        self.0.as_ref().is_some_and(|r| r.is_match(haystack))
    }
}
