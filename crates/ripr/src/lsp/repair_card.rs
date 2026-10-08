//! RepairCard projection through standard LSP (#4668, RIPR-SPEC-0198).
//!
//! The editor adapter never re-derives card facts: witness binding, portable
//! workspace identity, attempt recency, packet rendering, and card assembly
//! all delegate to the same `app::repair_card_handoff` authorities the CLI
//! `ripr agent card` handoff consumes (#4667). The adapter owns framing only —
//! one bounded copy action ([`seam_repair_card_action`]) and one bounded hover
//! section ([`repair_card_hover_lines`]) — and fails closed (omits the
//! projection) whenever a producer fact cannot be bound. Capability and budget
//! may omit detail on the wire; neither the action nor the hover ever
//! strengthens readiness, actionability, or currentness.

use super::state::AnalysisSnapshot;
use crate::agent::artifact::git_output;
use crate::agent::command_specs::{AgentArtifactRoute, agent_inspection_command_spec};
use crate::analysis::ClassifiedSeam;
use crate::analysis::repair_route::repair_packet_eligibility;
use crate::app::repair_card_handoff::{
    SeamCardFacts, assemble_repair_card, card_packet_json, evidence_tree_currentness,
    latest_attempt_for_seam, witness_from_findings, workspace_identity_for,
};
use crate::domain::{FixInstructionState, RepairCardDetailState, RepairCardV1};

/// Assemble the same [`RepairCardV1`] the CLI `ripr agent card` handoff
/// builds, from the completed snapshot's own authorities instead of re-running
/// the check pipeline. Returns `None` (fail-closed omission) when any producer
/// fact cannot be bound — the editor surface never reconstructs a card from
/// weaker evidence and never invents the fields the producers refused.
pub(super) fn seam_repair_card(
    entry: &ClassifiedSeam,
    snapshot: &AnalysisSnapshot,
) -> Option<RepairCardV1> {
    let readiness = &repair_packet_eligibility(entry).readiness;
    let (finding_id, witness) = match witness_from_findings(
        &snapshot.findings,
        entry,
        readiness.canonical_gap_id.as_deref(),
        &snapshot.classified_seams,
    ) {
        Some((finding_id, witness)) => (Some(finding_id), Some(witness)),
        None => (None, None),
    };
    let root = snapshot.root.as_path();
    let seam_id = entry.seam.id().as_str().to_string();
    // The card's semantic identity binds the live repository head (the digest
    // input carries the snapshot block), so the editor must resolve the same
    // head the CLI producer would, not a snapshot-cached spelling.
    let repository_head = git_output(root, &["rev-parse", "HEAD"])
        .ok()?
        .trim()
        .to_string();
    let workspace_identity = workspace_identity_for(entry, readiness).ok()?;
    let attempt = latest_attempt_for_seam(root, &seam_id).ok()?;
    // The editor card binds the same producer facts as the CLI handoff: the
    // content-hashed packet renders with the portable root (never the
    // checkout spelling) and both currentness axes project the one bounded
    // dirty-state probe, failing closed to omission when it cannot run.
    let currentness = evidence_tree_currentness(root, entry).ok()?;
    let packet_json = card_packet_json(entry);
    let next_command = agent_inspection_command_spec(
        AgentArtifactRoute::Packet,
        &root.to_string_lossy(),
        &seam_id,
    );
    assemble_repair_card(&SeamCardFacts {
        entry,
        witness: witness.as_ref(),
        finding_id: finding_id.as_deref(),
        attempt: attempt.as_ref(),
        packet_json: &packet_json,
        repository_head: &repository_head,
        workspace_identity: &workspace_identity,
        currentness,
        next_command: Some(next_command),
    })
    .ok()
}

