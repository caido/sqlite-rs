use crate::{dict::DictId, functions::errors::CodecError};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub codec: CodecKind,
    pub dict_id: DictId,
    pub uncompressed_len: u32,
}

impl Header {
    pub const WIRE_SIZE: usize = 1 + 4 + 4;

    pub fn new(dict_id: DictId, uncompressed_len: usize) -> Result<Self, CodecError> {
        let uncompressed_len =
            u32::try_from(uncompressed_len).map_err(|_| CodecError::PayloadTooLarge)?;
        Ok(Self {
            codec: CodecKind::Zstd,
            dict_id,
            uncompressed_len,
        })
    }

    pub fn write(&self, out: &mut Vec<u8>) {
        out.push(self.codec as u8);
        out.extend_from_slice(&self.dict_id.get().to_le_bytes());
        out.extend_from_slice(&self.uncompressed_len.to_le_bytes());
    }

    pub fn parse(blob: &[u8]) -> Result<(Self, &[u8]), CodecError> {
        let bytes = blob
            .get(..Self::WIRE_SIZE)
            .ok_or(CodecError::MalformedHeader)?;
        let codec = CodecKind::try_from(bytes[0])?;
        let dict_id = DictId::from(u32::from_le_bytes(bytes[1..5].try_into().unwrap()));
        let uncompressed_len = u32::from_le_bytes(bytes[5..9].try_into().unwrap());
        Ok((
            Self {
                codec,
                dict_id,
                uncompressed_len,
            },
            &blob[Self::WIRE_SIZE..],
        ))
    }
}

pub fn wrap(dict_id: DictId, original_len: usize, payload: Vec<u8>) -> Result<Vec<u8>, CodecError> {
    let header = Header::new(dict_id, original_len)?;
    let mut out = Vec::with_capacity(Header::WIRE_SIZE + payload.len());
    header.write(&mut out);
    out.extend_from_slice(&payload);
    Ok(out)
}
