use crate::{
    cache::{get_decoder, insert_into_caches, CacheKeySource},
    dict::{
        types::{ColumnKey, DictId},
        DICT_TABLE_NAME,
    },
    functions::{decompress_raw, decompress_with_decoder, CodecError, Level},
    setup::{SetupConfig, SetupConnection, SetupError, SqlIdent},
    utils::{quote_literal, quote_qualified},
    Header, SetupColumn, SetupTable,
};

const SAMPLE_BATCH_SIZE: usize = 64;
const SAVEPOINT: &str = "sqlite_compress_persist_dict";

/// Trains each configured column and returns the ids of dictionaries created.
///
/// Columns without enough new data are skipped.
///
/// # Errors
///
/// Returns an error when configuration is invalid, SQLite access fails, or
/// Zstandard cannot construct a dictionary from the collected samples.
pub fn train_all<C>(
    conn: &mut C,
    config: &SetupConfig,
    dict_capacity: usize,
) -> Result<Vec<DictId>, SetupError>
where
    C: SetupConnection + CacheKeySource,
{
    let mut dict_ids = Vec::new();

    for (table, column) in config.iter_columns() {
        match train_by_column(conn, table, column, config.compression_level, dict_capacity)? {
            Some(dict_id) => dict_ids.push(dict_id),
            None => continue,
        }
    }

    Ok(dict_ids)
}

/// Trains a new dictionary only when the column has enough additional samples.
///
/// A successful retrain atomically replaces the current dictionary for the
/// column and updates the cache used by the extension's compression scalar function.
///
/// # Errors
///
/// Returns an error when configuration is invalid, sample decoding or database
/// access fails, or the resulting dictionary cannot be persisted.
pub fn train_by_column<C>(
    conn: &mut C,
    table: &SetupTable,
    column: &SetupColumn,
    compression_level: Level,
    dict_capacity: usize,
) -> Result<Option<DictId>, SetupError>
where
    C: SetupConnection + CacheKeySource,
{
    validate_config(table.columns.len(), dict_capacity)?;

    let key = ColumnKey::new(
        table.schema.as_str(),
        table.name.as_str(),
        column.name.as_str(),
    );
    let table_name = table.as_qualified_name();
    let column_name = column.name.quote();
    let dict_store = quote_qualified(key.schema(), DICT_TABLE_NAME);

    let (enough, available) =
        has_enough_samples(conn, &table_name, &column_name, column.min_samples)?;

    if !enough {
        return Ok(None);
    }

    if !has_retrain_required(
        conn,
        &dict_store,
        key.table(),
        key.column(),
        available,
        column.retrain_growth,
    )? {
        return Ok(None);
    }

    let dictionary = build_dictionary(
        conn,
        key.schema(),
        &table_name,
        &column_name,
        column.max_samples,
        dict_capacity,
    )?;

    let dict_id = persist_dictionary(
        conn,
        &dict_store,
        key.table(),
        key.column(),
        &dictionary,
        available,
    )?;

    insert_into_caches(conn, &key, dict_id, &dictionary, compression_level);

    Ok(Some(dict_id))
}

/// Decodes a sample only when it has a supported extension header.
///
/// Blobs without a supported header remain in the training corpus unchanged.
/// Once a header is accepted, decompression failures are propagated rather than
/// silently training on a corrupted compressed value.
pub(crate) fn decode_sample<C>(
    conn: &mut C,
    blob: &[u8],
    schema: &str,
) -> Result<Vec<u8>, CodecError>
where
    C: SetupConnection + CacheKeySource,
{
    let (header, payload) = match Header::parse(blob) {
        Ok(parsed) => parsed,
        Err(CodecError::MalformedHeader)
        | Err(CodecError::UnknownCodec(_))
        | Err(CodecError::UnknownVersion(_)) => return Ok(blob.to_vec()),
        Err(e) => return Err(e),
    };

    let dict_id = DictId::from(header.dict_id.get());
    let len = header.uncompressed_len.get() as usize;
    if dict_id.get() == 0 {
        return decompress_raw(payload, len);
    }

    let decoder = get_decoder(conn, schema, dict_id).map_err(CodecError::DictError)?;
    decompress_with_decoder(payload, &decoder, len).map_err(CodecError::DecompressionFailed)
}

/// Rejects configurations that cannot produce a useful dictionary.
fn validate_config(column_size: usize, dict_capacity: usize) -> Result<(), SetupError> {
    if dict_capacity == 0 {
        return Err(SetupError::InvalidConfig(
            "dictionary capacity must be greater than zero",
        ));
    }

    if column_size == 0 {
        return Err(SetupError::InvalidConfig(
            "at least one column must be configured",
        ));
    }

    Ok(())
}

fn has_enough_samples<C>(
    conn: &mut C,
    table_name: &str,
    column_name: &str,
    min_samples: usize,
) -> Result<(bool, i64), SetupError>
where
    C: SetupConnection,
{
    let count = conn
        .query_i64(&format!(
            "SELECT COUNT(*) FROM {table_name} \
         WHERE {column_name} IS NOT NULL AND length({column_name}) > 0"
        ))
        .map_err(SetupError::from_conn)?;

    Ok((count >= min_samples as i64, count))
}

