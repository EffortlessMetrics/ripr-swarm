#[path = "source_identity.rs"]
mod source_identity;
fn main() -> Result<(), String> {
    // Cargo can reuse this compiled script in another worktree. Its synthesized
    // manifest environment must be read when the script runs, not baked in.
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| "policy build environment is missing CARGO_MANIFEST_DIR".to_string())?;
    let root = std::path::PathBuf::from(manifest_dir).join("../..");
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "tools/repo-policy",
    ] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
    let stamp = source_identity::source_identity(&root)?;
    println!("cargo:rustc-env=REPO_POLICY_SOURCE_ID={stamp}");
    let compiler = std::env::var("RUSTC").map_err(|e| e.to_string())?;
    let output = std::process::Command::new(compiler)
        .arg("-vV")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("policy compiler identity failed".into());
    }
    let compiler = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    println!(
        "cargo:rustc-env=REPO_POLICY_COMPILER_ID={}",
        compiler.replace('\n', "|")
    );
    Ok(())
}
