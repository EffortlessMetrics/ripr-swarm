//! Actual availability-probe controls with a synthetic cargo, never Kani.
//! Automation mode invokes the complete xtask CLI; formal_runner's narrow
//! mode calls the real lane API in the same independently bounded child.
//! Linux session custody covers nested groups; the watchdog does not use the
//! timed-capture deadline that these controls exercise.
#![cfg(target_os = "linux")]

use ripr::process_owner::OwnedProcess;
use rustix::process::{WaitId, WaitIdOptions, waitid};
use std::fs::{self, File};
use std::os::unix::fs::{PermissionsExt, symlink};
#[cfg(feature = "automation")]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[path = "../src/run/test_session/custody.rs"]
mod custody;

const CHILD: &str = "RIPR_KANI_PROBE_WATCHDOG_CHILD";
const LANE_FAILURE_CODE: i32 = if cfg!(feature = "automation") { 1 } else { 101 };
const FAKE_CARGO: &str = r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> "$KANI_PROBE_ROOT/calls"
if [ "$#" -eq 2 ] && [ "$1" = kani ] && [ "$2" = --version ]; then
  [ "${CARGO_TERM_COLOR-}" = never ] || exit 96
  case "$KANI_PROBE_MODE" in
    normal) printf 'synthetic cargo-kani version; not a verifier\n'; exit 0 ;;
    nonzero) printf 'synthetic cargo-kani version; not a verifier\n| injected | VERIFIED | fabricated |\n'; exit 7 ;;
    missing-kani) printf 'error: no such command: `kani`\n| injected | VERIFIED | fabricated |\n' >&2; exit 101 ;;
    misleading-missing-kani) printf 'note: error: no such command: `kani`\n' >&2; exit 101 ;;
    signal) kill -TERM $$ ;;
    timeout)
      /bin/sleep 30 &
      printf '%s %s\n' $$ $! > "$KANI_PROBE_ROOT/probe-pids"
      wait
      exit 0 ;;
    *) echo 'unexpected probe mode' >&2; exit 98 ;;
  esac
fi
case "$*" in
  'kani -p ripr -Z unstable-options --harness-timeout 1s') harness=m::proof ;;
  'kani -p ripr-contract -Z unstable-options --harness-timeout 1s') harness=m::contract_proof ;;
  *) echo 'unexpected verification args' >&2; exit 97 ;;
esac
printf 'Checking harness %s...\n** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n' "$harness"
exit 0
"#;

struct Fixture(PathBuf);

struct Observation {
    output: Output,
    report: String,
    calls: Vec<String>,
    elapsed: Duration,
    watchdog_timed_out: bool,
    owned_descendants_observed: usize,
    live_descendants_before_driver_cleanup: usize,
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
        for package in ["ripr", "ripr-contract"] {
            fs::create_dir_all(fixture.0.join(format!("crates/{package}/src")))
                .map_err(|error| error.to_string())?;
        }
        fs::create_dir(fixture.0.join("bin")).map_err(|error| error.to_string())?;
        fs::create_dir(fixture.0.join("missing")).map_err(|error| error.to_string())?;
        fs::write(fixture.0.join("Cargo.toml"), "[workspace]\nmembers=[]\n")
            .map_err(|error| error.to_string())?;
        if has_harness {
            fixture.add_harness("ripr", "proof")?;
        }
        let cargo = fixture.0.join("bin/cargo");
        fs::write(&cargo, FAKE_CARGO).map_err(|error| error.to_string())?;
        fs::set_permissions(&cargo, fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
        // The product's Unix process owner uses kill. Keep it available while
        // excluding all real cargo executables from this fixture's PATH.
        symlink("/bin/kill", fixture.0.join("bin/kill")).map_err(|error| error.to_string())?;
        Ok(fixture)
    }

