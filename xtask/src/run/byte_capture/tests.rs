//! Real children exercise the actual capture owner; no replay DTO creates an
//! execution observation. Fixture fallback cleanup uses a revalidated pidfd.

use super::*;
use crate::run::test_cleanup::finish_recorded_member;
use crate::run::test_session::custody::read_member;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|error| error.to_string())?;
        let root = temporary.join(format!(
            "ripr-observed-capture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        Ok(Self(root))
    }

    fn descendant_stat(&self) -> Result<String, String> {
        fs::read_to_string(self.0.join("descendant.stat"))
            .map_err(|error| format!("fixture descendant did not record its identity: {error}"))
    }

    fn descendant_live(&self) -> Result<bool, String> {
        let stat = self.descendant_stat()?;
        let pid = stat
            .split_whitespace()
            .next()
            .ok_or("missing fixture pid")?
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        Ok(read_member(pid)?.is_some_and(|member| member.state != "Z" && member.state != "X"))
    }

    fn cleanup_descendant(&self) -> Result<(), String> {
        finish_recorded_member(&self.descendant_stat()?)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Ok(stat) = fs::read_to_string(self.0.join("descendant.stat")) {
            let _ = finish_recorded_member(&stat);
        }
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn budget() -> ByteCaptureBudget {
    ByteCaptureBudget {
        timeout: Duration::from_secs(10),
        stdout_bytes: 1024,
        stderr_bytes: 1024,
    }
}

fn observe(
    root: &Fixture,
    script: &str,
    argument: &str,
    limits: ByteCaptureBudget,
) -> Result<ObservedByteCapture, TimedCaptureError> {
    crate::run::capture_bytes_in_dir_with_env_budget_observed(
        Path::new("/bin/sh"),
        &[
            "-c".to_string(),
            script.to_string(),
            "capture-fixture".to_string(),
            argument.to_string(),
        ],
        &root.0,
        (&[], &[]),
        limits,
        "observed byte fixture",
    )
}

fn refusal<T>(
    result: Result<T, TimedCaptureError>,
    kind: TimedCaptureErrorKind,
    expected: &str,
) -> Result<(), String> {
    match result {
        Err(error) if error.kind == kind && error.message.contains(expected) => Ok(()),
        Err(error) => Err(format!("wrong capture refusal: {error:?}")),
        Ok(_) => Err(format!("capture accepted input that requires {expected:?}")),
    }
}

#[test]
fn actual_request_preserves_arguments_cwd_and_effective_explicit_environment() -> Result<(), String>
{
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = Fixture::new()?;
    let script = "printf '%s\\n' \"$1\" \"$RIPR_F0_VALUE\" \"${RIPR_F0_REMOVED+present}\" \"$RIPR_F0_RESET\"";
    let arguments = vec![
        "-c".into(),
        script.into(),
        "literal argv zero".into(),
        "argument with spaces".into(),
    ];
    let overrides = [
        ("RIPR_F0_VALUE", "superseded"),
        ("RIPR_F0_VALUE", "effective"),
        ("RIPR_F0_RESET", "set after removal"),
    ];
    let observed = crate::run::capture_bytes_in_dir_with_env_budget_observed(
        Path::new("/bin/sh"),
        &arguments,
        &root.0,
        (&overrides, &["RIPR_F0_REMOVED", "RIPR_F0_RESET"]),
        budget(),
        "actual request identity",
    )
    .map_err(TimedCaptureError::into_message)?;
    observed.consume(|request, output, limits, cleanup, elapsed| {
        assert_eq!(request.program, Path::new("/bin/sh"));
        assert_eq!(
            request.arguments,
            arguments.iter().map(OsString::from).collect::<Vec<_>>()
        );
        assert_eq!(request.cwd, root.0);
        let environment: std::collections::BTreeMap<_, _> =
            request.explicit_environment.into_iter().collect();
        assert_eq!(environment.len(), 3);
        assert_eq!(
            environment.get(&OsString::from("RIPR_F0_VALUE")),
            Some(&Some(OsString::from("effective")))
        );
        assert_eq!(
            environment.get(&OsString::from("RIPR_F0_REMOVED")),
            Some(&None)
        );
        assert_eq!(
            environment.get(&OsString::from("RIPR_F0_RESET")),
            Some(&Some(OsString::from("set after removal")))
        );
        assert_eq!(
            output.stdout,
            b"argument with spaces\neffective\n\nset after removal\n"
        );
        assert_eq!(output.stderr, b"");
        assert_eq!(output.status.and_then(|status| status.code()), Some(0));
        assert!(!output.timed_out);
        assert_eq!(limits.timeout, budget().timeout);
        assert_eq!(limits.stdout_bytes, budget().stdout_bytes);
        assert_eq!(limits.stderr_bytes, budget().stderr_bytes);
        assert_eq!(cleanup, ByteCaptureCleanup::LinuxGroupObservedEmpty);
        assert!(elapsed >= output.duration);
        Ok(())
    })
}

#[test]
fn actual_nonzero_exit_keeps_original_non_utf8_bytes_and_native_status() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = Fixture::new()?;
    observe(
        &root,
        r"printf '\000\200ok'; printf '\377bad' >&2; exit 23",
        "",
        budget(),
    )
    .map_err(TimedCaptureError::into_message)?
    .consume(|_, output, _, _, _| {
        assert_eq!(output.stdout, b"\0\x80ok");
        assert_eq!(output.stderr, b"\xffbad");
        assert_eq!(output.status.and_then(|status| status.code()), Some(23));
        assert!(!output.timed_out);
        Ok(())
    })
}

// The descendant publishes a start/session identity before the primary exits.
// It holds stdout, while stderr closes normally. The escape mode is deliberate
// and remains outside process-group containment; it must not create complete EOF.
const TREE: &str = r#"
if [ "$1" = escaped ]; then
    setsid sh -c 'cat /proc/$$/stat > descendant.stat; exec sleep 30' 2>/dev/null &
else
    sh -c 'cat /proc/$$/stat > descendant.stat; exec sleep 30' 2>/dev/null &
fi
attempt=0
while [ ! -s descendant.stat ]; do
    attempt=$((attempt + 1))
    if [ "$attempt" -gt 300 ]; then exit 99; fi
    sleep 0.01
done
printf 'complete stdout'
printf 'complete stderr' >&2
if [ "$1" = timeout ]; then wait; fi
"#;

fn sibling() -> Result<OwnedProcess, String> {
    let mut command = Command::new("sleep");
    command
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    OwnedProcess::spawn_isolated(command).map_err(|error| error.to_string())
}

#[test]
fn primary_exit_settles_pipe_descendant_without_touching_unrelated_sibling() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = Fixture::new()?;
    let mut unrelated = sibling()?;
    let captured = observe(&root, TREE, "normal", budget());
    let descendant_live = root.descendant_live()?;
    let sibling_live = unrelated
        .try_wait()
        .map_err(|error| error.to_string())?
        .is_none();
    root.cleanup_descendant()?;
    unrelated.terminate_tree()?;
    if descendant_live || !sibling_live {
        return Err(format!(
            "wrong cleanup scope: descendant_live={descendant_live}, sibling_live={sibling_live}"
        ));
    }
    captured
        .map_err(TimedCaptureError::into_message)?
        .consume(|_, output, _, cleanup, elapsed| {
            assert_eq!(output.stdout, b"complete stdout");
            assert_eq!(output.stderr, b"complete stderr");
            assert_eq!(output.status.and_then(|status| status.code()), Some(0));
            assert!(!output.timed_out);
            assert_eq!(cleanup, ByteCaptureCleanup::LinuxGroupObservedEmpty);
            if elapsed >= Duration::from_secs(10) {
                return Err(format!(
                    "normal capture waited for its leftover pipe: {elapsed:?}"
                ));
            }
            Ok(())
        })
}

