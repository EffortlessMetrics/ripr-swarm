//! Regression pins for the `ripr agent` adapter's refusal classification,
//! recovery narration and argument-validation errors that the sibling
//! `tests` module and the integration suites do not reach.

use super::super::tests::{args, unique_command_test_dir};
use super::*;
use crate::app::repair_attempt::{
    REPAIR_ATTEMPT_AFTER_REFUSAL_BUDGET, RepairAttemptId, ResolvedRepairAttempt,
};
use crate::edit_cage::{CagePathRule, EditCagePolicy};

fn sample_attempt() -> Result<ResolvedRepairAttempt, String> {
    Ok(ResolvedRepairAttempt {
        attempt_id: RepairAttemptId::parse("repair-attempt-0123456789abcdef01234567".to_string())?,
        seam_id: "seam:sample".to_string(),
        repository_head: "abc123".to_string(),
        manifest_path: PathBuf::from("attempt.json"),
        before_snapshot_path: PathBuf::from("before.json"),
        packet_path: PathBuf::from("packet.json"),
    })
}

fn sample_policy(inline_test_module_target: bool) -> Result<EditCagePolicy, String> {
    let target = CagePathRule::exact("src/lib.rs")?;
    Ok(EditCagePolicy {
        selected_target: target.clone(),
        allowed_edit_surface: vec![target],
        forbidden_paths: Vec::new(),
        expected_operational_writes: Vec::new(),
        ignored_build_output: None,
        untracked_build_lockfile: None,
        inline_test_module_target,
    })
}

fn require_contains(haystack: &str, fragments: &[&str]) -> Result<(), String> {
    for fragment in fragments {
        if !haystack.contains(fragment) {
            return Err(format!("expected `{fragment}` in:\n{haystack}"));
        }
    }
    Ok(())
}

#[test]
fn verify_typed_refusal_classifier_matches_only_whole_or_leading_clauses() {
    assert!(agent_verify_error_is_typed_refusal(
        AGENT_VERIFY_INPUT_DRIFT_ERROR
    ));
    assert!(agent_verify_error_is_typed_refusal(&format!(
        "{AGENT_VERIFY_NO_MOVEMENT_PREFIX}: nothing changed"
    )));
    assert!(agent_verify_error_is_typed_refusal(&format!(
        "{}: gap `g` is still open",
        crate::agent::artifact::GAP_BINDING_REFUSAL_PREFIX
    )));
    // A path-bearing operational error that merely echoes refusal text must
    // stay a failure, as must a drift message with trailing text.
    assert!(!agent_verify_error_is_typed_refusal(&format!(
        "failed to read agent verify --before snapshot dir/{AGENT_VERIFY_INPUT_DRIFT_ERROR}"
    )));
    assert!(!agent_verify_error_is_typed_refusal(&format!(
        "{AGENT_VERIFY_INPUT_DRIFT_ERROR} (extra)"
    )));
    assert!(!agent_verify_error_is_typed_refusal(
        "agent verify artifacts are incomparable: repository heads differ"
    ));
    assert!(!agent_verify_error_is_typed_refusal(""));
}

#[test]
fn after_refusal_reason_folds_narration_into_sentences() {
    assert_eq!(
        after_refusal_reason("  refused here.  ", &[]),
        "refused here"
    );
    let narration = vec![
        "attempt `a` stays open.".to_string(),
        "   ".to_string(),
        String::new(),
        "to recover: rerun.".to_string(),
    ];
    assert_eq!(
        after_refusal_reason("refused here.", &narration),
        "refused here. Attempt `a` stays open. To recover: rerun"
    );
}

#[test]
fn after_refusal_json_is_one_versioned_document_with_trimmed_error() -> Result<(), String> {
    let narration = vec!["line one".to_string(), "line two".to_string()];
    let rendered = render_repair_after_refusal_json("att-1", "  not allowed\n", &narration)?;
    if !rendered.ends_with("}\n") {
        return Err(format!("document must end with a newline: {rendered:?}"));
    }
    let value: serde_json::Value =
        serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
    assert_eq!(value["schema_version"], REPAIR_AFTER_REFUSAL_SCHEMA_VERSION);
    assert_eq!(value["kind"], "repair_after_refusal");
    assert_eq!(value["attempt_id"], "att-1");
    assert_eq!(value["error"], "not allowed");
    assert_eq!(
        value["narration"],
        serde_json::json!(["line one", "line two"])
    );
    Ok(())
}