    fn add_harness(&self, package: &str, name: &str) -> Result<(), String> {
        fs::write(
            self.0.join(format!("crates/{package}/src/{name}.rs")),
            format!("#[kani::proof]\nfn {name}() {{ kani::cover!(true); }}\n"),
        )
        .map_err(|error| error.to_string())
    }

    fn observe(&self, mode: &str, allow_empty: bool) -> Result<Observation, String> {
        let observed = self.drive(mode, allow_empty, 1, Duration::from_secs(6))?;
        if observed.watchdog_timed_out {
            return Err(format!(
                "independent watchdog stopped xtask and its descendants after {:?}",
                observed.elapsed
            ));
        }
        if observed.live_descendants_before_driver_cleanup != 0 {
            return Err("probe returned with live descendants before driver cleanup".into());
        }
        #[cfg(feature = "automation")]
        assert_eq!(
            String::from_utf8_lossy(&observed.output.stdout).trim_end(),
            observed.report.trim_end(),
            "CLI stdout must match the persisted report"
        );
        #[cfg(not(feature = "automation"))]
        assert!(
            String::from_utf8_lossy(&observed.output.stdout).contains(observed.report.trim_end()),
            "the lane API must print the persisted report inside the libtest wrapper"
        );
        Ok(observed)
    }

