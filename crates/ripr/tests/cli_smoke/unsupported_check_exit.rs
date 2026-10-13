//! The process status must agree with the producer's typed outcome (#7440).
use super::{
    DeclaredHunkScratch, init_producer_fixture_repo, run_command, run_ripr_with_stdin,
    unique_temp_workspace, workspace_root,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn check_json_outcome_exit_preserves_subject_and_coverage() -> TestResult {
    let root = unique_temp_workspace("unsupported-check-exit");
    init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let root_arg = root.to_string_lossy().into_owned();
    let valid = fs::read_to_string(workspace_root().join("fixtures/boundary_gap/diff.patch"))?;
    let partial = format!(
        "{valid}diff --git a/src/absent.rs b/src/absent.rs\n--- a/src/absent.rs\n+++ b/src/absent.rs\n@@ -0,0 +1,1 @@\n+pub fn absent() -> u8 {{ 1 }}\n"
    );
    let mixed = format!(
        "{valid}diff --git a/src/absent.rs b/src/absent.rs\n--- a/src/absent.rs\n+++ b/src/absent.rs\n@@ -0,0 +1,1 @@\n"
    );
    let header_only = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,7 +1,7 @@\n";
    for (label, patch, kind, complete, files, retained, limitation, exit) in [
        (
            "valid",
            valid.as_str(),
            "complete_with_findings",
            true,
            1,
            true,
            None,
            0,
        ),
        (
            "partial",
            partial.as_str(),
            "partial_with_limitations",
            false,
            2,
            true,
            Some("changed_file_absent_from_worktree"),
            0,
        ),
        ("empty", "", "no_scope", true, 0, false, None, 0),
        ("whitespace", " \n\t\n", "no_scope", true, 0, false, None, 0),
        (
            "garbage",
            "this is not a unified diff\n",
            "unsupported_input",
            false,
            0,
            false,
            Some("malformed_diff"),
            2,
        ),
        (
            "header-only",
            header_only,
            "unsupported_input",
            false,
            1,
            false,
            Some("malformed_diff"),
            2,
        ),
        (
            "mixed",
            mixed.as_str(),
            "unsupported_input",
            false,
            2,
            true,
            Some("malformed_diff"),
            2,
        ),
    ] {
        let patch_path = root.join("change.patch");
        fs::write(&patch_path, patch)?;
        let patch_arg = patch_path.to_string_lossy().into_owned();
        let named = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            None,
            &[
                "check", "--root", &root_arg, "--diff", &patch_arg, "--json", "--quiet",
            ],
        )?;
        let stdin = run_ripr_with_stdin(
            &[
                "check", "--root", &root_arg, "--diff", "-", "--json", "--quiet",
            ],
            patch.as_bytes(),
        )?;
        let mut named_outcome = None;
        for (channel, output) in [("file", named), ("stdin", stdin)] {
            // Parsing all bytes rejects prose, a second refusal document, and
            // a truncated output, rather than accepting a success-looking prefix.
            let report: Value = serde_json::from_slice(&output.stdout)?;
            assert_eq!(report["schema_version"], "0.2", "{label}/{channel}");
            assert_eq!(report["tool"], "ripr", "{label}/{channel}");
            assert_eq!(
                report["root"],
                root_arg.replace('\\', "/"),
                "{label}/{channel}"
            );
            assert_eq!(
                report["analysis_outcome"]["analysis_complete"], complete,
                "{label}/{channel}"
            );
            let outcome = &report["analysis_outcome"]["outcome"];
            assert_eq!(outcome["kind"], kind, "{label}/{channel}");
            assert_eq!(
                outcome["counts"]["changed_file_count"], files,
                "{label}/{channel}"
            );
            assert_eq!(
                outcome["identity"]["input_identity"],
                format!("sha256:{:x}", Sha256::digest(patch.as_bytes())),
                "{label}/{channel}"
            );
            let findings = report["findings"]
                .as_array()
                .ok_or("findings must be an array")?;
            if retained {
                assert!(
                    !findings.is_empty(),
                    "{label}/{channel}: the supported subject vanished"
                );
                assert!(
                    outcome["counts"]["probe_count"]
                        .as_u64()
                        .is_some_and(|count| count > 0)
                );
                assert!(
                    findings.iter().all(|finding| matches!(
                        finding["probe"]["file"].as_str(),
                        Some("src/lib.rs" | "./src/lib.rs")
                    ) && finding["probe"]["line"] == 2),
                    "{label}/{channel}: findings must identify the supplied change"
                );
            } else {
                assert!(findings.is_empty(), "{label}/{channel}");
                assert_eq!(outcome["counts"]["probe_count"], 0, "{label}/{channel}");
            }
            let limitations = outcome["limitations"]
                .as_array()
                .ok_or("limitations must be an array")?;
            if let Some(expected) = limitation {
                assert!(
                    limitations.iter().any(|item| item["kind"] == expected),
                    "{label}/{channel}: {limitations:?}"
                );
            } else {
                assert!(limitations.is_empty(), "{label}/{channel}");
            }
            if let Some(named) = named_outcome.as_ref() {
                assert_eq!(
                    named, outcome,
                    "{label}: file/stdin coverage and input identity diverged"
                );
            } else {
                named_outcome = Some(outcome.clone());
            }
            // Assert status after establishing the intended producer behavior:
            // setup, parsing, or empty-subject failures are not a RED witness.
            assert_eq!(
                output.status.code(),
                Some(exit),
                "{label}/{channel}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
    Ok(())
}

#[test]
fn unsupported_check_keeps_rendered_formats_and_repo_scope() -> TestResult {
    let root = unique_temp_workspace("unsupported-check-formats");
    init_producer_fixture_repo(&root)?;
    let _cleanup = DeclaredHunkScratch(root.clone());
    let root_arg = root.to_string_lossy().into_owned();
    let patch = root.join("garbage.patch");
    fs::write(&patch, "this is not a unified diff\n")?;
    let patch_arg = patch.to_string_lossy().into_owned();
    for format in ["human", "sarif", "badge-json", "badge-shields"] {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            None,
            &[
                "check", "--root", &root_arg, "--diff", &patch_arg, "--format", format, "--quiet",
            ],
        )?;
        if format == "human" {
            assert!(String::from_utf8_lossy(&output.stdout).contains("unsupported_input"));
        } else {
            let document: Value = serde_json::from_slice(&output.stdout)?;
            if format == "sarif" {
                assert_eq!(
                    document["runs"][0]["properties"]["analysis_outcome"]["kind"],
                    "unsupported_input"
                );
            } else if format == "badge-json" {
                assert_eq!(document["analysis_outcome"]["kind"], "unsupported_input");
                assert_eq!(document["analysis_complete"], false);
            } else {
                assert_eq!(
                    document["message"],
                    "analysis-incomplete: unsupported_input"
                );
            }
        }
        assert_eq!(
            output.status.code(),
            Some(2),
            "{format}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let repo = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        None,
        &[
            "check",
            "--root",
            &root_arg,
            "--diff",
            &patch_arg,
            "--format",
            "agent-seam-packets-json",
            "--quiet",
        ],
    )?;
    let document: Value = serde_json::from_slice(&repo.stdout)?;
    assert_eq!(document["analysis_outcome_status"], "not_applicable");
    assert_eq!(repo.status.code(), Some(0));
    Ok(())
}

#[test]
fn check_json_malformed_options_remain_empty_exit_two() -> TestResult {
    for args in [
        vec!["check", "--json", "--unknown-option-7440"],
        vec!["check", "--json", "--format"],
        vec!["check", "--json", "--mode", "nonsense"],
    ] {
        let output = run_command(env!("CARGO_BIN_EXE_ripr"), None, &args)?;
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            output.stdout.is_empty(),
            "{args:?}: usage must not masquerade as an analysis report"
        );
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
    Ok(())
}

#[test]
fn check_discovery_names_unsupported_and_partial_exits() -> TestResult {
    let output = run_command(env!("CARGO_BIN_EXE_ripr"), None, &["help", "--json"])?;
    assert_eq!(output.status.code(), Some(0));
    let document: Value = serde_json::from_slice(&output.stdout)?;
    let check = document["commands"]
        .as_array()
        .ok_or("machine discovery must enumerate commands")?
        .iter()
        .find(|row| row["id"] == "cmd:check")
        .ok_or("machine discovery must enumerate check")?;
    assert_eq!(check["exit"]["unsupported_input"], 2);
    assert_eq!(check["exit"]["partial_with_limitations"], 0);
    assert_eq!(check["exit"]["findings_including_exposed"], 0);
    let help = run_command(env!("CARGO_BIN_EXE_ripr"), None, &["check", "--help"])?;
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).contains("`unsupported_input` reports exit 2"));
    Ok(())
}
