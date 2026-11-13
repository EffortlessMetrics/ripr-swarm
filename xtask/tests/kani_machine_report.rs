//! Actual runner controls with a synthetic cargo child, not native Kani proof.
#![cfg(unix)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

const PASS: &str = "Checking harness m::proof...\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n";
const CARGO: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$KANI_MACHINE_ROOT/calls"
if [ "$*" = 'kani --version' ]; then
  echo 'synthetic version; not a native verifier'
  exit 0
fi
[ "$*" = 'kani -p ripr -Z unstable-options --harness-timeout 1s' ] || exit 97
cat "$KANI_MACHINE_ROOT/output"
if [ "$KANI_MACHINE_CHANGE" = yes ]; then
  printf '// changed during verification\n' >> "$KANI_MACHINE_ROOT/crates/ripr/src/lib.rs"
fi
exit 0
"#;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temp = std::env::temp_dir();
        fs::create_dir_all(&temp).map_err(|e| e.to_string())?;
        let root = temp.join(format!(
            "kani-machine-process-{}-{}",
            std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let fixture = Self(root);
        for dir in ["bin", "crates/ripr/src", "crates/ripr-contract/src"] {
            fs::create_dir_all(fixture.0.join(dir)).map_err(|e| e.to_string())?;
        }
        fs::write(fixture.0.join("Cargo.toml"), "[workspace]\nmembers=[]\n")
            .map_err(|e| e.to_string())?;
        fs::write(fixture.0.join("crates/ripr/src/lib.rs"),
            "#[kani::proof]\nfn proof() { kani::cover!(true); }\n")
            .map_err(|e| e.to_string())?;
        let executable = fixture.0.join("bin/cargo");
        fs::write(&executable, CARGO).map_err(|e| e.to_string())?;
        fs::set_permissions(executable, fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        Ok(fixture)
    }

    fn run(&self, native: &str, change: bool) -> Result<(Value, bool), String> {
        fs::write(self.0.join("output"), native).map_err(|e| e.to_string())?;
        fs::write(self.0.join("calls"), "").map_err(|e| e.to_string())?;
        let mut paths = vec![self.0.join("bin")];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        let path = std::env::join_paths(paths).map_err(|e| e.to_string())?;
        let output = xtask_bin::output(|command| {
            command.args(["kani", "--timeout-secs", "20", "--harness-timeout-secs", "1"])
                .current_dir(&self.0)
                .env("PATH", &path)
                .env("KANI_MACHINE_ROOT", &self.0)
                .env("KANI_MACHINE_CHANGE", if change { "yes" } else { "no" });
        })?;
        let calls = fs::read_to_string(self.0.join("calls")).map_err(|e| e.to_string())?;
        assert_eq!(calls.lines().collect::<Vec<_>>(), [
            "kani --version", "kani -p ripr -Z unstable-options --harness-timeout 1s"
        ]);
        let directory = self.0.join("target/ripr/reports");
        let document: Value = serde_json::from_slice(&fs::read(directory.join("kani.json"))
            .map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        // A field-set drift guard, not a replacement JSON Schema validator.
        // It examines actual runner output, not a source-model reconstruction.
        let schema: Value = serde_json::from_str(include_str!(
            "../../docs/verification/kani-observation-v1.schema.json"
        )).map_err(|e| e.to_string())?;
        for (value, definition) in [(&document, &schema), (&document["report"], &schema["$defs"]["report"])] {
            let fields: BTreeSet<&str> = value.as_object().ok_or("output must be an object")?
                .keys().map(String::as_str).collect();
            let declared: BTreeSet<&str> = definition["properties"].as_object()
                .ok_or("schema properties missing")?.keys().map(String::as_str).collect();
            assert_eq!(fields, declared, "runner/schema top-level field drift");
        }
        let markdown = fs::read_to_string(directory.join("kani.md")).map_err(|e| e.to_string())?;
        assert_eq!(document["markdown"], markdown);
        assert_eq!(document["markdown_sha256"], format!("sha256:{:x}", Sha256::digest(markdown.as_bytes())));
        let immutable = document["markdown_path"].as_str().ok_or("missing immutable Markdown path")?;
        assert_eq!(fs::read_to_string(directory.join(immutable)).map_err(|e| e.to_string())?, markdown);
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), markdown.trim_end());
        assert_eq!(document["report"]["phase"], "completed");
        assert_eq!(document["report"]["lane_success"], output.status.success());
        assert_eq!(document["report"]["registered_claims"], 0);
        assert_eq!(document["report"]["claim_assurance"], "not_established");
        assert!(document["report"]["results"].as_array().is_some_and(|r| r.len() == 1));
        assert!(document["report"]["invocations"].as_array().is_some_and(|r| r.len() == 2));
        Ok((document, output.status.success()))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn current_run_replaces_old_success_without_borrowing_its_evidence() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let (first, success) = fixture.run(PASS, false)?;
    assert!(success);
    let (second, success) = fixture.run("Checking harness m::proof...\nVERIFICATION:- SUCCESSFUL\n", false)?;
    assert!(!success);
    assert_ne!(first["report"]["run_id"], second["report"]["run_id"]);
    assert_eq!(second["report"]["results"][0]["observation"]["status"]["status"], "NOT-ESTABLISHED");
    assert_ne!(first["report"]["invocations"][1]["stdout"]["sha256"], second["report"]["invocations"][1]["stdout"]["sha256"]);
    Ok(())
}

#[test]
fn unexpected_native_subject_is_preserved_and_refuses_lane_success() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let (document, success) = fixture.run(&format!("{PASS}{}", PASS.replace("m::proof", "other::unexpected")), false)?;
    assert!(!success);
    assert_eq!(document["report"]["results"][0]["observation"]["status"]["status"], "VERIFIED");
    assert_eq!(document["report"]["invocations"][1]["unmatched_native_blocks"], serde_json::json!(["other::unexpected"]));
    Ok(())
}

#[test]
fn changed_proof_source_cannot_reuse_a_success_looking_row() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let (document, success) = fixture.run(PASS, true)?;
    assert!(!success);
    assert_eq!(document["report"]["selected_files_unchanged_after_run"], false);
    assert_eq!(document["report"]["results"][0]["observation"]["status"]["status"], "VERIFIED");
    assert!(document["report"]["source_recheck_errors"].as_array().is_some_and(|r| !r.is_empty()));
    Ok(())
}
