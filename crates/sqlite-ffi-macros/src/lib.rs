use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn sqlite_entrypoint(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let original_name = &input.sig.ident;
    let impl_mod = format_ident!("__impl_{}", original_name);

    quote! {
        mod #impl_mod {
            use super::*;

            #input
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #original_name(
            db: *mut ::libsqlite3_sys::sqlite3,
            error: *mut *mut ::core::ffi::c_char,
            api: *mut ::libsqlite3_sys::sqlite3_api_routines,
        ) -> ::core::ffi::c_int {
            let database = ::sqlite_ffi::Connection::from_raw(db);

            #[cfg(feature = "loadable_extension")]
            if let Err(::sqlite_ffi::SqliteError::Sqlite { code, .. }) =
                ::sqlite_ffi::Connection::init_extension(api)
            {
                return code;
            }

            match #impl_mod::#original_name(database) {
                Ok(()) => ::libsqlite3_sys::SQLITE_OK,
                Err(e) => unsafe { e.report(error) },
            }
        }
    }
    .into()
}
