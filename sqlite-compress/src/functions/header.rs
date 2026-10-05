use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, little_endian::U32};

use crate::{dict::DictId, functions::errors::CodecError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromBytes, KnownLayout, Immutable)]
#[repr(u8)]
pub enum CodecKind {
    Zstd = 0,
}

impl TryFrom<u8> for CodecKind {
    type Error = CodecError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Zstd),
            other => Err(CodecError::UnknownCodec(other)),
        }
    }
}

/// Binary prefix stored before every compressed payload.
///
/// It gives the decoder the version, codec, header_len, dict_id and uncompressed_len.
/// `header_len` lets later versions append fields while preserving the start of the payload.
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct Header {
    /// Version of the prefix format understood by this decoder.
    pub version: u8,
    /// Compression algorithm used for the payload.
    pub codec: u8,
    /// Number of bytes occupied by the full prefix, including future fields.
    pub header_len: u8,
    /// Dictionary used for compression; zero denotes a raw Zstandard payload.
    pub dict_id: U32,
    /// Original byte length used to bound decompression output.
    pub uncompressed_len: U32,
}

impl Header {
    /// Latest prefix version emitted by this crate.
    pub const VERSION: u8 = 1;
    /// Size of the fields understood by this version of the prefix.
    pub const PREFIX_SIZE: usize = size_of::<Self>();

    pub fn new(dict_id: DictId, uncompressed_len: u32) -> Self {
        Self {
            version: Self::VERSION,
            codec: CodecKind::Zstd as u8,
            header_len: Self::PREFIX_SIZE as u8,
            dict_id: U32::new(dict_id.get()),
            uncompressed_len: U32::new(uncompressed_len),
        }
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }

    /// Splits a stored value into its prefix and compressed payload.
    ///
    /// # Errors
    ///
    /// Returns an error when the prefix is truncated, uses a newer unsupported
    /// version, or names an unknown codec.
    pub fn parse(blob: &[u8]) -> Result<(Self, &[u8]), CodecError> {
        let (header, _) = Self::ref_from_prefix(blob).map_err(|_| CodecError::MalformedHeader)?;

        if header.version > Self::VERSION {
            return Err(CodecError::UnknownVersion(header.version));
        }

        let header_len = header.header_len as usize;
        // A newer prefix may append fields, but it cannot omit fields this
        // decoder reads from the fixed prefix.
        if header_len < Self::PREFIX_SIZE || blob.len() < header_len {
            return Err(CodecError::MalformedHeader);
        }

        let _codec = CodecKind::try_from(header.codec)?;
        let payload = &blob[header_len..];

        Ok((*header, payload))
    }
}

/// Prefixes a compressed payload with the metadata required to decode it.
///
/// # Errors
///
/// Returns [`CodecError::PayloadTooLarge`] when the original length cannot be
/// represented by the on-disk format.
pub fn wrap(dict_id: DictId, original_len: usize, payload: Vec<u8>) -> Result<Vec<u8>, CodecError> {
    let uncompressed_len = u32::try_from(original_len).map_err(|_| CodecError::PayloadTooLarge)?;
    let header = Header::new(dict_id, uncompressed_len);
    let mut out = Vec::with_capacity(Header::PREFIX_SIZE + payload.len());

    header.write(&mut out);
    out.extend_from_slice(&payload);
    Ok(out)
}