/// Decides whether sample growth justifies replacing the current dictionary.
///
/// The persisted row count records the sample population used by the previous
/// training run, rather than the number of bytes in its dictionary.
fn has_retrain_required<C>(
    conn: &mut C,
    dict_table: &str,
    table_name: &str,
    column_name: &str,
    available: i64,
    retrain_growth: usize,
) -> Result<bool, SetupError>
where
    C: SetupConnection,
{
    let last = conn
        .query_i64(&format!(
            "SELECT COALESCE(\
                (SELECT row_count FROM {dict_table} \
                 WHERE table_name = {} AND column_name = {} \
                 ORDER BY id DESC LIMIT 1),\
                -1)",
            quote_literal(table_name),
            quote_literal(column_name)
        ))
        .map_err(SetupError::from_conn)?;

    if last < 0 {
        return Ok(true);
    }

    Ok(available >= last + retrain_growth as i64)
}

/// Builds a dictionary from recent non-empty values without retaining all rows.
///
/// Values are fetched in batches so a connection can stream rows rather than
/// materializing the complete query result.
fn build_dictionary<C>(
    conn: &mut C,
    schema: &str,
    table_name: &str,
    column_name: &str,
    max_samples: usize,
    dict_capacity: usize,
) -> Result<Vec<u8>, SetupError>
where
    C: SetupConnection + CacheKeySource,
{
    let mut corpus = Vec::new();
    let mut sizes = Vec::new();
    let mut offset = 0usize;
    let mut remaining = max_samples;

    while remaining > 0 {
        let batch_limit = remaining.min(SAMPLE_BATCH_SIZE);
        let sql = format!(
            "SELECT {column_name} AS value FROM {table_name} \
         WHERE {column_name} IS NOT NULL AND length({column_name}) > 0 \
         ORDER BY rowid DESC \
         LIMIT {batch_limit} OFFSET {offset}"
        );

        let mut batch = Vec::new();
        conn.for_each_blob(&sql, |blob| {
            batch.push(blob.to_vec());
            Ok(())
        })
        .map_err(SetupError::from_conn)?;

        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len();
        for blob in &batch {
            let sample = decode_sample(conn, blob, schema).map_err(|e| {
                SetupError::DictTrain(format!("failed to decode training sample: {e}"))
            })?;
            corpus.extend_from_slice(&sample);
            sizes.push(sample.len());
        }

        offset += batch_len;
        remaining = remaining.saturating_sub(batch_len);
        if batch_len < batch_limit {
            break;
        }
    }

    if sizes.is_empty() {
        return Err(SetupError::DictTrain(
            "no non-empty samples available".to_string(),
        ));
    }

    zstd::dict::from_continuous(&corpus, &sizes, dict_capacity)
        .map_err(|error| SetupError::DictTrain(error.to_string()))
}

/// Replaces the current dictionary for one column as a single savepoint.
///
/// Demoting the previous row and inserting the replacement must succeed
/// together; otherwise readers could observe a column with no current id.
fn persist_dictionary<C>(
    conn: &mut C,
    schema_name: &str,
    table_name: &str,
    column_name: &str,
    dictionary: &[u8],
    row_count: i64,
) -> Result<DictId, SetupError>
where
    C: SetupConnection,
{
    let demote_sql = format!(
        "UPDATE {schema_name} SET is_current = 0 \
     WHERE table_name = {} AND column_name = {} AND is_current = 1",
        quote_literal(table_name),
        quote_literal(column_name),
    );

    let insert_sql = format!(
        "INSERT INTO {schema_name} (dict, trained_at, table_name, column_name, row_count, is_current) \
        VALUES (?1, strftime('%s', 'now'), {}, {}, {row_count}, 1) \
        RETURNING id",
        quote_literal(table_name),
        quote_literal(column_name)
    );

    conn.batch_execute(&format!("SAVEPOINT {SAVEPOINT};"))
        .map_err(SetupError::from_conn)?;

    let result = (|| -> Result<i64, SetupError> {
        conn.batch_execute(&demote_sql)
            .map_err(SetupError::from_conn)?;
        conn.execute_blob(&insert_sql, dictionary)
            .map_err(SetupError::from_conn)
    })();

    match result {
        Ok(id) => {
            conn.batch_execute(&format!("RELEASE SAVEPOINT {SAVEPOINT};"))
                .map_err(SetupError::from_conn)?;
            u32::try_from(id)
                .map(DictId::new)
                .map_err(|_| SetupError::DictTrain("dictionary id overflow".to_string()))
        }
        Err(e) => {
            let _ = conn.batch_execute(&format!(
                "ROLLBACK TO SAVEPOINT {SAVEPOINT}; RELEASE SAVEPOINT {SAVEPOINT};"
            ));
            Err(e)
        }
    }
}
