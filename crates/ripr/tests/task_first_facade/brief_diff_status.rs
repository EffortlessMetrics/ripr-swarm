use super::{
    Fixture, assert_exit, git_commit, run_git, run_ripr, run_ripr_env, weaken_boundary_predicate,
};
use serde_json::Value;
use std::process::Output;

const COMBINED: &str = "diff --cc src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@@ -1,1 -1,1 +1,1 @@@\n--parent\n++merge\n";
const ORDINARY: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";

fn brief(fixture: &Fixture, patch: &str) -> Result<Output, String> {
    brief_env(fixture, patch, &[])
}

fn brief_env(fixture: &Fixture, patch: &str, envs: &[(&str, &str)]) -> Result<Output, String> {
    let path = fixture.root.join("change.patch");
    std::fs::write(&path, patch).map_err(|e| e.to_string())?;
    run_ripr_env(
        &fixture.root,
        &[
            "agent",
            "brief",
            "--root",
            &fixture.root.to_string_lossy(),
            "--diff",
            &path.to_string_lossy(),
            "--json",
        ],
        envs,
    )
}

#[test]
fn brief_parser_errors_keep_diff_input_identity_and_budget_guards() -> Result<(), String> {
    let fixture = Fixture::boundary("brief-parser-errors")?;
    std::fs::write(
        fixture.root.join("src/second.rs"),
        "// a second source file\n",
    )
    .map_err(|e| e.to_string())?;
    let two_files = format!(
        "{ORDINARY}{}",
        ORDINARY.replace("src/lib.rs", "src/second.rs")
    );
    let one_file = brief_env(&fixture, ORDINARY, &[("RIPR_MAX_DIFF_INDEX_FILES", "1")])?;
    assert_exit(&one_file, 0, "one file at the parser limit");
    assert!(
        document(&one_file)?["top_seams"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );
    assert_refusal(
        &brief_env(&fixture, &two_files, &[("RIPR_MAX_DIFF_INDEX_FILES", "1")])?,
        "diff_scope_oversized",
    )?;
    let admitted = brief_env(&fixture, &two_files, &[("RIPR_MAX_DIFF_INDEX_FILES", "2")])?;
    assert_exit(&admitted, 0, "two files at the parser limit");
    assert!(
        document(&admitted)?["top_seams"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty())
    );

    let patch = fixture.root.join("change.patch");
    let legacy = run_ripr_env(
        &fixture.root,
        &[
            "check",
            "--root",
            &fixture.root.to_string_lossy(),
            "--diff",
            &patch.to_string_lossy(),
            "--format",
            "json",
        ],
        &[("RIPR_MAX_DIFF_INDEX_FILES", "1")],
    )?;
    assert_exit(&legacy, 2, "check retains its existing budget envelope");
    let legacy = document(&legacy)?;
    assert_eq!(
        legacy["analysis_scope"]["run_status"],
        "diff_scope_oversized"
    );
    assert_eq!(
        legacy["analysis_scope"]["repair_route"],
        "analysis/diff-scope-budget"
    );

    let deep_path = format!("{}lib.rs", "deep/".repeat(600));
    assert_refusal(
        &brief(
            &fixture,
            &format!("--- a/{deep_path}\n+++ b/{deep_path}\n@@ -1 +1 @@\n-old\n+new\n"),
        )?,
        "diff_path_too_deep",
    )?;

    weaken_boundary_predicate(&fixture.root)?;
    std::fs::write(
        fixture.root.join("src/second.rs"),
        "// a second changed file\n",
    )
    .map_err(|e| e.to_string())?;
    run_git(&fixture.root, &["add", "src"])?;
    git_commit(&fixture.root, "two-file base input")?;
    let diff = run_git(&fixture.root, &["diff", "HEAD^...HEAD", "--", "src"])?;
    assert_eq!(
        diff.matches("diff --git").count(),
        2,
        "base fixture must have two real changed files"
    );
    let base = run_ripr_env(
        &fixture.root,
        &[
            "agent",
            "brief",
            "--root",
            &fixture.root.to_string_lossy(),
            "--base",
            "HEAD^",
            "--json",
        ],
        &[("RIPR_MAX_DIFF_INDEX_FILES", "1")],
    )?;
    assert_refusal(&base, "diff_scope_oversized")?;
    assert_eq!(document(&base)?["base"], "HEAD^");
    Ok(())
}

fn document(output: &Output) -> Result<Value, String> {
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "invalid JSON: {error}; stdout={:?}; stderr={:?}",
            output.stdout, output.stderr
        )
    })
}

