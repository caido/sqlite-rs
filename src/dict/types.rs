use std::fmt::Display;
use std::num::TryFromIntError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DictId(u32);

impl TryFrom<u32> for DictId {
    type Error = TryFromIntError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(Self(value))
    }
}

impl DictId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
    pub fn get(self) -> u32 {
        self.0
    }
}

impl Display for DictId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
