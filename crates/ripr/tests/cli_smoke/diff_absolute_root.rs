//! Absolute `--root` must not leak root components into `changed_seams[].file`.
use super::{
    DeclaredHunkScratch, init_producer_fixture_repo, run_command, run_git, unique_temp_workspace,
};
use serde_json::Value;
use std::fs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn commit_boundary_change(root: &std::path::Path) -> TestResult {
    let source = fs::read_to_string(root.join("src/lib.rs"))?;
    assert!(
        source.contains("amount >= discount_threshold"),
        "fixture subject missing: {source}"
    );
    fs::write(
        root.join("src/lib.rs"),
        source.replace(
            "amount >= discount_threshold",
            "amount > discount_threshold",
        ),
    )?;
    run_git(root, &["add", "src/lib.rs"])?;
    run_git(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-m",
            "absolute-root path contract",
        ],
    )?;
    Ok(())
}

#[test]
fn diff_json_absolute_root_keeps_changed_seams_workspace_relative() -> TestResult {
    let root = unique_temp_workspace("diff-abs-root");
    let _cleanup = DeclaredHunkScratch(root.clone());
    init_producer_fixture_repo(&root)?;
    let base = run_command("git", Some(&root), &["rev-parse", "HEAD"])?;
    assert!(base.status.success(), "base setup failed");
    let base = String::from_utf8(base.stdout)?.trim().to_string();
    commit_boundary_change(&root)?;
    assert!(
        root.is_absolute(),
        "precondition: fixture root must be absolute, got {root:?}"
    );
    let root_arg = root.to_string_lossy().into_owned();
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["diff", "--root", &root_arg, "--base", &base, "--json"],
    )?;
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["kind"], "ripr_diff");
    let files = report["changed_files"]
        .as_array()
        .ok_or("changed_files must be an array")?;
    assert!(
        files.iter().any(|file| file["path"] == "src/lib.rs"),
        "changed_files lost the workspace-relative path: {files:?}"
    );
    let seams = report["changed_seams"]
        .as_array()
        .ok_or("changed_seams must be an array")?;
    assert!(!seams.is_empty(), "supported subject vanished: {seams:?}");
    assert!(
        seams
            .iter()
            .all(|seam| seam["file"] == "src/lib.rs" && seam["line"] == 2),
        "absolute --root leaked into changed_seams: {seams:?}"
    );
    let leaked = root_arg.trim_start_matches(['/', '\\']);
    assert!(
        seams.iter().all(|seam| seam["file"]
            .as_str()
            .is_none_or(|file| !file.contains(leaked))),
        "root components remained in changed_seams: {seams:?} root={root_arg}"
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rooted_cwd = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["diff", "--root", ".", "--base", &base, "--json"],
    )?;
    let rooted: Value = serde_json::from_slice(&rooted_cwd.stdout)?;
    let rooted_seams = rooted["changed_seams"]
        .as_array()
        .ok_or("rooted-CWD changed_seams must be an array")?;
    assert!(
        !rooted_seams.is_empty(),
        "rooted-CWD --root . lost the supported subject: {rooted_seams:?}"
    );
    assert!(
        rooted_seams
            .iter()
            .all(|seam| seam["file"] == "src/lib.rs" && seam["line"] == 2),
        "rooted-CWD --root . must keep the same workspace-relative seam: {rooted_seams:?}"
    );
    assert_eq!(rooted_cwd.status.code(), Some(0));
    Ok(())
}
