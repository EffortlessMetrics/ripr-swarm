#![cfg(feature = "lang-rust")]

#[path = "common/mod.rs"]
mod common;

use common::fixture_git::fixture_git_ok as git;
use ripr::process_owner::OwnedProcess;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

const NOTE: &str = "ripr: detected .ripr/suppressions.toml under the analyzed root; it was not applied to this check. Rerun this check with --suppression-policy .ripr/suppressions.toml to apply it.";
const POLICY: &str = "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"src/**\"\nreason = \"Accepted fixture findings\"\nowner = \"fixture\"\n";
const CAP: u64 = 256 * 1024;

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self, String> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..100 {
            let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("ripr-suppression-{}-{serial}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.to_string()),
            }
        }
        Err("could not exclusively allocate fixture scratch".into())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    scratch: Scratch,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, String> {
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|e| e.to_string())?;
        let scratch = Scratch::new()?;
        let root = scratch.path().join("analyzed repo");
        std::fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"suppression_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .map_err(|e| e.to_string())?;
        git(&root, &["init", "--initial-branch=main"])?;
        git(&root, &["config", "core.autocrlf", "false"])?;
        git(&root, &["config", "commit.gpgsign", "false"])?;
        git(&root, &["config", "user.email", "fixture@example.invalid"])?;
        git(&root, &["config", "user.name", "RIPR fixture"])?;
        for (operator, message) in [(">", "base"), (">=", "change")] {
            std::fs::write(
                root.join("src/lib.rs"),
                format!("pub fn boundary(n: u32) -> bool {{\n    n {operator} 10\n}}\n"),
            )
            .map_err(|e| e.to_string())?;
            git(&root, &["add", "."])?;
            git(&root, &["commit", "-qm", message])?;
        }
        Ok(Self { scratch, root })
    }

    fn policy(&self, text: &str) -> Result<(), String> {
        std::fs::create_dir_all(self.root.join(".ripr")).map_err(|e| e.to_string())?;
        std::fs::write(self.root.join(".ripr/suppressions.toml"), text).map_err(|e| e.to_string())
    }

    fn run(&self, cwd: &Path, format: &str, explicit: bool) -> Result<Output, String> {
        let stdout = self.scratch.path().join("stdout");
        let stderr = self.scratch.path().join("stderr");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command.current_dir(cwd).env_clear();
        for key in [
            "PATH",
            "SystemRoot",
            "SYSTEMROOT",
            "COMSPEC",
            "TEMP",
            "TMP",
            "TMPDIR",
            "HOME",
            "USERPROFILE",
            "RUSTUP_HOME",
            "CARGO_HOME",
            "LLVM_PROFILE_FILE",
            "CARGO_LLVM_COV",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("RIPR_CACHE_DIR", self.scratch.path().join("cache"))
            .args(["check", "--root"])
            .arg(&self.root)
            .args(["--base", "HEAD~1", "--format", format])
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&stdout).map_err(|e| e.to_string())?)
            .stderr(std::fs::File::create(&stderr).map_err(|e| e.to_string())?);
        if explicit {
            command.args(["--suppression-policy", ".ripr/suppressions.toml"]);
        }
        let mut child = OwnedProcess::spawn(command).map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(20);
        let outcome = (|| {
            loop {
                if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                    return Ok(status);
                }
                for path in [&stdout, &stderr] {
                    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > CAP {
                        return Err("CLI capture exceeded its 256 KiB bound".to_string());
                    }
                }
                if Instant::now() >= deadline {
                    return Err("CLI exceeded its 20-second deadline".to_string());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        })();
        let cleanup = child.terminate_tree();
        let status = match (outcome, cleanup) {
            (Ok(status), Ok(())) => status,
            (Err(error), Ok(())) | (Ok(_), Err(error)) => return Err(error),
            (Err(error), Err(cleanup)) => return Err(format!("{error}; cleanup: {cleanup}")),
        };
        Ok(Output {
            status,
            stdout: read_bounded(&stdout)?,
            stderr: read_bounded(&stderr)?,
        })
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > CAP {
        return Err("CLI capture exceeded its 256 KiB bound".into());
    }
    Ok(bytes)
}

fn success_json(output: &Output) -> Result<Value, String> {
    if !output.status.success() {
        return Err(format!(
            "check failed: {}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}

fn findings(value: &Value) -> Result<&Vec<Value>, String> {
    let rows = value["findings"].as_array().ok_or("missing findings")?;
    if rows.is_empty()
        || rows
            .iter()
            .any(|row| row["probe"]["file"] != "./src/lib.rs")
    {
        return Err(format!(
            "fixture must yield real src/lib.rs findings: {rows:?}"
        ));
    }
    Ok(rows)
}

fn note_count(output: &Output) -> usize {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| *line == NOTE)
        .count()
}

#[test]
fn default_suppression_disclosure_preserves_explicit_opt_in() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let cwd = fixture.scratch.path();
    let absent = fixture.run(cwd, "json", false)?;
    let before = success_json(&absent)?;
    findings(&before)?;
    assert!(note_count(&absent) == 0);
    fixture.policy(POLICY)?;
    let unused = fixture.run(cwd, "json", false)?;
    let unchanged = success_json(&unused)?;
    assert_eq!(
        unused.stdout, absent.stdout,
        "unused policy must not change JSON stdout"
    );
    assert_eq!(findings(&unchanged)?, findings(&before)?);
    assert_eq!(unchanged["summary"], before["summary"]);
    assert!(unchanged.get("suppression_policy").is_none());
    assert!(
        findings(&unchanged)?
            .iter()
            .all(|row| row.get("suppressed").is_none())
    );
    let explicit = fixture.run(cwd, "json", true)?;
    let applied = success_json(&explicit)?;
    let rows = findings(&applied)?;
    assert!(
        rows.iter()
            .all(|row| row["suppressed"] == true && row["suppressed_by"] == "src/**")
    );
    assert_eq!(
        applied["summary"]["suppressed_by_policy"].as_u64(),
        Some(rows.len() as u64)
    );
    assert!(note_count(&explicit) == 0);
    // Final independent oracle: all behavior controls above must pass before RED.
    assert!(
        note_count(&unused) == 1,
        "missing unused-policy recovery note: {}",
        String::from_utf8_lossy(&unused.stderr)
    );
    Ok(())
}

#[test]
fn default_suppression_malformed_unused_remains_nonfatal() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let absent_output = fixture.run(fixture.scratch.path(), "json", false)?;
    let absent = success_json(&absent_output)?;
    findings(&absent)?;
    fixture.policy("not [[[ valid\n")?;
    let unused = fixture.run(fixture.scratch.path(), "json", false)?;
    let value = success_json(&unused)?;
    assert_eq!(
        unused.stdout, absent_output.stdout,
        "malformed unused policy must not change JSON stdout"
    );
    assert_eq!(findings(&value)?, findings(&absent)?);
    assert!(value.get("suppression_policy").is_none());
    let explicit = fixture.run(fixture.scratch.path(), "json", true)?;
    assert_eq!(explicit.status.code(), Some(2));
    let refusal: Value = serde_json::from_slice(&explicit.stdout).map_err(|e| e.to_string())?;
    assert_eq!(
        refusal
            .pointer("/analysis_scope/run_status")
            .and_then(Value::as_str),
        Some("suppression_policy_invalid")
    );
    assert_eq!(
        refusal.pointer("/analysis_scope/downstream_consumable"),
        Some(&Value::Bool(false))
    );
    assert!(note_count(&explicit) == 0);
    assert!(
        note_count(&unused) == 1,
        "an unused malformed file must be disclosed, not validated"
    );
    Ok(())
}

#[test]
fn default_suppression_detection_uses_analyzed_root() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let cwd = fixture.scratch.path().join("foreign cwd");
    std::fs::create_dir_all(cwd.join(".ripr")).map_err(|e| e.to_string())?;
    std::fs::write(cwd.join(".ripr/suppressions.toml"), POLICY).map_err(|e| e.to_string())?;
    let absent = fixture.run(&cwd, "json", false)?;
    findings(&success_json(&absent)?)?;
    assert!(
        note_count(&absent) == 0,
        "foreign CWD policy must not be attributed to analyzed root"
    );
    let policy_path = fixture.root.join(".ripr/suppressions.toml");
    std::fs::create_dir_all(&policy_path).map_err(|e| e.to_string())?;
    let directory = fixture.run(&cwd, "json", false)?;
    findings(&success_json(&directory)?)?;
    assert!(
        note_count(&directory) == 0,
        "a directory is not an unused policy file"
    );
    std::fs::remove_dir(&policy_path).map_err(|e| e.to_string())?;
    fixture.policy(POLICY)?;
    let present = fixture.run(&cwd, "json", false)?;
    findings(&success_json(&present)?)?;
    assert!(note_count(&present) == 1);
    Ok(())
}

#[test]
fn default_suppression_human_formats_preserve_stdout() -> Result<(), String> {
    let fixture = Fixture::new()?;
    for format in ["human", "human-full", "github"] {
        if fixture.root.join(".ripr/suppressions.toml").exists() {
            std::fs::remove_file(fixture.root.join(".ripr/suppressions.toml"))
                .map_err(|e| e.to_string())?;
        }
        let absent = fixture.run(fixture.scratch.path(), format, false)?;
        assert!(absent.status.success());
        assert!(
            !absent.stdout.is_empty(),
            "fixture must expose findings in {format}"
        );
        assert!(note_count(&absent) == 0);
        fixture.policy(POLICY)?;
        let present = fixture.run(fixture.scratch.path(), format, false)?;
        assert_eq!(present.status.code(), absent.status.code());
        assert_eq!(present.stdout, absent.stdout);
        assert!(
            note_count(&present) == 1,
            "{format} must disclose unused policy"
        );
    }
    Ok(())
}

#[test]
fn default_suppression_implicit_consumers_do_not_claim_skipped_policy() -> Result<(), String> {
    let fixture = Fixture::new()?;
    fixture.policy("schema_version = 1\n")?;
    for format in ["sarif", "badge-json"] {
        let output = fixture.run(fixture.scratch.path(), format, false)?;
        assert!(
            output.status.success(),
            "{format}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let _: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
        assert!(
            note_count(&output) == 0,
            "{format} must retain its implicit policy authority"
        );
    }
    Ok(())
}
