#![cfg(feature = "lang-rust")]

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok;
use ripr::process_owner::OwnedProcess;
use serde_json::Value;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const PREFIX: &str = "ripr_probe_read_trace ";
const STREAM_CAP: u64 = 1024 * 1024;
const SOURCE: &str = "pub fn price(qty: u32) -> u32 {\n    if qty > 10 { 90 } else { 100 }\n}\npub fn op(bytes: &[u8]) -> bool {\n    bytes.contains(&b'=')\n}\npub mod alpha {\n    use super::op;\n    pub fn parse(bytes: &[u8]) -> bool {\n        op(bytes)\n    }\n}\npub mod beta {\n    pub fn parse(bytes: &[u8]) -> bool {\n        !bytes.is_empty()\n    }\n}\n#[cfg(test)]\nmod tests {\n    use super::op as parse;\n    use super::*;\n    #[test]\n    fn boundary() {\n        assert_eq!(price(10), 90);\n    }\n    #[test]\n    fn renamed_alias_calls_ambiguous_entry() {\n        assert!(parse(b\"=1.0\"));\n    }\n}\n";

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        let root = parent.join(format!(
            "ripr-probe-read-cli-{}-{}-{stamp}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        std::fs::create_dir(fixture.0.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(fixture.0.join(".scratch")).map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join("Cargo.toml"), "[package]\nname=\"probe_trace_fixture\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[workspace]\n").map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join(".gitignore"), "/target/\n/.scratch/\n")
            .map_err(|error| error.to_string())?;
        std::fs::write(fixture.0.join("src/lib.rs"), SOURCE).map_err(|error| error.to_string())?;
        fixture_git_ok(&fixture.0, &["init", "-q", "-b", "main"])?;
        fixture_git_ok(&fixture.0, &["add", "."])?;
        fixture_git_ok(
            &fixture.0,
            &[
                "-c",
                "user.name=RIPR fixture",
                "-c",
                "user.email=fixture@ripr.invalid",
                "commit",
                "-qm",
                "base",
            ],
        )?;
        let changed = SOURCE
            .replace("qty > 10", "qty >= 10")
            .replace("bytes.contains(&b'=')", "bytes.contains(&b'>')");
        assert_ne!(changed, SOURCE, "fixture must change the actual predicate");
        std::fs::write(fixture.0.join("src/lib.rs"), changed).map_err(|error| error.to_string())?;
        Ok(fixture)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read_stream(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(STREAM_CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > STREAM_CAP {
        return Err(format!(
            "fixture stream {} exceeded its read cap",
            path.display()
        ));
    }
    Ok(bytes)
}

fn check(fixture: &Fixture, traced: bool, label: &str) -> Result<Output, String> {
    let stdout = fixture.0.join(format!(".scratch/{label}.stdout"));
    let stderr = fixture.0.join(format!(".scratch/{label}.stderr"));
    let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .current_dir(&fixture.0)
        .args([
            "check",
            "--root",
            ".",
            "--base",
            "HEAD",
            "--worktree",
            "--mode",
            "draft",
            "--format",
            "json",
        ])
        .env_remove("RIPR_PROBE_READ_TRACE")
        .env_remove("RIPR_REPO_EXPOSURE_LATENCY_TRACE")
        .env("RIPR_CACHE_DIR", fixture.0.join(".scratch/cache"))
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).map_err(|error| error.to_string())?)
        .stderr(File::create(&stderr).map_err(|error| error.to_string())?);
    if traced {
        command.env("RIPR_PROBE_READ_TRACE", "1");
    }
    let mut child = OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            child.terminate_tree()?;
            return Err("owned fixture check exceeded its external deadline; no completed result established".to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: read_stream(&stdout)?,
        stderr: read_stream(&stderr)?,
    })
}

#[test]
fn real_check_trace_is_opt_in_and_preserves_nonempty_json() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let source_before =
        std::fs::read(fixture.0.join("src/lib.rs")).map_err(|error| error.to_string())?;
    let plain = check(&fixture, false, "plain")?;
    let traced = check(&fixture, true, "traced")?;
    assert!(
        plain.status.success(),
        "plain check failed: {}",
        String::from_utf8_lossy(&plain.stderr)
    );
    assert!(
        traced.status.success(),
        "traced check failed: {}",
        String::from_utf8_lossy(&traced.stderr)
    );
    let report: Value =
        serde_json::from_slice(&traced.stdout).map_err(|error| error.to_string())?;
    assert!(
        report["findings"]
            .as_array()
            .is_some_and(|findings| !findings.is_empty()),
        "fixture must reach nonempty real producer evidence: {report}"
    );
    assert_eq!(
        plain.stdout, traced.stdout,
        "trace must preserve complete report bytes"
    );
    assert!(
        !String::from_utf8_lossy(&plain.stderr)
            .lines()
            .any(|line| line.starts_with(PREFIX))
    );
    let stderr = std::str::from_utf8(&traced.stderr).map_err(|error| error.to_string())?;
    let rows = stderr
        .lines()
        .filter_map(|line| line.strip_prefix(PREFIX))
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    assert!(
        !rows.is_empty(),
        "enabled trace emitted no actual probe observation"
    );
    let mut reads = 0;
    for row in &rows {
        assert_eq!(row["event"], "probe_read_observation");
        assert_eq!(row["coverage"], "unknown");
        assert_eq!(row["reusable"], false);
        assert_eq!(row["analysis_completion"], "not_established");
        assert_eq!(row["classification_returned"], true);
        let id = row["probe_id"].as_str().ok_or("trace lost its probe id")?;
        assert!(!id.is_empty());
        assert!(
            report["findings"]
                .as_array()
                .is_some_and(|findings| findings
                    .iter()
                    .any(|finding| finding["probe"]["id"].as_str() == Some(id))),
            "trace id must name this fixture's actual producer evidence: {id}"
        );
        assert!(
            row["unknown_inputs"]
                .as_array()
                .is_some_and(|inputs| inputs.len() == 5)
        );
        reads += row["observed_read_events"]
            .as_u64()
            .ok_or("trace lost observed read count")?;
    }
    assert!(
        reads > 0,
        "real classifications must observe actual indexed reads"
    );
    assert!(
        rows.iter().any(|row| row["universe_observed"] == true),
        "fixture must reach a real whole-set view"
    );
    // #10933: the uniqueness-stop probe for `op` reaches
    // `source_imports_foreign_entry` inside its classification scope, so at
    // least one traced row carries a `memo`-kind read for the
    // `foreign_entry_import` product.
    assert!(
        rows.iter().any(|row| row["reads"]
            .as_array()
            .is_some_and(|reads| reads.iter().any(|entry| entry["kind"] == "memo"
                && entry["subject"]
                    .as_str()
                    .is_some_and(|subject| subject.starts_with("foreign_entry_import:"))))),
        "no traced probe row records a foreign_entry_import memo read"
    );
    assert_eq!(
        std::fs::read(fixture.0.join("src/lib.rs")).map_err(|error| error.to_string())?,
        source_before
    );
    Ok(())
}
