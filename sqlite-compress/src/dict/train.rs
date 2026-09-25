use sqlite_ffi::{first_value, sample_bytes, Connection, SqlValue};

use crate::{
    cache::{get_decoder_in_cache, insert_into_caches},
    dict::{
        types::{ColumnKey, DictId},
        DICT_TABLE_NAME,
    },
    functions::{decompress_raw, decompress_with_decoder, CodecError, Level},
    setup::{SetupConfig, SetupConnection, SetupError, SqlIdent},
    state::ExtensionState,
    utils::quote_qualified,
    Header, SetupColumn, SetupTable,
};

const SAMPLE_BATCH_SIZE: usize = 64;

pub fn train_all<C: SetupConnection>(
    connection: &C,
    config: &SetupConfig,
    dict_capacity: usize,
) -> Result<Vec<DictId>, SetupError> {
    let mut dict_ids = Vec::new();

    for (table, column) in config.iter_columns() {
        match train_by_column(
            connection,
            table,
            column,
            config.compression_level,
            dict_capacity,
        )? {
            Some(dict_id) => dict_ids.push(dict_id),
            None => continue,
        }
    }

    Ok(dict_ids)
}

pub fn train_by_column<C: SetupConnection>(
    connection: &C,
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

    let state = ExtensionState::from_db(unsafe { connection.sqlite_handle() })?;

    let (enough, available) =
        has_enough_samples(&state, &table_name, &column_name, column.min_samples)?;

    if !enough {
        return Ok(None);
    }

    if !has_retrain_required(
        &state,
        &dict_store,
        key.table(),
        key.column(),
        available,
        column.retrain_growth,
    )? {
        return Ok(None);
    }

    let dictionary = build_dictionary(
        &state,
        key.schema(),
        &table_name,
        &column_name,
        column.max_samples,
        dict_capacity,
    )?;

    let dict_id = persist_dictionary(
        &state,
        &dict_store,
        key.table(),
        key.column(),
        &dictionary,
        available,
    )?;

    insert_into_caches(&state.cache, &key, dict_id, &dictionary, compression_level);

    Ok(Some(dict_id))
}

pub(crate) fn decode_sample(
    state: &ExtensionState,
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

    let decoder = get_decoder_in_cache(state, schema, dict_id)?;

    decompress_with_decoder(payload, &decoder, len)
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

fn has_enough_samples<C: AsRef<Connection>>(
    connection: &C,
    table_name: &str,
    column_name: &str,
    min_samples: usize,
) -> Result<(bool, i64), SetupError> {
    let connection = connection.as_ref();

    let rows = connection
        .query(
            &format!(
                "SELECT COUNT(*) FROM {table_name} \
             WHERE {column_name} IS NOT NULL AND length({column_name}) > 0"
            ),
            &[],
        )
        .map_err(SetupError::from_conn)?;

    let count = first_value(&rows).and_then(SqlValue::as_i64).unwrap_or(0);

    Ok((count >= min_samples as i64, count))
}

/// Check if the retrain is required.
/// It is done by checking the last row count of the dictionary store.
/// If the last row count is less than the retrain growth, the retrain is required.
/// Last stored row_count for this column vs current sample count.
/// No prior dict → retrain. Otherwise need `last + retrain_growth` samples.
fn has_retrain_required<C: AsRef<Connection>>(
    connection: &C,
    dict_table: &str,
    table_name: &str,
    column_name: &str,
    available: i64,
    retrain_growth: usize,
) -> Result<bool, SetupError> {
    let connection = connection.as_ref();

    let rows = connection
        .query(
            &format!(
                "SELECT COALESCE(\
                (SELECT row_count FROM {dict_table} \
                 WHERE table_name = ?1 AND column_name = ?2 \
                 ORDER BY id DESC LIMIT 1),\
                -1)",
            ),
            &[
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
            ],
        )
        .map_err(SetupError::from_conn)?;

    let last = first_value(&rows).and_then(SqlValue::as_i64).unwrap_or(0);

    if last < 0 {
        return Ok(true);
    }

    Ok(available >= last + retrain_growth as i64)
}

/// Build the dictionary from the samples.
/// It is done by collecting the samples from the tables, and building the dictionary.
/// Using stream approach to avoid loading all the samples into memory.
fn build_dictionary(
    state: &ExtensionState,
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

        let rows = state
            .connection
            .query(
                &format!(
                    "SELECT {column_name} AS value FROM {table_name} \
         WHERE {column_name} IS NOT NULL AND length({column_name}) > 0 \
         ORDER BY rowid DESC \
         LIMIT ?1 OFFSET ?2"
                ),
                &[
                    SqlValue::Integer(batch_limit as i64),
                    SqlValue::Integer(offset as i64),
                ],
            )
            .map_err(SetupError::from_conn)?;

        if rows.is_empty() {
            break;
        }

        let batch_len = rows.len();
        for row in &rows {
            let Some(blob) = row.first().and_then(sample_bytes) else {
                continue;
            };

            let sample = decode_sample(state, &blob, schema).map_err(|e| {
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
    state: &ExtensionState,
    schema_name: &str,
    table_name: &str,
    column_name: &str,
    dictionary: &[u8],
    row_count: i64,
) -> Result<DictId, SetupError> {
    state
        .connection
        .execute(
            &format!(
                "INSERT INTO {schema_name} (dict, trained_at, table_name, column_name, row_count) \
        VALUES (?1, strftime('%s', 'now'), ?2, ?3, ?4) \
        RETURNING id",
            ),
            &[
                SqlValue::Blob(dictionary.to_vec()),
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
                SqlValue::Integer(row_count),
            ],
        )
        .map_err(SetupError::from_conn)
        .and_then(|id| {
            u32::try_from(id)
                .map(DictId::new)
                .map_err(|_| SetupError::DictTrain("dictionary id overflow".to_string()))
        })
}
