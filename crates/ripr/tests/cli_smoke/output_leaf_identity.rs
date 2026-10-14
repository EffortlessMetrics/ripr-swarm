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
// #7739: refuse aliases before either output can replace existing bytes.
const GATE_SENTINEL: &[u8] = b"\0existing gate output\xff\n";
const CLEAN_GATE_GUIDANCE: &str = r#"{
    "schema_version": "0.1", "tool": "ripr", "status": "advisory",
    "summary": {"unchanged_tests": true},
    "comments": [], "summary_only": [], "suppressed": []
}"#;

fn gate_fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let fixture = fixture()?;
    let cwd = fixture.0.join("child");
    std::fs::create_dir(cwd.join("repo"))?;
    std::fs::create_dir(cwd.join("reports"))?;
    std::fs::write(cwd.join("repo/guidance.json"), CLEAN_GATE_GUIDANCE)?;
    Ok(fixture)
}

fn gate_evaluate(
    cwd: &Path,
    out: &str,
    out_md: Option<&str>,
) -> Result<std::process::Output, std::io::Error> {
    let mut args = vec![
        "gate",
        "evaluate",
        "--root",
        "repo",
        "--pr-guidance",
        "guidance.json",
        "--mode",
        "visible-only",
        "--out",
        out,
    ];
    if let Some(out_md) = out_md {
        args.extend(["--out-md", out_md]);
    }
    spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(cwd),
        &args,
        &[],
        None,
        Some(b""),
    )
}

fn assert_gate_refusal(output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        output.stdout.is_empty(),
        "refusal reported a write: {output:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("gate --out and --out-md must not be the same path"),
        "{output:?}"
    );
}

fn assert_preserved_gate_output(cwd: &Path, out: &str, out_md: Option<&str>) -> TestResult {
    let destination = cwd.join(out);
    std::fs::write(&destination, GATE_SENTINEL)?;
    let output = gate_evaluate(cwd, out, out_md)?;
    // Check bytes first so RED identifies the actual destructive write, not
    // just the missing message or exit code.
    assert_eq!(std::fs::read(destination)?, GATE_SENTINEL, "{output:?}");
    assert_gate_refusal(&output);
    Ok(())
}

fn assert_gate_reports(cwd: &Path, out: &str, out_md: &str) -> TestResult {
    let report: serde_json::Value = serde_json::from_slice(&std::fs::read(cwd.join(out))?)?;
    assert_eq!(report["schema_version"], "0.1");
    assert_eq!(report["tool"], "ripr");
    assert_eq!(report["status"], "advisory");
    assert_eq!(report["mode"], "visible-only");
    assert_eq!(report["summary"]["blocking"], 0);
    let markdown = std::fs::read_to_string(cwd.join(out_md))?;
    assert!(
        markdown.starts_with("# RIPR Gate Decision\n\nDecision: advisory\nMode: visible-only\n")
    );
    assert!(markdown.contains("\nBlocking: 0\n"));
    // --root names a different directory. Output paths follow the invoking
    // cwd, so an implementation that anchors them to --root must fail.
    assert!(!cwd.join("repo/report.json").exists());
    assert!(!cwd.join("repo/report.md").exists());
    Ok(())
}

#[test]
fn gate_refuses_identical_explicit_destinations_before_writing() -> TestResult {
    let fixture = gate_fixture()?;
    assert_preserved_gate_output(&fixture.0.join("child"), "report.json", Some("report.json"))
}

#[test]
fn gate_refuses_dot_segment_aliases_before_writing() -> TestResult {
    let fixture = gate_fixture()?;
    let cwd = fixture.0.join("child");
    let mut aliases = vec!["./report.json", "reports/../report.json"];
    if cfg!(windows) {
        aliases.push(r"reports\..\report.json");
    }
    for alias in aliases {
        assert_preserved_gate_output(&cwd, "report.json", Some(alias))?;
    }
    Ok(())
}

#[test]
fn gate_refuses_relative_absolute_aliases_before_writing() -> TestResult {
    let fixture = gate_fixture()?;
    let cwd = fixture.0.join("child");
    let absolute = cwd.join("report.json").display().to_string();
    assert_preserved_gate_output(&cwd, "report.json", Some(&absolute))?;
    assert_preserved_gate_output(&cwd, &absolute, Some("report.json"))
}

#[test]
fn gate_refuses_implicit_markdown_collision_before_writing() -> TestResult {
    let fixture = gate_fixture()?;
    let cwd = fixture.0.join("child");
    assert_preserved_gate_output(&cwd, "report.md", None)?;
    // With no existing file or parent, refusal must not create either output
    // or even its containing directory.
    let output = gate_evaluate(&cwd, "uncreated/report.md", None)?;
    assert!(!cwd.join("uncreated").exists(), "{output:?}");
    assert_gate_refusal(&output);
    Ok(())
}

#[test]
fn gate_distinct_explicit_and_default_destinations_write_both_formats() -> TestResult {
    let fixture = gate_fixture()?;
    let cwd = fixture.0.join("child");
    // An explicit Markdown destination permits a .md-named JSON output.
    let output = gate_evaluate(&cwd, "decision.md", Some("report.md"))?;
    assert_success(&output);
    assert_gate_reports(&cwd, "decision.md", "report.md")?;
    let output = gate_evaluate(&cwd, "report.json", None)?;
    assert_success(&output);
    assert_gate_reports(&cwd, "report.json", "report.md")
}

#[test]
fn gate_distinguishes_parent_and_local_destinations() -> TestResult {
    let fixture = gate_fixture()?;
    let cwd = fixture.0.join("child");
    let output = gate_evaluate(&cwd, "../report.json", Some("report.json"))?;
    assert_success(&output);
    assert_gate_reports(&cwd, "../report.json", "report.json")
}

#[cfg(unix)]
#[test]
fn gate_keeps_backslash_filename_distinct_from_directory() -> TestResult {
    let fixture = gate_fixture()?;
    let cwd = fixture.0.join("child");
    let output = gate_evaluate(
        &cwd,
        r"reports\decision.json",
        Some("reports/decision.json"),
    )?;
    assert_success(&output);
    assert_gate_reports(&cwd, r"reports\decision.json", "reports/decision.json")
}
