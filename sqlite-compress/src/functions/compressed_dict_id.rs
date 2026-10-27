use sqlite_ffi::{Context, SqliteError, Value};

use crate::functions::{ParsedBlob, errors::CodecError, header::Header};

fn dict_id_from_blob(blob: &[u8]) -> Result<Option<u32>, CodecError> {
    match Header::parse_blob(blob)? {
        ParsedBlob::Framed(header, _) => Ok(Some(header.dict_id.get())),
        ParsedBlob::Plain => Ok(None),
    }
}

/// SQLite callback for `compressed_dict_id(blob)`.
///
/// Returns the dictionary ID stored in the blob header, or NULL if the blob
/// is not in a recognized compressed format.
pub fn sqlite_compressed_dict_id(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    match dict_id_from_blob(values[0].to_blob()).map_err(|e| SqliteError::Message(e.to_string()))? {
        Some(dict_id) => context.result_int64(i64::from(dict_id)),
        None => context.result_null(),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::dict_id_from_blob;
    use crate::{DictId, functions::Header};

    fn framed_blob(dict_id: u32) -> Vec<u8> {
        let header = Header::new(DictId::from(dict_id), 0);
        let mut blob = Vec::new();
        header.write(&mut blob);
        blob
    }

    #[test]
    fn returns_dictionary_id_from_framed_blob() {
        let blob = framed_blob(42);

        assert_eq!(dict_id_from_blob(&blob).unwrap(), Some(42));
    }

    #[test]
    fn returns_zero_for_raw_zstd_blob() {
        let blob = framed_blob(0);

        assert_eq!(dict_id_from_blob(&blob).unwrap(), Some(0));
    }

    #[test]
    fn returns_none_for_plain_blob() {
        assert_eq!(dict_id_from_blob(b"plain text").unwrap(), None);
    }

    #[test]
    fn returns_none_for_malformed_header() {
        assert_eq!(dict_id_from_blob(&[0, 1, 2]).unwrap(), None);
    }
}
