#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn standalone_cargo_artifact_lifecycle_tests_pass() -> Result<(), String> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let source = repository.join("tools/cargo-artifacts/main.rs");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("clock before Unix epoch: {error}"))?
        .as_nanos();
    let scratch = env::temp_dir().join(format!(
        "ripr-cargo-artifact-selftest-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&scratch)
        .map_err(|error| format!("create {}: {error}", scratch.display()))?;
    let executable = scratch.join(if cfg!(windows) {
        "cargo-artifacts-selftest.exe"
    } else {
        "cargo-artifacts-selftest"
    });
    let compiler = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let compile = Command::new(compiler)
        .args(["--edition=2024", "--test"])
        .arg(&source)
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|error| format!("compile {}: {error}", source.display()))?;
    if !compile.status.success() {
        let _ = fs::remove_dir_all(&scratch);
        return Err(format!(
            "standalone cargo-artifacts test compile failed with {}\nstdout:\n{}\nstderr:\n{}",
            compile.status,
            String::from_utf8_lossy(&compile.stdout),
            String::from_utf8_lossy(&compile.stderr)
        ));
    }

    let tests = Command::new(&executable)
        .arg("--test-threads=1")
        .output()
        .map_err(|error| format!("run {}: {error}", executable.display()))?;
    let cleanup = fs::remove_dir_all(&scratch);
    if !tests.status.success() {
        return Err(format!(
            "standalone cargo-artifacts tests failed with {}\nstdout:\n{}\nstderr:\n{}",
            tests.status,
            String::from_utf8_lossy(&tests.stdout),
            String::from_utf8_lossy(&tests.stderr)
        ));
    }
    cleanup.map_err(|error| format!("remove {}: {error}", scratch.display()))?;
    Ok(())
}