/// The wire spelling of a typed state enum. Presentation only: the enum
/// remains the authority and this never re-interprets a value.
fn state_wire_name<T: serde::Serialize>(state: &T) -> String {
    serde_json::to_value(state)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Bounded hover section for one assembled card: canonical identity,
/// instruction state, next-action presence, and per-state detail-family
/// availability counts. Verbatim typed fields only — the card itself, with
/// its stable detail references, rides behind the copy action.
pub(super) fn repair_card_hover_lines(card: &RepairCardV1) -> Vec<String> {
    let mut current = 0usize;
    let mut stale = 0usize;
    let mut unavailable = 0usize;
    let mut other = 0usize;
    for reference in &card.detail_references {
        match reference.state {
            RepairCardDetailState::Current => current += 1,
            RepairCardDetailState::Stale => stale += 1,
            RepairCardDetailState::Unavailable => unavailable += 1,
            RepairCardDetailState::Malformed
            | RepairCardDetailState::WrongRoot
            | RepairCardDetailState::Missing => other += 1,
        }
    }
    let instruction = state_wire_name(&card.instruction.state);
    let mut lines = vec![
        String::new(),
        "## Repair card".to_string(),
        format!("Card: `{}`", card.repair_card_id),
        format!("Instruction: `{instruction}`"),
    ];
    if card.instruction.state == FixInstructionState::Unavailable {
        lines.push("Next action: none (no producer-owned route)".to_string());
    } else {
        match &card.next_action {
            Some(action) => lines.push(format!("Next action: `{}`", action.display)),
            None => lines.push("Next action: none (route gate closed)".to_string()),
        }
    }
    lines.push(format!(
        "Detail: {current} current · {stale} stale · {unavailable} unavailable · {other} other of {} families",
        card.detail_references.len()
    ));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::REPAIR_CARD_SCHEMA_VERSION;

    fn digest_fixture_card() -> RepairCardV1 {
        // Reuse the pure assembly fixture shape through the shared builder so
        // the hover projection is tested against a real card, not a hand-drawn
        // shell. Facts come from `app::repair_card_handoff`'s own test
        // helpers via a minimal local replica: only the fields the hover
        // section reads are exercised here.
        RepairCardV1 {
            schema_version: REPAIR_CARD_SCHEMA_VERSION.to_string(),
            repair_card_id: "card-digest".to_string(),
            snapshot: crate::domain::RepairCardSnapshot {
                workspace_identity: "workspace:demo".to_string(),
                repository_head: "abc123".to_string(),
                currentness: crate::domain::RepairCardSnapshotCurrentness::Current,
            },
            subject: crate::domain::RepairCardSubject {
                seam_id: "seam:demo".to_string(),
                canonical_gap_id: Some("gap:demo".to_string()),
                finding_id: Some("probe:demo".to_string()),
            },
            instruction: crate::domain::FixInstructionSummary {
                state: crate::domain::FixInstructionState::FixSiteReady,
                has_fix_site: true,
                has_suggested_assertion: false,
                limitation_kinds: Vec::new(),
            },
            readiness: crate::domain::RepairCardReadinessFacts {
                repair_ready: true,
                required_evidence: Vec::new(),
                present_evidence: Vec::new(),
                missing_evidence: Vec::new(),
            },
            changed_behavior: "expr".to_string(),
            exact_blocker: None,
            selected_target: None,
            assertion_goal: None,
            assertion_goal_detail: None,
            candidate_value: None,
            allowed_files: Vec::new(),
            forbidden_files: Vec::new(),
            done_when: crate::domain::RepairCardDoneWhen {
                static_movement: crate::domain::StaticMovementGoal::ClosedBySelectedRoute,
                focused_test_execution: crate::domain::FocusedExecutionGoal::ExplicitlyNotRun,
                edit_cage: crate::domain::EditCageGoal::Compliant,
                mutation_confirmation: crate::domain::MutationConfirmationGoal::NotRequested,
                currentness: crate::domain::CardCurrentnessGoal::Current,
            },
            stop_conditions: Vec::new(),
            next_action: Some(crate::domain::RepairCardCommandRef {
                command_id: "ripr:agent:packet".to_string(),
                role: "inspection".to_string(),
                display: "ripr agent packet --root . --seam-id seam:demo --json".to_string(),
            }),
            canonical_next_action: None,
            selected_basis: None,
            rejected_alternatives: Vec::new(),
            attempt: None,
            claim_boundary: crate::domain::REPAIR_CARD_CLAIM_BOUNDARY.to_string(),
            limitations: Vec::new(),
            detail_references: vec![
                crate::domain::RepairCardDetailRef {
                    family: crate::domain::RepairCardDetailFamily::CanonicalPacket,
                    state: RepairCardDetailState::Current,
                    route: Some("ripr agent packet --seam-id seam:demo --json".to_string()),
                    unavailable_reason: None,
                    detail_digest: "digest-a".to_string(),
                    omitted_bytes: 100,
                    ordinal: 0,
                    omission_class: crate::domain::RepairCardOmissionClass::AuthorityOwnedDetail,
                },
                crate::domain::RepairCardDetailRef {
                    family: crate::domain::RepairCardDetailFamily::RepairAttemptStatus,
                    state: RepairCardDetailState::Stale,
                    route: Some("ripr agent status --json".to_string()),
                    unavailable_reason: None,
                    detail_digest: "digest-b".to_string(),
                    omitted_bytes: 50,
                    ordinal: 1,
                    omission_class: crate::domain::RepairCardOmissionClass::AuthorityOwnedDetail,
                },
                crate::domain::RepairCardDetailRef {
                    family: crate::domain::RepairCardDetailFamily::MutationCalibration,
                    state: RepairCardDetailState::Unavailable,
                    route: None,
                    unavailable_reason: Some("none produced".to_string()),
                    detail_digest: String::new(),
                    omitted_bytes: 0,
                    ordinal: 2,
                    omission_class: crate::domain::RepairCardOmissionClass::Unavailable,
                },
            ],
            detail_summary: crate::domain::RepairCardDetailSummary::default(),
            complete_evidence_digest: String::new(),
        }
    }

    #[test]
    fn hover_lines_project_identity_state_next_action_and_detail_counts() -> Result<(), String> {
        let card = digest_fixture_card();
        let lines = repair_card_hover_lines(&card);
        let text = lines.join("\n");
        if !text.contains("## Repair card") {
            return Err("hover section missing its heading".to_string());
        }
        if !text.contains("Card: `card-digest`") {
            return Err("hover section must name the canonical card identity".to_string());
        }
        if !text.contains("Instruction: `fix_site_ready`") {
            return Err("hover section must project the typed instruction state".to_string());
        }
        if !text.contains("ripr agent packet --root . --seam-id seam:demo --json") {
            return Err("hover section must name the card's next action".to_string());
        }
        if !text.contains("1 current · 1 stale · 1 unavailable · 0 other of 3 families") {
            return Err("hover section must count detail availability per state".to_string());
        }
        Ok(())
    }

    #[test]
    fn unavailable_instruction_projects_no_route_and_no_next_action() -> Result<(), String> {
        let mut card = digest_fixture_card();
        card.instruction.state = crate::domain::FixInstructionState::Unavailable;
        card.next_action = None;
        let lines = repair_card_hover_lines(&card);
        let text = lines.join("\n");
        if !text.contains("Instruction: `unavailable`") {
            return Err("unavailable instruction must keep its typed state".to_string());
        }
        if !text.contains("Next action: none (no producer-owned route)") {
            return Err("unavailable instruction must not present a next action".to_string());
        }
        Ok(())
    }

    #[test]
    fn limited_route_gate_projects_closed_next_action_without_strengthening() -> Result<(), String>
    {
        let mut card = digest_fixture_card();
        // A ready instruction whose route gate closed keeps the state honest
        // and names the closure instead of inventing a route.
        card.next_action = None;
        let lines = repair_card_hover_lines(&card);
        let text = lines.join("\n");
        if !text.contains("Instruction: `fix_site_ready`") {
            return Err("a closed route gate must not weaken the instruction state".to_string());
        }
        if !text.contains("Next action: none (route gate closed)") {
            return Err("a closed route gate must be named as closed".to_string());
        }
        Ok(())
    }

    /// #7179: a finding inside nested same-kind spans binds the inner seam's
    /// editor card only — the snapshot's own classified seams are the
    /// candidate set, so the outer seam's card carries no witness while the
    /// inner card names the finding, exactly like the CLI handoff.
    #[test]
    fn nested_spans_bind_inner_editor_card_only() -> Result<(), String> {
        use crate::analysis::seams::{
            ExpectedSink, RepoSeam, RequiredDiscriminator, SeamGripClass, SeamKind, SeamSpan,
        };
        use crate::analysis::test_grip_evidence::{
            RelatedTestGrip, RelationConfidence, RelationReason, TestGripEvidence,
            TestTargetEvidence,
        };
        use crate::lsp::state::RefreshMetadata;

        fn nested_entry(byte_offset: usize, expression: &str, span: SeamSpan) -> ClassifiedSeam {
            let seam = RepoSeam::new(
                "src/lib.rs",
                "src/lib.rs::discounted_total",
                SeamKind::PredicateBoundary,
                byte_offset,
                2,
                expression,
                RequiredDiscriminator::BoundaryValue {
                    description: expression.to_string(),
                },
                ExpectedSink::ReturnValue,
            )
            .with_span(span);
            let seam_id = seam.id().clone();
            ClassifiedSeam {
                seam,
                evidence: TestGripEvidence {
                    seam_id,
                    related_tests: vec![std::sync::Arc::new(RelatedTestGrip {
                        test_name: "below_threshold_has_no_discount".to_string(),
                        file: std::path::PathBuf::from("tests/pricing.rs"),
                        line: 120,
                        test_target: Some(TestTargetEvidence::fixture(
                            "below_threshold_has_no_discount",
                            std::path::Path::new("tests/pricing.rs"),
                            120,
                        )),
                        oracle_kind: crate::domain::OracleKind::ExactValue,
                        oracle_strength: crate::domain::OracleStrength::Strong,
                        evidence_summary: "exact value assertion".to_string(),
                        relation_reason: RelationReason::DirectOwnerCall,
                        relation_confidence: RelationConfidence::High,
                    })],
                    reach: crate::domain::StageEvidence::new(
                        crate::domain::StageState::Yes,
                        crate::domain::Confidence::Medium,
                        "test stage",
                    ),
                    activate: crate::domain::StageEvidence::new(
                        crate::domain::StageState::Yes,
                        crate::domain::Confidence::Medium,
                        "test stage",
                    ),
                    propagate: crate::domain::StageEvidence::new(
                        crate::domain::StageState::Yes,
                        crate::domain::Confidence::Medium,
                        "test stage",
                    ),
                    observe: crate::domain::StageEvidence::new(
                        crate::domain::StageState::Yes,
                        crate::domain::Confidence::Medium,
                        "test stage",
                    ),
                    discriminate: crate::domain::StageEvidence::new(
                        crate::domain::StageState::No,
                        crate::domain::Confidence::Medium,
                        "test stage",
                    ),
                    observed_values: Vec::new(),
                    missing_discriminators: Vec::new(),
                    statically_contradicted_related_tests: 0,
                    new_test_target: None,
                },
                class: SeamGripClass::WeaklyGripped,
            }
        }

        let outer = nested_entry(
            20,
            "(amount > 20)\n        == flag",
            SeamSpan {
                start_line: 2,
                start_column: 8,
                end_line: 3,
                end_column: 16,
            },
        );
        let inner = nested_entry(
            21,
            "amount > 20",
            SeamSpan {
                start_line: 2,
                start_column: 9,
                end_line: 2,
                end_column: 15,
            },
        );
        let producer_id =
            "gap:rust:src/lib.rs:discounted_total:predicate_boundary:predicate:amount==20";
        // One weakly-exposed producer-gap finding with an oracle row, so a
        // bound witness is observable and a wrong bind would fail this test.
        // (`mcp::gaps::test_finding` is module-private; this is its shape.)
        let mut finding = {
            use crate::domain::{
                ActivationEvidence, Confidence, DeltaKind, ExposureClass, Finding,
                FindingCanonicalGap, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId,
                RelatedTest, RevealEvidence, RiprEvidence, SourceLocation, StageEvidence,
                StageState,
            };
            let stage = || StageEvidence::new(StageState::Unknown, Confidence::Unknown, "test");
            Finding {
                id: "finding:test:1".to_string(),
                canonical_gap: Some(FindingCanonicalGap {
                    id: producer_id.to_string(),
                    language: "rust".to_string(),
                    file: "src/lib.rs".to_string(),
                    owner: "discounted_total".to_string(),
                    behavior_kind: "predicate_boundary".to_string(),
                    probe_kind: "predicate".to_string(),
                    normalized_discriminator: "amount==20".to_string(),
                }),
                probe: Probe {
                    id: ProbeId("probe:test:1".to_string()),
                    location: SourceLocation::new("src/lib.rs", 2, 5),
                    owner: None,
                    family: ProbeFamily::Predicate,
                    delta: DeltaKind::Control,
                    before: None,
                    after: Some("amount > 20".to_string()),
                    expression: "amount > 20".to_string(),
                    expected_sinks: Vec::new(),
                    required_oracles: Vec::new(),
                },
                class: ExposureClass::WeaklyExposed,
                ripr: RiprEvidence {
                    reach: stage(),
                    infect: stage(),
                    propagate: stage(),
                    reveal: RevealEvidence {
                        observe: stage(),
                        discriminate: stage(),
                    },
                },
                confidence: 0.5,
                evidence: Vec::new(),
                missing: Vec::new(),
                flow_sinks: Vec::new(),
                activation: ActivationEvidence::default(),
                stop_reasons: Vec::new(),
                related_tests_matched_total: Some(1),
                related_tests: vec![RelatedTest {
                    name: "exact_boundary_gets_the_discount".to_string(),
                    file: std::path::PathBuf::from("tests/pricing.rs"),
                    line: 12,
                    oracle: Some("assert_eq!(discounted_total(100, 100), 90)".to_string()),
                    oracle_kind: OracleKind::ExactValue,
                    oracle_strength: OracleStrength::Strong,
                    relation_reason: None,
                    relation_confidence: None,
                    miss: None,
                }],
                recommended_next_step: None,
                language: Some(crate::domain::LanguageId::Rust),
                language_status: None,
                owner_kind: None,
                static_limit_kind: None,
                changed_sink: None,
                observed_sink: None,
                oracle_alignment: None,
                alignment_reason: None,
                source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
            }
        };
        finding.probe.location.line = 2;

        // The editor card binds the live repository head and the working-tree
        // currentness probe, so the snapshot roots at a committed fixture repo.
        let root = std::env::temp_dir().join(format!(
            "ripr-lsp-nested-card-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| format!("clock error: {error}"))?
                .as_nanos()
        ));
        std::fs::create_dir_all(&root)
            .map_err(|error| format!("fixture directory failed: {error}"))?;
        std::fs::write(root.join("README.md"), "nested fixture\n")
            .map_err(|error| format!("fixture file failed: {error}"))?;
        crate::testing::fixture_git::fixture_git_ok(&root, &["init", "-q"])?;
        crate::testing::fixture_git::fixture_git_ok(
            &root,
            &["config", "user.name", "ripr fixture"],
        )?;
        crate::testing::fixture_git::fixture_git_ok(
            &root,
            &["config", "user.email", "fixture@ripr.invalid"],
        )?;
        crate::testing::fixture_git::fixture_git_ok(&root, &["add", "."])?;
        crate::testing::fixture_git::fixture_git_ok(&root, &["commit", "-qm", "fixture"])?;
        let snapshot = AnalysisSnapshot {
            root: root.clone(),
            rust_consumed_sources: Default::default(),
            input_identity: None,
            base: None,
            mode: crate::app::Mode::Draft,
            refresh: RefreshMetadata::default(),
            findings: vec![finding],
            analysis_outcome: None,
            diagnostic_profile: crate::config::LspDiagnosticProfile::Full,
            classified_seams: vec![outer, inner],
            gap_artifacts: Vec::new(),
            gap_artifact_rejections: Vec::new(),
            harness_facts: super::super::state::HarnessFactsOnSnapshot::NotRegistered,
            diagnostics_by_uri: std::collections::BTreeMap::new(),
            diagnostic_uri_index: None,
            delivery_selection: None,
            seams_deferred: false,
            partial_scope: None,
            component_outcomes: Vec::new(),
            out_of_scope_test_file_findings: 0,
        };
        let inner_card = seam_repair_card(&snapshot.classified_seams[1], &snapshot);
        let outer_card = seam_repair_card(&snapshot.classified_seams[0], &snapshot);
        crate::testing::fixture_git::remove_fixture_tree(&root)?;

        let inner_card =
            inner_card.ok_or_else(|| "the inner seam must project an editor card".to_string())?;
        if inner_card.subject.finding_id.as_deref() != Some("finding:test:1") {
            return Err(format!(
                "the inner card must name the nested finding, got {:?}",
                inner_card.subject.finding_id
            ));
        }
        let outer_card =
            outer_card.ok_or_else(|| "the outer seam must project an editor card".to_string())?;
        if outer_card.subject.finding_id.is_some() {
            return Err(format!(
                "the outer card must name no finding, got {:?}",
                outer_card.subject.finding_id
            ));
        }
        for (name, card, expected) in [
            (
                "inner",
                &inner_card,
                crate::domain::RepairCardDetailState::Current,
            ),
            (
                "outer",
                &outer_card,
                crate::domain::RepairCardDetailState::Unavailable,
            ),
        ] {
            let fix = card
                .detail_references
                .iter()
                .find(|reference| {
                    reference.family == crate::domain::RepairCardDetailFamily::FixInstruction
                })
                .ok_or_else(|| format!("the {name} card omitted its fix instruction"))?;
            if fix.state != expected {
                return Err(format!(
                    "the {name} fix instruction must be {expected:?}, got {:?}",
                    fix.state
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn state_wire_name_serializes_without_reinterpretation() -> Result<(), String> {
        if state_wire_name(&FixInstructionState::FixSiteReady) != "fix_site_ready" {
            return Err("fix_site_ready spelling drifted".to_string());
        }
        if state_wire_name(&RepairCardDetailState::Unavailable) != "unavailable" {
            return Err("unavailable spelling drifted".to_string());
        }
        Ok(())
    }
}
