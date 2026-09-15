use libsqlite3_sys::sqlite3_libversion;
use std::ffi::CStr;

pub fn lib_version() -> &'static str {
    let version = unsafe { lib_version_raw() };
    version
        .to_str()
        .expect("SQLITE version is not a valid string")
}

unsafe fn lib_version_raw() -> &'static CStr {
    unsafe { CStr::from_ptr(sqlite3_libversion()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lib_version_is_sqlite3() {
        let v = lib_version();
        assert_ne!(v, "")
    }
}