fn assert_refusal(output: &Output, reason: &str) -> Result<(), String> {
    assert_exit(output, 2, reason);
    let value = document(output)?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(
        value["analysis_scope"]["run_status"],
        "diff_input_unsupported"
    );
    assert_eq!(value["analysis_scope"]["downstream_consumable"], false);
    assert_eq!(
        value["analysis_scope"]["repair_route"],
        "analysis/diff-input"
    );
    assert_eq!(
        value["run_limitations"][0]["category"],
        "diff_input_unsupported"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains(reason));
    assert!(
        value.get("top_seams").is_none(),
        "refusal must not present a successful brief"
    );
    Ok(())
}

#[test]
fn brief_empty_and_ordinary_patches_keep_successful_selection() -> Result<(), String> {
    let empty = Fixture::empty("brief-empty-patch")?;
    let output = brief(&empty, "")?;
    assert_exit(&output, 0, "empty patch and empty inventory");
    let value = document(&output)?;
    assert_eq!(value["limits"]["returned"], 0);
    assert_eq!(value["top_seams"], serde_json::json!([]));

    let boundary = Fixture::boundary("brief-ordinary-patch")?;
    let fallback = brief(&boundary, "")?;
    assert_exit(
        &fallback,
        0,
        "empty patch keeps documented repository fallback",
    );
    let value = document(&fallback)?;
    assert_eq!(
        value["limits"]["returned"], 1,
        "fixture must contain a real seam"
    );
    assert_eq!(
        value["top_seams"][0]["why_now"]["reason"],
        "repo_actionable_fallback"
    );

    weaken_boundary_predicate(&boundary.root)?;
    let patch = run_git(&boundary.root, &["diff", "HEAD", "--", "src/lib.rs"])?;
    assert!(
        patch.contains("@@"),
        "ordinary control must have a real hunk"
    );
    let output = brief(&boundary, &patch)?;
    assert_exit(&output, 0, "ordinary two-way patch");
    let value = document(&output)?;
    assert_eq!(value["limits"]["returned"], 1);
    assert_eq!(
        value["top_seams"][0]["why_now"]["reason"],
        "changed_line_intersects_seam"
    );
    Ok(())
}

#[test]
fn brief_parser_refusals_cannot_become_empty_or_fallback_successes() -> Result<(), String> {
    let empty = Fixture::empty("brief-refused-empty")?;
    let boundary = Fixture::boundary("brief-refused-boundary")?;
    for fixture in [&empty, &boundary] {
        for patch in [
            COMBINED.to_string(),
            format!("{COMBINED}{ORDINARY}"),
            format!("{ORDINARY}{COMBINED}"),
        ] {
            assert_refusal(&brief(fixture, &patch)?, "combined_hunk_unsupported")?;
        }
        assert_refusal(
            &brief(
                fixture,
                "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,1 +1,2 @@\n-old\n+new\n",
            )?,
            "malformed_diff",
        )?;
        assert_refusal(
            &brief(
                fixture,
                "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -0,0 +1,5 @@\n+<<<<<<< ours\n+left\n+=======\n+right\n+>>>>>>> theirs\n",
            )?,
            "unresolved_conflict_markers",
        )?;
    }
    Ok(())
}

#[test]
fn brief_base_uses_the_same_parser_refusal_before_inventory() -> Result<(), String> {
    let fixture = Fixture::boundary("brief-base-conflict")?;
    let source = fixture.root.join("src/lib.rs");
    std::fs::write(
        &source,
        "<<<<<<< ours\nleft\n=======\nright\n>>>>>>> theirs\n",
    )
    .map_err(|e| e.to_string())?;
    run_git(&fixture.root, &["add", "src/lib.rs"])?;
    git_commit(&fixture.root, "committed conflict input")?;
    let diff = run_git(&fixture.root, &["diff", "HEAD^...HEAD", "--", "src/lib.rs"])?;
    assert!(
        diff.contains("+<<<<<<< ours") && diff.contains("+>>>>>>> theirs"),
        "base control must supply actual committed conflict lines: {diff}"
    );
    let output = run_ripr(
        &fixture.root,
        &[
            "agent",
            "brief",
            "--root",
            &fixture.root.to_string_lossy(),
            "--base",
            "HEAD^",
            "--json",
        ],
    )?;
    assert_refusal(&output, "unresolved_conflict_markers")?;
    assert_eq!(document(&output)?["base"], "HEAD^");
    Ok(())
}
