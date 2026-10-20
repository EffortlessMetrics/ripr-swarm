use super::{Fixture, assert_exit, run_git, run_ripr, weaken_boundary_predicate};
use serde_json::Value;
use std::process::Output;

const COMBINED: &str = "diff --cc src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@@ -1,1 -1,1 +1,1 @@@\n--parent\n++merge\n";
const ORDINARY: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";

fn brief(fixture: &Fixture, patch: &str) -> Result<Output, String> {
    let path = fixture.root.join("change.patch");
    std::fs::write(&path, patch).map_err(|e| e.to_string())?;
    run_ripr(
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
    )
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
    let output = run_ripr(
        &fixture.root,
        &[
            "agent",
            "brief",
            "--root",
            &fixture.root.to_string_lossy(),
            "--base",
            "HEAD",
            "--json",
        ],
    )?;
    assert_refusal(&output, "unresolved_conflict_markers")?;
    assert_eq!(document(&output)?["base"], "HEAD");
    Ok(())
}
