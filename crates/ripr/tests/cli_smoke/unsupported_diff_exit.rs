//! Process status follows the producer outcome without replacing its report.
use super::{
    DeclaredHunkScratch, init_producer_fixture_repo, run_command, run_git, unique_temp_workspace,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;

const CONFLICT: &str = "<<<<<<< ours\npub fn value() -> u32 { 1 }\n=======\npub fn value() -> u32 { 2 }\n>>>>>>> theirs\n";
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn commit_fixture(root: &std::path::Path) -> TestResult {
    run_git(root, &["add", "src"])?;
    run_git(
        root,
        &[
            "-c",
            "user.name=RIPR test",
            "-c",
            "user.email=ripr@example.invalid",
            "commit",
            "-m",
            "contract subject",
        ],
    )?;
    Ok(())
}

#[test]
fn diff_json_outcomes_preserve_report_and_exit_contract() -> TestResult {
    for (case, kind, complete, files, retained, limitation, exit) in [
        ("complete", "complete_with_findings", true, 1, true, None, 0),
        ("empty", "no_scope", true, 0, false, None, 0),
        (
            "partial",
            "partial_with_limitations",
            false,
            2,
            true,
            Some("changed_file_absent_from_worktree"),
            0,
        ),
        (
            "conflict",
            "unsupported_input",
            false,
            1,
            false,
            Some("unresolved_conflict_markers"),
            2,
        ),
        (
            "mixed",
            "unsupported_input",
            false,
            2,
            true,
            Some("unresolved_conflict_markers"),
            2,
        ),
    ] {
        let root = unique_temp_workspace(&format!("diff-exit-{case}"));
        init_producer_fixture_repo(&root)?;
        let _cleanup = DeclaredHunkScratch(root.clone());
        let base = run_command("git", Some(&root), &["rev-parse", "HEAD"])?;
        assert!(base.status.success(), "{case}: base setup failed");
        let base = String::from_utf8(base.stdout)?.trim().to_string();
        if retained {
            let source = fs::read_to_string(root.join("src/lib.rs"))?;
            assert!(source.contains("amount >= discount_threshold"));
            fs::write(
                root.join("src/lib.rs"),
                source.replace(
                    "amount >= discount_threshold",
                    "amount > discount_threshold",
                ),
            )?;
        }
        if matches!(case, "conflict" | "mixed") {
            fs::write(root.join("src/conflict.rs"), CONFLICT)?;
        }
        if case == "partial" {
            fs::write(root.join("src/absent.rs"), "pub fn absent() -> u32 { 1 }\n")?;
        }
        if case != "empty" {
            commit_fixture(&root)?;
        }
        if case == "partial" {
            fs::remove_file(root.join("src/absent.rs"))?;
        }
        let range = format!("{base}...HEAD");
        let patch = run_command(
            "git",
            Some(&root),
            &[
                "-c",
                "core.quotePath=true",
                "-c",
                "diff.orderFile=/dev/null",
                "diff",
                "--relative",
                "--unified=0",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--inter-hunk-context=0",
                &range,
            ],
        )?;
        assert!(patch.status.success(), "{case}: diff setup failed");
        if case == "empty" {
            assert!(patch.stdout.is_empty());
        } else {
            assert!(!patch.stdout.is_empty(), "{case}: intended subject missing");
        }
        // Root-relative coordinate proof. Absolute --root seam projection
        // has a separate retained defect; this exit mapping does not fix it.
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&root),
            &["diff", "--root", ".", "--base", &base, "--json"],
        )?;
        // Parse every stdout byte: prose, an appended refusal, or truncation
        // cannot pass as the original report.
        let report: Value = serde_json::from_slice(&output.stdout)?;
        assert_eq!(report["schema_version"], "0.1", "{case}");
        assert_eq!(report["kind"], "ripr_diff", "{case}");
        assert_eq!(report["base"], base, "{case}");
        assert_eq!(report["head"], "HEAD", "{case}");
        assert_eq!(
            report["analysis_outcome"]["analysis_complete"], complete,
            "{case}"
        );
        let outcome = &report["analysis_outcome"]["outcome"];
        assert_eq!(outcome["kind"], kind, "{case}");
        assert_eq!(outcome["counts"]["changed_file_count"], files, "{case}");
        assert_eq!(
            outcome["identity"]["input_identity"],
            format!("sha256:{:x}", Sha256::digest(&patch.stdout)),
            "{case}"
        );
        let seams = report["changed_seams"]
            .as_array()
            .ok_or("changed_seams must be an array")?;
        if retained {
            assert!(!seams.is_empty(), "{case}: supported subject vanished");
            assert!(
                seams
                    .iter()
                    .all(|seam| seam["file"] == "src/lib.rs" && seam["line"] == 2),
                "{case}: {seams:?}"
            );
        } else {
            assert!(seams.is_empty(), "{case}: unexpected subjects {seams:?}");
        }
        let limitations = outcome["limitations"]
            .as_array()
            .ok_or("limitations must be an array")?;
        if let Some(expected) = limitation {
            assert!(
                limitations.iter().any(|item| item["kind"] == expected),
                "{case}: {limitations:?}"
            );
        } else {
            assert!(limitations.is_empty(), "{case}: {limitations:?}");
        }
        assert_eq!(
            report["runtime_status"]["diff"]["downstream_consumable"], complete,
            "{case}"
        );
        let stderr = String::from_utf8(output.stderr)?;
        // Setup and semantic assertions precede the process-status oracle.
        assert_eq!(output.status.code(), Some(exit), "{case}: {stderr}");
        assert_eq!(
            stderr.contains("diff: analysis could not complete (unsupported_input)"),
            exit == 2,
            "{case}: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn diff_human_unsupported_and_usage_keep_stream_contracts() -> TestResult {
    let root = unique_temp_workspace("diff-human-exit");
    init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    fs::write(root.join("src/conflict.rs"), CONFLICT)?;
    commit_fixture(&root)?;
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &["diff", "--root", ".", "--base", "HEAD~1"],
    )?;
    let stdout = String::from_utf8(output.stdout)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stdout.contains("analysis outcome: unsupported_input (analysis incomplete)."),
        "{stdout}"
    );
    assert!(
        !stdout.contains("diff: analysis could not complete"),
        "{stdout}"
    );
    assert!(
        stderr.contains("diff: analysis could not complete (unsupported_input)"),
        "{stderr}"
    );
    assert_eq!(output.status.code(), Some(2));
    let usage = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        None,
        &["diff", "--json", "--unknown-diff-exit-option"],
    )?;
    assert_eq!(usage.status.code(), Some(2));
    assert!(usage.stdout.is_empty());
    assert!(!usage.stderr.is_empty());
    let refused = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(&root),
        &[
            "diff",
            "--root",
            ".",
            "--base",
            "missing-diff-exit-ref",
            "--json",
        ],
    )?;
    let refusal: Value = serde_json::from_slice(&refused.stdout)?;
    assert_eq!(refusal["schema_version"], "0.1");
    assert_eq!(refusal["analysis_scope"]["run_status"], "base_unresolvable");
    assert_eq!(refused.status.code(), Some(2));
    assert!(!refused.stderr.is_empty());
    Ok(())
}

#[test]
fn diff_discovery_names_unsupported_and_partial_exits() -> TestResult {
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), None, &["help", "--json"])?;
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let document: Value = serde_json::from_slice(&output.stdout)?;
    let diff = document["commands"]
        .as_array()
        .ok_or("commands missing")?
        .iter()
        .find(|row| row["id"] == "cmd:diff")
        .ok_or("diff metadata missing")?;
    assert_eq!(diff["exit"]["unsupported_input"], 2);
    assert_eq!(diff["exit"]["partial_with_limitations"], 0);
    assert_eq!(diff["exit"]["findings_including_exposed"], 0);
    let help = run_command(env!("CARGO_BIN_EXE_ripr"), None, &["diff", "--help"])?;
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("`unsupported_input` reports exit 2"));
    Ok(())
}
