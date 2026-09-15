use std::ffi::CString;

use libsqlite3_sys::{SQLITE_DONE, SQLITE_OK, SQLITE_ROW, sqlite3, sqlite3_column_value, sqlite3_finalize, sqlite3_prepare_v2, sqlite3_step, sqlite3_value_blob, sqlite3_value_bytes};

use crate::SqliteError;

pub fn query_blobs(db: *mut sqlite3, sql: &str) -> Result<Vec<Vec<u8>>, SqliteError> {
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

    let mut out = Vec::new();

    unsafe {
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
                    sqlite3_finalize(stmt);
                    return Err(SqliteError::Sqlite {
                        code,
                        message: "step failed".into(),
                    });
                }
            }
        }
        sqlite3_finalize(stmt);
    }
    Ok(out)
}
