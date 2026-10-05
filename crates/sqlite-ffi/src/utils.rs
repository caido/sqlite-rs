use std::ffi::c_void;

pub(super) type XDestroy = Option<unsafe extern "C" fn(*mut c_void)>;

pub(super) unsafe fn to_sqlite_destroy<T>(p: *mut T) -> (*mut c_void, XDestroy) {
    unsafe extern "C" fn destroy<T>(p: *mut c_void) {
        if !p.is_null() {
            unsafe {
                drop(Box::from_raw(p.cast::<T>()));
            }
        }
    }

    (p.cast(), Some(destroy::<T>))
}

pub(super) unsafe fn ptr_as_ref<'a, T>(ptr: *mut c_void) -> Option<&'a T> {
    unsafe { ptr.cast::<T>().as_ref() }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use crate::utils::ptr_as_ref;

    #[test]
    fn client_as_ref_reads_allocation() {
        let ptr = Box::into_raw(Box::new(42u32)).cast::<c_void>();
        let got = unsafe { ptr_as_ref::<u32>(ptr) }.unwrap();
        assert_eq!(*got, 42);
        unsafe { drop(Box::from_raw(ptr.cast::<u32>())) };
    }
}
