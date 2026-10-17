use super::{
    init_git_fixture_repo, replay_printed_ripr, run_command, unique_temp_workspace, workspace_root,
};
use serde_json::Value;
use std::path::{Path, PathBuf};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn fixture(name: &str, parent: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let source = workspace_root().join("fixtures").join(name);
    let root = parent.join(name);
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::copy(source.join("input/Cargo.toml"), root.join("Cargo.toml"))?;
    std::fs::copy(source.join("input/src/lib.rs"), root.join("src/lib.rs"))?;
    if source.join("input/tests").exists() {
        std::fs::create_dir_all(root.join("tests"))?;
        for entry in std::fs::read_dir(source.join("input/tests"))? {
            let entry = entry?;
            std::fs::copy(entry.path(), root.join("tests").join(entry.file_name()))?;
        }
    }
    std::fs::copy(source.join("diff.patch"), root.join("diff.patch"))?;
    init_git_fixture_repo(&root)?;
    Ok(root)
}

/// Literal fixture identities and stage/reason pins challenge a fake repair
/// that reclassifies unknowns, appends generic escalation, or varies by mode.
/// The human drill-in must carry the same guidance as machine check output,
/// then its real scope-preserving context command must select this finding.
#[test]
fn unknown_explain_real_reasons_have_supported_actions_in_every_mode() -> TestResult {
    let scratch = unique_temp_workspace("unknown-explain");
    let result: TestResult = (|| {
        std::fs::create_dir_all(&scratch)?;
        for (name, id, class, reason, required) in [
            (
                "propagate_stdout_macro",
                "probe:src_lib.rs:call_deletion:75eda2b6",
                "propagation_unknown",
                "proc_macro_opaque",
                vec!["macro", "output or effect", "original and changed"],
            ),
            (
                "helper_chain_one_hop",
                "probe:src_lib.rs:predicate:ac178c7f",
                "infection_unknown",
                "infection_evidence_unknown",
                vec![
                    "local or computed operand",
                    "original and changed",
                    "ripr.toml",
                    "does not support",
                ],
            ),
            (
                "constant_declaration_probe_reconciliation",
                "probe:src_lib.rs:static_unknown:c48715e6",
                "static_unknown",
                "no_changed_rust_line",
                vec!["declaration", "consumes", "original and changed"],
            ),
        ] {
            let root = fixture(name, &scratch)?;
            let root_arg = root.display().to_string();
            let diff_arg = root.join("diff.patch").display().to_string();
            let mut first = None;
            for mode in ["draft", "fast", "deep", "ready"] {
                let check = run_command(
                    env!("CARGO_BIN_EXE_ripr"),
                    Some(&scratch),
                    &[
                        "check", "--root", &root_arg, "--diff", &diff_arg, "--mode", mode,
                        "--format", "json",
                    ],
                )?;
                assert!(check.status.success(), "{name}/{mode}: {check:?}");
                let document: Value = serde_json::from_slice(&check.stdout)?;
                let findings = document["findings"].as_array().ok_or("no findings array")?;
                assert!(!findings.is_empty(), "{name}/{mode} analyzed no findings");
                let finding = findings
                    .iter()
                    .find(|f| f["id"] == id)
                    .ok_or("literal fixture finding absent")?;
                assert_eq!(finding["classification"], class, "{name}/{mode}");
                if name == "propagate_stdout_macro" {
                    assert_eq!(finding["ripr"]["observe"]["state"], "no");
                    assert_eq!(finding["ripr"]["discriminate"]["state"], "no");
                    let related = finding["related_tests"]
                        .as_array()
                        .ok_or("no related tests")?;
                    assert_eq!(related.len(), 1);
                    assert_eq!(related[0]["name"], "report_does_not_panic");
                    assert_eq!(related[0]["oracle_strength"], "none");
                    assert_eq!(related[0]["oracle"], "");
                    assert_eq!(related[0]["miss"], "no_assertion");
                }
                assert!(
                    finding["stop_reasons"]
                        .as_array()
                        .ok_or("no stop reasons")?
                        .iter()
                        .any(|r| r == reason)
                );
                let invariant = (
                    finding["classification"].clone(),
                    finding["confidence"].clone(),
                    finding["ripr"].clone(),
                    finding["stop_reasons"].clone(),
                );
                if let Some(expected) = &first {
                    assert_eq!(&invariant, expected, "{name}/{mode}");
                } else {
                    first = Some(invariant);
                }
                let step = finding["recommended_next_step"]
                    .as_str()
                    .ok_or("unknown has no action")?;
                for need in &required {
                    assert!(step.contains(need), "{name}/{mode} missing {need}: {step}");
                }
                if name == "propagate_stdout_macro" {
                    assert!(
                        step.contains("what the related test actually checks"),
                        "{step}"
                    );
                    assert!(step.contains("If it has an assertion"), "{step}");
                    assert!(!step.contains("an existing test observes"), "{step}");
                    assert!(!step.contains("inspect its exact assertion"), "{step}");
                }
                assert!(step.contains("project's configured test runner"), "{step}");
                assert!(
                    step.contains("`cargo test` for ordinary Cargo tests"),
                    "{step}"
                );
                assert!(step.contains("cargo test"), "{step}");
                assert!(step.contains("may remain unknown"), "{step}");
                assert!(
                    !step.contains("deep static analysis")
                        && !step.contains("use deep mode")
                        && !step.contains("teach ripr"),
                    "{step}"
                );
                let explain = run_command(
                    env!("CARGO_BIN_EXE_ripr"),
                    Some(&scratch),
                    &[
                        "explain", "--root", &root_arg, "--diff", &diff_arg, "--mode", mode, id,
                    ],
                )?;
                assert!(explain.status.success(), "{explain:?}");
                let text = String::from_utf8(explain.stdout)?;
                assert!(text.contains("To investigate this uncertainty:"), "{text}");
                assert!(
                    !text.contains("To change the verdict, a test needs to:"),
                    "{text}"
                );
                let section = text
                    .split_once("To investigate this uncertainty:\n")
                    .ok_or("no uncertainty section")?
                    .1;
                let body = section
                    .split("  Stop reasons:\n")
                    .next()
                    .ok_or("no section body")?
                    .trim();
                assert_eq!(
                    body,
                    format!("- {step}"),
                    "uncertainty-section/JSON advice differs: {text}"
                );
                assert!(
                    !text.contains("deep static analysis") && !text.contains("use deep mode"),
                    "{text}"
                );
                let next = text
                    .lines()
                    .find_map(|line| line.strip_prefix("Next: "))
                    .ok_or("no scoped context action")?;
                let context = replay_printed_ripr(next, &scratch)?;
                assert!(context.status.success(), "{context:?}");
                assert!(
                    String::from_utf8(context.stdout)?.contains(id),
                    "context selected a different finding"
                );
            }
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}

/// Retain the useful recipes for decidable findings. Unknown work must not
/// turn a confirmed gap into generic uncertainty or erase its specific need.
#[test]
fn unknown_explain_repair_preserves_decidable_recipes() -> TestResult {
    let scratch = unique_temp_workspace("decidable-explain");
    let result: TestResult = (|| {
        std::fs::create_dir_all(&scratch)?;
        for (name, class, need) in [
            (
                "boundary_gap",
                "weakly_exposed",
                "use an input that reaches `amount == discount_threshold`",
            ),
            (
                "error_path_oracle_execution_no_assertion",
                "reachable_unrevealed",
                "assert on what `read_all` returns or changes",
            ),
            (
                "all_no_path_disclosure",
                "no_static_path",
                "call `apply_fee(amount: i32, fee: i32) -> i32` and assert on its result",
            ),
        ] {
            let root = fixture(name, &scratch)?;
            let root_arg = root.display().to_string();
            let diff_arg = root.join("diff.patch").display().to_string();
            let check = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&scratch),
                &[
                    "check", "--root", &root_arg, "--diff", &diff_arg, "--format", "json",
                ],
            )?;
            assert!(check.status.success(), "{check:?}");
            let document: Value = serde_json::from_slice(&check.stdout)?;
            let finding = document["findings"]
                .as_array()
                .ok_or("no findings")?
                .iter()
                .find(|f| f["classification"] == class)
                .ok_or("expected decidable class absent")?;
            let id = finding["id"].as_str().ok_or("no id")?;
            let explain = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&scratch),
                &["explain", "--root", &root_arg, "--diff", &diff_arg, id],
            )?;
            assert!(explain.status.success(), "{explain:?}");
            let text = String::from_utf8(explain.stdout)?;
            assert!(
                text.contains("To change the verdict, a test needs to:"),
                "{text}"
            );
            assert!(text.contains(need), "{text}");
            assert!(!text.contains("To investigate this uncertainty:"), "{text}");
        }
        Ok(())
    })();
    std::fs::remove_dir_all(&scratch)?;
    result
}
