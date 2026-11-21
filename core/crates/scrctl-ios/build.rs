fn main() {
    println!("cargo:rerun-if-env-changed=XDB_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_XDB").is_some() {
        if let Some(path) = std::env::var_os("XDB_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", path.to_string_lossy());
        }
        println!("cargo:rustc-link-lib=dylib=xdb_client");
    }
}
