use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn sqlite_entrypoint(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    let original_name = &input.sig.ident;

    let name = if attr.is_empty() {
        format_ident!("sqlite3_entrypoint_init")
    } else {
        parse_macro_input!(attr as Ident)
    };

    quote! {
        #input

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn #name(
            db: *mut ::libsqlite3_sys::sqlite3,
            _error: *mut *mut ::core::ffi::c_char,
            api: *mut ::libsqlite3_sys::sqlite3_api_routines,
        ) -> ::core::ffi::c_int {

            let database = ::sqlite_ffi::Connection::from_raw(db);

            #[cfg(feature = "loadable_extension")]
            if let Err(::sqlite_ffi::SqliteError::Sqlite { code, .. }) =
                ::sqlite_ffi::Connection::init_extension(api)
            {
                return code;
            }

            #original_name(database);
            ::libsqlite3_sys::SQLITE_OK
        }
    }
    .into()
}
