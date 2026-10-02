//! #4789 / #4835 quarantine: match public static output with real test collection.
//! No-op property spellings must not borrow the ordinary assertion positive.
use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace, render_check};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct Scratch {
    directory: PathBuf,
    cleanup_attempted: bool,
}
impl Scratch {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ripr-property-quarantine-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        Self::create_at(path)
    }

    fn create_at(path: PathBuf) -> Result<Self, String> {
        // Establish ownership exclusively before installing the cleanup guard.
        // A stale/preexisting directory is never adopted or recursively removed.
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        let scratch = Self {
            directory: path,
            cleanup_attempted: false,
        };
        std::fs::create_dir(scratch.directory.join("src")).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}
impl Scratch {
    fn cleanup(mut self) -> Result<(), String> {
        self.cleanup_attempted = true;
        std::fs::remove_dir_all(&self.directory).map_err(|error| {
            format!(
                "explicit fixture cleanup failed at {}: {error}",
                self.directory.display()
            )
        })
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if self.cleanup_attempted {
            return;
        }
        if std::thread::panicking() {
            eprintln!(
                "retained failed property control at {}",
                self.directory.display()
            );
            return;
        }
        if let Err(error) = std::fs::remove_dir_all(&self.directory) {
            eprintln!(
                "fallback fixture cleanup failed at {}: {error}",
                self.directory.display()
            );
        }
    }
}

const STREAM_LIMIT: u64 = 64 * 1024;
fn read_bounded_stream(path: &Path) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.take(STREAM_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > STREAM_LIMIT {
        return Err(format!(
            "runtime stream {} exceeds {STREAM_LIMIT} bytes",
            path.display()
        ));
    }
    Ok(bytes)
}

#[test]
fn property_runtime_stream_overflow_is_a_proof_failure() -> Result<(), String> {
    let scratch = Scratch::new()?;
    let output = scratch.directory.join("overflow");
    std::fs::write(&output, vec![b'x'; STREAM_LIMIT as usize + 1])
        .map_err(|error| error.to_string())?;
    assert!(read_bounded_stream(&output).is_err());
    std::fs::write(&output, "small").map_err(|error| error.to_string())?;
    assert_eq!(read_bounded_stream(&output)?, b"small");
    scratch.cleanup()
}

