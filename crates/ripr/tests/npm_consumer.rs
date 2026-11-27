//! Clean npm install-to-use TypeScript journey for issue #4627.
//!
//! Runs the dependency-free Node controls in `packaging/npm/qualification/`
//! with the candidate `ripr` binary staged as the native npm payload. The live
//! journey installs the launcher and native tarballs from an isolated loopback
//! registry with scripts disabled, analyzes a TypeScript subject through a
//! project-local npm script, follows the explain continuation from a foreign
//! CWD with the registry torn down, and runs the missing/wrong optional
//! package, PATH-decoy, reinstall and uninstall controls.
//!
//! The release rehearsal runs the same journey against the qualified
//! release tarballs per target (#4632); this test is not that qualification.

#![cfg(all(unix, feature = "lang-typescript"))]

use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn npm_registry_journey_installs_the_candidate_payload_and_fails_closed() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_ripr"));
    let output = Command::new("node")
        .arg("--test")
        .arg("--test-reporter=tap")
        .arg(root.join("packaging/npm/qualification/test/journey.test.cjs"))
        .current_dir(&root)
        .env("RIPR_PACKAGE_TEST_BINARY", &binary)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("Node must be available for the npm journey controls: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(format!(
            "npm journey controls failed ({}):\n{stdout}{stderr}",
            output.status
        ));
    }
    let count = |prefix: &str| {
        stdout
            .lines()
            .find_map(|line| line.strip_prefix(prefix)?.trim().parse::<usize>().ok())
    };
    // Every control, including the live journey, must execute on a supported
    // launcher platform; a skip would let the row pass with nothing proved.
    let supported = cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "gnu"
    )) || cfg!(all(
        target_os = "linux",
        target_arch = "aarch64",
        target_env = "gnu"
    )) || cfg!(target_os = "macos");
    if count("# tests ").is_none_or(|value| value < 7)
        || count("# pass ").is_none_or(|value| value == 0)
        || !stdout.lines().any(|line| line == "# fail 0")
        || (supported && count("# skipped ") != Some(0))
        || (supported
            && !stdout
                .lines()
                .any(|line| line.starts_with("ok ") && line.contains("live journey")))
    {
        return Err(format!(
            "npm journey controls did not report the full executed set:\n{stdout}{stderr}"
        ));
    }
    Ok(())
}
