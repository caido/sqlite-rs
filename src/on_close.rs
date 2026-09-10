use std::{
    ffi::{c_void, CString},
    panic::{catch_unwind, AssertUnwindSafe},
    str::FromStr,
    sync::atomic::{AtomicU64, Ordering},
};

use sqlite_loadable::{
    api, ext::sqlite3ext_create_function_v2, prelude::*, Error, ErrorKind, FunctionFlags, Result,
    SQLITE_OK,
};

static NEXT_ON_CLOSE_ID: AtomicU64 = AtomicU64::new(1);

struct OnClose<F> {
    db: *mut sqlite3,
    callback: Option<F>,
}

unsafe extern "C" fn on_close_destroy<F>(p_app: *mut c_void)
where
    F: FnOnce(*mut sqlite3) + Send + 'static,
{
    let mut state = unsafe { Box::from_raw(p_app.cast::<OnClose<F>>()) };

    let callback = state
        .callback
        .take()
        .expect("SQLite must invoke an on-close callback once");

    let _ = catch_unwind(AssertUnwindSafe(|| callback(state.db)));
}

unsafe extern "C" fn on_close_sentinel(
    context: *mut sqlite3_context,
    _argc: i32,
    _argv: *mut *mut sqlite3_value,
) {
    api::result_null(context);
}

pub(crate) fn on_close<F>(db: *mut sqlite3, callback: F) -> Result<()>
where
    F: FnOnce(*mut sqlite3) + Send + 'static,
{
    let state = Box::new(OnClose {
        db,
        callback: Some(callback),
    });

    let id = NEXT_ON_CLOSE_ID.fetch_add(1, Ordering::Relaxed);
    let function_name = CString::from_str(&format!("__sqlite_compress_on_close_{id}"))?;

    let rc = unsafe {
        sqlite3ext_create_function_v2(
            db,
            function_name.as_ptr().cast(),
            0,
            FunctionFlags::UTF8.bits(),
            Box::into_raw(state).cast(),
            Some(on_close_sentinel),
            None,
            None,
            Some(on_close_destroy::<F>),
        )
    };

    if rc == SQLITE_OK {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::DefineScalarFunction(rc)))
    }
}

#[cfg(test)]
mod integration_close {
    use std::sync::Once;

    use rusqlite::{ffi::sqlite3_auto_extension, Connection};

    use crate::{
        cache::{self, insert_into_caches, test_has_cache, CacheKeySource, DbKey},
        dict::{ColumnKey, DictId},
        sqlite3_compress_init, DEFAULT_LEVEL,
    };

    static INIT: Once = Once::new();

    fn register_extension() {
        INIT.call_once(|| unsafe {
            #[allow(clippy::missing_transmute_annotations)]
            sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite3_compress_init as *const (),
            )));
        });
    }
    struct HandleKey(DbKey);

    impl CacheKeySource for HandleKey {
        fn db_key(&self) -> DbKey {
            self.0
        }
    }

    #[test]
    fn close_clears_handle_map_and_registry() {
        register_extension();

        let column = ColumnKey::new("main", "t", "c");
        let dict = b"not-a-real-zstd-dict-but-enough-for-cache-insert";
        let dict_id = DictId::new(1);

        let (addr, key) = {
            let conn = Connection::open_in_memory().unwrap();
            let handle = unsafe { conn.handle() };
            let addr = handle as usize;
            let key = DbKey::for_handle(handle.cast());
            assert!(DbKey::test_is_mapped(addr));
            insert_into_caches(&HandleKey(key), &column, dict_id, dict, DEFAULT_LEVEL);
            assert!(test_has_cache(key));
            let _: i32 = conn
                .query_row(
                    "SELECT typeof(compress(x'00', 'main', 't', 'c')) IS NOT NULL",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(1);
            (addr, key)
        };

        assert!(
            !DbKey::test_is_mapped(addr),
            "handle address must be unmapped on close"
        );

        assert!(
            !test_has_cache(key),
            "registry cache must be dropped on close"
        );
    }

    #[test]
    fn for_handle_after_remove_allocates_fresh_key() {
        let addr = 0xDEAD_BEEF_usize;
        let key_a = DbKey::for_handle(addr as *mut _);

        assert!(DbKey::remove_handle_if_matches(addr, key_a));
        cache::remove_cache(key_a);
        let key_b = DbKey::for_handle(addr as *mut _);
        assert_ne!(key_a, key_b);
        assert!(!test_has_cache(key_a));
    }
}
