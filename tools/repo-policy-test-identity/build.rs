fn main() -> Result<(), String> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or("missing policy identity macro manifest directory")?;
    let root = std::path::Path::new(&manifest).join("../..");
    // Full resource custody includes additions, ignored files and relocation.
    // This watcher belongs only to test consumers, not the normal policy crate.
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "tools/repo-policy",
        "tools/repo-policy-contracts",
        "tools/repo-policy-test-identity",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    Ok(())
}
