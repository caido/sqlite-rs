use std::{path::PathBuf, process::Command};

fn main() {
    let output = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .expect("xcrun must be available to locate the macOS SDK");

    assert!(
        output.status.success(),
        "xcrun could not locate the macOS SDK"
    );

    let sdk_path = PathBuf::from(
        String::from_utf8(output.stdout)
            .expect("SDK path is UTF-8")
            .trim(),
    );

    cc::Build::new()
        .file("src/clientdata.c")
        .include(sdk_path.join("usr/include"))
        .compile("sqlite_clientdata_shim");

    println!("cargo:rerun-if-changed=src/clientdata.c");
}