#[test]
fn short_head_truncates_long_heads_and_keeps_short_or_unsliceable_ones() {
    assert_eq!(short_head("0123456789abcdef"), "0123456789ab");
    assert_eq!(short_head("0123456789ab"), "0123456789ab");
    assert_eq!(short_head("abc"), "abc");
    assert_eq!(short_head(""), "");
    // A multi-byte character straddling byte 12 cannot be sliced: the whole
    // head is returned rather than panicking.
    let straddling = "01234567890\u{e9}tail";
    assert_eq!(short_head(straddling), straddling);
}

#[test]
fn agent_start_out_dir_anchors_relative_dirs_at_root_only() {
    let root = Path::new("some-root");
    assert_eq!(
        resolve_agent_start_out_dir(root, Path::new("out")),
        Path::new("some-root").join("out")
    );
    let absolute = std::env::temp_dir().join("ripr-start-out");
    assert_eq!(resolve_agent_start_out_dir(root, &absolute), absolute);
}

#[test]
fn repair_after_retry_budget_lines_name_the_spent_budget_and_new_attempt_route()
-> Result<(), String> {
    let attempt = sample_attempt()?;
    let lines = repair_after_retry_budget_lines(Path::new("some-checkout"), None, &attempt, 5);
    if lines.len() != 3 {
        return Err(format!("expected cause, inspect, recover lines: {lines:?}"));
    }
    require_contains(
        &lines[0],
        &[
            "recorded 5 after-phase refusals",
            &format!("{REPAIR_ATTEMPT_AFTER_REFUSAL_BUDGET}-refusal retry budget"),
            "did not run again",
        ],
    )?;
    require_contains(
        &lines[1],
        &["ripr agent status", "--attempt", "repair-attempt-"],
    )?;
    require_contains(
        &lines[2],
        &["to recover:", "--seam-id", "seam:sample", "--phase before"],
    )?;
    if lines.iter().any(|line| line.contains("--phase after")) {
        return Err(format!("a spent budget must not advise a retry: {lines:?}"));
    }
    Ok(())
}

#[test]
fn repair_after_no_movement_lines_advise_retry_until_the_budget_is_spent() -> Result<(), String> {
    let attempt = sample_attempt()?;
    let policy = sample_policy(false)?;
    let root = Path::new("some-checkout");

    let first = repair_after_no_movement_lines(root, None, &attempt, &policy, 0);
    assert_eq!(first.len(), 3, "{first:?}");
    require_contains(&first[0], &["remains `awaiting_edit`"])?;
    require_contains(
        &first[1],
        &[
            "to recover: make the focused test edit in `src/lib.rs`",
            "leave production code unchanged",
            "rerun this after-phase command",
        ],
    )?;
    require_contains(&first[2], &["ripr agent status", "--attempt"])?;
    if first.iter().any(|line| line.contains("retry budget")) {
        return Err(format!(
            "first refusal must not warn about budget: {first:?}"
        ));
    }

    let repeated = repair_after_no_movement_lines(root, None, &attempt, &policy, 1);
    assert_eq!(repeated.len(), 4, "{repeated:?}");
    require_contains(
        &repeated[1],
        &[
            "after-phase refusal 2 of the 5-refusal retry budget",
            "discriminator may be wrong",
        ],
    )?;

    let inline = repair_after_no_movement_lines(root, None, &attempt, &sample_policy(true)?, 0);
    require_contains(
        &inline[1],
        &[
            "add new test functions inside the existing `#[cfg(test)]` module of `src/lib.rs`",
            "existing tests unchanged",
        ],
    )?;

    let spent = repair_after_no_movement_lines(
        root,
        None,
        &attempt,
        &policy,
        REPAIR_ATTEMPT_AFTER_REFUSAL_BUDGET - 1,
    );
    assert_eq!(spent.len(), 3, "{spent:?}");
    require_contains(
        &spent[0],
        &[
            "recorded after-phase refusal 5 and spent the 5-refusal retry budget",
            "next after phase refuses without running",
        ],
    )?;
    require_contains(&spent[1], &["--seam-id", "--phase before"])?;
    if spent.iter().any(|line| line.contains("--phase after")) {
        return Err(format!(
            "the budget-spending refusal must not advise a retry: {spent:?}"
        ));
    }
    // A saturated counter still reads as spent instead of overflowing.
    let saturated = repair_after_no_movement_lines(root, None, &attempt, &policy, u32::MAX);
    require_contains(&saturated[0], &["spent the 5-refusal retry budget"])?;
    Ok(())
}

