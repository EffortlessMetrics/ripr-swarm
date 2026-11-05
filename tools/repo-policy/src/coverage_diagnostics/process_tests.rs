use super::*;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::process::Stdio;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self> {
        crate::test_support::scratch("coverage-storage").map(Self)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("retain coverage storage fixture: {error}");
        }
    }
}

#[test]
fn fixed_command_preserves_full_population_and_inherited_environment() {
    let command = coverage_command();
    assert_eq!(command.get_program(), "cargo");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            "llvm-cov",
            "--workspace",
            "--all-features",
            "--lcov",
            "--output-path",
            "lcov.info"
        ]
    );
    assert_eq!(command.get_envs().count(), 0);
    assert!(command.get_current_dir().is_none());
}

#[test]
fn observation_limit_preserves_output_profiles_and_native_failure() -> Result<()> {
    let fixture = Fixture::new()?;
    let stdout = fixture.0.join("stdout");
    let stderr = fixture.0.join("stderr");
    let bin = fixture.0.join("bin");
    fs::create_dir(&bin).map_err(|error| error.to_string())?;
    symlink("/bin/sh", bin.join("cargo")).map_err(|error| error.to_string())?;
    let home = std::env::var("HOME").map_err(|error| format!("inherited HOME setup: {error}"))?;
    assert!(!home.is_empty(), "nonempty inherited HOME setup");
    fs::write(fixture.0.join("expected-home"), format!("{home}\n"))
        .map_err(|error| error.to_string())?;
    // The executable is the stable system shell. Its ordinary input file is
    // named for the first fixed Cargo argument and is never made executable.
    fs::write(
        fixture.0.join("llvm-cov"),
        r#"test "$#" -eq 5 || exit 81
test "$1" = --workspace && test "$2" = --all-features || exit 82
test "$3" = --lcov && test "$4" = --output-path && test "$5" = lcov.info || exit 83
IFS= read -r expected_home < expected-home
test "$HOME" = "$expected_home" || exit 84
i=0
while [ ! -f release ] && [ "$i" -lt 100 ]; do
    i=$((i+1))
    /bin/sleep 0.01
done
test -f release || exit 99
printf '%s|%s\n' "$LLVM_PROFILE_FILE" "$CARGO_LLVM_COV"
printf 'stderr retained\n' >&2
/bin/sleep 0.05
printf 'after observation cap\n'
exit 23
"#,
    )
    .map_err(|error| error.to_string())?;
    let mut command = coverage_command();
    command
        .current_dir(&fixture.0)
        .env("PATH", &bin)
        .env("LLVM_PROFILE_FILE", "unchanged-profile-%p-%m.profraw")
        .env("CARGO_LLVM_COV", "1")
        .stdout(Stdio::from(
            File::create(&stdout).map_err(|error| error.to_string())?,
        ))
        .stderr(Stdio::from(
            File::create(&stderr).map_err(|error| error.to_string())?,
        ));
    let mut events = Vec::new();
    let mut write_error = None;
    let code = run_observed(command, Duration::from_millis(1), 1, |event| {
        if event["phase"] == "during"
            && let Err(error) = fs::write(fixture.0.join("release"), b"continue")
        {
            write_error = Some(error.to_string());
        }
        events.push(event);
    })?;
    if let Some(error) = write_error {
        return Err(error);
    }
    assert_eq!(code, 23);
    assert_eq!(
        events
            .iter()
            .map(|event| event["phase"].as_str())
            .collect::<Vec<_>>(),
        [Some("before"), Some("during"), Some("final")]
    );
    assert_eq!(events.last().ok_or("missing final receipt")?["exit"], 23);
    assert_eq!(
        fs::read_to_string(stdout).map_err(|error| error.to_string())?,
        "unchanged-profile-%p-%m.profraw|1\nafter observation cap\n"
    );
    assert_eq!(
        fs::read_to_string(stderr).map_err(|error| error.to_string())?,
        "stderr retained\n"
    );
    Ok(())
}

#[test]
fn native_success_and_signal_survive_observation() -> Result<()> {
    for (script, expected, signal) in [("exit 0", 0, None), ("kill -TERM $$", 143, Some(15))] {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        let mut events = Vec::new();
        let code = run_observed(command, Duration::from_secs(60), 0, |event| {
            events.push(event);
        })?;
        assert_eq!(code, expected);
        let final_event = events.last().ok_or("missing final receipt")?;
        assert_eq!(final_event["signal"], json!(signal));
        assert_eq!(final_event["cleanup_error"], Value::Null);
    }
    Ok(())
}

