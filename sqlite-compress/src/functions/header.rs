use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes, little_endian::U32};

use crate::{dict::DictId, functions::errors::CodecError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromBytes, KnownLayout, Immutable)]
#[repr(u8)]
pub enum CodecKind {
    Plain = 1,
    Zstd = 2,
}

impl TryFrom<u8> for CodecKind {
    type Error = CodecError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            1 => Ok(Self::Plain),
            2 => Ok(Self::Zstd),
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
    /// Identifies this file format.
    pub magic: u8,
    /// Compression algorithm used for the payload.
    pub codec: u8,
    /// Version of the prefix format understood by this decoder.
    pub version: u8,
    /// Number of bytes occupied by the full prefix, including future fields.
    pub header_len: u8,
    /// Dictionary used for compression; zero denotes a raw Zstandard payload.
    pub dict_id: U32,
    /// Original byte length used to bound decompression output.
    pub uncompressed_len: U32,
}

pub(crate) enum ParsedBlob<'a> {
    /// Header valid + compressed payload
    Framed(Header, &'a [u8]),
    /// Not our format → treat as plain text
    Plain,
}

impl Header {
    pub const MAGIC: u8 = 0xD4;

    /// Latest prefix version emitted by this crate.
    pub const VERSION: u8 = 1;
    /// Size of the fields understood by this version of the prefix.
    pub const PREFIX_SIZE: usize = size_of::<Self>();

    pub fn new(dict_id: DictId, uncompressed_len: u32) -> Self {
        Self::new_with_codec(CodecKind::Zstd, dict_id, uncompressed_len)
    }

    pub(crate) fn new_with_codec(codec: CodecKind, dict_id: DictId, uncompressed_len: u32) -> Self {
        Self {
            magic: Self::MAGIC,
            codec: codec as u8,
            version: Self::VERSION,
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
    fn parse(blob: &[u8]) -> Result<(Self, &[u8]), CodecError> {
        if blob.first().copied() != Some(Self::MAGIC) {
            return Err(CodecError::MalformedHeader);
        }

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

        // Reject codec values other than Plain and Zstd.
        let _codec = CodecKind::try_from(header.codec)?;
        let payload = &blob[header_len..];

        Ok((*header, payload))
    }

    pub(crate) fn parse_blob(blob: &[u8]) -> Result<ParsedBlob<'_>, CodecError> {
        match Self::parse(blob) {
            Ok((header, payload)) => Ok(ParsedBlob::Framed(header, payload)),
            Err(CodecError::MalformedHeader)
            | Err(CodecError::UnknownCodec(_))
            | Err(CodecError::UnknownVersion(_)) => Ok(ParsedBlob::Plain),
            Err(e) => Err(e),
        }
    }
}

pub fn wrap(dict_id: DictId, original_len: usize, payload: Vec<u8>) -> Result<Vec<u8>, CodecError> {
    wrap_with_codec(CodecKind::Zstd, dict_id, original_len, payload)
}

/// Prefixes a compressed payload with the metadata required to decode it.
///
/// # Errors
///
/// Returns [`CodecError::PayloadTooLarge`] when the original length cannot be
/// represented by the on-disk format.
pub(crate) fn wrap_with_codec(
    codec: CodecKind,
    dict_id: DictId,
    original_len: usize,
    payload: Vec<u8>,
) -> Result<Vec<u8>, CodecError> {
    let uncompressed_len = u32::try_from(original_len).map_err(|_| CodecError::PayloadTooLarge)?;

    let header = Header::new_with_codec(codec, dict_id, uncompressed_len);
    let mut out = Vec::with_capacity(Header::PREFIX_SIZE + payload.len());

    header.write(&mut out);
    out.extend_from_slice(&payload);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{CodecKind, Header, ParsedBlob};
    use crate::dict::DictId;

    fn framed_blob(codec: CodecKind, payload: &[u8]) -> Vec<u8> {
        let header = Header::new_with_codec(codec, DictId::from(0), payload.len() as u32);

        let mut blob = Vec::new();
        header.write(&mut blob);
        blob.extend_from_slice(payload);
        blob
    }

    #[test]
    fn codec_values_are_non_zero_and_parse_consistently() {
        assert_eq!(CodecKind::Plain as u8, 1);
        assert_eq!(CodecKind::Zstd as u8, 2);

        assert_eq!(CodecKind::try_from(1).unwrap(), CodecKind::Plain);
        assert_eq!(CodecKind::try_from(2).unwrap(), CodecKind::Zstd);
        assert!(CodecKind::try_from(0).is_err());
    }

    #[test]
    fn parses_framed_plain_payload() {
        let blob = framed_blob(CodecKind::Plain, b"raw payload");

        match Header::parse_blob(&blob).unwrap() {
            ParsedBlob::Framed(header, payload) => {
                assert_eq!(header.magic, Header::MAGIC);
                assert_eq!(header.codec, CodecKind::Plain as u8);
                assert_eq!(header.version, Header::VERSION);
                assert_eq!(payload, b"raw payload");
            }
            ParsedBlob::Plain => panic!("valid Plain frame was not recognized"),
        }
    }

    #[test]
    fn parses_framed_zstd_payload() {
        let blob = framed_blob(CodecKind::Zstd, b"zstd payload");

        match Header::parse_blob(&blob).unwrap() {
            ParsedBlob::Framed(header, payload) => {
                assert_eq!(header.codec, CodecKind::Zstd as u8);
                assert_eq!(payload, b"zstd payload");
            }
            ParsedBlob::Plain => panic!("valid Zstd frame was not recognized"),
        }
    }

    #[test]
    fn treats_unknown_codec_as_plain() {
        let mut blob = framed_blob(CodecKind::Zstd, b"payload");
        blob[1] = 0;

        assert!(matches!(
            Header::parse_blob(&blob).unwrap(),
            ParsedBlob::Plain
        ));
    }

    #[test]
    fn treats_unsupported_newer_version_as_plain() {
        let mut blob = framed_blob(CodecKind::Zstd, b"payload");
        blob[2] = Header::VERSION + 1;

        assert!(matches!(
            Header::parse_blob(&blob).unwrap(),
            ParsedBlob::Plain
        ));
    }

    #[test]
    fn treats_unframed_data_as_plain() {
        assert!(matches!(
            Header::parse_blob(b"arbitrary binary data").unwrap(),
            ParsedBlob::Plain
        ));
    }
}
