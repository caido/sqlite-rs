use sqlite_ffi::{first_value, Connection, SqlValue, SqliteError};

use crate::DictId;

pub(super) struct Query;

impl Query {
    pub fn insert_or_update_last_rowid<C: AsRef<Connection>>(
        conn: &C,
        prune_table: &str,
        table_name: &str,
        column_name: &str,
        target_dict_id: DictId,
        last_rowid: i64,
    ) -> Result<i64, SqliteError> {
        let conn = conn.as_ref();

        conn.execute(
            &format!(
                "INSERT INTO {prune_table} \
                 (table_name, column_name, target_dict_id, last_rowid) \
                 VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(table_name, column_name) DO UPDATE SET \
                 last_rowid = excluded.last_rowid, \
                 target_dict_id = excluded.target_dict_id"
            ),
            &[
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
                SqlValue::Integer(i64::from(target_dict_id.get())),
                SqlValue::Integer(last_rowid),
            ],
        )
    }

    pub fn select_current_ids<C: AsRef<Connection>>(
        conn: &C,
        dict_table: &str,
    ) -> Result<Vec<Vec<SqlValue>>, SqliteError> {
        let conn = conn.as_ref();

        conn.query(
        &format!("SELECT table_name, column_name, MAX(id) AS id FROM {dict_table} GROUP BY table_name, column_name"),
        &[],
        )
    }

    pub fn select_last_rowid<C: AsRef<Connection>>(
        conn: &C,
        prune_table: &str,
        table_name: &str,
        column_name: &str,
    ) -> Result<i64, SqliteError> {
        let conn = conn.as_ref();

        let rows = conn.query(
            &format!(
                "SELECT last_rowid FROM {prune_table} WHERE table_name = ?1 AND column_name = ?2 "
            ),
            &[
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
            ],
        )?;

        Ok(first_value(&rows).and_then(SqlValue::as_i64).unwrap_or(0))
    }

    pub fn select_rows_to_prune<C: AsRef<Connection>>(
        conn: &C,
        column: &str,
        table: &str,
        rowid: i64,
        limit: i64,
    ) -> Result<Vec<Vec<SqlValue>>, SqliteError> {
        let conn = conn.as_ref();

        conn.query(
            &format!(
                "SELECT {column}, rowid FROM {table} \
                 WHERE rowid > ?1 ORDER BY rowid LIMIT ?2"
            ),
            &[SqlValue::Integer(rowid), SqlValue::Integer(limit)],
        )
    }

    pub fn update_last_rowid<C: AsRef<Connection>>(
        conn: &C,
        table: &str,
        column: &str,
        data: Vec<u8>,
        rowid: i64,
    ) -> Result<i64, SqliteError> {
        let conn = conn.as_ref();

        conn.execute(
            &format!("UPDATE {table} SET {column} = ?1 WHERE rowid = ?2"),
            &[SqlValue::Blob(data), SqlValue::Integer(rowid)],
        )
    }

    pub fn delete_prune_row<C: AsRef<Connection>>(
        conn: &C,
        prune_table: &str,
        table_name: &str,
        column_name: &str,
    ) -> Result<i64, SqliteError> {
        let conn = conn.as_ref();

        conn.execute(
            &format!("DELETE FROM {prune_table} WHERE table_name = ?1 AND column_name = ?2"),
            &[
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
            ],
        )
    }

    pub fn delete_older_dicts<C: AsRef<Connection>>(
        conn: &C,
        dict_table: &str,
        current_id: DictId,
        table_name: &str,
        column_name: &str,
    ) -> Result<i64, SqliteError> {
        let conn = conn.as_ref();

        conn.execute(
            &format!(
                "DELETE FROM {dict_table} WHERE id < ?1 AND table_name = ?2 AND column_name = ?3"
            ),
            &[
                SqlValue::Integer(i64::from(current_id.get())),
                SqlValue::Text(table_name.to_string()),
                SqlValue::Text(column_name.to_string()),
            ],
        )
    }
}
