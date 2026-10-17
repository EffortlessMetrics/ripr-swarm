//! A timeout's machine next action must remain machine-readable on recovery.
use super::{DeclaredHunkScratch, probe_command, run_owned_stdin_probe, unique_external_workspace};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Output, Stdio};
use std::time::Duration;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn bounded(mut command: std::process::Command) -> Result<Output, std::io::Error> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run_owned_stdin_probe(command, &[], Duration::from_secs(15))
}

#[test]
fn timeout_json_retry_replays_as_json_from_a_foreign_directory() -> TestResult {
    let sandbox = unique_external_workspace("pilot-timeout-json-retry")?;
    fs::create_dir_all(&sandbox)?;
    let _cleanup = DeclaredHunkScratch(sandbox.clone());
    let root = sandbox.join("workspace's root");
    let packet = root.join("packet's directory");
    let foreign = sandbox.join("foreign");
    let shim = sandbox.join("shim");
    for dir in [root.join("src"), foreign.clone(), shim.clone()] {
        fs::create_dir_all(dir)?;
    }
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"timeout_retry\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"rust\"]\n",
    )?;
    fs::write(
        root.join("src/lib.rs"),
        "pub fn price(amount: u32, threshold: u32) -> u32 {\n    if amount >= threshold { amount - 10 } else { amount }\n}\n\n#[cfg(test)]\nmod t {\n    use super::*;\n    #[test]\n    fn below() {\n        assert_eq!(price(1, 100), 1);\n    }\n}\n",
    )?;
    // The default-base probe runs inside pilot's deadline. Hold that probe
    // beyond the 100ms budget, rather than racing a small inventory against it.
    // exec gives the Git owner one primary to terminate/reap on cancellation
    // or at its independent 1s limit.
    let marker = sandbox.join("probe-entered");
    let git = shim.join("git");
    fs::write(
        &git,
        "#!/bin/sh\ncase \"$*\" in\n  *symbolic-ref*) : > \"$RIPR_RETRY_PROBE_MARKER\"; exec sleep 2 ;;\n  *) exit 1 ;;\nesac\n",
    )?;
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755))?;
    let mut paths = vec![shim];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut first = probe_command(env!("CARGO_BIN_EXE_ripr"));
    first
        .current_dir(&foreign)
        .args(["pilot", "--root"])
        .arg(&root)
        .arg("--out")
        .arg(&packet)
        .args([
            "--format",
            "json",
            "--max-seams",
            "3",
            "--timeout-ms",
            "100",
        ])
        .env("PATH", std::env::join_paths(paths)?)
        .env("RIPR_GIT_TIMEOUT", "1")
        .env("RIPR_RETRY_PROBE_MARKER", &marker);
    let partial = bounded(first)?;
    assert_eq!(partial.status.code(), Some(0), "{partial:?}");
    assert!(
        marker.is_file(),
        "the deliberately stalled probe did not run"
    );
    let document: Value = serde_json::from_slice(&partial.stdout)?;
    assert_eq!(document["schema_version"], "0.4");
    assert_eq!(document["status"], "partial");
    assert_eq!(document["reason"], "timeout");
    assert_eq!(document["completed_phases"], json!([]));
    assert_eq!(document.get("actionable_seams_total"), Some(&Value::Null));
    assert_eq!(
        document.get("withheld_static_limitations_total"),
        Some(&Value::Null)
    );
    assert_eq!(document["top_actionable_seams"], json!([]));
    assert_eq!(
        document["outputs_written"],
        json!(["pilot_summary_json", "pilot_summary_md"])
    );
    assert_eq!(partial.stdout, fs::read(packet.join("pilot-summary.json"))?);
    for file in [
        "repo-exposure.json",
        "repo-exposure.md",
        "agent-seam-packets.json",
    ] {
        assert!(
            !packet.join(file).exists(),
            "partial analysis invented {file}"
        );
    }

    let retry = document["next"]["retry_command"]
        .as_str()
        .ok_or("missing retry")?;
    let words = super::shell_words::posix_words(retry)?;
    assert_eq!(words.first().map(String::as_str), Some("ripr"));
    let mut second = probe_command(env!("CARGO_BIN_EXE_ripr"));
    second.current_dir(&foreign).args(words.iter().skip(1));
    let recovered = bounded(second)?;
    assert_eq!(recovered.status.code(), Some(0), "{recovered:?}");
    // Establish the nonempty recovery subject independently of stdout. The
    // old command completes analysis and writes JSON, but prints human text.
    let written = fs::read(packet.join("pilot-summary.json"))?;
    let complete: Value = serde_json::from_slice(&written)?;
    assert_eq!(complete["schema_version"], "0.4");
    assert_eq!(complete["status"], "complete");
    assert_eq!(complete["current_change"]["state"], "unavailable");
    assert_eq!(
        complete["current_change"].get("actionable_seams_in_change"),
        Some(&Value::Null)
    );
    assert_eq!(complete["max_seams"], 3);
    assert_eq!(
        complete["root"].as_str().map(std::path::Path::new),
        Some(root.as_path())
    );
    assert_eq!(complete["actionable_seams_total"], 1);
    assert_eq!(complete["top_actionable_seams"][0]["file"], "src/lib.rs");
    assert_eq!(complete["top_actionable_seams"][0]["line"], 2);
    assert_eq!(
        complete["top_actionable_seams"][0]["kind"],
        "predicate_boundary"
    );
    assert_eq!(
        complete["top_actionable_seams"][0]["owner"],
        "src/lib.rs::price"
    );
    let replayed: Value = serde_json::from_slice(&recovered.stdout).map_err(|error| {
        format!(
            "retry stdout was not JSON: {error}; stdout={:?}; stderr={:?}",
            String::from_utf8_lossy(&recovered.stdout),
            String::from_utf8_lossy(&recovered.stderr)
        )
    })?;
    assert_eq!(replayed, complete);
    assert_eq!(recovered.stdout, written);
    Ok(())
}
