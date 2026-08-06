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

    /// Weighted pick: JSON 40%, HTML 25%, Form 35% of the remaining MVP mix
    /// (JSON 40 / HTML 25 / Form 35 among the three kinds).
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
    /// ~100–300 B
    Tiny,
    /// ~500 B–2 KiB
    Small,
    /// ~5–20 KiB
    Medium,
}

impl SizeBucket {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Small => "small",
            Self::Medium => "medium",
        }
    }

    pub fn target_range(self) -> (usize, usize) {
        match self {
            Self::Tiny => (100, 300),
            Self::Small => (500, 2 * 1024),
            Self::Medium => (5 * 1024, 20 * 1024),
        }
    }

    /// Weighted pick: Small 50%, Medium 35%, Tiny 15%.
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
