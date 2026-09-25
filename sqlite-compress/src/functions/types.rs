/// Zstandard compression level passed to encoder construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Level(i32);

impl Level {
    pub const fn new(level: i32) -> Self {
        Self(level)
    }
    pub fn get(self) -> i32 {
        self.0
    }
}
