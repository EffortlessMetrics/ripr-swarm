//! #15249: every command that takes `--root` refuses a missing root with the
//! shared `<command> root X is not a directory; pass the directory that
//! contains the workspace ...` wording, naming the command actually run,
//! exit 2, and before it writes or blames another tool.

use super::{ignore_remove_dir_all, spawn_command, unique_temp_workspace};
use std::path::PathBuf;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        ignore_remove_dir_all(&self.0);
    }
}

/// Spelling the user runs, its shared-wording name, and the arguments that
/// would otherwise pass argv parsing (the root is appended last).
const CASES: &[(&str, &[&str])] = &[
    (
        "agent repair --phase before",
        &["agent", "repair", "--seam-id", "x"],
    ),
    ("diff", &["diff"]),
    (
        "pr-review front-panel",
        &["pr-review", "front-panel", "--pr-guidance", "guidance.json"],
    ),
    ("first-pr", &["first-pr"]),
    ("start-here", &["start-here"]),
    ("gate check", &["gate", "check", "--base", "HEAD"]),
    (
        "reports ts-false-actionable",
        &["reports", "ts-false-actionable", "--corpus", "corpus.json"],
    ),
    (
        "receipt write",
        &[
            "receipt",
            "write",
            "--gap",
            "g",
            "--verify-command",
            "true",
            "--status",
            "passed",
        ],
    ),
    ("evidence-health", &["evidence-health"]),
    (
        "feedback record",
        &[
            "feedback",
            "record",
            "--snapshot",
            "s",
            "--reason",
            "useful_actionable",
        ],
    ),
];

#[test]
fn missing_root_is_refused_by_the_shared_wording_naming_the_command_run() -> TestResult {
    let base = unique_temp_workspace("missing-root-refusal");
    std::fs::create_dir_all(&base)?;
    let _cleanup = Cleanup(base.clone());
    let missing = base.join("nonexistent");
    let missing_text = missing.to_str().ok_or("fixture root is not UTF-8")?;

    for (name, args) in CASES {
        let mut argv: Vec<&str> = args.to_vec();
        argv.extend(["--root", missing_text]);
        let output = spawn_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&base),
            &argv,
            &[],
            None,
            Some(b""),
        )?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{name}: a missing root exits 2: {stderr}"
        );
        let expected = format!(
            "{name} root {missing_text} is not a directory; pass the directory that contains the workspace"
        );
        assert!(stderr.contains(&expected), "{name}: {stderr}");
        assert!(
            !stderr.contains("git is installed")
                && !stderr.contains("default base")
                && !stderr.contains("gate evaluate"),
            "{name}: the refusal must not blame git, the base or another command: {stderr}"
        );
    }

    // Nothing was created: not the missing root, and no report under the
    // invoking directory.
    let created: Vec<_> = std::fs::read_dir(&base)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    assert!(
        created.is_empty(),
        "a refused root must write nothing: {created:?}"
    );
    Ok(())
}

#[test]
fn a_file_root_is_refused_by_the_same_wording() -> TestResult {
    let base = unique_temp_workspace("file-root-refusal");
    std::fs::create_dir_all(&base)?;
    let _cleanup = Cleanup(base.clone());
    let file = base.join("Cargo.toml");
    std::fs::write(&file, b"[package]\n")?;
    let file_text = file.to_str().ok_or("fixture root is not UTF-8")?;
    let output = spawn_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&base),
        &["start-here", "--root", file_text],
        &[],
        None,
        Some(b""),
    )?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains(&format!("start-here root {file_text} is not a directory;")),
        "{stderr}"
    );
    Ok(())
}