#[test]
fn before_phase_dirty_premise_refusal_names_paths_and_commit_first_recovery() {
    let violations = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
    let message = before_phase_dirty_premise_refusal(
        "seam:sample",
        Path::new("some-checkout"),
        None,
        &violations,
    );
    for fragment in [
        "repair attempt cannot start",
        "differ from HEAD: src/a.rs, src/b.rs",
        "`git commit -- src/a.rs`",
        "--seam-id seam:sample --phase before",
        "No workflow was prepared and no repair attempt was started.",
    ] {
        assert!(
            message.contains(fragment),
            "missing `{fragment}`: {message}"
        );
    }
    // With no violations the recovery still renders, naming no path.
    let empty = before_phase_dirty_premise_refusal_with_restart("ripr restart", &[]);
    assert!(empty.contains("differ from HEAD: ."), "{empty}");
    assert!(empty.contains("`git commit -- `"), "{empty}");
    assert!(empty.contains("rerun `ripr restart`"), "{empty}");
}

#[test]
fn seam_id_translation_passes_plain_ids_through_without_resolving() -> Result<(), String> {
    // A plain structured seam id has no namespace marker, so it must reach
    // the packet-owned not-found refusal untouched, even for a root that
    // does not exist.
    let translated =
        translate_agent_repair_seam_id(Path::new("target/ripr/no-such-root"), "f3c9e4d21a0b7c88")
            .map_err(|error| format!("{error:?}"))?;
    assert_eq!(translated, "f3c9e4d21a0b7c88");
    Ok(())
}

#[test]
fn unknown_seam_hint_falls_back_to_the_pilot_listing_for_other_shapes() {
    let hint = unknown_seam_id_hint(Path::new("some-checkout"), "f3c9e4d21a0b7c88");
    assert!(hint.starts_with("Run `ripr pilot --root "), "{hint}");
    assert!(hint.contains("current seam IDs"), "{hint}");
    assert!(!hint.contains("finding ID"), "{hint}");
}

#[test]
fn policy_omitted_hint_routes_to_the_brief_not_a_relisting() {
    let hint = policy_omitted_seam_hint(Path::new("some-checkout"), "seam one");
    assert!(
        hint.contains("ripr agent brief --root") && hint.contains("--json"),
        "{hint}"
    );
    assert!(
        hint.contains("'seam one'"),
        "seam id must be shell-quoted: {hint}"
    );
    assert!(
        hint.contains("re-listing cannot change this policy decision"),
        "{hint}"
    );
}

#[test]
fn agent_receipt_refuses_a_missing_root_with_failure() {
    let result = agent(&args(&[
        "receipt",
        "--root",
        "target/ripr/missing-agent-refusal-root",
        "--verify-json",
        "agent-verify.json",
        "--seam-id",
        "seam-a",
        "--json",
    ]));
    assert!(
        matches!(
            &result,
            Err(CommandError::Failure(message))
                if message.contains("is not a directory")
                    && message.contains("target/ripr/missing-agent-refusal-root")
        ),
        "{result:?}"
    );
}

/// `agent verify` has no up-front root check: a missing root fails closed
/// at snapshot-path validation, before either snapshot is read.
#[test]
fn agent_verify_refuses_a_missing_root_before_reading_snapshots() {
    let result = agent(&args(&[
        "verify",
        "--root",
        "target/ripr/missing-agent-refusal-root",
        "--before",
        "before.json",
        "--after",
        "after.json",
        "--json",
    ]));
    assert!(
        matches!(
            &result,
            Err(CommandError::Failure(message))
                if message.contains("canonicalize agent verify root")
                    && message.contains("target/ripr/missing-agent-refusal-root")
        ),
        "{result:?}"
    );
}

#[test]
fn agent_verify_execute_refuses_a_missing_root_as_failure_not_decision() {
    // The unusable-root disposition is printed on stdout, but the exit stays
    // the operational failure code: the command could not complete.
    let result = agent(&args(&[
        "verify-execute",
        "--root",
        "target/ripr/missing-agent-verify-execute-root",
        "--packet",
        "packet.json",
        "--result-json",
        "result.json",
        "--json",
    ]));
    assert!(
        matches!(
            &result,
            Err(CommandError::Failure(message)) if message.contains("is not a directory")
        ),
        "{result:?}"
    );
    if let Err(error) = result {
        assert_eq!(error.exit_code(), 2);
    }
}

