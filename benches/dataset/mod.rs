#![allow(dead_code)]

pub mod generate;
pub mod types;

pub use generate::{generate_sample, generate_stream};
pub use types::{PayloadKind, Sample, SizeBucket};