/// Match the existing advisory-write harness: file-backed streams avoid pipe
/// backpressure, and the shared process owner terminates/reaps on timeout or
/// error before Scratch can remove its owned directory.
fn run_bounded(
    mut command: Command,
    root: &Path,
    label: &str,
    budget: Duration,
) -> Result<Output, String> {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    command
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).map_err(|error| error.to_string())?)
        .stderr(std::fs::File::create(&stderr).map_err(|error| error.to_string())?);
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= budget {
            child.terminate_tree()?;
            return Err(format!(
                "{label} exceeded its {budget:?} budget; owned process terminated and reaped"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Output {
        status,
        stdout: read_bounded_stream(&stdout)?,
        stderr: read_bounded_stream(&stderr)?,
    })
}

#[test]
fn property_quarantine_scratch_does_not_adopt_existing_directory() -> Result<(), String> {
    let scratch = Scratch::new()?;
    let marker = scratch.directory.join("existing-owner");
    std::fs::write(&marker, "keep").map_err(|error| error.to_string())?;
    assert!(Scratch::create_at(scratch.directory.clone()).is_err());
    assert_eq!(
        std::fs::read_to_string(marker).map_err(|error| error.to_string())?,
        "keep"
    );
    scratch.cleanup()
}

const OWNER: &str = "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const DIFF: &str = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,5 +1,5 @@\n pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n-    if amount > discount_threshold {\n+    if amount >= discount_threshold {\n         amount - 10\n     } else {\n         amount\n";

#[test]
fn property_macro_quarantine_matches_runtime_collection_and_discrimination() -> Result<(), String> {
    let assertion = "assert_eq!(discounted_total(100, 100), 90);";
    for (name, tests, exposed, test_count, limit) in [
        ("ordinary", format!("#[test]\nfn boundary() {{ {assertion} }}\n"), true, 1, None),
        ("no_assertion", "#[test]\nfn boundary() { let _ = discounted_total(100, 100); }\n".to_string(), false, 1, None),
        ("noop_property_assertion", "macro_rules! prop_assert_eq { ($($args:tt)*) => {} }\n#[test]\nfn boundary() { prop_assert_eq!(discounted_total(100, 100), 90); }\n".to_string(), false, 1, Some("rust_macro_wrapped_assertion_unresolved")),
        ("noop_proptest", format!("macro_rules! proptest {{ ($($args:tt)*) => {{}} }}\nproptest! {{ #[test] fn boundary() {{ {assertion} }} }}\n"), false, 0, Some("rust_macro_reach_unresolved")),
        ("noop_quickcheck", format!("macro_rules! quickcheck {{ ($($args:tt)*) => {{}} }}\nquickcheck! {{ fn boundary() -> bool {{ {assertion} true }} }}\n"), false, 0, Some("rust_macro_reach_unresolved")),
        ("noop_qualified_collision", "macro_rules! proptest { ($($args:tt)*) => {} }\nproptest! { #[test] fn boundary() { assert_eq!(other::discounted_total(100, 100), 90); } }\n".to_string(), false, 0, Some("rust_macro_reach_unresolved")),
    ] {
        let scratch = Scratch::new()?;
        let root = &scratch.directory;
        std::fs::write(root.join("Cargo.toml"), "[package]\nname=\"property_quarantine\"\nversion=\"0.1.0\"\nedition=\"2024\"\n").map_err(|error| error.to_string())?;
        let source = format!("{OWNER}\n{tests}");
        if limit.is_some() {
            let retained = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../fixtures/property_macro_{name}/input/src/lib.rs"));
            assert_eq!(source, std::fs::read_to_string(retained).map_err(|error| error.to_string())?,
                "{name}: governed fixture and runtime subject must be identical");
        }

        std::fs::write(root.join("src/lib.rs"), &source).map_err(|error| error.to_string())?;
        std::fs::write(root.join("diff.patch"), DIFF).map_err(|error| error.to_string())?;
        let report = check_workspace(CheckInput {
            root: root.clone(), diff_file: Some(root.join("diff.patch")), mode: Mode::Fast,
            format: OutputFormat::Json, include_unchanged_tests: true, ..CheckInput::default()
        })?;
        assert_eq!(report.findings.len(), 1, "{name}: one intended predicate");
        assert_eq!(report.findings[0].class == ExposureClass::Exposed, exposed, "{name}");
        let json: serde_json::Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?).map_err(|error| error.to_string())?;
        let finding = &json["findings"][0];
        assert_eq!(finding["oracle_strength"], if exposed { "strong" } else { "none" }, "{name}");
        if test_count == 0 {
            assert_ne!(finding["ripr"]["reach"]["state"], "yes", "{name}: a lexical name collision is never reach");
            assert_eq!(finding["related_tests"].as_array().map(Vec::len), Some(0), "{name}");
        }
        if let Some(limit) = limit {
            assert_eq!(finding["static_limit_kind"], limit, "{name}");
            let guidance = finding["recommended_next_step"].as_str().ok_or("missing guidance")?;
            assert!(guidance.contains("macro"), "{name}: {guidance}");
            assert!(!guidance.contains("Add "), "{name}: unsupported is not a new-test request");
            let human = render_check(&report, &OutputFormat::Human)?;
            assert!(human.contains("static_limited"), "{name}: {human}");
        }
        for wrong in [false, true] {
            let runtime_source = root.join("runtime.rs");
            std::fs::write(&runtime_source, if wrong { source.replace("amount >= discount_threshold", "amount > discount_threshold") } else { source.clone() }).map_err(|error| error.to_string())?;
            let binary = root.join(format!("runtime{}", std::env::consts::EXE_SUFFIX));
            let mut command = Command::new("rustc");
            command.arg("--edition=2024").arg("--test").arg(&runtime_source).arg("-o").arg(&binary);
            let build = run_bounded(command, root, "rustc", Duration::from_secs(120))?;
            assert!(build.status.success(), "{name}: compilation must succeed: {}", String::from_utf8_lossy(&build.stderr));
            let run = run_bounded(Command::new(&binary), root, "runtime", Duration::from_secs(10))?;
            let stdout = String::from_utf8_lossy(&run.stdout);
            assert!(stdout.contains(&format!("running {test_count} test")), "{name}: {stdout}");
            assert_eq!(run.status.success(), !(wrong && exposed), "{name}: wrong={wrong}: {stdout}");
        }
        scratch.cleanup()?;
    }
    Ok(())
}
