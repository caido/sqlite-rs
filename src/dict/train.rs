use crate::dict::insert_into_caches;
use crate::dict::types::DictId;
use crate::setup::{SchemaName, SetupConfig, SetupConnection, SetupError, SqlIdent};
use std::collections::HashSet;

/// Train the dictionary if the condition is met (enough samples and retrain growth).
/// It is done by collecting the samples from the tables, and building the dictionary.
/// Then, the dictionary is persisted in the database.
/// Finally, the dictionary is sync to the caches, sync to the cache is mandatory to ensure the dictionary is ready to be used.
/// By [get_decoder_cached](crate::dict::get_decoder_cached), [get_encoder_cached](crate::dict::get_encoder_cached),
/// the dictionary is cached in memory.
pub fn train<C>(
    conn: &mut C,
    config: &SetupConfig,
    dict_capacity: usize,
    max_samples: usize,
    min_samples: usize,
) -> Result<DictId, SetupError>
where
    C: SetupConnection,
{
    validate_config(config, dict_capacity)?;
    let available = ensure_enough_samples(conn, config, min_samples)?;
    let schemas = unique_schemas(config);

    let retrain_growth = config.retrain_growth.try_into().unwrap_or(5000);

    if !retrain_required(conn, &schemas, available, retrain_growth)? {
        return Err(SetupError::DictTrain(format!(
            "retrain skipped: need at least {retrain_growth} new samples since last train"
        )));
    }

    let dictionary = build_dictionary(conn, config, max_samples, dict_capacity)?;

    let dict_id = persist_dictionary(conn, &schemas, &dictionary, available)?;
    insert_into_caches(dict_id, &dictionary, config.compression_level);
    Ok(dict_id)
}

/// Validate the config is valid.
/// It is done by checking the dictionary capacity and the tables and columns configuration.
fn validate_config(config: &SetupConfig, dict_capacity: usize) -> Result<(), SetupError> {
    if dict_capacity == 0 {
        return Err(SetupError::InvalidConfig(
            "dictionary capacity must be greater than zero",
        ));
    }

    if config.tables.is_empty() {
        return Err(SetupError::InvalidConfig(
            "at least one table must be configured",
        ));
    }

    if config.tables.iter().all(|table| table.columns.is_empty()) {
        return Err(SetupError::InvalidConfig(
            "at least one column must be configured",
        ));
    }

    Ok(())
}

/// Since dictionary is created by schema,
/// we don't to train twice the same schema.
fn unique_schemas(config: &SetupConfig) -> Vec<&SchemaName> {
    let mut schemas = Vec::new();
    let mut seen = HashSet::new();
    for table in &config.tables {
        if seen.insert(&table.schema) {
            schemas.push(&table.schema);
        }
    }
    schemas
}

fn ensure_enough_samples<C>(
    conn: &mut C,
    config: &SetupConfig,
    min_samples: usize,
) -> Result<i64, SetupError>
where
    C: SetupConnection,
{
    let available = count_available_samples(conn, config)?;
    if available < min_samples as i64 {
        return Err(SetupError::DictTrain(format!(
            "not enough samples: have {available}, need at least {min_samples}"
        )));
    }
    Ok(available)
}

fn count_available_samples<C>(conn: &mut C, config: &SetupConfig) -> Result<i64, SetupError>
where
    C: SetupConnection,
{
    let mut parts = Vec::new();
    for table in &config.tables {
        let table_name = table.as_qualified_name();
        for column in &table.columns {
            let column_name = column.quote();
            parts.push(format!(
                "SELECT COUNT(*) AS c FROM {table_name} \
                 WHERE {column_name} IS NOT NULL AND length({column_name}) > 0"
            ));
        }
    }
    conn.query_i64(&format!(
        "SELECT COALESCE(SUM(c), 0) FROM ({})",
        parts.join(" UNION ALL ")
    ))
    .map_err(SetupError::from_conn)
}

/// Check if the retrain is required.
/// It is done by checking the last row count of the dictionary store.
/// If the last row count is less than the retrain growth, the retrain is required.
fn retrain_required<C>(
    conn: &mut C,
    schemas: &[&SchemaName],
    available: i64,
    retrain_growth: i64,
) -> Result<bool, SetupError>
where
    C: SetupConnection,
{
    let mut last_row_count: Option<i64> = None;
    for schema in schemas {
        let schema_name = schema.as_zstd_schema_name();
        let exists = conn
            .query_i64(&format!("SELECT COUNT(*) FROM {schema_name}"))
            .map_err(SetupError::from_conn)?;

        if exists == 0 {
            continue;
        }

        let row_count = conn
            .query_i64(&format!(
                "SELECT row_count FROM {schema_name} \
                 ORDER BY id DESC LIMIT 1"
            ))
            .map_err(SetupError::from_conn)?;

        last_row_count = Some(last_row_count.map_or(row_count, |n| n.max(row_count)));
    }

    match last_row_count {
        None => Ok(true),
        Some(last) => Ok(available >= last + retrain_growth),
    }
}

/// Build the dictionary from the samples.
/// It is done by collecting the samples from the tables, and building the dictionary.
/// Using stream approach to avoid loading all the samples into memory.
fn build_dictionary<C>(
    conn: &mut C,
    config: &SetupConfig,
    max_samples: usize,
    dict_capacity: usize,
) -> Result<Vec<u8>, SetupError>
where
    C: SetupConnection,
{
    let mut corpus = Vec::new();
    let mut sizes = Vec::new();

    for table in &config.tables {
        let table_name = table.as_qualified_name();
        for column in &table.columns {
            let column_name = column.quote();
            let sql = format!(
                "SELECT {column_name} FROM {table_name} \
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
fn persist_dictionary<C>(
    conn: &mut C,
    schemas: &[&SchemaName],
    dictionary: &[u8],
    row_count: i64,
) -> Result<DictId, SetupError>
where
    C: SetupConnection,
{
    let mut dict_id = None;

    for schema in schemas {
        let schema_name = schema.as_zstd_schema_name();
        let sql = match dict_id {
            None => format!(
                "INSERT INTO {schema_name} (dict, trained_at, row_count) \
                 VALUES (?1, strftime('%s', 'now'), {row_count}) \
                 RETURNING id"
            ),
            Some(id) => format!(
                "INSERT INTO {schema_name} (id, dict, trained_at, row_count) \
                 VALUES ({id}, ?1, strftime('%s', 'now'), {row_count}) \
                 RETURNING id"
            ),
        };

        let id = conn
            .execute_blob(&sql, dictionary)
            .map_err(SetupError::from_conn)?;

        let id = u32::try_from(id)
            .map(DictId::new)
            .map_err(|_| SetupError::DictTrain("dictionary id overflow".to_string()))?;

        dict_id = Some(id);
    }

    dict_id.ok_or_else(|| SetupError::DictTrain("no schema to persist dictionary".to_string()))
}
