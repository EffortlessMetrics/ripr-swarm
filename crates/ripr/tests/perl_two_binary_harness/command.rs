//! Test-local capture for the Perl producer/consumer journey (#8238).
//! Files avoid an unbounded pipe drain after the primary exits. The existing
//! OwnedProcess owns Windows containment; Unix commands get their own group.

use ripr::process_owner::OwnedProcess;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[cfg(unix)]
const CLEANUP_BUDGET: Duration = Duration::from_secs(5);
const CAPTURE_LIMIT: u64 = 4 * 1024 * 1024;

struct CaptureDir {
    path: PathBuf,
    retain: bool,
}

impl CaptureDir {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
        loop {
            let path = parent.join(format!(
                "ripr-perl-command-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        retain: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("create {}: {error}", path.display())),
            }
        }
    }

    fn stream(&self, name: &str) -> Result<Stdio, String> {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.path.join(name))
            .map(Stdio::from)
            .map_err(|error| format!("create {name} capture: {error}"))
    }
}

impl Drop for CaptureDir {
    fn drop(&mut self) {
        if self.retain {
            eprintln!(
                "retain command capture after incomplete cleanup: {}",
                self.path.display()
            );
            return;
        }
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            eprintln!("remove command capture {}: {error}", self.path.display());
        }
    }
}

fn read_capture(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(CAPTURE_LIMIT + 1).read_to_end(&mut bytes))
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    if bytes.len() as u64 > CAPTURE_LIMIT {
        return Err(format!(
            "{} exceeds {CAPTURE_LIMIT}-byte capture limit",
            path.display()
        ));
    }
    Ok(bytes)
}

fn wait(child: &mut OwnedProcess, budget: Duration) -> Result<std::process::ExitStatus, String> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if start.elapsed() < budget => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => return Err(format!("command exceeded {budget:?} deadline")),
            Err(error) => return Err(format!("observe command: {error}")),
        }
    }
}

#[cfg(unix)]
fn kill_command(target: &str) -> Command {
    let mut command = Command::new("/bin/kill");
    command.args(["-KILL", "--", target]).env("LC_ALL", "C");
    command
}

#[cfg(unix)]
fn terminate_group(pid: u32, capture: &CaptureDir) -> Result<(), String> {
    let mut command = kill_command(&format!("-{pid}"));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(capture.stream("cleanup-stderr")?);
    let mut kill =
        OwnedProcess::spawn(command).map_err(|error| format!("spawn group cleanup: {error}"))?;
    let result = wait(&mut kill, CLEANUP_BUDGET);
    let reap = kill.terminate_tree();
    reap?;
    let status = result?;
    let stderr = read_capture(&capture.path.join("cleanup-stderr"))?;
    // An already-finished group has nothing left to terminate. Other failures
    // remain failures, including a missing utility or permission refusal.
    if status.success() || String::from_utf8_lossy(&stderr).contains("No such process") {
        Ok(())
    } else {
        Err(format!(
            "group cleanup failed ({status}): {}",
            String::from_utf8_lossy(&stderr)
        ))
    }
}