#[test]
fn agent_status_refuses_a_malformed_attempt_id_before_reading_artifacts() -> Result<(), String> {
    let root = unique_command_test_dir("agent-status-bad-attempt");
    std::fs::create_dir_all(&root).map_err(|error| format!("create temp root: {error}"))?;
    let result = agent(&args(&[
        "status",
        "--root",
        &root.display().to_string(),
        "--attempt",
        "not-a-repair-attempt-id",
        "--json",
    ]));
    let _ = std::fs::remove_dir_all(&root);
    match result {
        Err(CommandError::Failure(message)) if message.starts_with("agent status --attempt: ") => {
            Ok(())
        }
        other => Err(format!("expected a malformed-id failure, got {other:?}")),
    }
}

#[test]
fn agent_status_accepts_an_out_dir_that_names_the_default_workflow_dir() -> Result<(), String> {
    let root = unique_command_test_dir("agent-status-default-out");
    std::fs::create_dir_all(&root).map_err(|error| format!("create temp root: {error}"))?;
    let default_out = agent(&args(&[
        "status",
        "--root",
        &root.display().to_string(),
        "--out",
        "target/ripr/workflow",
        "--json",
    ]));
    let elsewhere = agent(&args(&[
        "status",
        "--root",
        &root.display().to_string(),
        "--out",
        "elsewhere",
        "--json",
    ]));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(default_out, Ok(()));
    match elsewhere {
        Err(CommandError::Failure(message))
            if message.contains("agent status --out elsewhere is not supported") =>
        {
            Ok(())
        }
        other => Err(format!("a foreign --out must fail closed, got {other:?}")),
    }
}

#[test]
fn agent_receipt_with_an_unknown_attempt_refuses_before_reading_the_verify_json()
-> Result<(), String> {
    let root = unique_command_test_dir("agent-receipt-unknown-attempt");
    std::fs::create_dir_all(&root).map_err(|error| format!("create temp root: {error}"))?;
    let result = agent(&args(&[
        "receipt",
        "--root",
        &root.display().to_string(),
        "--verify-json",
        &root.join("agent-verify.json").display().to_string(),
        "--seam-id",
        "seam-a",
        "--attempt",
        "repair-attempt-0123456789abcdef01234567",
        "--json",
    ]));
    let _ = std::fs::remove_dir_all(&root);
    match result {
        Err(CommandError::Failure(message)) if !message.contains("agent-verify.json") => Ok(()),
        other => Err(format!(
            "attempt binding must refuse before the verify read: {other:?}"
        )),
    }
}

#[test]
fn agent_receipt_with_a_malformed_attempt_id_is_a_failure() -> Result<(), String> {
    let root = unique_command_test_dir("agent-receipt-bad-attempt");
    std::fs::create_dir_all(&root).map_err(|error| format!("create temp root: {error}"))?;
    let result = agent(&args(&[
        "receipt",
        "--root",
        &root.display().to_string(),
        "--verify-json",
        "agent-verify.json",
        "--seam-id",
        "seam-a",
        "--attempt",
        "nope",
        "--json",
    ]));
    let _ = std::fs::remove_dir_all(&root);
    match result {
        Err(CommandError::Failure(_)) => Ok(()),
        other => Err(format!("expected a failure, got {other:?}")),
    }
}

#[test]
fn json_brief_with_a_missing_root_answers_with_the_root_unusable_envelope() {
    let result = agent(&args(&[
        "brief",
        "--root",
        "target/ripr/missing-agent-brief-json-root",
        "--diff",
        "change.diff",
        "--json",
    ]));
    assert!(
        matches!(
            &result,
            Err(CommandError::Failure(message)) if message.contains("is not a directory")
        ),
        "{result:?}"
    );
}

#[test]
fn agent_packet_unknown_seam_is_a_decision_not_a_failure() -> Result<(), String> {
    let root = unique_command_test_dir("agent-packet-unknown-seam");
    std::fs::create_dir_all(root.join("src")).map_err(|error| format!("create src: {error}"))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"unknown_seam_fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| format!("write manifest: {error}"))?;
    std::fs::write(root.join("src/lib.rs"), "pub fn one() -> i32 {\n    1\n}\n")
        .map_err(|error| format!("write source: {error}"))?;
    let result = agent(&args(&[
        "packet",
        "--root",
        &root.display().to_string(),
        "--seam-id",
        "0000000000000000",
        "--json",
    ]));
    let _ = std::fs::remove_dir_all(&root);
    match result {
        Err(CommandError::Decision(message)) if message.contains("0000000000000000") => Ok(()),
        other => Err(format!("an unknown seam id must be a decision: {other:?}")),
    }
}
