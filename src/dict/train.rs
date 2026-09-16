use crate::{
    cache::{get_decoder_in_cache, insert_into_caches},
    conn::Connection,
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

pub fn train_all<C>(
    conn: &C,
    config: &SetupConfig,
    dict_capacity: usize,
) -> Result<Vec<DictId>, SetupError>
where
    C: SetupConnection,
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

pub fn train_by_column<C: SetupConnection>(
    conn: &C,
    table: &SetupTable,
    column: &SetupColumn,
    compression_level: Level,
    dict_capacity: usize,
) -> Result<Option<DictId>, SetupError> {
    validate_config(table.columns.len(), dict_capacity)?;

    let key = ColumnKey::new(
        table.schema.as_str(),
        table.name.as_str(),
        column.name.as_str(),
    );
    let table_name = table.as_qualified_name();
    let column_name = column.name.quote();
    let dict_store = quote_qualified(key.schema(), DICT_TABLE_NAME);

    let conn = Connection::from_db(conn.sqlite_handle()).unwrap();

    let (enough, available) =
        has_enough_samples(&conn, &table_name, &column_name, column.min_samples)?;

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
        &conn,
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

    insert_into_caches(&conn.cache, &key, dict_id, &dictionary, compression_level);

    Ok(Some(dict_id))
}

pub(crate) fn decode_sample(
    conn: &Connection,
    blob: &[u8],
    schema: &str,
) -> Result<Vec<u8>, CodecError> {
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

    let decoder = get_decoder_in_cache(&conn, schema, dict_id).unwrap();

    decompress_with_decoder(payload, &decoder, len).map_err(CodecError::DecompressionFailed)
}

/// Validate the config is valid.
/// It is done by checking the dictionary capacity and the tables and columns configuration.
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

fn has_enough_samples(
    conn: &Connection,
    table_name: &str,
    column_name: &str,
    min_samples: usize,
) -> Result<(bool, i64), SetupError> {
    let count = conn
        .db
        .query_i64(&format!(
            "SELECT COUNT(*) FROM {table_name} \
         WHERE {column_name} IS NOT NULL AND length({column_name}) > 0"
        ))
        .map_err(SetupError::from_conn)?;

    Ok((count >= min_samples as i64, count))
}

/// Check if the retrain is required.
/// It is done by checking the last row count of the dictionary store.
/// If the last row count is less than the retrain growth, the retrain is required.
/// Last stored row_count for this column vs current sample count.
/// No prior dict → retrain. Otherwise need `last + retrain_growth` samples.
fn has_retrain_required(
    conn: &Connection,
    dict_table: &str,
    table_name: &str,
    column_name: &str,
    available: i64,
    retrain_growth: usize,
) -> Result<bool, SetupError> {
    let last = conn
        .db
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

/// Build the dictionary from the samples.
/// It is done by collecting the samples from the tables, and building the dictionary.
/// Using stream approach to avoid loading all the samples into memory.
fn build_dictionary(
    conn: &Connection,
    schema: &str,
    table_name: &str,
    column_name: &str,
    max_samples: usize,
    dict_capacity: usize,
) -> Result<Vec<u8>, SetupError> {
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

        let batch = conn.db.query_blobs(&sql).map_err(SetupError::from_conn)?;

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

/// Persist the dictionary in the database.
/// It is done by inserting the dictionary into the dictionary store.
/// The dictionary store is a table with the following columns:
/// - id: the id of the dictionary
/// - dict: the dictionary
/// - trained_at: the timestamp of the training
/// - row_count: the number of rows in the dictionary
fn persist_dictionary(
    conn: &Connection,
    schema_name: &str,
    table_name: &str,
    column_name: &str,
    dictionary: &[u8],
    row_count: i64,
) -> Result<DictId, SetupError> {
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

    conn.db.batch_execute(&format!("SAVEPOINT {SAVEPOINT};"));

    let result = (|| -> Result<i64, SetupError> {
        conn.db.batch_execute(&demote_sql);
        conn.db
            .execute_blob(&insert_sql, dictionary)
            .map_err(SetupError::from_conn)
    })();

    match result {
        Ok(id) => {
            conn.db
                .batch_execute(&format!("RELEASE SAVEPOINT {SAVEPOINT};"));
            u32::try_from(id)
                .map(DictId::new)
                .map_err(|_| SetupError::DictTrain("dictionary id overflow".to_string()))
        }
        Err(e) => {
            let _ = conn.db.batch_execute(&format!(
                "ROLLBACK TO SAVEPOINT {SAVEPOINT}; RELEASE SAVEPOINT {SAVEPOINT};"
            ));
            Err(e)
        }
    }
}
