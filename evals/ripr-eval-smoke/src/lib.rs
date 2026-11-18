//! Smoke harness: invokes a `ripr` binary (path from `RIPR_BIN`) as a subprocess.

use std::process::Command;

/// Runs `<bin> --version` and returns trimmed stdout.
pub fn ripr_version(bin: &str) -> Result<String, String> {
    let out = Command::new(bin)
        .arg("--version")
        .output()
        .map_err(|e| format!("failed to spawn {bin}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{bin} --version exited with {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_is_an_error() {
        assert!(ripr_version("ripr-binary-that-does-not-exist").is_err());
    }

    #[test]
    fn real_ripr_reports_version_when_configured() -> Result<(), String> {
        if let Ok(bin) = std::env::var("RIPR_BIN") {
            let v = ripr_version(&bin)?;
            assert!(v.starts_with("ripr"), "unexpected version output: {v}");
        }
        Ok(())
    }
}
