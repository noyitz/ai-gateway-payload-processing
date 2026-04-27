fn main() {
    let lib_dir = std::env::var("GO_PLUGINS_LIB_DIR")
        .unwrap_or_else(|_| "/usr/local/lib".to_string());
    println!("cargo:rustc-link-search=native={}", lib_dir);
    println!("cargo:rustc-link-lib=dylib=go_plugins_ffi");
    println!("cargo:rerun-if-changed={}/libgo_plugins_ffi.so", lib_dir);
    println!("cargo:rerun-if-changed={}/libgo_plugins_ffi.dylib", lib_dir);
}
