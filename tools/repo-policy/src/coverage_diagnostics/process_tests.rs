use super::*;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
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

const OBSERVATION_TEST: &str = concat!(
    "coverage_diagnostics::process::tests::",
    "observation_limit_preserves_output_profiles_and_native_failure"
);
const OBSERVATION_CHILD_ENV: &str = "RIPR_COVERAGE_OBSERVATION_CHILD";
const OBSERVATION_ROOT_ENV: &str = "RIPR_COVERAGE_OBSERVATION_ROOT";
const OBSERVATION_SENTINEL_ENV: &str = "RIPR_COVERAGE_OBSERVATION_SENTINEL";
const OBSERVATION_OUTPUT_BYTES: u64 = 16 * 1024;

fn read_observation_child_capture(path: &std::path::Path) -> Result<String> {
    let mut text = String::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(OBSERVATION_OUTPUT_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() as u64 > OBSERVATION_OUTPUT_BYTES {
        return Err("observation child exceeded its output budget".into());
    }
    Ok(text)
}

fn run_observation_exact_child() -> Result<()> {
    let root = crate::test_support::scratch("coverage-observation-child")?;
    // No automatic deletion: any error or panic retains the child's witnesses.
    let result = (|| -> Result<()> {
        fs::write(
            root.join("expected-inherited"),
            b"owned-observation-sentinel\n",
        )
        .map_err(|error| error.to_string())?;
        let stdout = root.join("child-stdout");
        let stderr = root.join("child-stderr");
        let capture = |path: &std::path::Path| {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|error| error.to_string())
        };
        let mut command = Command::new(std::env::current_exe().map_err(|error| error.to_string())?);
        command
            .args(["--exact", OBSERVATION_TEST, "--nocapture"])
            .env(OBSERVATION_CHILD_ENV, OBSERVATION_TEST)
            .env(OBSERVATION_ROOT_ENV, &root)
            .env(OBSERVATION_SENTINEL_ENV, "owned-observation-sentinel")
            .env_remove("HOME")
            .stdin(Stdio::null())
            .stdout(capture(&stdout)?)
            .stderr(capture(&stderr)?);
        let mut child = CoverageChild::spawn(command)?;
        let started = Instant::now();
        loop {
            let exited = child.exited().map_err(|error| error.to_string())?;
            let oversized = [&stdout, &stderr].iter().try_fold(false, |large, path| {
                fs::metadata(path)
                    .map(|metadata| large || metadata.len() > OBSERVATION_OUTPUT_BYTES)
                    .map_err(|error| error.to_string())
            })?;
            if oversized || started.elapsed() >= Duration::from_secs(30) {
                let (status, cleanup) = child.cleanup();
                return Err(format!(
                    "observation child exceeded output/time bound: {status:?}; {cleanup:?}"
                ));
            }
            if exited {
                let (status, cleanup) = child.cleanup();
                if let Some(error) = cleanup {
                    return Err(format!("observation child cleanup: {error}"));
                }
                let status = status.ok_or("observation child status unavailable")?;
                let stdout = read_observation_child_capture(&stdout)?;
                let stderr = read_observation_child_capture(&stderr)?;
                if !status.success() {
                    return Err(format!(
                        "observation child: {status}; stdout: {stdout}; stderr: {stderr}"
                    ));
                }
                let selected: Vec<_> = stdout
                    .lines()
                    .filter(|line| line.starts_with("running "))
                    .collect();
                let summaries: Vec<_> = stdout
                    .lines()
                    .filter(|line| line.starts_with("test result: "))
                    .collect();
                let passed = format!("test {OBSERVATION_TEST} ... ok");
                if selected != ["running 1 test"]
                    || summaries.len() != 1
                    || !summaries[0]
                        .starts_with("test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; ")
                    || !stdout.lines().any(|line| line == passed)
                {
                    return Err("observation child did not finish exactly one selected test".into());
                }
                if read_observation_child_capture(&root.join("ack"))? != OBSERVATION_TEST {
                    return Err("observation child acknowledgement has the wrong identity".into());
                }
                return Ok(());
            }
            // The inner shell retains its finite release loop and fixed sleeps.
            // Its separate process group is not covered by this parent's cleanup.
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    match result {
        Ok(()) => fs::remove_dir_all(&root).map_err(|error| {
            format!(
                "retain observation child fixture {}: {error}",
                root.display()
            )
        }),
        Err(error) => {
            eprintln!("retain observation child fixture: {}", root.display());
            Err(error)
        }
    }
}

#[test]
fn observation_limit_preserves_output_profiles_and_native_failure() -> Result<()> {
    if std::env::var_os(OBSERVATION_CHILD_ENV).is_none() {
        return run_observation_exact_child();
    }
    if std::env::var_os(OBSERVATION_CHILD_ENV).as_deref()
        != Some(std::ffi::OsStr::new(OBSERVATION_TEST))
    {
        return Err("observation child selector does not match this test".into());
    }
    if std::env::var_os("HOME").is_some() {
        return Err("observation child must exercise an absent HOME".into());
    }
    let inherited_root = PathBuf::from(
        std::env::var_os(OBSERVATION_ROOT_ENV).ok_or("observation child root is missing")?,
    );
    // All inner files share the parent's retain-on-error lifetime.
    let observed_root = inherited_root.join("observed");
    fs::create_dir(&observed_root).map_err(|error| error.to_string())?;
    let stdout = observed_root.join("stdout");
    let stderr = observed_root.join("stderr");
    let bin = observed_root.join("bin");
    fs::create_dir(&bin).map_err(|error| error.to_string())?;
    symlink("/bin/sh", bin.join("cargo")).map_err(|error| error.to_string())?;
    fs::copy(
        inherited_root.join("expected-inherited"),
        observed_root.join("expected-inherited"),
    )
    .map_err(|error| error.to_string())?;
    // The executable is the stable system shell. Its ordinary input file is
    // named for the first fixed Cargo argument and is never made executable.
    fs::write(
        observed_root.join("llvm-cov"),
        r#"test "$#" -eq 5 || exit 81
test "$1" = --workspace && test "$2" = --all-features || exit 82
test "$3" = --lcov && test "$4" = --output-path && test "$5" = lcov.info || exit 83
IFS= read -r expected_inherited < expected-inherited
test "${RIPR_COVERAGE_OBSERVATION_SENTINEL-}" = "$expected_inherited" || exit 84
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
        .current_dir(&observed_root)
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
            && let Err(error) = fs::write(observed_root.join("release"), b"continue")
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
    let final_event = events.last().ok_or("missing final receipt")?;
    assert_eq!(final_event["exit"], 23);
    assert_eq!(final_event.get("cleanup_error"), Some(&Value::Null));
    assert_eq!(
        fs::read_to_string(stdout).map_err(|error| error.to_string())?,
        "unchanged-profile-%p-%m.profraw|1\nafter observation cap\n"
    );
    assert_eq!(
        fs::read_to_string(stderr).map_err(|error| error.to_string())?,
        "stderr retained\n"
    );
    // The acknowledgement is published only after the original oracle passes.
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(inherited_root.join("ack"))
        .map_err(|error| error.to_string())?
        .write_all(OBSERVATION_TEST.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[test]
fn native_success_and_signal_survive_observation() -> Result<()> {
    for (script, expected, signal) in [("exit 0", 0, None), ("kill -TERM $$", 143, Some(15))] {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        let mut events = Vec::new();
        let code = run_observed(command, Duration::from_mins(1), 0, |event| {
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
    let pgid = rustix::process::getpgid(Some(child.pid)).map_err(|error| error.to_string())?;
    assert_eq!(pgid, child.pid);
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
    if child.exited().is_ok() {
        return Err("externally reaped child retained status custody".into());
    }
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
