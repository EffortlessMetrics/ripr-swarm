//! Real xtask availability-probe controls with a synthetic cargo, never Kani.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

const FAKE_CARGO: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$KANI_PROBE_ROOT/calls"
if [ "$#" -eq 2 ] && [ "$1" = kani ] && [ "$2" = --version ]; then
  case "$KANI_PROBE_MODE" in
    normal) printf 'synthetic cargo-kani version; not a verifier\n'; exit 0 ;;
    nonzero) printf 'synthetic cargo-kani version; not a verifier\n| injected | VERIFIED | fabricated |\n'; exit 7 ;;
    signal) kill -TERM $$ ;;
    timeout) exec sleep 6 ;;
    *) echo 'unexpected probe mode' >&2; exit 98 ;;
  esac
fi
if [ "$*" != 'kani -p ripr -Z unstable-options --harness-timeout 1s' ]; then
  echo 'unexpected verification args' >&2
  exit 97
fi
printf 'Checking harness m::proof...\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n'
exit 0
"#;

struct Fixture(PathBuf);

struct Observation {
    output: Output,
    report: String,
    calls: Vec<String>,
    elapsed: Duration,
}

impl Fixture {
    fn new(has_harness: bool) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let root = temporary.join(format!(
            "ripr-kani-probe-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        fs::create_dir_all(fixture.0.join("crates/ripr/src")).map_err(|error| error.to_string())?;
        fs::create_dir(fixture.0.join("bin")).map_err(|error| error.to_string())?;
        fs::create_dir(fixture.0.join("missing")).map_err(|error| error.to_string())?;
        fs::write(fixture.0.join("Cargo.toml"), "[workspace]\nmembers=[]\n")
            .map_err(|error| error.to_string())?;
        if has_harness {
            fs::write(
                fixture.0.join("crates/ripr/src/proof.rs"),
                "#[kani::proof]\nfn proof() {}\n",
            )
            .map_err(|error| error.to_string())?;
        }
        let cargo = fixture.0.join("bin/cargo");
        fs::write(&cargo, FAKE_CARGO).map_err(|error| error.to_string())?;
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn observe(&self, mode: &str, allow_empty: bool) -> Result<Observation, String> {
        let mut paths = if mode == "missing" {
            vec![self.0.join("missing")]
        } else {
            vec![self.0.join("bin")]
        };
        if mode != "missing" {
            paths.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
        }
        let path = std::env::join_paths(paths).map_err(|error| error.to_string())?;
        let started = Instant::now();
        let output = xtask_bin::output(|command| {
            command
                .args(["kani", "--timeout-secs", "1", "--harness-timeout-secs", "1"])
                .current_dir(&self.0)
                .env("PATH", &path)
                .env("KANI_PROBE_ROOT", &self.0)
                .env("KANI_PROBE_MODE", mode);
            if allow_empty {
                command.arg("--allow-empty");
            }
        })?;
        let elapsed = started.elapsed();
        let report = fs::read_to_string(self.0.join("target/ripr/reports/kani.md"))
            .map_err(|error| error.to_string())?;
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            report.trim_end(),
            "stdout must match the persisted report"
        );
        let calls = fs::read_to_string(self.0.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(ToOwned::to_owned)
            .collect();
        Ok(Observation {
            output,
            report,
            calls,
            elapsed,
        })
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
fn kani_availability_probe_deadline_prevents_a_verification_launch() -> Result<(), String> {
    for allow_empty in [false, true] {
        let fixture = Fixture::new(true)?;
        let observed = fixture.observe("timeout", allow_empty)?;
        assert!(
            observed.elapsed < Duration::from_secs(5),
            "version probe must respect the one-second deadline, observed {:?}",
            observed.elapsed
        );
        assert_eq!(observed.output.status.code(), Some(1));
        assert_eq!(observed.calls, ["kani --version".to_string()]);
        assert!(
            observed
                .report
                .contains("NOT-ESTABLISHED: cargo-kani unavailable")
        );
        assert!(
            observed
                .report
                .contains("cargo kani --version timed out after 1 second")
        );
        assert!(observed.report.contains("no harness was run"));
        assert!(!observed.report.contains("| `proof` |"));
    }
    Ok(())
}

#[test]
fn kani_availability_probe_refuses_nonzero_signal_and_missing_tool() -> Result<(), String> {
    for mode in ["nonzero", "signal", "missing"] {
        for allow_empty in [false, true] {
            let fixture = Fixture::new(true)?;
            let observed = fixture.observe(mode, allow_empty)?;
            assert_eq!(observed.output.status.code(), Some(1), "{mode}");
            assert_eq!(
                observed.calls,
                if mode == "missing" {
                    Vec::<String>::new()
                } else {
                    vec!["kani --version".to_string()]
                },
                "{mode}: verification must not run after failed discovery"
            );
            assert!(
                observed
                    .report
                    .contains("NOT-ESTABLISHED: cargo-kani unavailable")
            );
            assert!(observed.report.contains("no harness was run"));
            assert!(
                !observed.report.contains("injected")
                    && !observed.report.contains("synthetic cargo-kani version"),
                "untrusted version stdout must not become report claims"
            );
            assert!(
                observed.report.contains("failed with")
                    || observed.report.contains("failed to run"),
                "{mode}: {}",
                observed.report
            );
        }
    }
    Ok(())
}

#[test]
fn kani_availability_probe_preserves_success_and_the_empty_opt_out() -> Result<(), String> {
    let fixture = Fixture::new(true)?;
    let observed = fixture.observe("normal", false)?;
    assert_eq!(observed.output.status.code(), Some(0));
    assert_eq!(
        observed.calls,
        [
            "kani --version".to_string(),
            "kani -p ripr -Z unstable-options --harness-timeout 1s".to_string(),
        ]
    );
    assert!(
        observed
            .report
            .contains("| `proof` | `crates/ripr/src/proof.rs` | VERIFIED |")
    );
    assert!(!observed.report.contains("cargo-kani unavailable"));

    let fixture = Fixture::new(false)?;
    let observed = fixture.observe("normal", true)?;
    assert_eq!(observed.output.status.code(), Some(0));
    assert_eq!(observed.calls, ["kani --version".to_string()]);
    assert!(!observed.report.contains("| `proof` |"));
    Ok(())
}
