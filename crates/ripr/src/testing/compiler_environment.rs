//! Exact unit-test children for compiler-admission fixtures (#13285).
//!
//! Re-enter the current, already instrumented executable with a plain compiler
//! environment. Profiling variables and the parent environment remain intact.

use std::ffi::OsStr;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const CHILD_TEST: &str = "RIPR_COMPILER_FIXTURE_CHILD_TEST";
const CHILD_ACK: &str = "RIPR_COMPILER_FIXTURE_CHILD_ACK";
const UNTRUSTED_WRAPPER: &str = "ripr-untrusted-compiler-wrapper-13285";
static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn plain(
    module: &str,
    test: &str,
    assertions: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    run(module, test, None, assertions)
}

/// Establish the same fixture's plain positive before injecting one wrapper.
pub(crate) fn untrusted_wrapper(
    module: &str,
    test: &str,
    plain_control: &str,
    assertions: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    run(module, test, Some(plain_control), assertions)
}

fn run(
    module: &str,
    test: &str,
    plain_control: Option<&str>,
    assertions: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let selector = exact_selector(module, test)?;
    if let Some(selected) = std::env::var_os(CHILD_TEST) {
        if selected.as_os_str() != OsStr::new(&selector) {
            return Err(format!(
                "compiler fixture child selected the wrong test for {selector}"
            ));
        }
        let acknowledgement = std::env::var_os(CHILD_ACK)
            .ok_or_else(|| "compiler fixture child has no acknowledgement path".to_string())?;
        assertions()?;
        // A marker is published only after the real fixture assertions succeed.
        // Exclusive creation also refuses a second execution of the same body.
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(&acknowledgement))
            .map_err(|error| format!("create compiler fixture acknowledgement: {error}"))?;
        return file
            .write_all(selector.as_bytes())
            .map_err(|error| format!("write compiler fixture acknowledgement: {error}"));
    }
    if let Some(control) = plain_control {
        run_child(&exact_selector(module, control)?, false)?;
    }
    run_child(&selector, plain_control.is_some())
}

fn exact_selector(module: &str, test: &str) -> Result<String, String> {
    let qualified = format!("{module}::{test}");
    let (_, selector) = qualified
        .split_once("::")
        .ok_or_else(|| "compiler fixture selector has no crate prefix".to_string())?;
    Ok(selector.to_string())
}

fn compiler_admission_variable(key: &OsStr) -> bool {
    let key = key.to_string_lossy().to_ascii_uppercase();
    if matches!(
        key.as_str(),
        "RUSTFLAGS"
            | "CARGO_ENCODED_RUSTFLAGS"
            | "RUSTC"
            | "RUSTC_WRAPPER"
            | "RUSTC_WORKSPACE_WRAPPER"
    ) {
        return true;
    }
    key.starts_with("CARGO_") && (key.ends_with("RUSTFLAGS") || key.contains("RUSTC"))
}