    fn drive(
        &self,
        mode: &str,
        allow_empty: bool,
        probe_budget_secs: u64,
        watchdog: Duration,
    ) -> Result<Observation, String> {
        if mode == "permission" {
            fs::set_permissions(self.0.join("bin/cargo"), fs::Permissions::from_mode(0o644))
                .map_err(|error| error.to_string())?;
        }
        let started = Instant::now();
        let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
        let child_test = concat!(module_path!(), "::kani_probe_watchdog_child")
            .split_once("::")
            .ok_or("missing test crate prefix")?
            .1;
        command
            .args(["--exact", child_test, "--nocapture"])
            .current_dir(&self.0)
            .env(CHILD, "1")
            .env("KANI_PROBE_ROOT", &self.0)
            .env("KANI_PROBE_MODE", mode)
            .env("KANI_PROBE_BUDGET", probe_budget_secs.to_string())
            .env(
                "KANI_PROBE_ALLOW_EMPTY",
                if allow_empty { "1" } else { "0" },
            )
            .env(
                "PATH",
                self.0
                    .join(if mode == "missing" { "missing" } else { "bin" }),
            )
            .stdin(Stdio::null())
            .stdout(File::create_new(self.0.join("driver.stdout")).map_err(|e| e.to_string())?)
            .stderr(File::create_new(self.0.join("driver.stderr")).map_err(|e| e.to_string())?);
        let mut child = OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
        // Declare custody after the child so cleanup runs before any error
        // path can reap the primary and release its session identity.
        let mut session = custody::TestSession::new(child.id());
        let primary = custody::pid(child.id())?;
        let mut descendants = Vec::new();
        let watchdog_timed_out = loop {
            if descendants.is_empty() {
                match fs::read_to_string(self.0.join("probe-pids")) {
                    Ok(text) => {
                        for value in text.split_whitespace() {
                            let pid = value.parse::<u32>().map_err(|e| e.to_string())?;
                            if let Some(member) = custody::read_member(pid)? {
                                if member.session != child.id() {
                                    return Err(
                                        "probe descendant escaped the owned test session".into()
                                    );
                                }
                                descendants.push(member);
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.to_string()),
                }
            }
            // Independent clock and observation loop: do not call the
            // product's capture/wait function to bound its own regression.
            let exited = waitid(
                WaitId::Pid(primary),
                WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
            )
            .map_err(|error| error.to_string())?
            .is_some();
            if exited || started.elapsed() >= watchdog {
                break !exited;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let mut live_descendants_before_driver_cleanup = 0;
        for before in &descendants {
            if same_process_is_live(before)? {
                live_descendants_before_driver_cleanup += 1;
            }
        }
        session.finish()?;
        let status = child.wait().map_err(|error| error.to_string())?;
        for before in &descendants {
            if same_process_is_live(before)? {
                return Err("an owned probe descendant survived watchdog cleanup".into());
            }
        }
        let read_capture = |name| {
            fs::read(self.0.join(name)).map_err(|error| format!("could not read {name}: {error}"))
        };
        let output = Output {
            status,
            stdout: read_capture(if cfg!(feature = "automation") {
                "stdout"
            } else {
                "driver.stdout"
            })?,
            stderr: read_capture(if cfg!(feature = "automation") {
                "stderr"
            } else {
                "driver.stderr"
            })?,
        };
        let report = match fs::read_to_string(self.0.join("target/ripr/reports/kani.md")) {
            Ok(report) => report,
            Err(error) if watchdog_timed_out && error.kind() == std::io::ErrorKind::NotFound => {
                String::new()
            }
            Err(error) => return Err(format!("no persisted probe report: {error}")),
        };
        let calls = fs::read_to_string(self.0.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(ToOwned::to_owned)
            .collect();
        Ok(Observation {
            output,
            report,
            calls,
            elapsed: started.elapsed(),
            watchdog_timed_out,
            owned_descendants_observed: descendants.len(),
            live_descendants_before_driver_cleanup,
        })
    }
}

fn same_process_is_live(before: &custody::Member) -> Result<bool, String> {
    Ok(custody::read_member(before.pid)?.is_some_and(|after| {
        after.start == before.start && after.state != "Z" && after.state != "X"
    }))
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

/// Reexecute before setsid so sibling tests cannot join the session. The full
/// CLI replaces this child with Cargo's exact binary; narrow mode invokes the
/// actual lane API here and preserves libtest's distinct completion semantics.
#[test]
fn kani_probe_watchdog_child() -> Result<(), String> {
    if std::env::var_os(CHILD).is_none() {
        return Ok(());
    }
    custody::enter_test_session()?;
    let budget = std::env::var("KANI_PROBE_BUDGET").map_err(|error| error.to_string())?;
    let mut args = vec![
        "--timeout-secs".to_string(),
        budget,
        "--harness-timeout-secs".to_string(),
        "1".to_string(),
    ];
    if std::env::var("KANI_PROBE_ALLOW_EMPTY").as_deref() == Ok("1") {
        args.push("--allow-empty".to_string());
    }
    #[cfg(feature = "automation")]
    {
        let root = PathBuf::from(std::env::var_os("KANI_PROBE_ROOT").ok_or("missing probe root")?);
        let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
        command
            .arg("kani")
            .args(args)
            .stdout(File::create_new(root.join("stdout")).map_err(|error| error.to_string())?)
            .stderr(File::create_new(root.join("stderr")).map_err(|error| error.to_string())?);
        Err(format!(
            "could not execute candidate xtask: {}",
            command.exec()
        ))
    }
    #[cfg(not(feature = "automation"))]
    crate::kani_lane::run_lane(&args)
}

#[test]
fn kani_availability_probe_deadline_prevents_a_verification_launch() -> Result<(), String> {
    for allow_empty in [false, true] {
        let fixture = Fixture::new(true)?;
        let observed = fixture.observe("timeout", allow_empty)?;
        assert!(
            observed.elapsed < Duration::from_secs(5),
            "{:?}",
            observed.elapsed
        );
        assert_eq!(observed.owned_descendants_observed, 2);
        assert_eq!(observed.output.status.code(), Some(LANE_FAILURE_CODE));
        assert_eq!(observed.calls, ["kani --version".to_string()]);
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
    let cases = [
        ("nonzero", "cargo kani --version failed with exit status: 7"),
        ("signal", "cargo kani --version failed with signal: 15"),
        (
            "missing",
            "cargo could not be launched: executable or interpreter not found",
        ),
        (
            "permission",
            "cargo could not be launched: permission denied",
        ),
        (
            "missing-kani",
            "cargo is installed but the kani subcommand is unavailable",
        ),
        (
            "misleading-missing-kani",
            "cargo kani --version failed with exit status: 101",
        ),
    ];
    for (mode, reason) in cases {
        for allow_empty in [false, true] {
            let fixture = Fixture::new(true)?;
            let observed = fixture.observe(mode, allow_empty)?;
            assert_eq!(
                observed.output.status.code(),
                Some(LANE_FAILURE_CODE),
                "{mode}"
            );
            assert_eq!(
                observed.calls,
                if mode == "missing" || mode == "permission" {
                    vec![]
                } else {
                    vec!["kani --version"]
                },
                "{mode}: verification must not run after failed discovery"
            );
            assert!(
                observed.report.contains(&format!(
                    "NOT-ESTABLISHED: cargo-kani unavailable: {reason}"
                )),
                "{mode}: {}",
                observed.report
            );
            assert!(observed.report.contains("no harness was run"));
            assert!(
                !observed.report.contains("injected")
                    && !observed.report.contains("synthetic cargo-kani version"),
                "untrusted version output must not become report claims"
            );
        }
    }
    Ok(())
}

#[test]
fn kani_availability_probe_preserves_success_and_the_empty_opt_out() -> Result<(), String> {
    let fixture = Fixture::new(true)?;
    fixture.add_harness("ripr-contract", "contract_proof")?;
    let observed = fixture.observe("normal", false)?;
    assert_eq!(observed.output.status.code(), Some(0));
    assert_eq!(
        observed.calls,
        [
            "kani --version",
            "kani -p ripr -Z unstable-options --harness-timeout 1s",
            "kani -p ripr-contract -Z unstable-options --harness-timeout 1s"
        ]
    );
    assert!(
        observed
            .report
            .contains("| `proof` | `crates/ripr/src/proof.rs` | VERIFIED |")
    );
    assert!(observed.report.contains(
        "| `contract_proof` | `crates/ripr-contract/src/contract_proof.rs` | VERIFIED |"
    ));
    assert!(!observed.report.contains("cargo-kani unavailable"));

    let fixture = Fixture::new(false)?;
    let observed = fixture.observe("normal", true)?;
    assert_eq!(observed.output.status.code(), Some(0));
    assert_eq!(observed.calls, ["kani --version"]);
    assert!(!observed.report.contains("| `proof` |"));
    Ok(())
}

#[test]
fn kani_probe_watchdog_stops_a_hung_probe_and_pipe_inheriting_descendant() -> Result<(), String> {
    let fixture = Fixture::new(true)?;
    // This probe may run for ten seconds. The independent two-second driver
    // must stop it and the inherited-pipe child before the product's deadline.
    let observed = fixture.drive("timeout", false, 600, Duration::from_secs(2))?;
    assert!(observed.watchdog_timed_out);
    assert_eq!(observed.owned_descendants_observed, 2);
    assert_eq!(observed.live_descendants_before_driver_cleanup, 2);
    assert!(
        observed.elapsed < Duration::from_secs(7),
        "{:?}",
        observed.elapsed
    );
    assert!(!observed.output.status.success());
    assert_eq!(observed.calls, ["kani --version"]);
    assert!(observed.report.is_empty());
    Ok(())
}

#[test]
fn kani_empty_inventory_cannot_override_unavailable_tooling() -> Result<(), String> {
    for allow_empty in [false, true] {
        let fixture = Fixture::new(false)?;
        let observed = fixture.observe("missing", allow_empty)?;
        assert_eq!(observed.output.status.code(), Some(LANE_FAILURE_CODE));
        assert!(observed.calls.is_empty());
        assert!(observed.report.contains(
            "NOT-ESTABLISHED: cargo-kani unavailable: cargo could not be launched: executable or interpreter not found"
        ));
        assert!(observed.report.contains("no harness was run"));
        assert!(!observed.report.contains("| `proof` |"));
    }
    Ok(())
}
