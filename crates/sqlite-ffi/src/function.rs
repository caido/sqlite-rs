use std::{
    ffi::{CStr, CString, c_char, c_int, c_void},
    slice,
};

use libsqlite3_sys::{
    SQLITE_OK, SQLITE_UTF8, SQLITE_UTF16BE, sqlite3_context, sqlite3_create_function_v2,
    sqlite3_errmsg, sqlite3_user_data, sqlite3_value,
};

use crate::{Context, Database, Value, error::SqliteError};

pub enum TextRep {
    UTF16,
    UTF8,
}

impl TextRep {
    fn as_sqlite(&self) -> i32 {
        match self {
            Self::UTF16 => SQLITE_UTF16BE,
            Self::UTF8 => SQLITE_UTF8,
        }
    }
}

type XFunc = Option<
    unsafe extern "C" fn(arg1: *mut sqlite3_context, arg2: c_int, arg3: *mut *mut sqlite3_value),
>;

type XDestroy = Option<unsafe extern "C" fn(*mut c_void)>;

pub trait ScalarFunction: Fn(Context, &[Value]) + Send + 'static {}

impl<F> ScalarFunction for F where F: Fn(Context, &[Value]) + Send + 'static {}

impl Database {
    pub fn create_function_v2<F: ScalarFunction>(
        &self,
        name: &str,
        arg_number: i32,
        text_representation: TextRep,
        func: F,
    ) -> Result<(), SqliteError> {
        let c_name = CString::new(name)?;
        let (p_app, x_func, x_destroy) = unsafe { to_sqlite_func(func) };

        let rc = unsafe {
            create_function_v2_raw(
                self.conn.as_ptr(),
                c_name.as_ptr(),
                arg_number,
                text_representation.as_sqlite(),
                p_app,
                x_func,
                x_destroy,
            )
        };

        if rc != SQLITE_OK {
            let message = unsafe {
                CStr::from_ptr(sqlite3_errmsg(self.conn.as_ptr()))
                    .to_string_lossy()
                    .into_owned()
            };
            return Err(SqliteError::Sqlite {
                operation: "create function v2",
                code: rc,
                message,
            });
        }

        Ok(())
    }
}

unsafe fn to_sqlite_func<F: ScalarFunction>(func: F) -> (*mut c_void, XFunc, XDestroy) {
    let boxed: *mut F = Box::into_raw(Box::new(func));

    unsafe extern "C" fn transform<F: ScalarFunction>(
        ctx: *mut sqlite3_context,
        argc: c_int,
        argv: *mut *mut sqlite3_value,
    ) {
        unsafe {
            let f = sqlite3_user_data(ctx).cast::<F>();

            let raw_values = slice::from_raw_parts(argv, argc as usize);
            let values: Vec<Value> = raw_values.iter().copied().map(Value::from_raw).collect();

            (*f)(Context::from_raw(ctx), &values);
        }
    }

    unsafe extern "C" fn destroy<F>(p: *mut c_void) {
        unsafe {
            drop(Box::from_raw(p.cast::<F>()));
        }
    }

    (boxed.cast(), Some(transform::<F>), Some(destroy::<F>))
}

unsafe fn create_function_v2_raw(
    db: *mut libsqlite3_sys::sqlite3,
    function_name: *const c_char,
    arg_number: c_int,
    text_rep: c_int,
    p_app: *mut c_void,
    x_func: XFunc,
    x_destroy: Option<unsafe extern "C" fn(arg1: *mut ::core::ffi::c_void)>,
) -> i32 {
    unsafe {
        sqlite3_create_function_v2(
            db,
            function_name,
            arg_number,
            text_rep,
            p_app,
            x_func,
            None,
            None,
            x_destroy,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::ptr;

    use libsqlite3_sys::{
        SQLITE_ROW, sqlite3_close, sqlite3_column_int, sqlite3_finalize, sqlite3_open,
        sqlite3_prepare_v2, sqlite3_result_int64, sqlite3_step,
    };

    use super::*;

    #[test]
    fn registers_and_calls_scalar() {
        let mut db = ptr::null_mut();
        let rc = unsafe { sqlite3_open(c":memory:".as_ptr(), &mut db) };
        assert_eq!(rc, SQLITE_OK);

        let db = Database::from_raw(db);

        db.create_function_v2("add_one", 1, TextRep::UTF8, |context, args| {
            let n = args[0].to_i64();
            unsafe { sqlite3_result_int64(context.ctx.as_ptr(), n + 1) };
        })
        .unwrap();

        let mut stmt = ptr::null_mut();
        let sql = c"SELECT add_one(41)";
        unsafe {
            assert_eq!(
                sqlite3_prepare_v2(
                    db.conn.as_ptr(),
                    sql.as_ptr(),
                    -1,
                    &mut stmt,
                    ptr::null_mut()
                ),
                SQLITE_OK
            );
            assert_eq!(sqlite3_step(stmt), SQLITE_ROW);
            assert_eq!(sqlite3_column_int(stmt, 0), 42);
            sqlite3_finalize(stmt);
            sqlite3_close(db.conn.as_ptr());
        }
    }
}
