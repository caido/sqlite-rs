use sqlite_ffi::{Context, SqliteError, Value};
use zstd::stream::raw::{Decoder, Operation};

use crate::{
    cache::get_decoder_in_cache, functions::header::Header, DictError, DictId, ExtensionState,
};

const CHUNK: usize = 64 * 1024;

struct Progress {
    matched: bool,
    frame_done: bool,
}

pub fn sqlite_clike(context: Context, values: &[Value]) -> Result<(), SqliteError> {
    let schema = values[0].to_text()?;
    let table = values[1].to_text()?;
    let column = values[2].to_text()?;
    let rowid = values[3].to_i64();
    let needle = values[4].to_text()?.as_bytes();

    let state = ExtensionState::from_context(&context)?;
    let blob = state
        .connection
        .open_blob(schema, table, column, rowid, false)?;

    let total = blob.len();
    if total < Header::PREFIX_SIZE {
        return Err(SqliteError::Message(
            "compressed blob is shorter than the header".into(),
        ));
    }

    let mut compressed = vec![0u8; CHUNK.min(total)];
    blob.read_at(0, &mut compressed)?;

    let (header, _) =
        Header::parse(&compressed).map_err(|e| SqliteError::Message(e.to_string()))?;
    let dict_id = DictId::from(header.dict_id.get());
    let header_len = header.header_len as usize;
    let first_len = compressed.len();

    let prepared = if dict_id.get() != 0 {
        match get_decoder_in_cache(&state, schema, dict_id) {
            Ok(dict) => Some(dict),
            Err(DictError::NotReady) => None,
            Err(e) => return Err(SqliteError::Message(e.to_string())),
        }
    } else {
        None
    };

    let matched = if let Some(dict) = &prepared {
        let mut decoder = Decoder::with_prepared_dictionary(dict.as_ref())
            .map_err(|e| SqliteError::Message(e.to_string()))?;
        scan(
            &mut decoder,
            &blob,
            &mut compressed,
            header_len,
            first_len,
            needle,
        )?
    } else {
        let mut decoder = Decoder::new().map_err(|e| SqliteError::Message(e.to_string()))?;
        scan(
            &mut decoder,
            &blob,
            &mut compressed,
            header_len,
            first_len,
            needle,
        )?
    };

    context.result_int64(if matched { 1 } else { 0 });
    Ok(())
}

fn scan(
    decoder: &mut Decoder<'_>,
    blob: &sqlite_ffi::SqliteBlob,
    compressed: &mut [u8],
    header_len: usize,
    first_len: usize,
    needle: &[u8],
) -> Result<bool, SqliteError> {
    let mut plain = vec![0u8; CHUNK];
    let mut overlap = Vec::new();

    let first = feed(
        decoder,
        &compressed[header_len..first_len],
        &mut plain,
        &mut overlap,
        needle,
    )?;

    if first.matched || first.frame_done {
        eprintln!(
            "clike rust vecs: compressed={} plain={} overlap={}",
            compressed.len(),
            plain.capacity(),
            overlap.capacity(),
        );

        return Ok(first.matched);
    }

    let total = blob.len();
    let mut pos = first_len;
    while pos < total {
        let n = CHUNK.min(total - pos);
        blob.read_at(pos, &mut compressed[..n])?;
        pos += n;

        let step = feed(decoder, &compressed[..n], &mut plain, &mut overlap, needle)?;
        if step.matched || step.frame_done {
            eprintln!(
                "clike rust vecs: compressed={} plain={} overlap={}",
                compressed.len(),
                plain.capacity(),
                overlap.capacity(),
            );

            return Ok(step.matched);
        }
    }

    let tail = feed(decoder, &[], &mut plain, &mut overlap, needle)?;

    eprintln!(
        "clike rust vecs: compressed={} plain={} overlap={}",
        compressed.len(),
        plain.capacity(),
        overlap.capacity(),
    );
    Ok(tail.matched)
}

fn feed(
    decoder: &mut Decoder<'_>,
    mut input: &[u8],
    plain: &mut [u8],
    overlap: &mut Vec<u8>,
    needle: &[u8],
) -> Result<Progress, SqliteError> {
    loop {
        let status = decoder
            .run_on_buffers(input, plain)
            .map_err(|e| SqliteError::Message(e.to_string()))?;

        if status.bytes_written > 0 && take_match(overlap, &plain[..status.bytes_written], needle) {
            return Ok(Progress {
                matched: true,
                frame_done: status.remaining == 0,
            });
        }

        input = &input[status.bytes_read..];

        if status.remaining == 0 && status.bytes_written < plain.len() {
            return Ok(Progress {
                matched: false,
                frame_done: true,
            });
        }

        if input.is_empty() && status.bytes_written < plain.len() {
            return Ok(Progress {
                matched: false,
                frame_done: false,
            });
        }

        if status.bytes_read == 0 && status.bytes_written == 0 {
            return Err(SqliteError::Message("zstd decoder made no progress".into()));
        }
    }
}

fn take_match(overlap: &mut Vec<u8>, chunk: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return false;
    }

    overlap.extend_from_slice(chunk);
    let matched = overlap.windows(needle.len()).any(|window| window == needle);
    let keep = needle.len() - 1;
    if overlap.len() > keep {
        overlap.drain(..overlap.len() - keep);
    }
    matched
}
