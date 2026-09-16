use std::ffi::CString;

use libsqlite3_sys::{
    SQLITE_DONE, SQLITE_OK, SQLITE_ROW, SQLITE_TRANSIENT, sqlite3, sqlite3_bind_blob,
    sqlite3_column_bytes, sqlite3_column_int64, sqlite3_column_text, sqlite3_column_value,
    sqlite3_exec, sqlite3_finalize, sqlite3_prepare_v2, sqlite3_step, sqlite3_stmt,
    sqlite3_value_blob, sqlite3_value_bytes,
};

use crate::{Database, SqliteError};

impl Database {
    pub fn query_strings(&self, sql: &str) -> Result<Vec<String>, SqliteError> {
        with_stmt(self.conn.as_ptr(), sql, |stmt| unsafe {
            let mut out = Vec::new();

            loop {
                match sqlite3_step(stmt) {
                    SQLITE_ROW => {
                        let ptr = sqlite3_column_text(stmt, 0);
                        let len = sqlite3_column_bytes(stmt, 0) as usize;
                        if ptr.is_null() {
                            out.push(String::new());
                        } else {
                            let bytes = std::slice::from_raw_parts(ptr, len);
                            out.push(std::str::from_utf8(bytes)?.to_owned());
                        }
                    }
                    SQLITE_DONE => break,
                    code => {
                        return Err(SqliteError::Sqlite {
                            code,
                            message: "step failed".into(),
                        });
                    }
                }
            }
            Ok(out)
        })
    }

    pub fn query_blobs(&self, sql: &str) -> Result<Vec<Vec<u8>>, SqliteError> {
        with_stmt(self.conn.as_ptr(), sql, |stmt| unsafe {
            let mut out = Vec::new();

            loop {
                match sqlite3_step(stmt) {
                    SQLITE_ROW => {
                        let value = sqlite3_column_value(stmt, 0);
                        let len = sqlite3_value_bytes(value) as usize;
                        let ptr = sqlite3_value_blob(value) as *const u8;
                        if ptr.is_null() {
                            out.push(Vec::new());
                        } else {
                            out.push(std::slice::from_raw_parts(ptr, len).to_vec());
                        }
                    }
                    SQLITE_DONE => break,
                    code => {
                        return Err(SqliteError::Sqlite {
                            code,
                            message: "step failed".into(),
                        });
                    }
                }
            }
            Ok(out)
        })
    }

    pub fn query_i64(&self, sql: &str) -> Result<i64, SqliteError> {
        with_stmt(self.conn.as_ptr(), sql, |stmt| unsafe {
            match sqlite3_step(stmt) {
                SQLITE_ROW => {
                    let value = sqlite3_column_int64(stmt, 0);
                    Ok(value)
                }
                SQLITE_DONE => Ok(0), //add special msg;
                code => Err(SqliteError::Sqlite {
                    code,
                    message: "step failed".into(),
                }),
            }
        })
    }

    pub fn batch_execute(&self, sql: &str) -> i32 {
        let c_sql = CString::new(sql);

        unsafe {
            sqlite3_exec(
                self.conn.as_ptr(),
                c_sql.unwrap().as_ptr(),
                None,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        }
    }

    pub fn execute_blob(&self, sql: &str, blob: &[u8]) -> Result<i64, SqliteError> {
        with_stmt(self.conn.as_ptr(), sql, |stmt| unsafe {
            let rc = sqlite3_bind_blob(
                stmt,
                1, // ?1
                blob.as_ptr().cast(),
                blob.len() as i32,
                SQLITE_TRANSIENT(), // SQLite copie le blob
            );
            if rc != SQLITE_OK {
                return Err(SqliteError::Sqlite {
                    code: rc,
                    message: "bind failed".into(),
                });
            }

            match sqlite3_step(stmt) {
                SQLITE_ROW => Ok(sqlite3_column_int64(stmt, 0)),
                code => Err(SqliteError::Sqlite {
                    code,
                    message: "step failed".into(),
                }),
            }
        })
    }

    pub fn execute_blobs(&self, sql: &str, blobs: &[&[u8]]) -> Result<(), SqliteError> {
        with_stmt(self.conn.as_ptr(), sql, |stmt| unsafe {
            for (index, blob) in blobs.iter().enumerate() {
                let rc = sqlite3_bind_blob(
                    stmt,
                    (index + 1) as i32,
                    blob.as_ptr().cast(),
                    blob.len() as i32,
                    SQLITE_TRANSIENT(),
                );

                if rc != SQLITE_OK {
                    return Err(SqliteError::Sqlite {
                        code: rc,
                        message: "bind failed".into(),
                    });
                }
            }

            match sqlite3_step(stmt) {
                SQLITE_DONE => Ok(()),
                code => Err(SqliteError::Sqlite {
                    code,
                    message: "step failed".into(),
                }),
            }
        })
    }
}

fn with_stmt<T>(
    db: *mut sqlite3,
    sql: &str,
    f: impl FnOnce(*mut sqlite3_stmt) -> Result<T, SqliteError>,
) -> Result<T, SqliteError> {
    let mut stmt = std::ptr::null_mut();
    let c_sql = CString::new(sql);

    let rc = unsafe {
        sqlite3_prepare_v2(
            db,
            c_sql.unwrap().as_ptr(),
            -1,
            &mut stmt,
            std::ptr::null_mut(),
        )
    };

    if rc != SQLITE_OK {
        return Err(SqliteError::Sqlite {
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
