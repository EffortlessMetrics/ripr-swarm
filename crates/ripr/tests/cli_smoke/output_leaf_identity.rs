//! Output collision checks must compare native destination identities.
use super::{assert_success, spawn_command, unique_temp_workspace};
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        super::ignore_remove_dir_all(&self.0);
    }
}

fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("output leaf identity");
    std::fs::create_dir_all(std::env::temp_dir())?;
    std::fs::create_dir(&root)?;
    let fixture = Fixture(root);
    std::fs::create_dir(fixture.0.join("child"))?;
    std::fs::write(
        fixture.0.join("child/current.json"),
        r#"{"schema_version":"0.1","kind":"policy_operations",
        "generated_at":"unix_ms:10","current_policy_ceiling":"visible_only",
        "safe_to_promote_to":[],"not_safe_to_promote_to":[],
        "promotion_blockers":[],"input_artifacts":[],"current":{}}"#,
    )?;
    Ok(fixture)
}

fn policy_history(
    cwd: &Path,
    out: &str,
    jsonl: &str,
) -> Result<std::process::Output, std::io::Error> {
    spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(cwd),
        &[
            "policy",
            "history",
            "--current",
            "current.json",
            "--out",
            out,
            "--out-md",
            "report.md",
            "--out-jsonl",
            jsonl,
        ],
        &[],
        None,
        Some(b""),
    )
}

fn assert_reports(root: &Path, out: &Path, jsonl: &Path) -> TestResult {
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(root.join(out))?)?;
    assert_eq!(report["kind"], "policy_history");
    let line = std::fs::read_to_string(root.join(jsonl))?;
    assert_eq!(line.lines().count(), 1);
    let record: serde_json::Value = serde_json::from_str(&line)?;
    assert_eq!(record, report["example_append_record"]);
    assert_eq!(record["generated_at"], "unix_ms:10");
    Ok(())
}

#[test]
fn policy_history_distinguishes_parent_and_local_destinations() -> TestResult {
    let fixture = fixture()?;
    let cwd = fixture.0.join("child");
    let output = policy_history(&cwd, "../report.json", "report.json")?;
    assert_success(&output);
    assert_reports(&cwd, Path::new("../report.json"), Path::new("report.json"))
}

#[cfg(unix)]
#[test]
fn policy_history_keeps_backslash_filename_distinct_from_directory() -> TestResult {
    let fixture = fixture()?;
    let cwd = fixture.0.join("child");
    std::fs::create_dir(cwd.join("reports"))?;
    let output = policy_history(&cwd, r"reports\history.json", "reports/history.json")?;
    assert_success(&output);
    assert_reports(
        &cwd,
        Path::new(r"reports\history.json"),
        Path::new("reports/history.json"),
    )
}

#[test]
fn policy_history_still_refuses_native_aliases_before_writing() -> TestResult {
    let fixture = fixture()?;
    let cwd = fixture.0.join("child");
    std::fs::create_dir(cwd.join("reports"))?;
    let mut aliases = vec!["./current.json", "reports/../current.json"];
    if cfg!(windows) {
        aliases.push(r"reports\..\current.json");
    }
    let before = std::fs::read(cwd.join("current.json"))?;
    for alias in aliases {
        let output = policy_history(&cwd, "report.json", alias)?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("must not be the same path"));
        assert_eq!(std::fs::read(cwd.join("current.json"))?, before);
        assert!(!cwd.join("report.json").exists());
        assert!(!cwd.join("report.md").exists());
    }
    Ok(())
}