pub(super) fn capture(mut command: Command, budget: Duration) -> Result<Output, String> {
    if budget.is_zero() {
        return Err("command given zero deadline; not spawned".to_string());
    }
    let description = format!("{:?}", command.get_program());
    let mut capture = CaptureDir::new()?;
    command
        .stdin(Stdio::null())
        .stdout(capture.stream("stdout")?)
        .stderr(capture.stream("stderr")?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child =
        OwnedProcess::spawn(command).map_err(|error| format!("spawn {description}: {error}"))?;
    capture.retain = true;
    let observed = wait(&mut child, budget);
    // Always clean containment, including successful primary exit with a live
    // descendant. Attempt primary reaping even when group termination fails.
    #[cfg(unix)]
    let group_cleanup = terminate_group(child.id(), &capture);
    #[cfg(not(unix))]
    let group_cleanup: Result<(), String> = Ok(());
    let primary_cleanup = child.terminate_tree();
    drop(child);
    group_cleanup.map_err(|error| {
        format!(
            "{description}: {error}; observed {observed:?}; primary cleanup {primary_cleanup:?}"
        )
    })?;
    primary_cleanup.map_err(|error| format!("{description}: {error}; observed {observed:?}"))?;
    capture.retain = false;
    let status = observed.map_err(|error| format!("{description}: {error}"))?;
    Ok(Output {
        status,
        stdout: read_capture(&capture.path.join("stdout"))?,
        stderr: read_capture(&capture.path.join("stderr"))?,
    })
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn shell(script: &str, root: &Path) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]).current_dir(root);
        command
    }

    fn state(pid: u32) -> Result<Option<char>, String> {
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(text) => text
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.trim().chars().next())
                .map(Some)
                .ok_or_else(|| format!("invalid process state for {pid}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("read owned PID {pid}: {error}")),
        }
    }

    fn stopped_state(pid: u32) -> Result<Option<char>, String> {
        let observation = Instant::now();
        loop {
            let current = state(pid)?;
            if matches!(current, None | Some('Z'))
                || observation.elapsed() >= Duration::from_secs(2)
            {
                return Ok(current);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn capture_preserves_success_nonzero_and_both_streams() -> Result<(), String> {
        let root = CaptureDir::new()?;
        for code in [0, 17] {
            let output = capture(
                shell(
                    &format!("printf stdout; printf stderr >&2; exit {code}"),
                    &root.path,
                ),
                Duration::from_secs(2),
            )?;
            assert_eq!(output.status.code(), Some(code));
            assert_eq!(output.stdout, b"stdout");
            assert_eq!(output.stderr, b"stderr");
        }
        Ok(())
    }

    #[test]
    fn zero_deadline_does_not_spawn() -> Result<(), String> {
        let root = CaptureDir::new()?;
        let error = capture(shell("printf yes > spawned", &root.path), Duration::ZERO)
            .err()
            .ok_or("zero deadline accepted")?;
        assert!(error.contains("not spawned"), "{error}");
        assert!(!root.path.join("spawned").exists());
        Ok(())
    }

    #[test]
    fn oversized_capture_is_refused_instead_of_truncated() -> Result<(), String> {
        let root = CaptureDir::new()?;
        let error = capture(
            shell("dd if=/dev/zero bs=1048576 count=5 2>/dev/null", &root.path),
            Duration::from_secs(2),
        )
        .err()
        .ok_or("oversized capture accepted")?;
        assert!(error.contains("capture limit"), "{error}");
        Ok(())
    }

    fn descendant_control(primary_exits: bool) -> Result<(), String> {
        let root = CaptureDir::new()?;
        let tail = if primary_exits { "exit 0" } else { "wait" };
        let script = format!(
            "echo $$ > primary.pid; (trap '' TERM; echo ready > descendant.ready; exec sleep 20) & echo $! > descendant.pid; while [ ! -f descendant.ready ]; do :; done; {tail}"
        );
        let start = Instant::now();
        let output = capture(shell(&script, &root.path), Duration::from_secs(2));
        let primary: u32 = std::fs::read_to_string(root.path.join("primary.pid"))
            .map_err(|error| error.to_string())?
            .trim()
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        let descendant: u32 = std::fs::read_to_string(root.path.join("descendant.pid"))
            .map_err(|error| error.to_string())?
            .trim()
            .parse::<u32>()
            .map_err(|error| error.to_string())?;
        let primary_state = state(primary)?;
        let descendant_state = stopped_state(descendant)?;
        // A broken cleanup experiment must still stop only its owned fixture.
        if descendant_state.is_some_and(|state| state != 'Z') {
            let _ = capture(
                kill_command(&descendant.to_string()),
                Duration::from_secs(2),
            );
        }
        assert_eq!(primary_state, None, "primary was not reaped");
        assert!(
            matches!(descendant_state, None | Some('Z')),
            "descendant {descendant} remains {descendant_state:?}"
        );
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "capture did not honor its bound"
        );
        if primary_exits {
            assert!(output?.status.success());
        } else {
            assert!(
                output
                    .err()
                    .ok_or("stalled command succeeded")?
                    .contains("deadline")
            );
        }
        Ok(())
    }

    #[test]
    fn deadline_reaps_primary_and_terminates_term_resistant_descendant() -> Result<(), String> {
        descendant_control(false)
    }

    #[test]
    fn primary_exit_does_not_wait_for_inherited_pipe_holder() -> Result<(), String> {
        descendant_control(true)
    }

    #[test]
    fn producer_path_preserves_argv_packet_check_and_cleans_descendant() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;

        let root = CaptureDir::new()?;
        let exporter = root.path.join("exporter");
        std::fs::write(&exporter, "#!/bin/sh\necho $$ > primary.pid\nprintf '%s\\n' \"$@\" > args\nfor out do :; done\nprintf '{}' > \"$out\"\n(trap '' TERM; echo ready > descendant.ready; exec sleep 20) &\necho $! > descendant.pid\nwhile [ ! -f descendant.ready ]; do :; done\nexit 0\n").map_err(|error| error.to_string())?;
        std::fs::set_permissions(&exporter, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        let start = Instant::now();
        let packet = crate::run_producer(
            exporter.to_str().ok_or("non-UTF-8 exporter path")?,
            &root.path,
            "diff.patch",
            "facts.json",
        )?;
        for name in ["primary", "descendant"] {
            let pid = std::fs::read_to_string(root.path.join(format!("{name}.pid")))
                .map_err(|error| error.to_string())?
                .trim()
                .parse::<u32>()
                .map_err(|error| error.to_string())?;
            let observed = stopped_state(pid)?;
            if observed.is_some_and(|state| state != 'Z') {
                let _ = capture(kill_command(&pid.to_string()), Duration::from_secs(2));
            }
            if name == "primary" {
                assert_eq!(observed, None, "producer primary was not reaped");
            } else {
                assert!(
                    matches!(observed, None | Some('Z')),
                    "producer descendant {pid} remains {observed:?}"
                );
            }
        }
        assert_eq!(packet, root.path.join("facts.json"));
        assert_eq!(
            std::fs::read(packet).map_err(|error| error.to_string())?,
            b"{}"
        );
        assert_eq!(
            std::fs::read_to_string(root.path.join("args")).map_err(|error| error.to_string())?,
            "ripr-facts\n--schema\nripr-perl-facts-v1\n--root\n.\n--base\norigin/main\n--head\nHEAD\n--fact-classes\nowners,changes,tests,oracles\n--diff\ndiff.patch\n--out\nfacts.json\n"
        );
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "producer waited for an inherited pipe holder"
        );
        std::fs::write(&exporter, "#!/bin/sh\nexit 0\n").map_err(|error| error.to_string())?;
        let error = crate::run_producer(
            exporter.to_str().ok_or("non-UTF-8 exporter path")?,
            &root.path,
            "diff.patch",
            "missing.json",
        )
        .err()
        .ok_or("missing packet accepted")?;
        assert!(error.contains("packet not found"), "{error}");
        Ok(())
    }
}
