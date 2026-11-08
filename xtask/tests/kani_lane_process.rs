//! Real runner/process controls with a synthetic cargo-kani, not a verifier.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

const FAKE_CARGO: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$KANI_CONTROL_ROOT/calls"
if [ "$#" -eq 2 ] && [ "$1" = kani ] && [ "$2" = --version ]; then
  printf 'synthetic cargo-kani fixture; not a verifier\n'
  exit 0
fi
if [ "$*" != 'kani -p ripr -Z unstable-options --harness-timeout 1s' ]; then
  echo 'unexpected synthetic cargo invocation' >&2
  exit 97
fi
cat "$KANI_CONTROL_ROOT/native-output"
case "$KANI_CONTROL_MODE" in
  success) exit 0 ;;
  failure) exit 7 ;;
  signal) kill -TERM $$ ;;
  timeout) exec sleep 10 ;;
  *) exit 98 ;;
esac
"#;

const PASS: &str =
    "Checking harness a::one...\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n";

struct Fixture(PathBuf);

struct Observation {
    output: Output,
    report: String,
}

impl Fixture {
    fn new(harnesses: &[(&str, &str)], native_output: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let root = temporary.join(format!(
            "ripr-kani-process-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        fs::create_dir_all(fixture.0.join("crates/ripr/src")).map_err(|error| error.to_string())?;
        fs::create_dir(fixture.0.join("bin")).map_err(|error| error.to_string())?;
        for (file, name) in harnesses {
            fs::write(
                fixture.0.join("crates/ripr/src").join(file),
                format!("#[kani::proof]\nfn {name}() {{}}\n"),
            )
            .map_err(|error| error.to_string())?;
        }
        fs::write(fixture.0.join("Cargo.toml"), "[workspace]\nmembers=[]\n")
            .map_err(|error| error.to_string())?;
        fs::write(fixture.0.join("native-output"), native_output)
            .map_err(|error| error.to_string())?;
        let executable = fixture.0.join("bin/cargo");
        fs::write(&executable, FAKE_CARGO).map_err(|error| error.to_string())?;
        fs::set_permissions(executable, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn run(&self, mode: &str, allow_empty: bool) -> Result<Observation, String> {
        // Both PATH and cwd apply only to the spawned xtask, never the test process.
        let mut paths = vec![self.0.join("bin")];
        paths.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let path = std::env::join_paths(paths).map_err(|error| error.to_string())?;
        let timeout = if mode == "timeout" { "3" } else { "20" };
        let output = xtask_bin::output(|command| {
            command
                .args([
                    "kani",
                    "--timeout-secs",
                    timeout,
                    "--harness-timeout-secs",
                    "1",
                ])
                .current_dir(&self.0)
                .env("PATH", &path)
                .env("KANI_CONTROL_ROOT", &self.0)
                .env("KANI_CONTROL_MODE", mode);
            if allow_empty {
                command.arg("--allow-empty");
            }
        })?;
        let calls = fs::read_to_string(self.0.join("calls")).map_err(|error| error.to_string())?;
        assert_eq!(
            calls.lines().collect::<Vec<_>>(),
            [
                "kani --version",
                "kani -p ripr -Z unstable-options --harness-timeout 1s",
            ],
            "the actual driver must reach the synthetic child exactly once"
        );
        let report = fs::read_to_string(self.0.join("target/ripr/reports/kani.md"))
            .map_err(|error| error.to_string())?;
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim_end(),
            report.trim_end(),
            "stdout and the persisted report must agree"
        );
        Ok(Observation { output, report })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn assert_row(report: &str, name: &str, file: &str, status: &str) {
    assert!(
        report.contains(&format!(
            "| `{name}` | `crates/ripr/src/{file}` | {status} |"
        )),
        "expected {name} in {file} to be {status}: {report}"
    );
}

#[test]
fn kani_runner_retains_unique_success_and_explicit_failure() -> Result<(), String> {
    let fixture = Fixture::new(&[("one.rs", "one")], PASS)?;
    let observed = fixture.run("success", false)?;
    assert_eq!(observed.output.status.code(), Some(0));
    assert_row(&observed.report, "one", "one.rs", "VERIFIED");
    assert!(
        observed
            .report
            .contains("within the harness bounds | 0.1s |")
    );

    let fixture = Fixture::new(
        &[("one.rs", "one")],
        "Checking harness a::one...\nVERIFICATION:- FAILED\n",
    )?;
    let observed = fixture.run("failure", false)?;
    assert_eq!(observed.output.status.code(), Some(1));
    assert_row(&observed.report, "one", "one.rs", "FAILED");
    Ok(())
}

#[test]
fn kani_runner_refuses_success_text_after_nonzero_or_signal() -> Result<(), String> {
    for mode in ["failure", "signal"] {
        for allow_empty in [false, true] {
            let fixture = Fixture::new(&[("one.rs", "one")], PASS)?;
            let observed = fixture.run(mode, allow_empty)?;
            assert_eq!(observed.output.status.code(), Some(1));
            assert_row(&observed.report, "one", "one.rs", "VERIFIED");
            assert!(
                observed
                    .report
                    .contains("NOT-ESTABLISHED: cargo kani exited")
            );
            assert!(String::from_utf8_lossy(&observed.output.stderr).contains("cargo kani exited"));
        }
    }
    Ok(())
}

#[test]
fn kani_runner_refuses_ambiguous_sources_in_either_native_order() -> Result<(), String> {
    let success = "Checking harness a::same...\nVERIFICATION:- SUCCESSFUL\n";
    let failure = "Checking harness b::same...\nVERIFICATION:- FAILED\n";
    let unique = "Checking harness c::unique...\nVERIFICATION:- SUCCESSFUL\n";
    for text in [
        format!("{success}{unique}"),
        format!("{success}{failure}{unique}"),
        format!("{failure}{success}{unique}"),
    ] {
        let fixture = Fixture::new(
            &[("a.rs", "same"), ("b.rs", "same"), ("c.rs", "unique")],
            &text,
        )?;
        // Exit zero isolates attribution from the independent process refusal.
        let observed = fixture.run("success", true)?;
        assert_eq!(observed.output.status.code(), Some(1));
        assert_row(&observed.report, "same", "a.rs", "NOT-ESTABLISHED");
        assert_row(&observed.report, "same", "b.rs", "NOT-ESTABLISHED");
        assert_row(&observed.report, "unique", "c.rs", "VERIFIED");
    }
    Ok(())
}

#[test]
fn kani_runner_refuses_repeated_blocks_and_absent_results() -> Result<(), String> {
    for text in [
        format!("{PASS}{PASS}"),
        "compiled without a verdict\n".into(),
    ] {
        let fixture = Fixture::new(&[("one.rs", "one")], &text)?;
        let observed = fixture.run("success", false)?;
        assert_eq!(observed.output.status.code(), Some(1));
        assert_row(&observed.report, "one", "one.rs", "NOT-ESTABLISHED");
    }
    Ok(())
}

#[test]
fn kani_runner_preserves_completed_rows_after_a_real_timeout() -> Result<(), String> {
    let text = format!("{PASS}Checking harness b::two...\nCompiling\n");
    let fixture = Fixture::new(&[("one.rs", "one"), ("two.rs", "two")], &text)?;
    let observed = fixture.run("timeout", false)?;
    assert_eq!(observed.output.status.code(), Some(1));
    assert!(
        observed
            .report
            .contains("NOT-ESTABLISHED: cargo kani timed out")
    );
    assert_row(&observed.report, "one", "one.rs", "VERIFIED");
    assert_row(&observed.report, "two", "two.rs", "NOT-ESTABLISHED");
    assert!(String::from_utf8_lossy(&observed.output.stderr).contains("1 not established"));
    Ok(())
}
