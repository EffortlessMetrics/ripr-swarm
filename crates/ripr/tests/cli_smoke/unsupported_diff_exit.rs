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

#[test]
fn edited_binary_rename_is_unsupported_for_check_and_diff() -> TestResult {
    let root = unique_temp_workspace("binary-rename-outcome");
    init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    run_git(&root, &["config", "diff.renames", "true"])?;
    let payload: String = (0..128)
        .map(|row| format!("row-{row:03}:\0{}\n", "a".repeat(80)))
        .collect();
    fs::write(root.join("old.bin"), &payload)?;
    run_git(&root, &["add", "old.bin"])?;
    commit_fixture(&root)?;
    fs::rename(root.join("old.bin"), root.join("new.bin"))?;
    run_git(&root, &["add", "-A"])?;
    commit_fixture(&root)?;
    let edited = payload.replace("row-127:\0a", "row-127:\0z");
    assert_ne!(payload, edited, "binary content change must be real");
    fs::write(root.join("new.bin"), edited)?;
    run_git(&root, &["add", "new.bin"])?;
    commit_fixture(&root)?;

    for (case, head, kind, complete, exit) in [
        ("pure", "HEAD~1", "no_changed_lines", true, 0),
        ("edited", "HEAD", "unsupported_input", false, 2),
    ] {
        let range = format!("HEAD~2...{head}");
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
                "--find-renames",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--unified=0",
                "--src-prefix=a/",
                "--dst-prefix=b/",
                "--inter-hunk-context=0",
                &range,
            ],
        )?;
        assert!(patch.status.success(), "{case}: Git diff setup failed");
        let patch_text = String::from_utf8(patch.stdout.clone())?;
        assert!(patch_text.contains("rename from old.bin\n"), "{patch_text}");
        assert!(patch_text.contains("rename to new.bin\n"), "{patch_text}");
        assert!(!patch_text.contains("@@"), "{patch_text}");
        assert!(!patch_text.contains("deleted file mode"), "{patch_text}");
        assert_eq!(
            patch_text.contains("Binary files a/old.bin and b/new.bin differ"),
            !complete,
            "{case}: fixture must distinguish rename from content change: {patch_text}"
        );
        let patch_path = root.join(format!("{case}.patch"));
        fs::write(&patch_path, &patch.stdout)?;
        let patch_arg = patch_path.to_string_lossy().into_owned();

        for command in ["check", "diff"] {
            let mut args = vec![command, "--root", "."];
            if command == "check" {
                args.extend(["--diff", patch_arg.as_str(), "--quiet"]);
            } else {
                args.extend(["--base", "HEAD~2", "--head", head]);
            }
            args.push("--json");
            let output = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&root), &args)?;
            let report: Value = serde_json::from_slice(&output.stdout)?;
            let outcome = &report["analysis_outcome"]["outcome"];
            // Establish the registered, nonempty subject before checking the
            // behavioral discriminator: an empty parse must not pass.
            assert_eq!(
                outcome["counts"]["changed_file_count"], 1,
                "{case}/{command}"
            );
            assert_eq!(
                outcome["counts"]["changed_line_count"], 0,
                "{case}/{command}"
            );
            assert_eq!(outcome["counts"]["probe_count"], 0, "{case}/{command}");
            assert_eq!(
                outcome["identity"]["input_identity"],
                format!("sha256:{:x}", Sha256::digest(&patch.stdout)),
                "{case}/{command}"
            );
            assert_eq!(outcome["kind"], kind, "{case}/{command}");
            assert_eq!(report["analysis_outcome"]["analysis_complete"], complete);
            let limitations = outcome["limitations"]
                .as_array()
                .ok_or("limitations missing")?;
            if complete {
                assert!(limitations.is_empty(), "{case}/{command}: {limitations:?}");
            } else {
                let limitation = limitations
                    .iter()
                    .find(|item| item["kind"] == "malformed_diff")
                    .ok_or("binary content must retain the typed non-text limitation")?;
                assert_eq!(limitation["producer_stage"], "diff_parse");
                assert_eq!(limitation["recovery"]["kind"], "retry");
                assert!(limitation["bounded_detail"].as_str().is_some_and(|text| text.contains("well formed") && text.contains("binary")));
                assert!(
                    limitation["recovery"]["detail"].as_str().is_some_and(
                        |text| text.contains("binary and file-mode changes are not analyzed")
                    )
                );
            }
            assert_eq!(
                output.status.code(),
                Some(exit),
                "{case}/{command}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let _ = args.pop();
            let human = run_command(env!("CARGO_BIN_EXE_ripr"), Some(&root), &args)?;
            if !complete {
                let stdout = String::from_utf8(human.stdout)?;
                assert!(
                    stdout.contains(if command == "check" {
                        "the input is not supported"
                    } else {
                        "analysis outcome: unsupported_input (analysis incomplete)."
                    }),
                    "{command}: {stdout}"
                );
            }
            assert_eq!(human.status.code(), Some(exit), "{case}/{command}/human");
        }
    }
    Ok(())
}
