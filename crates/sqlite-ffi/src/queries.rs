use std::ffi::CString;

use libsqlite3_sys::{
    SQLITE_BLOB, SQLITE_DONE, SQLITE_FLOAT, SQLITE_INTEGER, SQLITE_NULL, SQLITE_OK, SQLITE_ROW,
    SQLITE_TEXT, SQLITE_TRANSIENT, sqlite3, sqlite3_bind_blob, sqlite3_bind_double,
    sqlite3_bind_int64, sqlite3_bind_null, sqlite3_bind_text, sqlite3_column_blob,
    sqlite3_column_bytes, sqlite3_column_count, sqlite3_column_double, sqlite3_column_int64,
    sqlite3_column_text, sqlite3_column_type, sqlite3_exec, sqlite3_finalize, sqlite3_prepare_v2,
    sqlite3_step, sqlite3_stmt,
};

use crate::{Connection, SqliteError};

impl Connection {
    pub fn query(&self, sql: &str, params: &[SqlValue]) -> Result<Vec<Vec<SqlValue>>, SqliteError> {
        with_stmt(self.conn.as_ptr(), sql, |stmt| unsafe {
            bind_over_values(stmt, params)?;

            let mut out = Vec::new();

            loop {
                match sqlite3_step(stmt) {
                    SQLITE_ROW => {
                        let count = sqlite3_column_count(stmt);
                        let mut res = Vec::with_capacity(count as usize);

                        for col in 0..count {
                            res.push(read_column(stmt, col)?);
                        }

                        out.push(res);
                    }
                    SQLITE_DONE => break,
                    code => {
                        return Err(SqliteError::Sqlite {
                            operation: "query strings",
                            code,
                            message: "step failed".into(),
                        });
                    }
                }
            }

            Ok(out)
        })
    }

    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<i64, SqliteError> {
        with_stmt(self.conn.as_ptr(), sql, |stmt| unsafe {
            bind_over_values(stmt, params)?;

            let mut id = None;
            loop {
                match sqlite3_step(stmt) {
                    SQLITE_ROW => {
                        if id.is_none() {
                            id = Some(sqlite3_column_int64(stmt, 0));
                        }
                    }
                    SQLITE_DONE => break,
                    code => {
                        return Err(SqliteError::Sqlite {
                            operation: "execute blob",
                            code,
                            message: "step failed".into(),
                        });
                    }
                }
            }
            Ok(id.unwrap_or(0))
        })
    }

    pub fn batch_execute(&self, sql: &str) -> Result<i32, SqliteError> {
        let c_sql = CString::new(sql)?;

        unsafe {
            let rc = sqlite3_exec(
                self.conn.as_ptr(),
                c_sql.as_ptr(),
                None,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );

            if rc != SQLITE_OK {
                return Err(SqliteError::Sqlite {
                    operation: "batch execute",
                    code: rc,
                    message: "exec failed".into(),
                });
            }

            Ok(rc)
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Float(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqlValue {
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Self::Blob(b) => Some(b),
            _ => None,
        }
    }
}

pub fn sample_bytes(v: &SqlValue) -> Option<Vec<u8>> {
    match v {
        SqlValue::Blob(b) if !b.is_empty() => Some(b.clone()),
        SqlValue::Text(s) if !s.is_empty() => Some(s.as_bytes().to_vec()),
        _ => None,
    }
}

pub fn first_value(rows: &[Vec<SqlValue>]) -> Option<&SqlValue> {
    rows.first().and_then(|row| row.first())
}

pub fn column_texts(rows: Vec<Vec<SqlValue>>) -> Vec<String> {
    rows.into_iter()
        .filter_map(|row| row.into_iter().next())
        .filter_map(|v| match v {
            SqlValue::Text(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn bind_over_values(stmt: *mut sqlite3_stmt, params: &[SqlValue]) -> Result<(), SqliteError> {
    unsafe {
        for (i, p) in params.iter().enumerate() {
            let rc = match p {
                SqlValue::Null => sqlite3_bind_null(stmt, i as i32 + 1),
                SqlValue::Integer(n) => sqlite3_bind_int64(stmt, i as i32 + 1, *n),
                SqlValue::Text(s) => {
                    let c_str = CString::new(s.as_str())?;
                    sqlite3_bind_text(
                        stmt,
                        i as i32 + 1,
                        c_str.as_ptr(),
                        c_str.as_bytes().len() as i32,
                        SQLITE_TRANSIENT(),
                    )
                }
                SqlValue::Blob(b) => sqlite3_bind_blob(
                    stmt,
                    i as i32 + 1,
                    b.as_ptr().cast(),
                    b.len() as i32,
                    SQLITE_TRANSIENT(),
                ),
                SqlValue::Float(f) => sqlite3_bind_double(stmt, i as i32 + 1, *f),
            };

            if rc != SQLITE_OK {
                return Err(SqliteError::Sqlite {
                    operation: "query",
                    code: rc,
                    message: "bind failed".into(),
                });
            }
        }
    }

    Ok(())
}

fn read_column(stmt: *mut sqlite3_stmt, index: i32) -> Result<SqlValue, SqliteError> {
    let rc: i32 = unsafe { sqlite3_column_type(stmt, index) };

    let res = match rc {
        SQLITE_NULL => SqlValue::Null,
        SQLITE_FLOAT => unsafe {
            let rc = sqlite3_column_double(stmt, index);
            SqlValue::Float(rc)
        },
        SQLITE_INTEGER => unsafe {
            let rc = sqlite3_column_int64(stmt, index);
            SqlValue::Integer(rc)
        },
        SQLITE_TEXT => unsafe {
            let ptr = sqlite3_column_text(stmt, index);
            let len = sqlite3_column_bytes(stmt, index) as usize;
            if ptr.is_null() {
                SqlValue::Text(String::new())
            } else {
                let bytes = std::slice::from_raw_parts(ptr, len);
                SqlValue::Text(std::str::from_utf8(bytes)?.to_owned())
            }
        },
        SQLITE_BLOB => unsafe {
            let ptr = sqlite3_column_blob(stmt, index);
            let len = sqlite3_column_bytes(stmt, index) as usize;
            if ptr.is_null() {
                SqlValue::Blob(Vec::new())
            } else {
                SqlValue::Blob(std::slice::from_raw_parts(ptr.cast::<u8>(), len).to_vec())
            }
        },
        code => {
            return Err(SqliteError::Sqlite {
                operation: "read column",
                code,
                message: "unknown column type".into(),
            });
        }
    };

    Ok(res)
}

fn with_stmt<T>(
    db: *mut sqlite3,
    sql: &str,
    f: impl FnOnce(*mut sqlite3_stmt) -> Result<T, SqliteError>,
) -> Result<T, SqliteError> {
    let mut stmt = std::ptr::null_mut();
    let c_sql = CString::new(sql)?;

    let rc = unsafe { sqlite3_prepare_v2(db, c_sql.as_ptr(), -1, &mut stmt, std::ptr::null_mut()) };

    if rc != SQLITE_OK {
        return Err(SqliteError::Sqlite {
            operation: "prepare statement",
            code: rc,
            message: "prepare failed".into(),
        });
    }

    let res = f(stmt);
    unsafe {
        sqlite3_finalize(stmt);
    }

    res
}
