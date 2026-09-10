use std::env;

fn main() {
    // `libsqlite3-sys` builds its bundled SQLite for the current target and
    // publishes the directory containing sqlite3.h / sqlite3ext.h through
    // Cargo's `links = "sqlite3"` metadata.
    let sqlite_include =
        env::var("DEP_SQLITE3_INCLUDE").expect("libsqlite3-sys must provide DEP_SQLITE3_INCLUDE");

    cc::Build::new()
        .file("src/clientdata.c")
        .include(sqlite_include)
        .compile("sqlite_clientdata_shim");

    println!("cargo:rerun-if-changed=src/clientdata.c");
}