#[test]
fn enforced_timeout_retains_status_and_settles_contained_descendant() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = Fixture::new()?;
    let limits = ByteCaptureBudget {
        timeout: Duration::from_secs(2),
        ..budget()
    };
    let captured = observe(&root, TREE, "timeout", limits);
    let descendant_live = root.descendant_live()?;
    root.cleanup_descendant()?;
    if descendant_live {
        return Err("timeout returned with its contained descendant still executing".into());
    }
    captured.map_err(TimedCaptureError::into_message)?.consume(
        |_, output, observed_limits, cleanup, elapsed| {
            assert!(output.timed_out);
            if output.status.is_none_or(|status| status.success()) {
                return Err("enforced timeout lost its native terminated status".into());
            }
            assert_eq!(output.stdout, b"complete stdout");
            assert_eq!(observed_limits.timeout, limits.timeout);
            assert_eq!(cleanup, ByteCaptureCleanup::LinuxGroupObservedEmpty);
            if elapsed >= Duration::from_secs(10) {
                return Err(format!(
                    "timeout escaped its capture/cleanup bound: {elapsed:?}"
                ));
            }
            Ok(())
        },
    )
}

#[test]
fn escaped_pipe_cannot_turn_zero_primary_exit_into_complete_observation() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = Fixture::new()?;
    let captured = observe(&root, TREE, "escaped", budget());
    let was_outside_containment = root.descendant_live()?;
    root.cleanup_descendant()?;
    if !was_outside_containment {
        return Err("escaped descendant did not exercise incomplete stdout".into());
    }
    refusal(
        captured,
        TimedCaptureErrorKind::Capture,
        "byte output is not established",
    )
}

