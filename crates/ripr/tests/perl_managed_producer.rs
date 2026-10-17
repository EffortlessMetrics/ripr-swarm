//! Managed Perl producer argument contract against a stand-in exporter.
//!
//! The real `perl-ripr-facts` exporter rejects `./`-prefixed and absolute path
//! arguments, runs git never, and emits `changes[]` only from a `--diff` file it
//! is handed. This stand-in enforces those same rules, so a regression in how
//! `ripr check` invokes a configured exporter fails here without needing the
//! unpublished exporter on PATH.

#![cfg(all(feature = "lang-perl", unix))]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const STAND_IN: &str = r#"#!/bin/sh
fail() { echo "stand-in exporter: $*" >&2; exit 1; }
[ "$1" = "ripr-facts" ] || fail "missing ripr-facts subcommand"
shift
root=""; out=""; diff=""
while [ $# -gt 0 ]; do
  case "$1" in
    --root) root="$2" ;;
    --out) out="$2" ;;
    --diff) diff="$2" ;;
  esac
  shift 2
done
[ "$root" = "." ] || fail "root must be '.', got '$root'"
case "$out" in ""|/*|./*|*..*) fail "out must be root-relative without ./ or .., got '$out'" ;; esac
case "$diff" in ""|/*|./*|*..*) fail "diff must be root-relative without ./ or .., got '$diff'" ;; esac
[ -d lib ] || fail "not run from the analysis root"
grep -q 'lib/Pricing.pm' "$diff" || fail "diff file does not name lib/Pricing.pm"
if [ -n "$STAND_IN_FAIL" ]; then fail "$STAND_IN_FAIL"; fi
cp "$STAND_IN_PACKET" "$out"
"#;

fn fixture_input() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/perl_cpan_alpha/input")
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| format!("create {}: {e}", to.display()))?;
    for entry in std::fs::read_dir(from).map_err(|e| format!("read {}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new(name: &str) -> Result<Self, String> {
        let root =
            std::env::temp_dir().join(format!("ripr-managed-perl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_dir_all(&fixture_input(), &root)?;
        let exporter = root.join("stand-in-exporter");
        std::fs::write(&exporter, STAND_IN).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&exporter, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("ripr.toml"),
            "[languages]\nenabled = [\"perl\"]\n\n[perl]\nproducer = \"perl-ripr-facts\"\nexecutable = \"stand-in-exporter\"\n",
        )
        .map_err(|e| e.to_string())?;
        Ok(Self { root })
    }

    fn check(&self, stand_in_fail: Option<&str>) -> Result<Value, String> {
        let packet = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../fixtures/perl_cpan_alpha/expected/regression-packets/actionable-weak-ok.json",
        );
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .arg("check")
            .arg("--root")
            .arg(&self.root)
            .arg("--diff")
            .arg(self.root.join("diff.patch"))
            .arg("--json")
            .env("RIPR_ALLOW_REPO_PERL_EXECUTABLE", "1")
            .env("PATH", format!("{}:/usr/bin:/bin", self.root.display()))
            .env("STAND_IN_PACKET", packet);
        if let Some(message) = stand_in_fail {
            command.env("STAND_IN_FAIL", message);
        }
        let output = command.output().map_err(|e| e.to_string())?;
        serde_json::from_slice(&output.stdout).map_err(|e| {
            format!(
                "parse check JSON: {e}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stderr)
            )
        })
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn perl_run(report: &Value) -> Option<&Value> {
    report
        .get("language_runs")?
        .as_array()?
        .iter()
        .find(|run| run.get("language").and_then(Value::as_str) == Some("perl"))
}

#[test]
fn managed_producer_receives_root_relative_paths_and_the_diff() -> Result<(), String> {
    let workspace = Workspace::new("args")?;
    let report = workspace.check(None)?;
    let findings = report
        .get("findings")
        .and_then(Value::as_array)
        .ok_or("report has no findings array")?;
    assert!(
        !findings.is_empty(),
        "a valid managed invocation must yield the packet's finding: {report}"
    );
    assert!(
        !report.to_string().contains("stand-in exporter"),
        "the stand-in must not have rejected the invocation: {report}"
    );
    Ok(())
}

#[test]
fn managed_producer_failure_names_the_exporter_stderr() -> Result<(), String> {
    let workspace = Workspace::new("stderr")?;
    let report = workspace.check(Some("unsupported-schema-marker"))?;
    let run = perl_run(&report).ok_or("report has no perl language run")?;
    let text = run.to_string();
    assert!(
        text.contains("unsupported-schema-marker"),
        "the exporter's own stderr must reach the failure reason: {text}"
    );
    Ok(())
}