#[test]
fn child_cleanup_retains_custody_and_refuses_external_reaping() -> Result<()> {
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30"]);
    let mut child = CoverageChild::spawn(command)?;
    assert_ne!(child.pid, rustix::process::getpgrp());
    assert!(!child.exited().map_err(|error| error.to_string())?);
    let (status, error) = child.cleanup();
    assert_eq!(error, None);
    assert_eq!(status.ok_or("missing cleanup status")?.signal(), Some(9));
    assert!(child.closed);
    // A deliberately wrong external reap must disarm signalling.
    let mut command = Command::new("sh");
    command.args(["-c", "exit 0"]);
    let mut child = CoverageChild::spawn(command)?;
    assert!(
        child
            .child
            .wait()
            .map_err(|error| error.to_string())?
            .success()
    );
    assert!(child.exited().is_err());
    assert!(child.closed);
    assert_eq!(child.cleanup().1, Some("owned_child_custody_unavailable"));
    Ok(())
}

#[test]
fn dropped_owner_terminates_and_reaps_its_child() -> Result<()> {
    use rustix::process::{WaitId, WaitIdOptions, waitid};
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30"]);
    let child = CoverageChild::spawn(command)?;
    let pid = child.pid;
    drop(child);
    assert!(matches!(
        waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG
        ),
        Err(rustix::io::Errno::CHILD)
    ));
    Ok(())
}

fn process_is_running(pid: u32) -> Result<bool> {
    let read = (|| -> io::Result<String> {
        let file = File::open(format!("/proc/{pid}/status"))?;
        let mut text = String::new();
        file.take(4096).read_to_string(&mut text)?;
        Ok(text)
    })();
    process_status_is_running(read)
}

fn process_status_is_running(read: io::Result<String>) -> Result<bool> {
    let text = match read {
        Ok(text) => text,
        // Match the existing exit observer: disappearance can race either the
        // open or the read, but permission, I/O and malformed data stay errors.
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error.to_string()),
    };
    let state = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("State:")
                .and_then(|rest| rest.split_whitespace().next())
        })
        .ok_or("missing process state")?;
    Ok(!matches!(state, "Z" | "X"))
}

#[test]
fn descendant_status_accepts_only_vanished_tasks_and_preserves_other_errors() {
    for errno in [libc::ENOENT, libc::ESRCH] {
        assert_eq!(
            process_status_is_running(Err(io::Error::from_raw_os_error(errno))),
            Ok(false)
        );
    }
    for errno in [libc::EACCES, libc::EIO] {
        let error = io::Error::from_raw_os_error(errno);
        let expected = error.to_string();
        assert_eq!(process_status_is_running(Err(error)), Err(expected));
    }
    assert_eq!(
        process_status_is_running(Ok("malformed status".into())),
        Err("missing process state".into())
    );
    for (state, expected) in [("R", true), ("S", true), ("Z", false), ("X", false)] {
        assert_eq!(
            process_status_is_running(Ok(format!("Name:\tfixture\nState:\t{state}\n"))),
            Ok(expected)
        );
    }
}

#[test]
fn normal_exit_cleanup_stops_a_confirmed_separate_descendant() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            r#"/bin/sh -c '
while IFS=": " read -r field value; do
    if [ "$field" = Pid ]; then
        printf "%s\n" "$value" > "$1/descendant"
        printf "ready\n" > "$1/descendant-ready"
        break
    fi
done < /proc/self/status
i=0
while [ "$i" -lt 500 ]; do i=$((i+1)); /bin/sleep 0.01; done
' descendant "$1" &
i=0
while [ ! -f "$1/primary-exit" ] && [ "$i" -lt 300 ]; do
    i=$((i+1))
    /bin/sleep 0.01
done
test -f "$1/primary-exit" || exit 97
exit 0
"#,
            "fixture",
        ])
        .arg(&fixture.0);
    let mut descendant = None;
    let mut setup_error = None;
    let code = run_observed(command, Duration::from_millis(1), 200, |event| {
        if event["phase"] != "during" || descendant.is_some() || setup_error.is_some() {
            return;
        }
        if !fixture.0.join("descendant-ready").is_file() {
            return;
        }
        let text = match fs::read_to_string(fixture.0.join("descendant")) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                setup_error = Some(error.to_string());
                return;
            }
        };
        let result = (|| -> Result<u32> {
            let pid = text
                .trim()
                .parse::<u32>()
                .map_err(|error| error.to_string())?;
            if pid == 0 || !process_is_running(pid)? {
                return Err("descendant was not alive at setup".into());
            }
            fs::write(fixture.0.join("primary-exit"), b"exit")
                .map_err(|error| error.to_string())?;
            Ok(pid)
        })();
        match result {
            Ok(pid) => descendant = Some(pid),
            Err(error) => setup_error = Some(error),
        }
    })?;
    if let Some(error) = setup_error {
        return Err(error);
    }
    assert_eq!(code, 0);
    let pid = descendant.ok_or("no confirmed descendant")?;
    let started = Instant::now();
    while process_is_running(pid)? && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_is_running(pid)?,
        "owned descendant remained after normal exit cleanup"
    );
    Ok(())
}
