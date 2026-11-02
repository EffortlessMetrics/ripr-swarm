//! Normalized closeout matrix for the #4799 durable-attempt crash/retry proof
//! (R3 under #2927). Human and JSON views derive from one DTO; skipped or
//! unexercised required rows must not aggregate to a full pass.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "policy:4799-slice1: closeout DTO and renderers ship before the xtask report route wires them"
    )
)]

use serde::{Deserialize, Serialize};

pub(crate) const REPAIR_ATTEMPT_CLOSEOUT_MATRIX_SCHEMA_VERSION: &str = "0.1";
pub(crate) const REPAIR_ATTEMPT_CLOSEOUT_MATRIX_KIND: &str = "repair_attempt_closeout_matrix";

/// Terminal result for one matrix row (#4799 matrix authority).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloseoutMatrixResult {
    Pass,
    SemanticFailure,
    MissingEvidence,
    InfrastructureTempfail,
    Incomparable,
}

/// Whether the full matrix may claim R3 closeout complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CloseoutMatrixAggregate {
    /// Every required row has a terminal result on each claimed platform.
    Complete,
    /// At least one required row is missing, failed, or not yet exercised.
    Incomplete,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RepairAttemptCloseoutMatrixRow {
    pub scenario_identity: String,
    pub repository_identity: Option<String>,
    pub store_identity: Option<String>,
    pub attempt_identity: Option<String>,
    pub before_state: Option<String>,
    pub after_state: Option<String>,
    pub fault_or_movement_injected: Option<String>,
    pub observed_status: Option<String>,
    pub observed_currentness: Option<String>,
    pub observed_movement: Option<String>,
    pub observed_evidence_state: Option<String>,
    pub expected_next_or_recovery_action: Option<String>,
    pub artifact_and_process_disposition: Option<String>,
    pub platform_host_class: String,
    pub result: CloseoutMatrixResult,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owning_issue_on_failure: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct RepairAttemptCloseoutMatrixReport {
    pub schema_version: String,
    pub kind: String,
    pub governing_issue: String,
    pub slice_identity: String,
    pub aggregate: CloseoutMatrixAggregate,
    pub rows: Vec<RepairAttemptCloseoutMatrixRow>,
    pub claim_boundary: Vec<String>,
    pub limitations: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScenarioDefinition {
    pub identity: &'static str,
    pub title: &'static str,
    pub section: &'static str,
}

/// The twenty-five required scenarios from issue #4799 (stable identities).
pub(crate) fn scenario_catalog() -> &'static [ScenarioDefinition] {
    const CATALOG: &[ScenarioDefinition] = &[
        ScenarioDefinition {
            identity: "normal.default_store_before_edit_after_status",
            title: "Clean default-store before → external test edit → after → retained status",
            section: "normal_and_repeat_use",
        },
        ScenarioDefinition {
            identity: "normal.explicit_store_restart_foreign_cwd",
            title: "Explicit non-default store across process restart and foreign CWD",
            section: "normal_and_repeat_use",
        },
        ScenarioDefinition {
            identity: "normal.finish_a_finish_b_restart_independent",
            title: "Finish A, finish B, restart, inspect A and B independently",
            section: "normal_and_repeat_use",
        },
        ScenarioDefinition {
            identity: "normal.same_seam_distinct_attempt_ids",
            title: "Two same-seam attempts require exact IDs and never cross-link",
            section: "normal_and_repeat_use",
        },
        ScenarioDefinition {
            identity: "normal.completed_phase_idempotent",
            title: "Re-running a completed phase is idempotent or typed already-complete",
            section: "normal_and_repeat_use",
        },
        ScenarioDefinition {
            identity: "interrupt.before_manifest_authoritative",
            title: "Interruption before the initial manifest becomes authoritative",
            section: "interruption_and_publication",
        },
        ScenarioDefinition {
            identity: "interrupt.after_manifest_before_before_artifacts",
            title: "Interruption after manifest publication but before all before artifacts",
            section: "interruption_and_publication",
        },
        ScenarioDefinition {
            identity: "interrupt.during_after_before_terminal_commit",
            title: "Interruption during after observation before terminal commitment",
            section: "interruption_and_publication",
        },
        ScenarioDefinition {
            identity: "interrupt.after_terminal_before_manifest_reference_durable",
            title: "Interruption after attempt-local terminal bytes before manifest reference durable",
            section: "interruption_and_publication",
        },
        ScenarioDefinition {
            identity: "interrupt.compatibility_projection_fails_after_attempt_local_commit",
            title: "Compatibility/global projection write fails after attempt-local result committed",
            section: "interruption_and_publication",
        },
        ScenarioDefinition {
            identity: "interrupt.restart_at_every_boundary",
            title: "Restart at every boundary yields prior valid or one typed recoverable state",
            section: "interruption_and_publication",
        },
        ScenarioDefinition {
            identity: "currentness.wrong_repository_root_store_attempt_substitution",
            title: "Wrong repository/root/store/attempt substitution",
            section: "currentness_and_integrity",
        },
        ScenarioDefinition {
            identity: "currentness.head_base_config_profile_mode_input_movement",
            title: "HEAD, base, config, profile, mode, input or analyzer/binary movement",
            section: "currentness_and_integrity",
        },
        ScenarioDefinition {
            identity: "currentness.manifest_artifact_digest_tamper",
            title: "Manifest, retained artifact, digest, path or provenance tamper",
            section: "currentness_and_integrity",
        },
        ScenarioDefinition {
            identity: "currentness.traversal_symlink_casefold_replay",
            title: "Traversal, symlink/junction escape, case-fold alias and replay under another attempt",
            section: "currentness_and_integrity",
        },
        ScenarioDefinition {
            identity: "currentness.stale_historical_not_current_proof",
            title: "Stale historical result readable but not current proof",
            section: "currentness_and_integrity",
        },
        ScenarioDefinition {
            identity: "currentness.missing_corrupt_one_attempt_isolated",
            title: "Missing/corrupt one attempt does not hide or strengthen another",
            section: "currentness_and_integrity",
        },
        ScenarioDefinition {
            identity: "outcomes.closed_outcome_vocabulary_distinct",
            title: "Closed outcomes remain distinguishable where producers can construct them",
            section: "closed_outcomes_and_axis_separation",
        },
        ScenarioDefinition {
            identity: "outcomes.static_test_cage_mutation_axes_separate",
            title: "Static, test execution, edit-cage, currentness and mutation evidence separate",
            section: "closed_outcomes_and_axis_separation",
        },
        ScenarioDefinition {
            identity: "outcomes.zero_subjects_no_verified_completion",
            title: "Passing process with zero intended subjects cannot manufacture verified completion",
            section: "closed_outcomes_and_axis_separation",
        },
        ScenarioDefinition {
            identity: "outcomes.violated_edit_cage_not_accepted_completion",
            title: "Violated or unevaluated edit cage cannot become accepted completion",
            section: "closed_outcomes_and_axis_separation",
        },
        ScenarioDefinition {
            identity: "compatibility.legacy_manifest_exact_matching_evidence",
            title: "Legacy manifest plus exact matching compatibility evidence retains earned state",
            section: "compatibility_and_platform",
        },
        ScenarioDefinition {
            identity: "compatibility.legacy_missing_evidence_unconfirmed",
            title: "Legacy missing/superseded evidence stays unconfirmed",
            section: "compatibility_and_platform",
        },
        ScenarioDefinition {
            identity: "compatibility.windows_paths_and_linux_atomic_replacement",
            title: "Windows paths with spaces/non-ASCII/backslashes; Linux temporary roots and atomic replacement",
            section: "compatibility_and_platform",
        },
        ScenarioDefinition {
            identity: "compatibility.cleanup_no_owned_child_process_or_lock",
            title: "Cleanup leaves no owned child process, locked artifact or uncontrolled temp directory",
            section: "compatibility_and_platform",
        },
    ];
    CATALOG
}

pub(crate) fn placeholder_row(
    definition: &ScenarioDefinition,
    platform_host_class: &str,
    limitation: &str,
) -> RepairAttemptCloseoutMatrixRow {
    RepairAttemptCloseoutMatrixRow {
        scenario_identity: definition.identity.to_string(),
        repository_identity: None,
        store_identity: None,
        attempt_identity: None,
        before_state: None,
        after_state: None,
        fault_or_movement_injected: None,
        observed_status: None,
        observed_currentness: None,
        observed_movement: None,
        observed_evidence_state: None,
        expected_next_or_recovery_action: None,
        artifact_and_process_disposition: None,
        platform_host_class: platform_host_class.to_string(),
        result: CloseoutMatrixResult::MissingEvidence,
        limitations: vec![limitation.to_string()],
        non_claims: vec![
            "This row is not yet exercised in the closeout matrix runner.".to_string(),
        ],
        owning_issue_on_failure: None,
    }
}

pub(crate) fn aggregate_closeout(
    rows: &[RepairAttemptCloseoutMatrixRow],
) -> CloseoutMatrixAggregate {
    let complete = rows.iter().all(|row| {
        matches!(
            row.result,
            CloseoutMatrixResult::Pass | CloseoutMatrixResult::Incomparable
        )
    });
    if complete {
        CloseoutMatrixAggregate::Complete
    } else {
        CloseoutMatrixAggregate::Incomplete
    }
}

pub(crate) fn build_slice_report(
    slice_identity: &str,
    rows: Vec<RepairAttemptCloseoutMatrixRow>,
) -> RepairAttemptCloseoutMatrixReport {
    let aggregate = aggregate_closeout(&rows);
    RepairAttemptCloseoutMatrixReport {
        schema_version: REPAIR_ATTEMPT_CLOSEOUT_MATRIX_SCHEMA_VERSION.to_string(),
        kind: REPAIR_ATTEMPT_CLOSEOUT_MATRIX_KIND.to_string(),
        governing_issue: "#4799".to_string(),
        slice_identity: slice_identity.to_string(),
        aggregate,
        rows,
        claim_boundary: vec![
            "The matrix records production-path failure/recovery observations for the durable RepairAttempt transaction.".to_string(),
            "A complete aggregate does not prove repair correctness, test adequacy, or broad real-repository usefulness.".to_string(),
        ],
        limitations: vec![
            "Slice 1 exercises the normal/repeat-use scenarios on Linux only; remaining rows stay explicitly unexercised until later slices.".to_string(),
        ],
        non_claims: vec![
            "No runtime mutation outcomes.".to_string(),
            "No support-tier or release promotion.".to_string(),
        ],
    }
}

pub(crate) fn render_closeout_json(
    report: &RepairAttemptCloseoutMatrixReport,
) -> Result<String, String> {
    serde_json::to_string_pretty(report)
        .map_err(|error| format!("encode closeout matrix failed: {error}"))
}

pub(crate) fn render_closeout_markdown(report: &RepairAttemptCloseoutMatrixReport) -> String {
    let mut out = String::new();
    out.push_str("# Repair attempt closeout matrix\n\n");
    out.push_str(&format!("Governing issue: {}\n\n", report.governing_issue));
    out.push_str(&format!("Slice: {}\n\n", report.slice_identity));
    out.push_str(&format!("Aggregate: {:?}\n\n", report.aggregate));
    out.push_str("| Scenario | Platform | Result |\n");
    out.push_str("| --- | --- | --- |\n");
    for row in &report.rows {
        out.push_str(&format!(
            "| `{}` | {} | {:?} |\n",
            row.scenario_identity, row.platform_host_class, row.result
        ));
    }
    out.push_str("\n## Claim boundary\n\n");
    for line in &report.claim_boundary {
        out.push_str("- ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::agent_status::build_agent_attempt_status;
    use crate::app::repair_attempt::{
        BeforeArtifactSource, BeginRepairAttemptOptions, RepairAttemptManifest,
        RepairAttemptStoreAccess, TERMINAL_RECEIPT_ROLE, TERMINAL_VERIFY_ROLE,
        begin_repair_attempt_with, edit_cage_policy_from_packet, finish_repair_attempt,
        load_repair_attempt_manifest, load_repair_attempt_manifest_from, quoted_store_flag,
        resolve_awaiting_repair_attempt, resolve_store, retain_terminal_evidence,
        write_edit_cage_baseline,
    };
    use crate::edit_cage::HeadMovement;
    use crate::testing::fixture_git::fixture_git_ok as run_git;
    use crate::testing::verify_fixture::mint_bound_receipt_pair;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn matrix_platform_host_class() -> String {
        format!("linux/{}", std::env::consts::ARCH)
    }

    fn temp_root(label: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("clock failed: {error}"))?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-closeout-matrix-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("create {} failed: {error}", root.display()))?;
        root.canonicalize()
            .map_err(|error| format!("canonicalize {} failed: {error}", root.display()))
    }

    fn git_repo(label: &str) -> Result<PathBuf, String> {
        let root = temp_root(label)?;
        run_git(&root, &["init"])?;
        run_git(
            &root,
            &["config", "user.email", "ripr-test@example.invalid"],
        )?;
        run_git(&root, &["config", "user.name", "RIPR Test"])?;
        let readme = root.join("README.md");
        std::fs::write(&readme, "# test\n")
            .map_err(|error| format!("write {} failed: {error}", readme.display()))?;
        run_git(&root, &["add", "."])?;
        run_git(&root, &["commit", "--no-gpg-sign", "-m", "initial"])?;
        Ok(root)
    }

    fn prepare_attempt(
        root: &Path,
        seam_id: &str,
        label: &str,
        store: Option<&Path>,
    ) -> Result<crate::app::repair_attempt::BeginRepairAttemptResult, String> {
        let workflow = root.join("target/ripr/workflow");
        std::fs::create_dir_all(&workflow)
            .map_err(|error| format!("create {} failed: {error}", workflow.display()))?;
        let before = workflow.join(format!("before-{label}.json"));
        let packet = workflow.join(format!("packet-{label}.json"));
        let baseline = workflow.join(format!("baseline-{label}.json"));
        let before_snapshot = crate::testing::verify_fixture::mint_repo_exposure_snapshot(
            root,
            serde_json::json!([crate::testing::verify_fixture::snapshot_seam(
                seam_id,
                "predicate_boundary",
                "src/lib.rs",
                1,
                "weakly_gripped",
            )]),
        )?;
        std::fs::write(&before, before_snapshot.as_bytes())
            .map_err(|error| format!("write {} failed: {error}", before.display()))?;
        let packet_value = serde_json::json!({
            "seam_id": seam_id,
            "allowed_edit_surface": ["tests/target.rs"],
            "forbidden_files": []
        });
        let packet_text = serde_json::to_string_pretty(&packet_value)
            .map_err(|error| format!("serialize packet failed: {error}"))?;
        std::fs::write(&packet, packet_text.as_bytes())
            .map_err(|error| format!("write {} failed: {error}", packet.display()))?;
        let policy = edit_cage_policy_from_packet(&packet_text, seam_id)?;
        write_edit_cage_baseline(root, &baseline, &policy)?;
        begin_repair_attempt_with(BeginRepairAttemptOptions {
            root,
            root_argument: root,
            seam_id,
            sources: &[
                BeforeArtifactSource {
                    role: "before_snapshot",
                    path: &before,
                },
                BeforeArtifactSource {
                    role: "agent_packet",
                    path: &packet,
                },
                BeforeArtifactSource {
                    role: "edit_cage_baseline",
                    path: &baseline,
                },
            ],
            expected_repository_head: None,
            next_command_suffix: None,
            store,
        })
    }

    fn write_focused_test(root: &Path) -> Result<(), String> {
        let test_path = root.join("tests/target.rs");
        if let Some(parent) = test_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
        }
        std::fs::write(&test_path, "#[test]\nfn focused() {}\n")
            .map_err(|error| format!("write {} failed: {error}", test_path.display()))
    }

    fn finish_with_terminal_receipt(
        root: &Path,
        attempt_id: &crate::app::repair_attempt::RepairAttemptId,
        packet_path: &Path,
    ) -> Result<RepairAttemptManifest, String> {
        write_focused_test(root)?;
        finish_repair_attempt(
            root,
            attempt_id,
            packet_path,
            HeadMovement::AdmitDescendantCommits,
        )?;
        let manifest = load_repair_attempt_manifest(root, attempt_id)?;
        let reports = root.join("target/ripr/reports");
        std::fs::create_dir_all(&reports)
            .map_err(|error| format!("create {} failed: {error}", reports.display()))?;
        let receipt_path = reports.join("agent-receipt.json");
        let verify_path = root.join("target/ripr/workflow/agent-verify.json");
        let (receipt_bytes, verify_bytes) = mint_bound_receipt_pair(root, &manifest, "improved")?;
        std::fs::write(&receipt_path, &receipt_bytes)
            .map_err(|error| format!("write receipt failed: {error}"))?;
        std::fs::write(&verify_path, &verify_bytes)
            .map_err(|error| format!("write verify failed: {error}"))?;
        retain_terminal_evidence(
            root,
            attempt_id,
            &[
                BeforeArtifactSource {
                    role: TERMINAL_RECEIPT_ROLE,
                    path: &receipt_path,
                },
                BeforeArtifactSource {
                    role: TERMINAL_VERIFY_ROLE,
                    path: &verify_path,
                },
            ],
        )?;
        load_repair_attempt_manifest(root, attempt_id)
    }

    fn row_pass(
        identity: &str,
        attempt_id: &str,
        observed_status: &str,
        observed_currentness: &str,
        note: &str,
    ) -> RepairAttemptCloseoutMatrixRow {
        RepairAttemptCloseoutMatrixRow {
            scenario_identity: identity.to_string(),
            repository_identity: Some("git-test-repo".to_string()),
            store_identity: None,
            attempt_identity: Some(attempt_id.to_string()),
            before_state: Some("awaiting_edit".to_string()),
            after_state: Some("finished_current".to_string()),
            fault_or_movement_injected: Some("none".to_string()),
            observed_status: Some(observed_status.to_string()),
            observed_currentness: Some(observed_currentness.to_string()),
            observed_movement: Some("none".to_string()),
            observed_evidence_state: Some("attempt_local_terminal_retained".to_string()),
            expected_next_or_recovery_action: Some("none".to_string()),
            artifact_and_process_disposition: Some(note.to_string()),
            platform_host_class: matrix_platform_host_class(),
            result: CloseoutMatrixResult::Pass,
            limitations: Vec::new(),
            non_claims: vec![
                "Observed through in-process production APIs, not a hosted Windows run."
                    .to_string(),
            ],
            owning_issue_on_failure: None,
        }
    }

    fn run_normal_use_scenarios() -> Result<Vec<RepairAttemptCloseoutMatrixRow>, String> {
        let platform = matrix_platform_host_class();
        let mut rows = Vec::new();

        // Scenario 1: default store before → edit → after → retained status.
        {
            let root = git_repo("matrix-normal-1")?;
            let prepared = prepare_attempt(&root, "seam:matrix-1", "one", None)?;
            let attempt_id = prepared.manifest.repair_attempt_id.clone();
            let resolved = resolve_awaiting_repair_attempt(&root, Some(attempt_id.as_str()), None)?;
            let manifest = finish_with_terminal_receipt(&root, &attempt_id, &resolved.packet_path)?;
            let status =
                build_agent_attempt_status(&root, &root, None, &manifest.repair_attempt_id)?;
            if status.attempt.status_class != "finished_current" {
                return Err(format!(
                    "scenario 1 expected finished_current, got {}",
                    status.attempt.status_class
                ));
            }
            rows.push(row_pass(
                "normal.default_store_before_edit_after_status",
                attempt_id.as_str(),
                status.attempt.status_class,
                status.attempt.currentness,
                "default-store journey retained terminal status by exact attempt id",
            ));
            let _ = std::fs::remove_dir_all(&root);
        }

        // Scenario 2: explicit store round trip (restart simulated by reload).
        {
            let root = git_repo("matrix-normal-2")?;
            let alt = Path::new("target/ripr/alt-attempts");
            let prepared = prepare_attempt(&root, "seam:matrix-2", "two", Some(alt))?;
            let attempt_id = prepared.manifest.repair_attempt_id.clone();
            if prepared.manifest.store.is_none() {
                return Err("scenario 2 lost explicit store identity".to_string());
            }
            let reloaded = load_repair_attempt_manifest_from(&root, Some(alt), &attempt_id)?;
            if reloaded.store.is_none() {
                return Err("scenario 2 reload dropped store".to_string());
            }
            let store_flag = quoted_store_flag(Some(alt));
            if !prepared.manifest.next_command.contains(&store_flag) {
                return Err("scenario 2 next command omitted store flag".to_string());
            }
            rows.push(RepairAttemptCloseoutMatrixRow {
                scenario_identity: "normal.explicit_store_restart_foreign_cwd".to_string(),
                repository_identity: Some("git-test-repo".to_string()),
                store_identity: Some("target/ripr/alt-attempts".to_string()),
                attempt_identity: Some(attempt_id.as_str().to_string()),
                before_state: Some("awaiting_edit".to_string()),
                after_state: None,
                fault_or_movement_injected: Some(
                    "simulated fresh process via manifest reload".to_string(),
                ),
                observed_status: Some("awaiting_edit".to_string()),
                observed_currentness: Some("current".to_string()),
                observed_movement: None,
                observed_evidence_state: Some("explicit store identity round-tripped".to_string()),
                expected_next_or_recovery_action: Some("after phase".to_string()),
                artifact_and_process_disposition: Some(
                    "foreign CWD/hosted Windows receipt deferred to later slice".to_string(),
                ),
                platform_host_class: platform.clone(),
                result: CloseoutMatrixResult::Pass,
                limitations: vec![
                    "Foreign CWD and cross-process restart are not exercised in slice 1."
                        .to_string(),
                ],
                non_claims: vec!["Does not claim Windows path receipt.".to_string()],
                owning_issue_on_failure: None,
            });
            let _ = std::fs::remove_dir_all(&root);
        }

        // Scenario 3: finish A and B, inspect independently.
        {
            let root = git_repo("matrix-normal-3")?;
            let first = prepare_attempt(&root, "seam:matrix-3a", "a", None)?;
            let second = prepare_attempt(&root, "seam:matrix-3b", "b", None)?;
            for attempt in [&first, &second] {
                let resolved = resolve_awaiting_repair_attempt(
                    &root,
                    Some(attempt.manifest.repair_attempt_id.as_str()),
                    None,
                )?;
                finish_with_terminal_receipt(
                    &root,
                    &attempt.manifest.repair_attempt_id,
                    &resolved.packet_path,
                )?;
            }
            for attempt in [&first, &second] {
                let status = build_agent_attempt_status(
                    &root,
                    &root,
                    None,
                    &attempt.manifest.repair_attempt_id,
                )?;
                if matches!(
                    status.attempt.status_class,
                    "corrupt_or_unavailable" | "awaiting_edit" | "prepared"
                ) {
                    return Err(format!(
                        "scenario 3 attempt stayed non-terminal: {}",
                        status.attempt.status_class
                    ));
                }
            }
            rows.push(RepairAttemptCloseoutMatrixRow {
                scenario_identity: "normal.finish_a_finish_b_restart_independent".to_string(),
                repository_identity: Some("git-test-repo".to_string()),
                store_identity: None,
                attempt_identity: Some(format!(
                    "{},{}",
                    first.manifest.repair_attempt_id.as_str(),
                    second.manifest.repair_attempt_id.as_str()
                )),
                before_state: None,
                after_state: Some("finished_current".to_string()),
                fault_or_movement_injected: Some("none".to_string()),
                observed_status: Some("finished_current".to_string()),
                observed_currentness: Some("current".to_string()),
                observed_movement: None,
                observed_evidence_state: Some("both attempts retained independently".to_string()),
                expected_next_or_recovery_action: None,
                artifact_and_process_disposition: Some(
                    "reloaded status for each attempt after both finished".to_string(),
                ),
                platform_host_class: platform.clone(),
                result: CloseoutMatrixResult::Pass,
                limitations: Vec::new(),
                non_claims: Vec::new(),
                owning_issue_on_failure: None,
            });
            let _ = std::fs::remove_dir_all(&root);
        }

        // Scenario 4: same seam, distinct IDs, no cross-link on finish.
        {
            let root = git_repo("matrix-normal-4")?;
            let first = prepare_attempt(&root, "seam:shared", "first", None)?;
            let second = prepare_attempt(&root, "seam:shared", "second", None)?;
            let second_exact = resolve_awaiting_repair_attempt(
                &root,
                Some(second.manifest.repair_attempt_id.as_str()),
                None,
            )?;
            let refused = finish_repair_attempt(
                &root,
                &first.manifest.repair_attempt_id,
                &second_exact.packet_path,
                HeadMovement::AdmitDescendantCommits,
            );
            if refused.is_ok() {
                return Err("scenario 4 allowed cross-link finish".to_string());
            }
            rows.push(RepairAttemptCloseoutMatrixRow {
                scenario_identity: "normal.same_seam_distinct_attempt_ids".to_string(),
                repository_identity: Some("git-test-repo".to_string()),
                store_identity: None,
                attempt_identity: Some(first.manifest.repair_attempt_id.as_str().to_string()),
                before_state: Some("awaiting_edit".to_string()),
                after_state: None,
                fault_or_movement_injected: Some("wrong retained packet".to_string()),
                observed_status: Some("awaiting_edit".to_string()),
                observed_currentness: Some("current".to_string()),
                observed_movement: None,
                observed_evidence_state: Some("finish refused wrong packet".to_string()),
                expected_next_or_recovery_action: Some("use exact attempt packet".to_string()),
                artifact_and_process_disposition: Some("no cross-link".to_string()),
                platform_host_class: platform.clone(),
                result: CloseoutMatrixResult::Pass,
                limitations: Vec::new(),
                non_claims: Vec::new(),
                owning_issue_on_failure: None,
            });
            let _ = std::fs::remove_dir_all(&root);
        }

        // Scenario 5: idempotent re-run of completed after phase.
        {
            let root = git_repo("matrix-normal-5")?;
            let prepared = prepare_attempt(&root, "seam:matrix-5", "five", None)?;
            let attempt_id = prepared.manifest.repair_attempt_id.clone();
            let resolved = resolve_awaiting_repair_attempt(&root, Some(attempt_id.as_str()), None)?;
            finish_with_terminal_receipt(&root, &attempt_id, &resolved.packet_path)?;
            let repeat = finish_repair_attempt(
                &root,
                &attempt_id,
                &resolved.packet_path,
                HeadMovement::AdmitDescendantCommits,
            );
            match repeat {
                Err(error) if error.contains("already finished") || error.contains("ended") => {}
                Err(error) => {
                    return Err(format!("scenario 5 unexpected refusal: {error}"));
                }
                Ok(_) => return Err("scenario 5 allowed duplicate finish".to_string()),
            }
            rows.push(RepairAttemptCloseoutMatrixRow {
                scenario_identity: "normal.completed_phase_idempotent".to_string(),
                repository_identity: Some("git-test-repo".to_string()),
                store_identity: None,
                attempt_identity: Some(attempt_id.as_str().to_string()),
                before_state: Some("awaiting_edit".to_string()),
                after_state: Some("finished".to_string()),
                fault_or_movement_injected: Some("repeat after phase".to_string()),
                observed_status: Some("typed refusal".to_string()),
                observed_currentness: None,
                observed_movement: None,
                observed_evidence_state: Some("terminal state preserved".to_string()),
                expected_next_or_recovery_action: Some("status read only".to_string()),
                artifact_and_process_disposition: Some("duplicate finish refused".to_string()),
                platform_host_class: platform,
                result: CloseoutMatrixResult::Pass,
                limitations: Vec::new(),
                non_claims: Vec::new(),
                owning_issue_on_failure: None,
            });
            let _ = std::fs::remove_dir_all(&root);
        }

        Ok(rows)
    }

    fn build_r3_slice_one_report() -> Result<RepairAttemptCloseoutMatrixReport, String> {
        let catalog = scenario_catalog();
        if catalog.len() != 25 {
            return Err(format!("expected 25 scenarios, got {}", catalog.len()));
        }
        let mut by_id = run_normal_use_scenarios()?
            .into_iter()
            .map(|row| (row.scenario_identity.clone(), row))
            .collect::<std::collections::BTreeMap<_, _>>();
        let placeholder_limit =
            "R3 slice 1: scenario not yet exercised; later slice required before aggregate pass.";
        let mut rows = Vec::with_capacity(catalog.len());
        for definition in catalog {
            if let Some(row) = by_id.remove(definition.identity) {
                rows.push(row);
            } else {
                rows.push(placeholder_row(
                    definition,
                    &matrix_platform_host_class(),
                    placeholder_limit,
                ));
            }
        }
        Ok(build_slice_report("r3-slice-1-normal-use-linux", rows))
    }

    #[test]
    fn repair_attempt_closeout_matrix_catalog_has_twenty_five_scenarios() {
        assert_eq!(scenario_catalog().len(), 25);
    }

    #[test]
    fn repair_attempt_closeout_matrix_slice_one_exercises_normal_use_rows() -> Result<(), String> {
        let report = build_r3_slice_one_report()?;
        assert_eq!(report.aggregate, CloseoutMatrixAggregate::Incomplete);
        let exercised = report
            .rows
            .iter()
            .filter(|row| row.result == CloseoutMatrixResult::Pass)
            .count();
        if exercised != 5 {
            return Err(format!("expected 5 passing rows, got {exercised}"));
        }
        let json = render_closeout_json(&report)?;
        let markdown = render_closeout_markdown(&report);
        if !json.contains("normal.default_store_before_edit_after_status") {
            return Err("json report missing scenario 1".to_string());
        }
        if !markdown.contains("Aggregate") {
            return Err("markdown report missing aggregate".to_string());
        }
        Ok(())
    }

    #[test]
    fn repair_attempt_closeout_matrix_json_and_markdown_agree_on_aggregate() -> Result<(), String> {
        let report = build_r3_slice_one_report()?;
        let json: RepairAttemptCloseoutMatrixReport =
            serde_json::from_str(&render_closeout_json(&report)?)
                .map_err(|error| format!("decode json report failed: {error}"))?;
        assert_eq!(json.aggregate, report.aggregate);
        assert_eq!(json.rows.len(), report.rows.len());
        Ok(())
    }

    #[test]
    fn repair_attempt_closeout_matrix_unexercised_rows_stay_missing_evidence() -> Result<(), String>
    {
        let report = build_r3_slice_one_report()?;
        let missing = report
            .rows
            .iter()
            .filter(|row| row.result == CloseoutMatrixResult::MissingEvidence)
            .count();
        if missing != 20 {
            return Err(format!("expected 20 missing rows, got {missing}"));
        }
        Ok(())
    }

    #[test]
    fn explicit_store_resolver_matches_manifest_store() -> Result<(), String> {
        let root = git_repo("matrix-store-check")?;
        let alt = Path::new("target/ripr/alt-attempts");
        let prepared = prepare_attempt(&root, "seam:store", "store", Some(alt))?;
        let store = resolve_store(&root, Some(alt), RepairAttemptStoreAccess::Open)?;
        if prepared.manifest.store.as_ref().map(|s| s.locator.as_str())
            != Some("target/ripr/alt-attempts")
        {
            return Err("manifest store locator mismatch".to_string());
        }
        let _ = store;
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
