#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct ScratchDirectory {
    path: PathBuf,
}

impl ScratchDirectory {
    fn create(path: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&path)
            .map_err(|error| format!("create {}: {error}", path.display()))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn standalone_cargo_artifact_lifecycle_tests_pass() -> Result<(), String> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let source = repository.join("tools/cargo-artifacts/main.rs");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("clock before Unix epoch: {error}"))?
        .as_nanos();
    let scratch = ScratchDirectory::create(env::temp_dir().join(format!(
        "ripr-cargo-artifact-selftest-{}-{nonce}",
        std::process::id()
    )))?;
    let executable = scratch.path().join(if cfg!(windows) {
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
    if !tests.status.success() {
        return Err(format!(
            "standalone cargo-artifacts tests failed with {}\nstdout:\n{}\nstderr:\n{}",
            tests.status,
            String::from_utf8_lossy(&tests.stdout),
            String::from_utf8_lossy(&tests.stderr)
        ));
    }
    Ok(())
}