#[test]
fn stream_overflow_withholds_observation_even_after_zero_exit() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    for (script, stream) in [("printf 12345", "stdout"), ("printf 12345 >&2", "stderr")] {
        let root = Fixture::new()?;
        let limits = ByteCaptureBudget {
            stdout_bytes: 4,
            stderr_bytes: 4,
            ..budget()
        };
        refusal(
            observe(&root, script, "", limits),
            TimedCaptureErrorKind::Capture,
            &format!("{stream} exceeds its 4-byte output budget"),
        )?;
    }
    Ok(())
}

#[test]
fn invalid_deadline_refuses_before_spawn_and_missing_program_retains_launch_kind()
-> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = Fixture::new()?;
    refusal(
        observe(
            &root,
            "touch unexpected-launch",
            "",
            ByteCaptureBudget {
                timeout: Duration::ZERO,
                ..budget()
            },
        ),
        TimedCaptureErrorKind::ProcessControl,
        "nonzero deadline",
    )?;
    if root.0.join("unexpected-launch").exists() {
        return Err("invalid deadline still launched the command".into());
    }
    refusal(
        crate::run::capture_bytes_in_dir_with_env_budget_observed(
            &root.0.join("absent-program"),
            &[],
            &root.0,
            (&[], &[]),
            budget(),
            "missing capture program",
        ),
        TimedCaptureErrorKind::Launch(std::io::ErrorKind::NotFound),
        "failed to run missing capture program",
    )
}

#[test]
fn escaped_file_capture_refuses_publication_and_preserves_prior_artifact() -> Result<(), String> {
    let _cwd = crate::acquire_test_cwd_read_guard();
    let root = Fixture::new()?;
    let destination = root.0.join("artifact.json");
    fs::write(&destination, b"prior-good-artifact").map_err(|error| error.to_string())?;
    let root_text = root.0.to_str().ok_or("fixture root is not UTF-8")?;
    let script = format!("cd \"$RIPR_F0_ROOT\" || exit 98\n{TREE}");
    let captured = crate::run::capture_stdout_to_file_with_timeout(
        "/bin/sh",
        &["-c".into(), script, "file-fixture".into(), "escaped".into()],
        &[("RIPR_F0_ROOT", root_text)],
        &destination,
        budget().timeout,
        "escaped file fixture",
    );
    let prior = fs::read(&destination).map_err(|error| error.to_string())?;
    root.cleanup_descendant()?;
    assert_eq!(prior, b"prior-good-artifact");
    match captured {
        Err(error)
            if error.contains("stdout file capture incomplete")
                && error.contains("staging retained") =>
        {
            Ok(())
        }
        Err(error) => Err(format!("wrong incomplete-file refusal: {error}")),
        Ok(output) => Err(format!(
            "escaped pipe published {} bytes",
            output.stdout_bytes
        )),
    }
}