fn run_child(selector: &str, untrusted_wrapper: bool) -> Result<(), String> {
    let mut scratch = Scratch::new()?;
    let acknowledgement = scratch.path.join("assertions-completed");
    let child_stdout = scratch.path.join("child-stdout.log");
    let child_stderr = scratch.path.join("child-stderr.log");
    let executable =
        std::env::current_exe().map_err(|error| format!("locate test executable: {error}"))?;
    let mut command = std::process::Command::new(executable);
    command
        .args(["--exact", selector, "--nocapture"])
        .stdin(std::process::Stdio::null())
        .env(CHILD_TEST, selector)
        .env(CHILD_ACK, &acknowledgement);
    // Only the fixture's compiler-admission variables are removed. In
    // particular, LLVM_PROFILE_FILE and cargo-llvm-cov's profile state are
    // inherited by this same compiled test image; no parent variable is edited.
    for (key, _) in std::env::vars_os() {
        if compiler_admission_variable(&key) {
            command.env_remove(key);
        }
    }
    if untrusted_wrapper {
        command.env("RUSTC_WRAPPER", UNTRUSTED_WRAPPER);
    }
    // The child is a full libtest harness: inherited stdout would print its
    // own `running 1 test` / `test ... ok` / `test result:` rows into the
    // parent suite's log, where the fail-closed windows-advisory summarizer
    // cannot attribute them (nightly 42164175174: 14637 provenance errors,
    // run 1 and run 2 incomplete_evidence, #9272). Files cannot fill, so the
    // parent's plain `try_wait` loop keeps the no-pipe-to-fill property; the
    // scratch is retained on failure with the paths in the error.
    let stdout_file = std::fs::File::create(&child_stdout)
        .map_err(|error| format!("create compiler fixture stdout capture: {error}"))?;
    let stderr_file = std::fs::File::create(&child_stderr)
        .map_err(|error| format!("create compiler fixture stderr capture: {error}"))?;
    command
        .stdout(std::process::Stdio::from(stdout_file))
        .stderr(std::process::Stdio::from(stderr_file));
    let mut child = crate::process_owner::OwnedProcess::spawn_isolated(command)
        .map_err(|error| format!("spawn compiler fixture {selector}: {error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    scratch.retain = true;
                    return Err(format!(
                        "compiler fixture {selector} failed: {status}; child stdout: {}; child stderr: {}; captured stdout:\n{}; captured stderr:\n{}",
                        child_stdout.display(),
                        child_stderr.display(),
                        capture_text(&child_stdout),
                        capture_text(&child_stderr)
                    ));
                }
                if let Err(error) = verify_acknowledgement(&acknowledgement, selector) {
                    scratch.retain = true;
                    return Err(format!(
                        "{error}; child stdout: {}; child stderr: {}; captured stdout:\n{}; captured stderr:\n{}",
                        child_stdout.display(),
                        child_stderr.display(),
                        capture_text(&child_stdout),
                        capture_text(&child_stderr)
                    ));
                }
                return Ok(());
            }
            Ok(None) => {}
            Err(error) => {
                if let Err(cleanup) = child.terminate_tree() {
                    scratch.retain = true;
                    return Err(format!(
                        "observe compiler fixture {selector}: {error}; cleanup failed: {cleanup}; retained {}",
                        scratch.path.display()
                    ));
                }
                return Err(format!("observe compiler fixture {selector}: {error}"));
            }
        }
        if started.elapsed() >= Duration::from_secs(30) {
            if let Err(error) = child.terminate_tree() {
                scratch.retain = true;
                return Err(format!(
                    "compiler fixture {selector} exceeded 30 seconds; cleanup failed: {error}; retained {}",
                    scratch.path.display()
                ));
            }
            return Err(format!("compiler fixture {selector} exceeded 30 seconds"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn capture_text(path: &Path) -> String {
    const CAPTURED_TAIL: usize = 4000;
    match std::fs::read(path) {
        // Byte-slice before the lossy decode: the tail boundary may split a
        // multi-byte character, and the failure path must not panic or hide
        // the diagnostic behind a non-UTF-8 refusal.
        Ok(bytes) if bytes.len() > CAPTURED_TAIL => {
            let tail = &bytes[bytes.len() - CAPTURED_TAIL..];
            format!(
                "...{} bytes total, last {CAPTURED_TAIL}:\n{}",
                bytes.len(),
                String::from_utf8_lossy(tail)
            )
        }
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(error) => format!("<unreadable: {error}>"),
    }
}

fn verify_acknowledgement(path: &Path, selector: &str) -> Result<(), String> {
    let actual = std::fs::read_to_string(path).map_err(|error| {
        format!("read compiler fixture acknowledgement for {selector}: {error}")
    })?;
    if actual != selector {
        return Err(format!(
            "compiler fixture acknowledgement has the wrong identity for {selector}"
        ));
    }
    Ok(())
}

struct Scratch {
    path: PathBuf,
    retain: bool,
}

impl Scratch {
    fn new() -> Result<Self, String> {
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent)
            .map_err(|error| format!("create compiler fixture parent: {error}"))?;
        let path = parent.join(format!(
            "ripr-compiler-fixture-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        // Do not adopt or remove an occupied path owned by another execution.
        std::fs::create_dir(&path)
            .map_err(|error| format!("claim compiler fixture {}: {error}", path.display()))?;
        Ok(Self {
            path,
            retain: false,
        })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.retain {
            return;
        }
        if let Err(error) = std::fs::remove_dir_all(&self.path) {
            eprintln!("remove compiler fixture {}: {error}", self.path.display());
        }
    }
}

#[test]
fn root_and_nested_selectors_keep_libtest_names() -> Result<(), String> {
    let root = "quickcheck_generator_rendered";
    let nested = "ripr::analysis::facts::quickcheck_generator_tests";
    assert_eq!(exact_selector(root, "positive")?, "positive");
    assert_eq!(
        exact_selector(nested, "positive")?,
        "analysis::facts::quickcheck_generator_tests::positive"
    );
    Ok(())
}

#[test]
fn zero_selected_tests_cannot_acknowledge_fixture_assertions() -> Result<(), String> {
    let selector = exact_selector(module_path!(), "no_such_quickcheck_fixture_13285")?;
    let error = run_child(&selector, false)
        .err()
        .ok_or("a zero-test child must not acknowledge fixture assertions")?;
    assert!(error.contains("acknowledgement"), "{error}");
    Ok(())
}

#[test]
fn acknowledgement_for_another_test_is_rejected() -> Result<(), String> {
    let scratch = Scratch::new()?;
    let acknowledgement = scratch.path.join("assertions-completed");
    std::fs::write(&acknowledgement, "another::fixture")
        .map_err(|error| format!("write wrong acknowledgement: {error}"))?;
    let error = verify_acknowledgement(&acknowledgement, "intended::fixture")
        .err()
        .ok_or("another test's acknowledgement must not satisfy this fixture")?;
    assert!(error.contains("wrong identity"), "{error}");
    Ok(())
}

#[test]
fn profiling_variables_are_not_compiler_admission_overrides() {
    for key in [
        "LLVM_PROFILE_FILE",
        "LLVM_PROFILE_CONTINUOUS_MODE",
        "CARGO_LLVM_COV",
        "__CARGO_LLVM_COV_RUSTC_WRAPPER_RUSTFLAGS",
    ] {
        assert!(!compiler_admission_variable(OsStr::new(key)), "{key}");
    }
}
