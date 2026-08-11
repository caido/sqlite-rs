#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    Json,
    Html,
    Form,
}

impl PayloadKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Html => "html",
            Self::Form => "form",
        }
    }

    pub fn from_weight(weight: u32) -> Self {
        match weight % 100 {
            0..=39 => Self::Json,
            40..=64 => Self::Html,
            _ => Self::Form,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeBucket {
    Tiny,
    Small,
    Medium,
    Large,
}

impl SizeBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    pub fn target_range(self) -> (usize, usize) {
        match self {
            Self::Tiny => (100, 300),
            Self::Small => (500, 2 * 1024),
            Self::Medium => (5 * 1024, 20 * 1024),
            Self::Large => (20 * 1024, 100 * 1024),
        }
    }

    pub fn from_weight(weight: u32) -> Self {
        match weight % 100 {
            0..=14 => Self::Tiny,
            15..=64 => Self::Small,
            _ => Self::Medium,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub kind: PayloadKind,
    pub size_bucket: SizeBucket,
    pub bytes: Vec<u8>,
}
