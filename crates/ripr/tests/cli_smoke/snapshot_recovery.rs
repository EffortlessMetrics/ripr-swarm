//! Recovery redirects must use the requested native snapshot filename.
use super::{assert_success, init_producer_fixture_repo, spawn_command, unique_temp_workspace};
use std::path::PathBuf;

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        super::ignore_remove_dir_all(&self.0);
    }
}

#[test]
fn missing_snapshot_recovery_preserves_native_redirect_target() -> TestResult {
    let root = unique_temp_workspace("snapshot redirect identity");
    std::fs::create_dir_all(std::env::temp_dir())?;
    std::fs::create_dir(&root)?;
    let fixture = Fixture(root);
    init_producer_fixture_repo(&fixture.0)?;
    std::fs::create_dir(fixture.0.join("captures"))?;
    std::fs::write(fixture.0.join("mutants.json"), "[]")?;
    let snapshot = spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&fixture.0),
        &["check", "--format", "repo-exposure-json"],
        &[],
        None,
        Some(b""),
    )?;
    assert_success(&snapshot);
    let report: serde_json::Value = serde_json::from_slice(&snapshot.stdout)?;
    assert!(
        report["seams"]
            .as_array()
            .is_some_and(|seams| !seams.is_empty()),
        "fixture must produce a nonempty repository snapshot: {report}"
    );
    std::fs::write(fixture.0.join("existing.json"), &snapshot.stdout)?;

    // Backslash, quote, spaces and UTF-8 exercise actual shell parsing.
    // The slash-spelled decoy catches normalization even when the retry exits 0.
    for name in [
        r"captures\before's ü.json",
        "plain.json",
        "missing snapshot's ü.json",
    ] {
        let decoy = fixture.0.join("captures/before's ü.json");
        std::fs::write(&decoy, b"preserve this existing snapshot\n")?;
        for args in [
            vec!["outcome", "--before", name, "--after", "existing.json"],
            vec!["outcome", "--before", "existing.json", "--after", name],
            vec![
                "calibrate",
                "cargo-mutants",
                "--mutants-json",
                "mutants.json",
                "--repo-exposure-json",
                name,
            ],
        ] {
            assert!(!fixture.0.join(name).exists(), "missing-path setup");
            let refusal = spawn_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&fixture.0),
                &args,
                &[],
                None,
                Some(b""),
            )?;
            assert_eq!(refusal.status.code(), Some(2), "{refusal:?}");
            assert!(refusal.stdout.is_empty(), "no report on read failure");
            let stderr = String::from_utf8(refusal.stderr)?;
            let command = stderr
                .split_once("Run `")
                .ok_or("missing capture instruction")?
                .1
                .split_once('`')
                .ok_or("unterminated capture instruction")?
                .0;
            // A shell function selects the candidate executable while leaving
            // every byte of the emitted command (especially its redirect) intact.
            let binary = env!("CARGO_BIN_EXE_ripr").replace('\'', "'\\''");
            let script = format!("ripr() {{ '{binary}' \"$@\"; }}\n{command}\n");
            let replay = spawn_command(
                "sh",
                Some(&fixture.0),
                &["-s"],
                &[],
                None,
                Some(script.as_bytes()),
            )?;
            assert_success(&replay);
            assert_eq!(
                std::fs::read(&decoy)?,
                b"preserve this existing snapshot\n",
                "successful retry must not overwrite the normalized-path decoy: {command}"
            );
            let captured: serde_json::Value =
                serde_json::from_slice(&std::fs::read(fixture.0.join(name))?)?;
            assert!(
                captured["seams"]
                    .as_array()
                    .is_some_and(|seams| !seams.is_empty()),
                "retry must capture real evidence at the requested filename: {captured}"
            );
            std::fs::remove_file(fixture.0.join(name))?;
        }
    }
    Ok(())
}
