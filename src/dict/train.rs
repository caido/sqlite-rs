use crate::dict::insert_into_caches;
use crate::dict::types::DictId;
use crate::setup::{SchemaName, SetupConfig, SetupConnection, SetupError, SqlIdent};
use std::collections::HashSet;

const RETRAIN_GROWTH: usize = 5000;

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

    if !retrain_required(conn, &schemas, available)? {
        return Err(SetupError::DictTrain(format!(
            "retrain skipped: need at least {RETRAIN_GROWTH} new samples since last train"
        )));
    }

    let (samples, schemas) = collect_samples(conn, config, max_samples)?;
    let dictionary = build_dictionary(&samples, dict_capacity)?;
    let dict_id = next_dict_id(conn, &schemas)?;

    persist_dictionary(conn, &schemas, dict_id, &dictionary, available)?;
    insert_into_caches(dict_id, &dictionary, config.compression_level);
    Ok(dict_id)
}

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

fn retrain_required<C>(
    conn: &mut C,
    schemas: &[&SchemaName],
    available: i64,
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
        Some(last) => Ok(available >= last + RETRAIN_GROWTH as i64),
    }
}

fn collect_samples<'a, C>(
    conn: &mut C,
    config: &'a SetupConfig,
    max_samples: usize,
) -> Result<(Vec<Vec<u8>>, Vec<&'a SchemaName>), SetupError>
where
    C: SetupConnection,
{
    let mut samples = Vec::new();
    let mut schemas = Vec::new();
    let mut schemas_seen = HashSet::new();

    for table in &config.tables {
        if schemas_seen.insert(&table.schema) {
            schemas.push(&table.schema);
        }

        let table_name = table.as_qualified_name();

        for column in &table.columns {
            let column_name = column.quote();
            let mut column_samples = conn
                .query_blobs(&format!(
                    "SELECT {column_name} FROM {table_name} \
                     WHERE {column_name} IS NOT NULL AND length({column_name}) > 0 \
                     ORDER BY rowid DESC \
                     LIMIT {max_samples}"
                ))
                .map_err(SetupError::from_conn)?;

            samples.append(&mut column_samples);
        }
    }

    if samples.is_empty() {
        return Err(SetupError::DictTrain(
            "no non-empty samples available".to_string(),
        ));
    }

    Ok((samples, schemas))
}

fn build_dictionary(samples: &[Vec<u8>], dict_capacity: usize) -> Result<Vec<u8>, SetupError> {
    let sample_refs: Vec<&[u8]> = samples.iter().map(Vec::as_slice).collect();
    zstd::dict::from_samples(&sample_refs, dict_capacity)
        .map_err(|error| SetupError::DictTrain(error.to_string()))
}

fn next_dict_id<C>(conn: &mut C, schemas: &[&SchemaName]) -> Result<DictId, SetupError>
where
    C: SetupConnection,
{
    let mut latest_id = 0_i64;
    for schema in schemas {
        let schema_name = schema.as_zstd_schema_name();
        let id = conn
            .query_i64(&format!("SELECT COALESCE(MAX(id), 0) FROM {schema_name}"))
            .map_err(SetupError::from_conn)?;
        latest_id = latest_id.max(id);
    }

    latest_id
        .checked_add(1)
        .and_then(|id| u32::try_from(id).ok())
        .map(DictId::new)
        .ok_or_else(|| SetupError::DictTrain("dictionary id overflow".to_string()))
}

fn persist_dictionary<C>(
    conn: &mut C,
    schemas: &[&SchemaName],
    dict_id: DictId,
    dictionary: &[u8],
    row_count: i64,
) -> Result<(), SetupError>
where
    C: SetupConnection,
{
    for schema in schemas {
        let schema_name = schema.as_zstd_schema_name();
        conn.execute_blob(
            &format!(
                "INSERT INTO {schema_name} \
                 (id, dict, trained_at, row_count) \
                 VALUES ({dict_id}, ?1, strftime('%s', 'now'), {row_count})"
            ),
            dictionary,
        )
        .map_err(SetupError::from_conn)?;
    }
    Ok(())
}
