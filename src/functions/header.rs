use zerocopy::{little_endian::U32, FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes};

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

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct Header {
    pub codec: u8,
    pub dict_id: U32,
    pub uncompressed_len: U32,
    pub schema_len: u8,
}

impl Header {
    pub const PREFIX_SIZE: usize = size_of::<Self>();

    pub fn new(dict_id: DictId, uncompressed_len: u32, schema_len: u8) -> Self {
        Self {
            codec: CodecKind::Zstd as u8,
            dict_id: U32::new(dict_id.get()),
            uncompressed_len: U32::new(uncompressed_len),
            schema_len,
        }
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(self.as_bytes());
    }

    pub fn parse(blob: &[u8]) -> Result<(Self, &str, &[u8]), CodecError> {
        let (header, rest) =
            Self::ref_from_prefix(blob).map_err(|_| CodecError::MalformedHeader)?;

        let _codec = CodecKind::try_from(header.codec)?;
        let schema_len = header.schema_len as usize;
        if rest.len() < schema_len {
            return Err(CodecError::MalformedHeader);
        }

        let (schema_bytes, payload) = rest.split_at(schema_len);
        let schema = std::str::from_utf8(schema_bytes).map_err(|_| CodecError::MalformedHeader)?;

        Ok((*header, schema, payload))
    }
}

pub fn wrap(
    dict_id: DictId,
    schema: &str,
    original_len: usize,
    payload: Vec<u8>,
) -> Result<Vec<u8>, CodecError> {
    let uncompressed_len = u32::try_from(original_len).map_err(|_| CodecError::PayloadTooLarge)?;
    let schema = if dict_id.get() == 0 { "" } else { schema };
    let schema_len = u8::try_from(schema.len()).map_err(|_| CodecError::PayloadTooLarge)?;
    let header = Header::new(dict_id, uncompressed_len, schema_len);
    let mut out = Vec::with_capacity(Header::PREFIX_SIZE + schema.len() + payload.len());

    header.write(&mut out);
    out.extend_from_slice(schema.as_bytes());

    out.extend_from_slice(&payload);
    Ok(out)
}
