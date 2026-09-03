use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        LazyLock,
    },
};

use parking_lot::Mutex;

static NEXT_DB_ID: AtomicU64 = AtomicU64::new(1);
static HANDLE_KEYS: LazyLock<Mutex<HashMap<usize, DbKey>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DbKey(u64);

impl DbKey {
    pub fn new() -> Self {
        Self(NEXT_DB_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn for_handle(ptr: *mut std::ffi::c_void) -> Self {
        let addr = ptr as usize;
        *HANDLE_KEYS.lock().entry(addr).or_default()
    }
}

impl Default for DbKey {
    fn default() -> Self {
        Self::new()
    }
}
