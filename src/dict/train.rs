use crate::{
    dict::{
        dict_table, insert_into_caches,
        types::{ColumnKey, DictId},
    },
    functions::Level,
    setup::{SetupConfig, SetupConnection, SetupError, SqlIdent},
    utils::quote_literal,
    SetupColumn, SetupTable, CURRENT_DICT_IDS,
};

/// Train the dictionary if the condition is met (enough samples and retrain growth).
/// It is done by collecting the samples from the tables, and building the dictionary.
/// Then, the dictionary is persisted in the database.
/// Finally, the dictionary is sync to the caches, sync to the cache is mandatory to ensure the dictionary is ready to be used.
/// By [get_decoder_cached](crate::dict::get_decoder_cached), [get_encoder_cached](crate::dict::get_encoder_cached),
/// the dictionary is cached in memory.
pub fn train_all<C>(
    conn: &mut C,
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

pub fn train_by_column<C>(
    conn: &mut C,
    table: &SetupTable,
    column: &SetupColumn,
    compression_level: Level,
    dict_capacity: usize,
) -> Result<Option<DictId>, SetupError>
where
    C: SetupConnection,
{
    validate_config(table.columns.len(), dict_capacity)?;

    let key = ColumnKey::new(
        table.schema.as_str(),
        table.name.as_str(),
        column.name.as_str(),
    );
    let table_name = table.as_qualified_name();
    let column_name = column.name.quote();
    let dict_store = dict_table(key.schema());

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

    insert_into_caches(key.schema(), dict_id, &dictionary, compression_level);
    CURRENT_DICT_IDS.lock().insert(key, dict_id);

    Ok(Some(dict_id))
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

/// Check if the retrain is required.
/// It is done by checking the last row count of the dictionary store.
/// If the last row count is less than the retrain growth, the retrain is required.
/// Last stored row_count for this column vs current sample count.
/// No prior dict → retrain. Otherwise need `last + retrain_growth` samples.
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

/// Build the dictionary from the samples.
/// It is done by collecting the samples from the tables, and building the dictionary.
/// Using stream approach to avoid loading all the samples into memory.
fn build_dictionary<C>(
    conn: &mut C,
    table_name: &str,
    column_name: &str,
    max_samples: usize,
    dict_capacity: usize,
) -> Result<Vec<u8>, SetupError>
where
    C: SetupConnection,
{
    let mut corpus = Vec::new();
    let mut sizes = Vec::new();

    let sql = format!(
        "SELECT {column_name} AS value FROM {table_name} \
     WHERE {column_name} IS NOT NULL AND length({column_name}) > 0 \
     ORDER BY rowid DESC \
     LIMIT {max_samples}"
    );

    conn.for_each_blob(&sql, |blob| {
        corpus.extend_from_slice(blob);
        sizes.push(blob.len());
        Ok(())
    })
    .map_err(SetupError::from_conn)?;

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
    let sql = format!(
        "INSERT INTO {schema_name} (dict, trained_at, table_name, column_name, row_count) \
        VALUES (?1, strftime('%s', 'now'), {}, {}, {row_count}) \
        RETURNING id",
        quote_literal(table_name),
        quote_literal(column_name)
    );

    let id = conn
        .execute_blob(&sql, dictionary)
        .map_err(SetupError::from_conn)?;

    u32::try_from(id)
        .map(DictId::new)
        .map_err(|_| SetupError::DictTrain("dictionary id overflow".to_string()))
}
