use super::*;
use crate::agent::loop_commands::check_repo_exposure_command;
use crate::analysis::ClassifiedSeam;
use crate::analysis::seams::SeamGripClass;
use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
use crate::analysis::test_grip_evidence::{
    RelatedTestGrip, RelationConfidence, RelationReason, TestGripEvidence,
};
use crate::app::Mode;
use crate::domain::{
    Confidence, MissingDiscriminatorFact, OracleKind, OracleStrength, StageEvidence, StageState,
    ValueFact,
};
use crate::output::markdown::powershell_command;
use crate::output::path::display_path;
use crate::output::pilot::ranking::{top_actionable_seams, withheld_static_limitations};
use crate::output::python_repair_card::PythonRepairCard;
use std::path::{Path, PathBuf};

mod spec_0237;

fn seam(file: &str, line: usize, expression: &str) -> RepoSeam {
    RepoSeam::new(
        file,
        "pricing::discounted_total",
        SeamKind::PredicateBoundary,
        line * 10,
        line,
        expression,
        RequiredDiscriminator::BoundaryValue {
            description: expression.to_string(),
        },
        ExpectedSink::ReturnValue,
    )
}

fn stage(state: StageState) -> StageEvidence {
    StageEvidence::new(state, Confidence::Medium, "stage summary")
}

fn missing() -> MissingDiscriminatorFact {
    MissingDiscriminatorFact {
        value: "input that hits the boundary: amount >= discount_threshold".to_string(),
        reason: "observed values do not include the equality-boundary case".to_string(),
        flow_sink: None,
    }
}

fn related_test() -> RelatedTestGrip {
    RelatedTestGrip {
        test_name: "below_threshold_has_no_discount".to_string(),
        file: PathBuf::from("tests/pricing.rs"),
        line: 12,
        test_target: Some(
            crate::analysis::test_grip_evidence::TestTargetEvidence::fixture(
                "below_threshold_has_no_discount",
                std::path::Path::new("tests/pricing.rs"),
                12,
            ),
        ),
        oracle_kind: OracleKind::ExactValue,
        oracle_strength: OracleStrength::Strong,
        evidence_summary: "exact value assertion".to_string(),
        relation_reason: RelationReason::DirectOwnerCall,
        relation_confidence: RelationConfidence::High,
    }
}

fn pilot_artifacts() -> PilotArtifacts {
    PilotArtifacts {
        repo_exposure_json: PathBuf::from("target/ripr/pilot/repo-exposure.json"),
        repo_exposure_md: PathBuf::from("target/ripr/pilot/repo-exposure.md"),
        agent_seam_packets_json: PathBuf::from("target/ripr/pilot/agent-seam-packets.json"),
        pilot_summary_json: PathBuf::from("target/ripr/pilot/pilot-summary.json"),
        pilot_summary_md: PathBuf::from("target/ripr/pilot/pilot-summary.md"),
    }
}

fn pilot_context(artifacts: &PilotArtifacts) -> PilotSummaryContext<'_> {
    PilotSummaryContext {
        root: Path::new("."),
        mode: &Mode::Draft,
        config_path: Some(Path::new("ripr.toml")),
        max_seams: 5,
        timeout_ms: 30_000,
        artifacts,
        python_first_use: None,
        language_routes: None,
        current_change: None,
        seam_limit: None,
    }
}

fn python_repair_card() -> PythonRepairCard {
    PythonRepairCard {
        card_version: "python_repair_card.v1".to_string(),
        source: "check_python_preview".to_string(),
        canonical_gap_id:
            "gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold"
                .to_string(),
        language: "python".to_string(),
        language_status: "preview".to_string(),
        authority_boundary: "preview_advisory_only".to_string(),
        repair_action: "strengthen_existing_test".to_string(),
        changed_owner: "calculate_discount".to_string(),
        changed_behavior: "predicate_boundary changed at src/pricing.py:2: `amount >= threshold`"
            .to_string(),
        current_test_evidence:
            "tests/test_pricing.py:6 test_calculate_discount_above_threshold currently has oracle_strength=weak, oracle_kind=broad_assertion: assert result"
                .to_string(),
        missing_discriminator: "amount == threshold".to_string(),
        recommended_test_shape:
            "Strengthen the existing pytest boundary assertion for `amount == threshold`."
                .to_string(),
        suggested_assertion: "Assert the owner result or effect at the boundary `amount == threshold`."
            .to_string(),
        suggested_test_file: "tests/test_pricing.py".to_string(),
        suggested_test_name: "test_calculate_discount_above_threshold".to_string(),
        suggested_test_node_id: Some(
            "tests/test_pricing.py::test_calculate_discount_above_threshold".to_string(),
        ),
        verify_command:
            "pytest tests/test_pricing.py::test_calculate_discount_above_threshold".to_string(),
        verify_command_confidence: "high".to_string(),
        receipt_command: None,
        receipt_status: "unavailable_until_python_gap_ledger".to_string(),
        receipt_guidance:
            "Save this `ripr check --format json` report, then run `ripr first-pr --check-output <check.json>` or `ripr reports gap-ledger --check-output <check.json>` to materialize a gap ledger with a concrete receipt command."
                .to_string(),
        stop_conditions: vec![
            "Stop if imports, fixtures, or test setup cannot call the changed owner.".to_string(),
            "Stop if the expected value for the missing discriminator is ambiguous.".to_string(),
            "Stop if adding the test appears to require a production-code edit.".to_string(),
        ],
        limits: vec![
            "Syntax-first Python preview evidence only.".to_string(),
            "No source edits, generated tests, mutation execution, provider calls, or gate authority."
                .to_string(),
            "Verify success alone is not a gap-closure receipt.".to_string(),
        ],
    }
}

fn python_first_use() -> PilotPythonFirstUse {
    PilotPythonFirstUse {
        status: super::types::PilotPythonFirstUseStatus::Ready,
        findings_total: 1,
        repair_cards_total: 1,
        limitation_count: 0,
        analysis_error: None,
        top_repair_card: Some(python_repair_card()),
    }
}

fn pilot_context_with_python<'a>(
    artifacts: &'a PilotArtifacts,
    python_first_use: &'a PilotPythonFirstUse,
) -> PilotSummaryContext<'a> {
    PilotSummaryContext {
        root: Path::new("."),
        mode: &Mode::Draft,
        config_path: None,
        max_seams: 5,
        timeout_ms: 30_000,
        artifacts,
        python_first_use: Some(python_first_use),
        language_routes: None,
        current_change: None,
        seam_limit: None,
    }
}

fn classified_with(
    class: SeamGripClass,
    file: &str,
    line: usize,
    missing_discriminators: Vec<MissingDiscriminatorFact>,
    related_tests: Vec<RelatedTestGrip>,
) -> ClassifiedSeam {
    let seam = seam(file, line, "amount >= discount_threshold");
    ClassifiedSeam {
        evidence: TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: related_tests.into_iter().map(std::sync::Arc::new).collect(),
            reach: stage(StageState::Yes),
            activate: stage(StageState::Yes),
            propagate: stage(StageState::Yes),
            observe: stage(StageState::Yes),
            discriminate: stage(StageState::Weak),
            observed_values: Vec::<ValueFact>::new(),
            missing_discriminators,
            statically_contradicted_related_tests: 0,
            new_test_target: None,
        },
        seam,
        class,
    }
}

#[test]
fn pilot_ranking_prefers_actionable_class_order_before_tie_breakers() {
    let ungripped = classified_with(
        SeamGripClass::Ungripped,
        "src/a.rs",
        10,
        vec![missing()],
        vec![related_test()],
    );
    let weak = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/z.rs",
        99,
        Vec::new(),
        Vec::new(),
    );

    let entries = [ungripped, weak];
    let ranked = top_actionable_seams(&entries, 5, None);
    assert_eq!(ranked[0].class, SeamGripClass::WeaklyGripped);
    assert_eq!(ranked[1].class, SeamGripClass::Ungripped);
}

#[test]
fn pilot_ranking_uses_evidence_tie_breakers_then_stable_location() {
    let no_missing = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/a.rs",
        10,
        Vec::new(),
        vec![related_test()],
    );
    let with_missing = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/b.rs",
        10,
        vec![missing()],
        Vec::new(),
    );
    let stable_first = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/c.rs",
        10,
        Vec::new(),
        Vec::new(),
    );
    let stable_second = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/d.rs",
        10,
        Vec::new(),
        Vec::new(),
    );

    let entries = [stable_second, stable_first, no_missing, with_missing];
    let ranked = top_actionable_seams(&entries, 5, None);
    assert_eq!(display_path(ranked[0].seam.file()), "src/b.rs");
    assert_eq!(display_path(ranked[1].seam.file()), "src/a.rs");
    assert_eq!(display_path(ranked[2].seam.file()), "src/c.rs");
    assert_eq!(display_path(ranked[3].seam.file()), "src/d.rs");
}

/// A seam of `owner` in `file`; the shared `seam` helper fixes one owner.
fn classified_in_owner(
    class: SeamGripClass,
    file: &str,
    owner: &str,
    line: usize,
) -> ClassifiedSeam {
    let mut entry = classified_with(class, file, line, Vec::new(), Vec::new());
    entry.seam = RepoSeam::new(
        file,
        owner,
        SeamKind::PredicateBoundary,
        line * 10,
        line,
        "amount >= discount_threshold",
        RequiredDiscriminator::BoundaryValue {
            description: "amount >= discount_threshold".to_string(),
        },
        ExpectedSink::ReturnValue,
    );
    entry.evidence.seam_id = entry.seam.id().clone();
    entry
}

fn ranked_places(ranked: &[&ClassifiedSeam]) -> Vec<(String, usize)> {
    ranked
        .iter()
        .map(|entry| (entry.seam.owner().to_string(), entry.seam.display_line()))
        .collect()
}

#[test]
fn pilot_ranking_takes_one_seam_per_owner_before_a_second() {
    // #5770: three adjacent seams of one function sorted ahead of another
    // function's seam by location alone; each owner now gets one pick first.
    let entries = [
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 10),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 11),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 12),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::as_str", 40),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/b.rs", "b::parse", 5),
    ];

    assert_eq!(
        ranked_places(&top_actionable_seams(&entries, 3, None)),
        [
            ("a::clone".to_string(), 10),
            ("a::as_str".to_string(), 40),
            ("b::parse".to_string(), 5),
        ]
    );
    // Past one round, the owner's remaining seams follow in location order.
    assert_eq!(
        ranked_places(&top_actionable_seams(&entries, 5, None))[3..],
        [("a::clone".to_string(), 11), ("a::clone".to_string(), 12)]
    );
}

#[test]
fn pilot_ranking_spreads_owners_without_crossing_class_order() {
    // The same owner name in another file is another function.
    let entries = [
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "fmt", 1),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "fmt", 2),
        classified_in_owner(SeamGripClass::Ungripped, "src/b.rs", "parse", 1),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/c.rs", "fmt", 1),
    ];

    let ranked = top_actionable_seams(&entries, 4, None);
    assert_eq!(
        ranked
            .iter()
            .map(|entry| (display_path(entry.seam.file()), entry.seam.display_line()))
            .collect::<Vec<_>>(),
        [
            ("src/a.rs".to_string(), 1),
            ("src/c.rs".to_string(), 1),
            ("src/a.rs".to_string(), 2),
            ("src/b.rs".to_string(), 1),
        ]
    );
}

#[test]
fn pilot_ranking_counts_owner_rounds_across_classes() {
    // A function already listed for a weak seam does not get a fresh first
    // pick among the unrevealed ones: the other function's two lead.
    let entries = [
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/z.rs", "z::fmt", 1),
        classified_in_owner(SeamGripClass::ReachableUnrevealed, "src/z.rs", "z::fmt", 9),
        classified_in_owner(
            SeamGripClass::ReachableUnrevealed,
            "src/b.rs",
            "b::parse",
            1,
        ),
        classified_in_owner(
            SeamGripClass::ReachableUnrevealed,
            "src/b.rs",
            "b::parse",
            2,
        ),
    ];

    assert_eq!(
        ranked_places(&top_actionable_seams(&entries, 4, None)),
        [
            ("z::fmt".to_string(), 1),
            ("b::parse".to_string(), 1),
            ("b::parse".to_string(), 2),
            ("z::fmt".to_string(), 9),
        ]
    );
}

#[test]
fn pilot_summary_md_names_unlisted_seams_on_an_owners_first_pick_only() {
    // a::clone is listed twice (rounds 0 and 1) with two seams left over.
    let entries = [
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 10),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 11),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 12),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 13),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/b.rs", "b::parse", 5),
    ];
    let artifacts = pilot_artifacts();
    let mut context = pilot_context(&artifacts);
    context.max_seams = 3;
    let md = render_pilot_summary_md(&entries, context);

    let note = "   - Also in this function: 2 more actionable seams not listed here\n";
    assert_eq!(md.matches(note).count(), 1, "{md}");
    // In the ranked list, the note sits inside entry 1 (a::clone at line 10),
    // before entry 2 (b::parse) and entry 3 (a::clone's second pick).
    let ranked = md.find("## Ranked Seams").map_or("", |start| &md[start..]);
    let entry = |prefix: &str| ranked.find(prefix).unwrap_or(usize::MAX);
    let at = ranked.find(note).unwrap_or(usize::MAX);
    assert!(ranked.contains("src/a.rs:10"), "{md}");
    assert!(entry("1. `") < at && at < entry("2. `"), "{md}");
    assert!(entry("3. `") > entry("2. `"), "{md}");
}

#[test]
fn pilot_summary_md_marks_owner_counts_as_lower_bounds_after_a_seam_limit() {
    // #6602: a seam limit dropped classified seams before ranking, so a
    // function may have more seams than the kept slice shows.
    let entries = [
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 10),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 11),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 12),
    ];
    let limit = crate::analysis::SeamLimitInfo {
        analyzed: 3,
        total: 7,
        source: crate::analysis::SeamLimitSource::Default,
    };
    let artifacts = pilot_artifacts();
    let mut context = pilot_context(&artifacts);
    context.max_seams = 1;
    context.seam_limit = Some(&limit);
    let md = render_pilot_summary_md(&entries, context);

    assert!(
        md.contains("- Seam limit reached: ranked 3 of 7 seams; Rust seam counts below cover those only\n- Actionable seams: at least 3, showing up to 1\n\n"),
        "{md}"
    );
    assert!(
        md.contains(
            "   - Also in this function: at least 2 more actionable seams not listed here\n"
        ),
        "{md}"
    );

    context.seam_limit = None;
    let md = render_pilot_summary_md(&entries, context);
    assert!(!md.contains("Seam limit reached"), "{md}");
    assert!(
        md.contains("- Actionable seams: 3 total, showing up to 1\n"),
        "{md}"
    );
    assert!(
        md.contains("   - Also in this function: 2 more actionable seams not listed here\n"),
        "{md}"
    );
}

#[test]
fn pilot_summary_md_counts_an_owners_unlisted_seams_once() {
    let entries = [
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 10),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 11),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/a.rs", "a::clone", 12),
        classified_in_owner(SeamGripClass::WeaklyGripped, "src/b.rs", "b::parse", 5),
        // Solved seams are not "more to do" in the function.
        classified_in_owner(SeamGripClass::StronglyGripped, "src/b.rs", "b::parse", 6),
    ];
    let artifacts = pilot_artifacts();
    let mut context = pilot_context(&artifacts);
    context.max_seams = 2;
    let md = render_pilot_summary_md(&entries, context);

    assert_eq!(
        md.matches("   - Also in this function: 2 more actionable seams not listed here\n")
            .count(),
        1,
        "{md}"
    );
    assert!(!md.contains("more actionable seam not listed"), "{md}");

    context.max_seams = 5;
    let md = render_pilot_summary_md(&entries, context);
    assert!(!md.contains("Also in this function"), "{md}");
}

#[test]
fn pilot_ranking_excludes_solved_governed_classes() {
    let strong = classified_with(
        SeamGripClass::StronglyGripped,
        "src/strong.rs",
        1,
        Vec::new(),
        Vec::new(),
    );
    let intentional = classified_with(
        SeamGripClass::Intentional,
        "src/intentional.rs",
        2,
        Vec::new(),
        Vec::new(),
    );
    let suppressed = classified_with(
        SeamGripClass::Suppressed,
        "src/suppressed.rs",
        3,
        Vec::new(),
        Vec::new(),
    );
    let opaque = classified_with(
        SeamGripClass::Opaque,
        "src/opaque.rs",
        4,
        Vec::new(),
        Vec::new(),
    );

    // #5497: opaque is a static limitation, so it is withheld rather than
    // ranked; the solved and governed classes are neither.
    let entries = [strong, intentional, suppressed, opaque];
    let ranked = top_actionable_seams(&entries, 5, None);
    assert!(ranked.is_empty());
    assert_eq!(withheld_static_limitations(&entries), 1);
}

#[test]
fn pilot_ranking_admits_gap_classes_only() {
    // #5497: one seam of every class, each in its own function. Only the
    // gap classes rank; the classes the classifier reached by stopping on a
    // stage it could not establish are withheld and counted, and the solved
    // and governed classes are neither ranked nor withheld.
    let entries = SeamGripClass::ALL
        .into_iter()
        .enumerate()
        .map(|(idx, class)| classified_in_owner(class, "src/lib.rs", &format!("f{idx}"), idx + 1))
        .collect::<Vec<_>>();

    let ranked = top_actionable_seams(&entries, entries.len(), None)
        .iter()
        .map(|entry| entry.class)
        .collect::<Vec<_>>();
    assert_eq!(
        ranked,
        [
            SeamGripClass::WeaklyGripped,
            SeamGripClass::Ungripped,
            SeamGripClass::ReachableUnrevealed,
        ]
    );
    assert_eq!(withheld_static_limitations(&entries), 5);
    for class in SeamGripClass::ALL {
        assert_eq!(
            entries
                .iter()
                .any(|entry| entry.class == class && ranked.contains(&entry.class)),
            matches!(
                class,
                SeamGripClass::WeaklyGripped
                    | SeamGripClass::Ungripped
                    | SeamGripClass::ReachableUnrevealed
            ),
            "{class:?}"
        );
        assert!(
            !(ranked.contains(&class) && class.is_static_limitation()),
            "{class:?}"
        );
    }
}

#[test]
fn pilot_summary_ranks_a_true_gap_ahead_of_withheld_limitations() {
    // #5497 mixed queue: one ungripped seam and three static limitations.
    let entries = [
        classified_in_owner(SeamGripClass::ActivationUnknown, "src/a.rs", "a::f", 1),
        classified_in_owner(SeamGripClass::Opaque, "src/a.rs", "a::f", 2),
        classified_in_owner(SeamGripClass::PropagationUnknown, "src/b.rs", "b::g", 3),
        classified_in_owner(SeamGripClass::Ungripped, "src/c.rs", "c::h", 4),
    ];
    let artifacts = pilot_artifacts();
    let context = pilot_context(&artifacts);

    let json = render_pilot_summary_json(&entries, context);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(value["actionable_seams_total"], 1, "{json}");
    assert_eq!(value["withheld_static_limitations_total"], 3, "{json}");
    let top = value["top_actionable_seams"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(top.len(), 1, "{json}");
    assert_eq!(top[0]["grip_class"], "ungripped", "{json}");

    let md = render_pilot_summary_md(&entries, context);
    assert!(
        md.contains(
            "- Withheld: 3 seams (static evidence is unknown or opaque, so they are static limitations, not gaps; listed in `target/ripr/pilot/repo-exposure.md`)\n"
        ),
        "{md}"
    );
    assert!(md.contains("src/c.rs:4"), "{md}");
    assert!(!md.contains("src/a.rs:1"), "{md}");

    let terminal = render_pilot_terminal(&entries, context);
    assert!(
        terminal.contains(
            "  withheld: 3 seams (static evidence is unknown or opaque, so they are static limitations, not gaps)\n"
        ),
        "{terminal}"
    );
    assert!(terminal.contains("src/c.rs:4"), "{terminal}");
}

#[test]
fn pilot_summary_with_only_limitations_is_not_a_clean_result() {
    // #5497: nothing ranks, but the withheld seams are named, not dropped.
    let entries = [
        classified_in_owner(SeamGripClass::ActivationUnknown, "src/a.rs", "a::f", 1),
        classified_in_owner(SeamGripClass::Opaque, "src/b.rs", "b::g", 2),
    ];
    let artifacts = pilot_artifacts();
    let context = pilot_context(&artifacts);

    let json = render_pilot_summary_json(&entries, context);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(value["actionable_seams_total"], 0, "{json}");
    assert_eq!(value["withheld_static_limitations_total"], 2, "{json}");
    assert_eq!(
        value["top_actionable_seams"],
        serde_json::json!([]),
        "{json}"
    );
    assert!(value["next"]["repair_command"].is_null(), "{json}");
    // No gap to snapshot or measure, so the JSON offers no follow-up pair.
    assert!(value["next"]["after_snapshot_command"].is_null(), "{json}");
    assert!(value["next"]["outcome_command"].is_null(), "{json}");

    let md = render_pilot_summary_md(&entries, context);
    assert!(
        md.contains("None ranked: 2 seams were withheld because their static evidence is unknown or opaque. ripr cannot tell whether a test discriminates them, so this is not a clean result. Inspect them in `target/ripr/pilot/repo-exposure.md`.\n"),
        "{md}"
    );
    assert!(!md.contains("No actionable seam was ranked"), "{md}");
    assert!(!md.contains("After adding one focused test"), "{md}");
    assert!(md.contains("No gap to test:"), "{md}");

    let terminal = render_pilot_terminal(&entries, context);
    assert!(
        terminal.contains("  none ranked: 2 seams were withheld"),
        "{terminal}"
    );
    assert!(
        !terminal.contains("none ranked by the default pilot policy"),
        "{terminal}"
    );
    assert!(
        !terminal.contains("Run after adding the focused test"),
        "{terminal}"
    );
    assert!(terminal.contains("No gap to test:"), "{terminal}");

    // With nothing withheld, the empty ranking keeps its old wording.
    let md = render_pilot_summary_md(&[], context);
    assert!(
        md.contains("No actionable seam was ranked by the default pilot policy."),
        "{md}"
    );
    assert!(!md.contains("Withheld"), "{md}");

    // A seam limit cut seams pilot never classified: they may hold gaps, so
    // the withheld count is a lower bound and "No gap to test" would claim an
    // absence the run did not establish.
    let limit = crate::analysis::SeamLimitInfo {
        analyzed: 2,
        total: 9,
        source: crate::analysis::SeamLimitSource::Default,
    };
    let mut limited = pilot_context(&artifacts);
    limited.seam_limit = Some(&limit);
    let expected_next = "No gap ranked among the 2 seams pilot analyzed, but the seam limit left 7 of 9 seams unanalyzed and they may hold gaps: raise or remove RIPR_PILOT_SEAM_BUDGET and RIPR_REPO_EXPOSURE_SEAM_LIMIT, then rerun pilot.";
    let md = render_pilot_summary_md(&entries, limited);
    assert!(
        md.contains("None ranked: at least 2 seams were withheld"),
        "{md}"
    );
    assert!(md.contains("- Withheld: at least 2 seams ("), "{md}");
    assert!(md.contains(expected_next), "{md}");
    assert!(!md.contains("No gap to test:"), "{md}");
    let terminal = render_pilot_terminal(&entries, limited);
    assert!(
        terminal.contains("  seam limit: ranked 2 of 9 seams\n"),
        "{terminal}"
    );
    assert!(
        terminal.contains("  withheld: at least 2 seams ("),
        "{terminal}"
    );
    assert!(terminal.contains(expected_next), "{terminal}");
    assert!(!terminal.contains("No gap to test:"), "{terminal}");
}

#[test]
fn pilot_summary_json_contains_config_state_artifacts_and_next_commands() {
    let entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let artifacts = PilotArtifacts {
        repo_exposure_json: PathBuf::from("target/ripr/pilot/repo-exposure.json"),
        repo_exposure_md: PathBuf::from("target/ripr/pilot/repo-exposure.md"),
        agent_seam_packets_json: PathBuf::from("target/ripr/pilot/agent-seam-packets.json"),
        pilot_summary_json: PathBuf::from("target/ripr/pilot/pilot-summary.json"),
        pilot_summary_md: PathBuf::from("target/ripr/pilot/pilot-summary.md"),
    };
    let context = PilotSummaryContext {
        root: Path::new("."),
        mode: &Mode::Draft,
        config_path: Some(Path::new("ripr.toml")),
        max_seams: 5,
        timeout_ms: 30_000,
        artifacts: &artifacts,
        python_first_use: None,
        language_routes: None,
        current_change: None,
        seam_limit: None,
    };

    let json = crate::testing::cwd_placeholder::project_cwd_text(&render_pilot_summary_json(
        &[entry],
        context,
    ));
    assert!(json.contains(r#""schema_version": "0.3""#));
    assert!(json.contains(r#""status": "complete""#));
    assert!(json.contains(r#""state": "loaded""#));
    assert!(json.contains(r#""top_actionable_seams""#));
    assert!(json.contains(r#""missing_discriminator""#));
    assert!(json.contains("ripr outcome --before <cwd>/target/ripr/pilot/repo-exposure.json"));
}

#[test]
fn pilot_summary_md_spells_out_first_screen_recommendation() {
    let entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let artifacts = pilot_artifacts();
    let md = crate::testing::cwd_placeholder::project_cwd_text(&render_pilot_summary_md(
        &[entry],
        pilot_context(&artifacts),
    ));

    for needle in [
        "## What Was Inspected",
        "## Top Recommendation",
        "- Inspected seam:",
        "(weak, weakly_gripped)",
        " weak (`weakly_gripped`) src/pricing.rs:88 ",
        "- weak, weakly_gripped\n",
        "- Why it matters: missing discriminator: input that hits the boundary: amount >= discount_threshold",
        "- Focused test: none: this seam has no test target that `ripr agent repair` can use (for example, the only tests are inline `#[cfg(test)]` in a file outside the test-surface paths, the only tests are in another crate, or static evidence names no exact discriminator), so it will not start a repair attempt here",
        "Target seam:",
        "Target placement blocked:",
        "## Ranked Seams\n\nNone of these seams can start a repair attempt (`ripr agent repair`); they are ranked for inspection by hand. Repair scope: `ripr agent repair --help`.",
        "## Next Commands",
        "No repair attempt is available for the top seam. Next, add a test for `pricing::discounted_total` in the crate that owns src/pricing.rs, then rerun repo exposure and compare the snapshots:",
        "ripr outcome --before <cwd>/target/ripr/pilot/repo-exposure.json",
    ] {
        assert!(md.contains(needle), "missing markdown needle: {needle}");
    }
    // #4216 row 3: a list with no repair start is not headed "actionable",
    // and the producer's route-readiness jargon stays in the JSON packets.
    assert!(!md.contains("## Ranked Actionable Seams"), "{md}");
    assert!(
        !md.contains("- Focused test: not applicable (route limited"),
        "{md}"
    );
}

/// The bash fence content is pinned byte-for-byte: adding the PowerShell
/// variant must never reshape the form existing consumers copy today (#2628).
#[test]
fn pilot_summary_md_pairs_bash_next_commands_with_powershell_variants() -> Result<(), String> {
    let entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let artifacts = pilot_artifacts();
    let md = render_pilot_summary_md(&[entry], pilot_context(&artifacts));
    let cwd = crate::agent::loop_commands::bound_root(".");

    // Issue #3872: the after-snapshot redirect anchors at the resolved --root,
    // so both presented forms build from the same builder output the pilot
    // renderer uses (the anchor math itself is pinned in loop_commands tests).
    let after_snapshot = check_repo_exposure_command(
        &crate::agent::loop_commands::bound_root("."),
        "draft",
        "target/ripr/pilot/after.repo-exposure.json",
    );
    let bash_block = format!(
        "```bash\n{after_snapshot}\nripr outcome --before {cwd}/target/ripr/pilot/repo-exposure.json --after {cwd}/target/ripr/pilot/after.repo-exposure.json\n```"
    );
    assert!(
        md.contains(bash_block.as_str()),
        "bash next-commands block drifted:\n{md}"
    );
    // The default pilot path is unquoted in the bash form; PowerShell parses
    // method-call arguments in expression mode, so the WriteAllText target must
    // arrive as a quoted literal (PR #3617 review), and the write is guarded by
    // $LASTEXITCODE with the status propagated so a failed run cannot publish
    // the artifact (PR #3625 review, codex P1).
    let powershell_snapshot = powershell_command(&after_snapshot)
        .ok_or_else(|| "redirect commands gain a powershell variant".to_string())?;
    assert!(
        md.contains(powershell_snapshot.as_str()),
        "powershell after-snapshot translation missing:\n{md}"
    );
    // Disclosure precedes the first copyable command, mirroring the landed
    // agent_workflow ordering, and states the cmd.exe boundary.
    let disclosure = md
        .find("cmd.exe is not supported")
        .ok_or_else(|| format!("pilot markdown must state the cmd.exe boundary: {md}"))?;
    let first_fence = md
        .find("```bash")
        .ok_or_else(|| format!("pilot markdown must fence the bash commands: {md}"))?;
    assert!(
        disclosure < first_fence,
        "shell disclosure at {disclosure} must precede the first command fence at {first_fence}"
    );
    // The redirect-free outcome command translates to itself in PowerShell, so
    // the variant fence still carries a runnable second command.
    let powershell_block = md
        .find("```powershell\n")
        .map(|start| &md[start..])
        .ok_or_else(|| format!("pilot markdown must fence the powershell commands: {md}"))?;
    assert!(
        powershell_block.contains(
            &format!("ripr outcome --before {cwd}/target/ripr/pilot/repo-exposure.json --after {cwd}/target/ripr/pilot/after.repo-exposure.json")
        ),
        "powershell outcome command missing:\n{powershell_block}"
    );
    Ok(())
}

#[test]
fn pilot_terminal_prints_top_test_and_follow_up_commands() {
    let entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let seam_id = entry.seam.id().as_str().to_string();
    let artifacts = pilot_artifacts();
    let terminal = crate::testing::cwd_placeholder::project_cwd_text(&render_pilot_terminal(
        &[entry],
        pilot_context(&artifacts),
    ));

    for needle in [
        "Inspected:",
        "root: .",
        "mode: draft",
        "config: loaded ripr.toml",
        "Top recommendation:",
        "inspected seam:",
        "why it matters: missing discriminator: input that hits the boundary: amount >= discount_threshold",
        "focused test: none: this seam has no test target that `ripr agent repair` can use (for example, the only tests are inline `#[cfg(test)]` in a file outside the test-surface paths, the only tests are in another crate, or static evidence names no exact discriminator), so it will not start a repair attempt here",
        "assertion: not_applicable",
        "Detailed brief:",
        "target/ripr/pilot/pilot-summary.md",
        "Structured packet:",
        "target/ripr/pilot/agent-seam-packets.json",
        "Next, by hand: add a test for `pricing::discounted_total` in the crate that owns src/pricing.rs, then compare against this run:",
        "ripr outcome --before <cwd>/target/ripr/pilot/repo-exposure.json",
    ] {
        assert!(
            terminal.contains(needle),
            "missing terminal needle: {needle}"
        );
    }
    // Issue #3872: the after-snapshot redirect anchors at the resolved --root.
    let after_snapshot = check_repo_exposure_command(
        &crate::agent::loop_commands::bound_root("."),
        "draft",
        "target/ripr/pilot/after.repo-exposure.json",
    );
    assert!(
        terminal
            .contains(crate::testing::cwd_placeholder::project_cwd_text(&after_snapshot).as_str()),
        "missing anchored after-snapshot needle:\n{terminal}"
    );
    // A route-limited seam keeps the snapshot comparison: the repair
    // transaction has no target here (#3906).
    assert!(!terminal.contains("Next, in order:"), "{terminal}");
    // #4216 row 3: no producer jargon and no wait on "producer evidence" the
    // user cannot supply; the next step is a test the user writes.
    assert!(!terminal.contains("route limited"), "{terminal}");
    assert!(!terminal.contains("producer"), "{terminal}");
    assert!(!terminal.contains("ripr pilot"), "{terminal}");

    // The id leads the line, so the next documented step
    // (`ripr agent repair --seam-id <id>`) is reachable from the screen alone.
    assert!(
        terminal.contains(&format!(
            "inspected seam: {seam_id} src/pricing.rs:88 predicate_boundary in pricing::discounted_total (weak, weakly_gripped)"
        )),
        "the terminal seam line must lead with the seam id:\n{terminal}"
    );

    // This seam's repair route is limited, so the paste-ready repair command
    // must not appear: the id is what the user needs here, not a transaction
    // against a target the route does not have. The applicable-route half of
    // this contract is proved end to end by
    // `pilot_writes_default_packet_outputs_for_boundary_gap_fixture`.
    assert!(
        !terminal.contains("repair this seam:"),
        "a route-limited seam must not be offered a repair transaction:\n{terminal}"
    );
}

#[test]
fn timeout_summary_json_is_partial_and_points_to_retry() {
    let artifacts = PilotArtifacts {
        repo_exposure_json: PathBuf::from("target/ripr/pilot/repo-exposure.json"),
        repo_exposure_md: PathBuf::from("target/ripr/pilot/repo-exposure.md"),
        agent_seam_packets_json: PathBuf::from("target/ripr/pilot/agent-seam-packets.json"),
        pilot_summary_json: PathBuf::from("target/ripr/pilot/pilot-summary.json"),
        pilot_summary_md: PathBuf::from("target/ripr/pilot/pilot-summary.md"),
    };
    let context = PilotSummaryContext {
        root: Path::new("."),
        mode: &Mode::Draft,
        config_path: None,
        max_seams: 5,
        timeout_ms: 1,
        artifacts: &artifacts,
        python_first_use: None,
        language_routes: None,
        current_change: None,
        seam_limit: None,
    };

    let json = render_pilot_timeout_summary_json(context);
    assert!(json.contains(r#""schema_version": "0.3""#));
    assert!(json.contains(r#""status": "partial""#));
    assert!(json.contains(r#""reason": "timeout""#));
    assert!(json.contains(r#""actionable_seams_total": null"#));
    assert!(json.contains(&format!(
        "ripr pilot --root {0} --out {0}/target/ripr/pilot --mode draft",
        crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root("."))
    )));
    assert!(json.contains("--timeout-ms 120000"));
}

#[test]
fn timeout_summary_json_records_loaded_config_path() {
    let artifacts = pilot_artifacts();
    let json = render_pilot_timeout_summary_json(pilot_context(&artifacts));
    assert!(
        json.contains(r#""config": {"state": "loaded", "path": "ripr.toml"}"#),
        "expected loaded-config path in timeout JSON, got:\n{json}"
    );
}

fn pilot_context_without_config<'a>(artifacts: &'a PilotArtifacts) -> PilotSummaryContext<'a> {
    PilotSummaryContext {
        root: Path::new("."),
        mode: &Mode::Draft,
        config_path: None,
        max_seams: 5,
        timeout_ms: 30_000,
        artifacts,
        python_first_use: None,
        language_routes: None,
        current_change: None,
        seam_limit: None,
    }
}

#[test]
fn timeout_summary_md_explains_partial_status_and_retry_command() {
    let artifacts = pilot_artifacts();
    let md = crate::testing::cwd_placeholder::project_cwd_text(&render_pilot_timeout_summary_md(
        pilot_context(&artifacts),
    ));

    for needle in [
        "# RIPR Pilot Summary",
        "## Scope",
        "- Status: `partial`",
        "- Reason: analysis timed out after 30000 ms",
        "- Config: loaded `ripr.toml`",
        "## Outputs",
        "Analysis did not finish within the pilot budget",
        "- Pilot summary JSON: `target/ripr/pilot/pilot-summary.json`",
        "## Next Command",
        "ripr pilot --root <cwd> --out <cwd>/target/ripr/pilot --mode draft",
        "--timeout-ms 120000",
    ] {
        assert!(md.contains(needle), "missing timeout-md needle: {needle}");
    }
}

/// The retry command carries no redirect and no quoting, so it runs unchanged
/// in PowerShell: the bash form stays byte-identical and no identical
/// PowerShell block repeats it (#2628, F60-12).
#[test]
fn timeout_summary_md_pairs_bash_retry_with_powershell_variant() -> Result<(), String> {
    let artifacts = pilot_artifacts();
    let md = render_pilot_timeout_summary_md(pilot_context(&artifacts));

    let retry = format!(
        "ripr pilot --root {0} --out {0}/target/ripr/pilot --mode draft --max-seams 5 --timeout-ms 120000",
        crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root("."))
    );
    assert!(
        md.contains(&format!("```bash\n{retry}\n```")),
        "bash retry block drifted:\n{md}"
    );
    assert!(
        !md.contains("```powershell"),
        "an unchanged retry must not repeat as a PowerShell block:\n{md}"
    );
    let disclosure = md
        .find("cmd.exe is not supported")
        .ok_or_else(|| format!("pilot timeout markdown must state the cmd.exe boundary: {md}"))?;
    let first_fence = md
        .find("```bash")
        .ok_or_else(|| format!("pilot timeout markdown must fence the bash command: {md}"))?;
    assert!(
        disclosure < first_fence,
        "shell disclosure at {disclosure} must precede the first command fence at {first_fence}"
    );
    Ok(())
}

#[test]
fn timeout_summary_md_reports_missing_config_branch_when_no_config_loaded() {
    let artifacts = pilot_artifacts();
    let md = render_pilot_timeout_summary_md(pilot_context_without_config(&artifacts));
    assert!(
        md.contains("- Config: missing; using built-in defaults"),
        "expected missing-config line in timeout markdown, got:\n{md}"
    );
}

#[test]
fn timeout_terminal_lists_written_files_and_retry_command() {
    let artifacts = pilot_artifacts();
    let terminal = crate::testing::cwd_placeholder::project_cwd_text(
        &render_pilot_timeout_terminal(pilot_context(&artifacts)),
    );

    for needle in [
        "RIPR pilot partial.",
        "Reason:",
        "analysis timed out after 30000 ms",
        "Config:",
        "loaded: ripr.toml",
        "Written:",
        "target/ripr/pilot/pilot-summary.json",
        "target/ripr/pilot/pilot-summary.md",
        "Next:",
        "ripr pilot --root <cwd> --out <cwd>/target/ripr/pilot --mode draft",
    ] {
        assert!(
            terminal.contains(needle),
            "missing timeout-terminal needle: {needle}"
        );
    }
}

#[test]
fn timeout_terminal_reports_missing_config_branch_when_no_config_loaded() {
    let artifacts = pilot_artifacts();
    let terminal = render_pilot_timeout_terminal(pilot_context_without_config(&artifacts));
    assert!(
        terminal.contains("missing: using built-in defaults"),
        "expected missing-config line in timeout terminal, got:\n{terminal}"
    );
}

#[test]
fn pilot_summary_json_reports_missing_config_when_none_loaded() {
    let entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let artifacts = pilot_artifacts();
    let json = render_pilot_summary_json(&[entry], pilot_context_without_config(&artifacts));
    assert!(
        json.contains(r#""state": "missing", "path": null"#),
        "expected missing-config JSON branch, got:\n{json}"
    );
}

#[test]
fn pilot_summary_md_reports_missing_config_when_none_loaded() {
    let entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let artifacts = pilot_artifacts();
    let md = render_pilot_summary_md(&[entry], pilot_context_without_config(&artifacts));
    assert!(
        md.contains("- Config: missing; using built-in defaults"),
        "expected missing-config line in pilot markdown, got:\n{md}"
    );
}

#[test]
fn pilot_terminal_reports_missing_config_when_none_loaded() {
    let entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let artifacts = pilot_artifacts();
    let terminal = render_pilot_terminal(&[entry], pilot_context_without_config(&artifacts));
    assert!(
        terminal.contains("config: missing, using built-in defaults"),
        "expected missing-config line in pilot terminal, got:\n{terminal}"
    );
}

#[test]
fn pilot_summary_renderers_omit_recommendation_when_no_actionable_seams() {
    let entries = [
        classified_with(
            SeamGripClass::StronglyGripped,
            "src/a.rs",
            10,
            Vec::new(),
            Vec::new(),
        ),
        classified_with(
            SeamGripClass::Intentional,
            "src/b.rs",
            20,
            Vec::new(),
            Vec::new(),
        ),
        classified_with(
            SeamGripClass::Suppressed,
            "src/c.rs",
            30,
            Vec::new(),
            Vec::new(),
        ),
    ];
    let artifacts = pilot_artifacts();

    let json = render_pilot_summary_json(&entries, pilot_context(&artifacts));
    assert!(
        json.contains(r#""actionable_seams_total": 0"#),
        "expected zero actionable seams in JSON, got:\n{json}"
    );
    assert!(
        json.contains(r#""top_actionable_seams": []"#),
        "expected empty top_actionable_seams array, got:\n{json}"
    );

    let md = render_pilot_summary_md(&entries, pilot_context(&artifacts));
    assert!(
        md.contains("No actionable seam was ranked by the default pilot policy."),
        "expected no-actionable-seam markdown line, got:\n{md}"
    );

    let terminal = render_pilot_terminal(&entries, pilot_context(&artifacts));
    assert!(
        terminal.contains("none ranked by the default pilot policy"),
        "expected no-recommendation terminal line, got:\n{terminal}"
    );
}

#[test]
fn pilot_summary_json_projects_python_first_use_repair_card() {
    let artifacts = pilot_artifacts();
    let python = python_first_use();
    let json = render_pilot_summary_json(&[], pilot_context_with_python(&artifacts, &python));

    for needle in [
        r#""python_first_use": {"#,
        r#""status": "ready""#,
        r#""language": "python""#,
        r#""language_status": "preview""#,
        r#""authority_boundary": "preview_advisory_only""#,
        r#""findings_total": 1"#,
        r#""repair_cards_total": 1"#,
        r#""top_repair_card": {"#,
        "gap:python:src/pricing.py:calculate_discount:predicate_boundary:predicate:amount>=threshold",
        r#""repair_action": "strengthen_existing_test""#,
        r#""changed_owner": "calculate_discount""#,
        r#""missing_discriminator": "amount == threshold""#,
        r#""suggested_test_file": "tests/test_pricing.py""#,
        r#""suggested_test_name": "test_calculate_discount_above_threshold""#,
        r#""verify_command": "pytest tests/test_pricing.py::test_calculate_discount_above_threshold""#,
        r#""receipt_status": "unavailable_until_python_gap_ledger""#,
        r#""receipt_guidance": "Save this `ripr check --format json` report, then run `ripr first-pr --check-output <check.json>` or `ripr reports gap-ledger --check-output <check.json>` to materialize a gap ledger with a concrete receipt command.""#,
        r#""deferred_features": ["outcome_receipts", "runtime_mutation_execution", "gate_authority", "generated_tests"]"#,
    ] {
        assert!(
            json.contains(needle),
            "missing Python JSON needle: {needle}"
        );
    }
}

#[test]
fn pilot_markdown_and_terminal_use_python_repair_card_when_no_seam_ranked() {
    let artifacts = pilot_artifacts();
    let python = python_first_use();
    let context = pilot_context_with_python(&artifacts, &python);
    let md = render_pilot_summary_md(&[], context);
    let terminal = render_pilot_terminal(&[], context);

    for needle in [
        "## Top Recommendation",
        "Top Python repairable gap",
        "Repair action: `strengthen_existing_test`",
        "Changed owner: `calculate_discount`",
        "Missing discriminator: `amount == threshold`",
        "Suggested test target: `test_calculate_discount_above_threshold` in `tests/test_pricing.py`",
        "Verify: `pytest tests/test_pricing.py::test_calculate_discount_above_threshold`",
        "Receipt status: `unavailable_until_python_gap_ledger`",
        "Receipt guidance: Save this `ripr check --format json` report, then run `ripr first-pr --check-output <check.json>` or `ripr reports gap-ledger --check-output <check.json>` to materialize a gap ledger with a concrete receipt command.",
        "## Python Preview First Use",
    ] {
        assert!(
            md.contains(needle),
            "missing Python markdown needle: {needle}"
        );
    }

    for needle in [
        "Top recommendation:",
        "language: python (preview)",
        "repair action: strengthen_existing_test",
        "changed owner: calculate_discount",
        "missing discriminator: amount == threshold",
        "recommended repair: strengthen test_calculate_discount_above_threshold in tests/test_pricing.py",
        "verify: pytest tests/test_pricing.py::test_calculate_discount_above_threshold",
        "receipt status: unavailable_until_python_gap_ledger",
        "receipt guidance: Save this `ripr check --format json` report, then run `ripr first-pr --check-output <check.json>` or `ripr reports gap-ledger --check-output <check.json>` to materialize a gap ledger with a concrete receipt command.",
        "Python preview:",
        "status: ready",
    ] {
        assert!(
            terminal.contains(needle),
            "missing Python terminal needle: {needle}"
        );
    }
    assert!(
        !terminal.contains("none ranked by the default pilot policy"),
        "Python repair card should replace the no-recommendation top line"
    );
}

#[test]
fn why_line_uses_static_discriminator_summary_when_no_missing_discriminator() {
    let seam = seam("src/pricing.rs", 88, "amount >= discount_threshold");
    let entry = ClassifiedSeam {
        evidence: TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: vec![std::sync::Arc::new(related_test())],
            reach: stage(StageState::Yes),
            activate: stage(StageState::Yes),
            propagate: stage(StageState::Yes),
            observe: stage(StageState::Yes),
            discriminate: StageEvidence::new(
                StageState::Weak,
                Confidence::Medium,
                "weak boundary oracle",
            ),
            observed_values: Vec::<ValueFact>::new(),
            missing_discriminators: Vec::new(),
            statically_contradicted_related_tests: 0,
            new_test_target: None,
        },
        seam,
        class: SeamGripClass::WeaklyGripped,
    };
    assert_eq!(
        super::render::why_line(&entry),
        "static discriminator summary: weak boundary oracle"
    );
}

#[test]
fn why_line_falls_back_to_class_label_when_no_summary_or_missing_discriminator() {
    let seam = seam("src/pricing.rs", 88, "amount >= discount_threshold");
    let entry = ClassifiedSeam {
        evidence: TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: Vec::new(),
            reach: stage(StageState::Yes),
            activate: stage(StageState::Yes),
            propagate: stage(StageState::Yes),
            observe: stage(StageState::Yes),
            discriminate: StageEvidence::new(StageState::Weak, Confidence::Medium, "   "),
            observed_values: Vec::<ValueFact>::new(),
            missing_discriminators: Vec::new(),
            statically_contradicted_related_tests: 0,
            new_test_target: None,
        },
        seam,
        class: SeamGripClass::Ungripped,
    };
    assert_eq!(
        super::render::why_line(&entry),
        "ungripped static seam evidence"
    );
}

/// A route-ready seam related to one test per file in `test_files` (#3906).
/// The missing-discriminator shape and a fully observed `discriminate` stage
/// are what `repair_route_readiness` needs to select a target.
fn route_ready_entry(test_files: &[&str]) -> ClassifiedSeam {
    let related = test_files
        .iter()
        .map(|file| {
            let mut test = related_test();
            test.file = PathBuf::from(file);
            test
        })
        .collect();
    let mut entry = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![MissingDiscriminatorFact {
            value: "discount_threshold (equality boundary)".to_string(),
            reason: "observed values do not include the equality-boundary case".to_string(),
            flow_sink: None,
        }],
        related,
    );
    entry.evidence.discriminate = stage(StageState::Yes);
    entry
}

/// Every pilot surface offers `agent repair` only when the fail-closed
/// repair-packet flip holds, not when route readiness alone does (#3906). The
/// second entry adds a TypeScript observer beside the same Rust test: the Rust
/// test still resolves a target, so the route stays ready and the focused-test
/// outline stays applicable, but the oracle path is unresolved from Rust
/// evidence. A renderer gated on readiness or on the outline passes the first
/// half and fails the second. The third is eligible but its recommended test
/// sits in the production file, which `agent repair` refuses to edit.
#[test]
fn pilot_offers_agent_repair_only_past_the_repair_packet_flip() -> Result<(), String> {
    use crate::analysis::repair_route::repair_packet_eligibility;
    use crate::output::agent_seam_packets::targeted_test_brief_outline_for_classified_seam;

    let artifacts = pilot_artifacts();
    for (test_files, eligible, offered_expected) in [
        (&["tests/pricing.rs"][..], true, true),
        (
            &["tests/pricing.rs", "tests/pricing.test.ts"][..],
            false,
            false,
        ),
        // Eligible, but the recommended test is an inline module in a
        // production file, which `agent repair` refuses as an edit target.
        (&["src/pricing.rs"][..], true, false),
    ] {
        let test_file = test_files.join(" + ");
        let entry = route_ready_entry(test_files);
        // Fixture preconditions: both are route ready with an applicable
        // focused-test outline; only the Rust-only one is eligible.
        let eligibility = repair_packet_eligibility(&entry);
        if !eligibility.readiness.is_repair_ready() {
            return Err(format!("{test_file}: fixture must be route ready"));
        }
        if eligibility.eligible() != eligible {
            return Err(format!("{test_file}: eligibility must be {eligible}"));
        }
        if targeted_test_brief_outline_for_classified_seam(&entry).is_not_applicable() {
            return Err(format!(
                "{test_file}: the focused-test outline must stay applicable"
            ));
        }
        // The inline-test case must reach the test-surface check with its
        // production file as the recommended target, or its row is vacuous.
        let recommended = crate::output::agent_seam_packets::recommended_test_for(&entry).file;
        if eligible && crate::analysis::is_test_surface_path(&recommended) != offered_expected {
            return Err(format!(
                "{test_file}: recommended test `{recommended}` test-surface must be {offered_expected}"
            ));
        }
        let command = format!(
            "ripr agent repair --root {} --seam-id {} --phase before",
            crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root(".")),
            entry.seam.id().as_str()
        );
        let entries = [entry];

        let terminal = render_pilot_terminal(&entries, pilot_context(&artifacts));
        let json = render_pilot_summary_json(&entries, pilot_context(&artifacts));
        let summary: serde_json::Value =
            serde_json::from_str(&json).map_err(|e| format!("parse pilot JSON: {e}\n{json}"))?;
        let md = render_pilot_summary_md(&entries, pilot_context(&artifacts));

        // Both must be the ranked top seam, or the negative half is vacuous.
        if !terminal.contains(&format!(
            "inspected seam: {} ",
            entries[0].seam.id().as_str()
        )) {
            return Err(format!("{test_file}: must be the top seam\n{terminal}"));
        }
        let offered = [
            terminal.contains(&format!("  repair this seam: {command}\n")),
            terminal.contains(&format!("  1. {command}\n")),
            summary
                .pointer("/next/repair_command")
                .and_then(serde_json::Value::as_str)
                == Some(command.as_str()),
            md.contains(&command),
        ];
        if offered != [offered_expected; 4] {
            return Err(format!(
                "{test_file}: terminal line, closing step, JSON, Markdown = {offered:?}, want all {offered_expected}\n{terminal}\n{json}\n{md}"
            ));
        }
        if !offered_expected {
            if !summary
                .pointer("/next/repair_command")
                .is_some_and(serde_json::Value::is_null)
            {
                return Err(format!(
                    "{test_file}: JSON repair_command must be null\n{json}"
                ));
            }
            if terminal.contains("agent repair") || terminal.contains("Next, in order:") {
                return Err(format!(
                    "{test_file}: no repair route on screen\n{terminal}"
                ));
            }
        }
    }
    Ok(())
}

fn discovered_files(
    entries: &[(crate::domain::LanguageId, &str)],
) -> Vec<(crate::domain::LanguageId, PathBuf)> {
    entries
        .iter()
        .map(|(language, path)| (*language, PathBuf::from(path)))
        .collect()
}

#[test]
fn pilot_language_routes_state_follows_rust_seams_and_discovered_languages() {
    use super::language_routes::PilotLanguageRoutesState;
    use crate::domain::LanguageId;
    use crate::output::repo_exposure::{PythonRepoExposureGuidance, TsFullRepoGuidance};

    let root = Path::new(".");
    let rust_only = PilotLanguageRoutes::from_discovered(root, false, &[LanguageId::Rust], &[]);
    assert_eq!(rust_only.state, PilotLanguageRoutesState::NotDetected);
    assert!(rust_only.routes.is_empty());
    assert!(rust_only.required().is_none());

    let files = discovered_files(&[
        (LanguageId::Perl, "lib/App.pm"),
        (LanguageId::Python, "src/app.py"),
        (LanguageId::JavaScript, "web/b.js"),
        (LanguageId::TypeScript, "web/c.ts"),
        (LanguageId::TypeScript, "web/d.ts"),
    ]);
    let enabled = [LanguageId::Rust, LanguageId::TypeScript];
    let required = PilotLanguageRoutes::from_discovered(root, false, &enabled, &files);
    assert_eq!(required.state, PilotLanguageRoutesState::Required);
    assert_eq!(required.state.as_str(), "required");
    // A language this binary cannot analyze (#4252: TypeScript and Python
    // in a Rust-only build, Perl without `lang-perl`) gets the unavailable
    // route whatever ripr.toml enables: no command, the adapter notice.
    let assert_unavailable = |route: &super::language_routes::PilotLanguageRoute| {
        assert_eq!(route.language_status(), "unavailable");
        assert_eq!(route.route(), "unavailable_in_this_binary");
        assert_eq!(route.command, None);
        assert_eq!(route.guidance, route.language.unavailable_adapter_notice());
    };
    let typescript_available = LanguageId::TypeScript.is_available();
    let summary = required
        .routes
        .iter()
        .map(|route| (route.language, route.file_count, route.enabled))
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        vec![
            (LanguageId::TypeScript, 2, typescript_available),
            // JavaScript runs through the TypeScript-family adapter.
            (LanguageId::JavaScript, 1, typescript_available),
            (LanguageId::Python, 1, false),
            (LanguageId::Perl, 1, false),
        ]
    );
    for route in &required.routes[..2] {
        if !typescript_available {
            assert_unavailable(route);
            continue;
        }
        assert_eq!(route.command.as_deref(), Some(route_command().as_str()));
        assert_eq!(route.guidance_category, Some(TsFullRepoGuidance::CATEGORY));
        assert_eq!(
            route.guidance.as_deref(),
            Some(TsFullRepoGuidance::REPAIR_ROUTE)
        );
    }
    let python = &required.routes[2];
    if LanguageId::Python.is_available() {
        assert_eq!(python.command.as_deref(), Some(route_command().as_str()));
        assert_eq!(
            python.guidance_category,
            Some(PythonRepoExposureGuidance::CATEGORY)
        );
        assert_eq!(
            python.guidance.as_deref(),
            Some(PythonRepoExposureGuidance::REPAIR_ROUTE)
        );
    } else {
        assert_unavailable(python);
    }
    let perl = &required.routes[3];
    if LanguageId::Perl.is_available() {
        assert_eq!(perl.language_status(), "preview");
        assert_eq!(perl.command.as_deref(), Some(route_command().as_str()));
    } else {
        assert_unavailable(perl);
    }
    let route = route_command();
    let expected_commands: Vec<&str> = if typescript_available || LanguageId::Python.is_available()
    {
        vec![route.as_str()]
    } else {
        Vec::new()
    };
    assert_eq!(
        PilotLanguageRoutes::commands(&required.routes),
        expected_commands
    );

    let supplementary = PilotLanguageRoutes::from_discovered(root, true, &enabled, &files);
    assert_eq!(supplementary.state, PilotLanguageRoutesState::Supplementary);
    assert_eq!(supplementary.routes, required.routes);
    assert!(supplementary.required().is_none());
}

#[test]
// Compares the route label across runnable TypeScript and Python routes;
// a build lacking either has no such pair (#4252).
#[cfg(all(feature = "lang-typescript", feature = "lang-python"))]
fn pilot_terminal_route_label_has_one_shape_for_every_language() {
    use crate::domain::LanguageId;
    use crate::output::repo_exposure::{PythonRepoExposureGuidance, TsFullRepoGuidance};

    // Re-walk N10: the route label is `route:` for every language. The
    // guidance category stays in JSON and Markdown, so TypeScript
    // (`typescript_diff_first`) and Python (`python_diff_first`) print the
    // same label.
    let artifacts = pilot_artifacts();
    let files = discovered_files(&[
        (LanguageId::TypeScript, "web/c.ts"),
        (LanguageId::JavaScript, "web/d.js"),
        (LanguageId::Python, "src/pricing.py"),
    ]);
    let enabled = [LanguageId::Rust, LanguageId::TypeScript, LanguageId::Python];
    let routes = PilotLanguageRoutes::from_discovered(Path::new("."), false, &enabled, &files);
    let runnable: Vec<_> = routes
        .routes
        .iter()
        .filter(|route| route.command.is_some())
        .collect();
    assert!(
        runnable.iter().any(|route| {
            route.language == LanguageId::TypeScript
                && route.guidance_category == Some(TsFullRepoGuidance::CATEGORY)
        }),
        "{routes:?}"
    );
    assert!(
        runnable.iter().any(|route| {
            route.language == LanguageId::Python
                && route.guidance_category == Some(PythonRepoExposureGuidance::CATEGORY)
        }),
        "{routes:?}"
    );

    let context = PilotSummaryContext {
        language_routes: Some(&routes),
        seam_limit: None,
        ..pilot_context(&artifacts)
    };
    let terminal = render_pilot_terminal(&[], context);
    for route in &runnable {
        let header = format!("  {}: ", route.language.as_str());
        let route_line = terminal
            .lines()
            .skip_while(|line| !line.starts_with(&header))
            .nth(1)
            .unwrap_or_default();
        assert_eq!(
            route_line,
            format!("    route: {}", route_command()),
            "{} route label differs:\n{terminal}",
            route.language.as_str()
        );
    }
}

#[test]
fn pilot_names_unanalyzed_languages_instead_of_an_empty_complete_ranking() -> Result<(), String> {
    use super::language_routes::PilotLanguageRoutesState;
    use crate::domain::LanguageId;

    // A Go repository: no Rust seams, no routed language. Pilot said
    // "complete", "none ranked", and offered a test-then-compare loop.
    let artifacts = pilot_artifacts();
    let root = Path::new(".");
    let go_only = PilotLanguageRoutes::from_discovered(root, false, &[LanguageId::Rust], &[])
        .with_unanalyzed(vec![("Go", 2)], false);
    assert_eq!(go_only.state, PilotLanguageRoutesState::UnanalyzedOnly);
    let context = PilotSummaryContext {
        language_routes: Some(&go_only),
        seam_limit: None,
        ..pilot_context(&artifacts)
    };
    let terminal = render_pilot_terminal(&[], context);
    assert!(
        !terminal.contains("none ranked by the default pilot policy"),
        "{terminal}"
    );
    assert!(
        terminal.contains("languages ripr does not analyze"),
        "{terminal}"
    );
    assert!(terminal.contains("found: Go (2 files)"), "{terminal}");
    assert!(!terminal.contains("ripr outcome --before"), "{terminal}");
    assert!(
        terminal.ends_with("No follow-up command applies: review changes in these languages with their own tests.\n"),
        "{terminal}"
    );
    let md = render_pilot_summary_md(&[], context);
    assert!(md.contains("Found: Go (2 files)."), "{md}");
    assert!(!md.contains("ripr outcome --before"), "{md}");
    let json = render_pilot_summary_json(&[], context);
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
    assert_eq!(parsed["language_routes"]["state"], "unanalyzed_only");
    assert_eq!(
        parsed["language_routes"]["unanalyzed_languages"][0]["language"],
        "Go"
    );
    assert_eq!(
        parsed["language_routes"]["unanalyzed_languages"][0]["file_count"],
        2
    );
    // No seam exists to snapshot or measure, so JSON offers no follow-up
    // command either.
    assert!(parsed["next"]["after_snapshot_command"].is_null(), "{json}");
    assert!(parsed["next"]["outcome_command"].is_null(), "{json}");
    assert!(parsed["next"]["repair_command"].is_null(), "{json}");

    // Rust seams present: the ranking stands and the Go files stay a JSON
    // note, so Rust users' output is unchanged.
    let with_rust = PilotLanguageRoutes::from_discovered(root, true, &[LanguageId::Rust], &[])
        .with_unanalyzed(vec![("Go", 2)], true);
    assert_eq!(with_rust.state, PilotLanguageRoutesState::NotDetected);
    assert!(with_rust.unanalyzed_only().is_none());
    let json = render_pilot_summary_json(
        &[],
        PilotSummaryContext {
            language_routes: Some(&with_rust),
            seam_limit: None,
            ..pilot_context(&artifacts)
        },
    );
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
    assert_eq!(
        parsed["language_routes"]["unanalyzed_languages"][0]["language"],
        "Go"
    );

    // A Rust crate with no seams yet (a lone `pub const`) plus a CI script
    // is a Rust repository, not an unanalyzed one.
    let seamless_rust = PilotLanguageRoutes::from_discovered(root, false, &[LanguageId::Rust], &[])
        .with_unanalyzed(vec![("Shell", 1)], true);
    assert!(seamless_rust.unanalyzed_only().is_none());

    // Nothing unanalyzed: the JSON shape is unchanged for Rust users.
    let plain = PilotLanguageRoutes::from_discovered(root, true, &[LanguageId::Rust], &[]);
    let json = render_pilot_summary_json(
        &[],
        PilotSummaryContext {
            language_routes: Some(&plain),
            seam_limit: None,
            ..pilot_context(&artifacts)
        },
    );
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
    assert_eq!(
        parsed["language_routes"],
        serde_json::json!({"state": "not_detected", "routes": []})
    );
    assert!(
        parsed["next"]["after_snapshot_command"].is_string(),
        "{json}"
    );
    assert!(parsed["next"]["outcome_command"].is_string(), "{json}");
    Ok(())
}

#[test]
fn pilot_names_rust_exclusion_instead_of_silently_ranking_nothing() -> Result<(), String> {
    use crate::domain::LanguageId;

    // #5205: Rust files exist but Rust is disabled. The ranking is empty by
    // config, not by merit, and all three surfaces say so.
    let artifacts = pilot_artifacts();
    let root = Path::new(".");
    let excluded = PilotLanguageRoutes::from_discovered(root, false, &[LanguageId::Python], &[])
        .with_unanalyzed(Vec::new(), true)
        .with_rust_exclusion(Some(3));
    assert_eq!(excluded.rust_exclusion(), Some(3));
    let context = PilotSummaryContext {
        language_routes: Some(&excluded),
        seam_limit: None,
        ..pilot_context(&artifacts)
    };
    let terminal = render_pilot_terminal(&[], context);
    assert!(
        terminal.contains("Excluded from pilot's Rust seam scan:"),
        "{terminal}"
    );
    assert!(
        terminal.contains("rust: 3 files (not enabled in ripr.toml [languages])"),
        "{terminal}"
    );
    assert!(terminal.contains("Next, to rank Rust seams:"), "{terminal}");
    assert!(!terminal.contains("ripr outcome --before"), "{terminal}");
    let md = render_pilot_summary_md(&[], context);
    assert!(
        md.contains("## Excluded From Pilot's Rust Seam Scan"),
        "{md}"
    );
    assert!(md.contains("To rank Rust seams:"), "{md}");
    let json = render_pilot_summary_json(&[], context);
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
    assert_eq!(
        parsed["language_routes"]["rust_excluded_from_scope"]["file_count"],
        3
    );
    assert_eq!(
        parsed["language_routes"]["rust_excluded_from_scope"]["enabled"],
        false
    );
    assert!(parsed["next"]["after_snapshot_command"].is_null(), "{json}");
    assert!(parsed["next"]["outcome_command"].is_null(), "{json}");

    // No exclusion: the JSON shape is unchanged for Rust users.
    let plain = PilotLanguageRoutes::from_discovered(root, true, &[LanguageId::Rust], &[])
        .with_rust_exclusion(None);
    let json = render_pilot_summary_json(
        &[],
        PilotSummaryContext {
            language_routes: Some(&plain),
            seam_limit: None,
            ..pilot_context(&artifacts)
        },
    );
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
    assert_eq!(
        parsed["language_routes"],
        serde_json::json!({"state": "not_detected", "routes": []})
    );
    Ok(())
}

#[test]
fn pilot_renderers_show_language_routes_only_without_rust_seams() -> Result<(), String> {
    use crate::domain::LanguageId;

    let artifacts = pilot_artifacts();
    let files = discovered_files(&[
        (LanguageId::TypeScript, "web/c.ts"),
        (LanguageId::Perl, "lib/App.pm"),
    ]);
    let root = Path::new(".");

    // Rust seams exist: terminal and Markdown are byte-identical to a run
    // without language routes, and JSON lists the routes as supplementary.
    let entries = [classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    )];
    let supplementary =
        PilotLanguageRoutes::from_discovered(root, true, &[LanguageId::Rust], &files);
    let with_routes = PilotSummaryContext {
        language_routes: Some(&supplementary),
        seam_limit: None,
        ..pilot_context(&artifacts)
    };
    assert_eq!(
        render_pilot_terminal(&entries, with_routes),
        render_pilot_terminal(&entries, pilot_context(&artifacts))
    );
    assert_eq!(
        render_pilot_summary_md(&entries, with_routes),
        render_pilot_summary_md(&entries, pilot_context(&artifacts))
    );
    let json = render_pilot_summary_json(&entries, with_routes);
    assert!(json.contains("\"state\": \"supplementary\""), "{json}");
    assert!(json.contains("\"language\": \"typescript\""), "{json}");
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
    assert_eq!(parsed["language_routes"]["routes"][1]["language"], "perl");

    // No Rust seams: every surface names the routes and none reads as clean.
    let required = PilotLanguageRoutes::from_discovered(root, false, &[LanguageId::Rust], &files);
    let context = PilotSummaryContext {
        language_routes: Some(&required),
        seam_limit: None,
        ..pilot_context(&artifacts)
    };
    let terminal = render_pilot_terminal(&[], context);
    assert!(
        !terminal.contains("none ranked by the default pilot policy"),
        "{terminal}"
    );
    let md = render_pilot_summary_md(&[], context);
    assert!(
        md.contains("## Languages Outside The Rust Seam Scan"),
        "{md}"
    );
    if let Some(notice) = LanguageId::TypeScript.unavailable_adapter_notice() {
        // #4252: a Rust-only build names the rebuild and invents no route.
        assert!(
            terminal.contains(&format!(
                "typescript: 1 file (not available in this build)\n    {notice}\n"
            )),
            "{terminal}"
        );
        assert!(md.contains(&notice), "{md}");
        if !LanguageId::Perl.is_available() {
            // Neither discovered language is analyzable: no command at all.
            assert!(!terminal.contains("route: ripr check"), "{terminal}");
            assert!(
                terminal.ends_with("No follow-up command applies: this ripr binary cannot analyze the languages listed above.\n"),
                "{terminal}"
            );
            assert!(!md.contains("```bash"), "{md}");
        }
    } else {
        assert!(
            terminal.contains(&format!("typescript: 1 file (preview, diff-first; not enabled in ripr.toml [languages])\n    route: {}\n", route_command())),
            "{terminal}"
        );
        assert!(
            terminal.ends_with(&format!(
                "Next, analyze the changed code in these languages:\n  {}\n",
                route_command()
            )),
            "{terminal}"
        );
        assert!(
            md.contains(&format!("```bash\n{}\n```", route_command())),
            "{md}"
        );
    }
    assert!(!terminal.contains("ripr outcome --before"), "{terminal}");
    assert!(!md.contains("ripr outcome --before"), "{md}");
    let json = render_pilot_summary_json(&[], context);
    assert!(json.contains("\"state\": \"required\""), "{json}");
    if let Some(notice) = LanguageId::Perl.unavailable_adapter_notice() {
        assert!(
            terminal.contains(&format!(
                "perl: 1 file (not available in this build)\n    {notice}\n"
            )),
            "{terminal}"
        );
        assert!(md.contains(&notice), "{md}");
    }

    // Only unavailable languages: no runnable command is invented.
    if !LanguageId::Perl.is_available() {
        let perl_only = PilotLanguageRoutes::from_discovered(
            root,
            false,
            &[LanguageId::Rust],
            &discovered_files(&[(LanguageId::Perl, "lib/App.pm")]),
        );
        let context = PilotSummaryContext {
            language_routes: Some(&perl_only),
            seam_limit: None,
            ..pilot_context(&artifacts)
        };
        let terminal = render_pilot_terminal(&[], context);
        assert!(
            terminal.ends_with("No follow-up command applies: this ripr binary cannot analyze the languages listed above.\n"),
            "{terminal}"
        );
        let md = render_pilot_summary_md(&[], context);
        assert!(md.ends_with("No follow-up command applies: this ripr binary cannot analyze the languages listed above.\n"), "{md}");
        assert!(!md.contains("```bash"), "{md}");
    }
    Ok(())
}

/// rc rehearsal (py-pricing): with no Rust seam, a Python repair card was
/// the top recommendation but the closing block said `ripr check`, which only
/// leads back to pilot. The closing block now names the card's route:
/// `ripr first-pr` before the edit (it names the receipt command, and stops
/// selecting the gap once the edit closes it), the test edit, the card's
/// verify command, and the receipt command. A card that carries its own
/// receipt command skips first-pr.
#[test]
fn pilot_terminal_next_follows_the_python_repair_card_route() -> Result<(), String> {
    use crate::domain::LanguageId;

    let artifacts = pilot_artifacts();
    let python = python_first_use();
    let files = discovered_files(&[(LanguageId::Python, "pricing/__init__.py")]);
    let routes =
        PilotLanguageRoutes::from_discovered(Path::new("."), false, &[LanguageId::Python], &files);
    let first_pr = format!(
        "ripr first-pr --root {}",
        crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root("."))
    );
    for language_routes in [None, Some(&routes)] {
        let context = PilotSummaryContext {
            language_routes,
            ..pilot_context_with_python(&artifacts, &python)
        };
        let terminal = render_pilot_terminal(&[], context);
        let expected = format!(
            "Next, in order:\n  1. {first_pr} (names this gap's receipt command; run any regeneration command it prints first)\n  2. strengthen test_calculate_discount_above_threshold in tests/test_pricing.py (test files only): Assert the owner result or effect at the boundary `amount == threshold`.\n  3. pytest tests/test_pricing.py::test_calculate_discount_above_threshold\n  4. run the receipt command step 1 printed\n"
        );
        assert!(terminal.ends_with(&expected), "{terminal}");
        assert!(
            !terminal.contains("Next, analyze the changed code"),
            "{terminal}"
        );
        assert!(!terminal.contains("ripr outcome --before"), "{terminal}");
        let md = render_pilot_summary_md(&[], context);
        let next = md
            .split("## Next Commands")
            .nth(1)
            .ok_or_else(|| format!("missing Next Commands: {md}"))?;
        assert!(
            next.contains(&format!(
                "```bash\n{first_pr}\npytest tests/test_pricing.py::test_calculate_discount_above_threshold\n```"
            )),
            "{next}"
        );
        assert!(!next.contains("ripr check --root"), "{next}");
    }

    // A card that carries its own receipt command uses it directly.
    let mut carded = python_first_use();
    if let Some(card) = carded.top_repair_card.as_mut() {
        card.receipt_command = Some("ripr receipt write --gap g --status not_run".to_string());
    }
    let context = pilot_context_with_python(&artifacts, &carded);
    let terminal = render_pilot_terminal(&[], context);
    assert!(
        terminal.ends_with("  2. pytest tests/test_pricing.py::test_calculate_discount_above_threshold\n  3. ripr receipt write --gap g --status not_run\n"),
        "{terminal}"
    );
    assert!(!terminal.contains("ripr first-pr --root"), "{terminal}");
    let md = render_pilot_summary_md(&[], context);
    assert!(
        md.contains("```bash\npytest tests/test_pricing.py::test_calculate_discount_above_threshold\nripr receipt write --gap g --status not_run\n```"),
        "{md}"
    );
    Ok(())
}

/// A `-U0` unified diff that changes `line` of `file`, as the default
/// `ripr check` diff loader produces it.
fn one_line_diff(file: &str, line: usize) -> String {
    format!(
        "diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n@@ -{line} +{line} @@\n-    amount > discount_threshold\n+    amount >= discount_threshold\n"
    )
}

fn changed(file: &str, line: usize) -> PilotCurrentChange {
    PilotCurrentChange::from_diff_text(
        Path::new("."),
        Some("origin/main".to_string()),
        &one_line_diff(file, line),
    )
}

#[test]
fn seam_budget_keeps_only_actionable_changed_seams() {
    let change = changed("src/a.rs", 10);
    let changed_actionable =
        classified_with(SeamGripClass::WeaklyGripped, "src/a.rs", 10, vec![], vec![]);
    let changed_solved = classified_with(
        SeamGripClass::StronglyGripped,
        "src/a.rs",
        10,
        vec![],
        vec![],
    );
    let untouched_actionable =
        classified_with(SeamGripClass::WeaklyGripped, "src/b.rs", 10, vec![], vec![]);
    assert!(change.touches(&changed_solved));
    assert!(change.keeps_past_budget(&changed_actionable));
    // A solved changed seam must not take a budget slot from an actionable
    // seam: with budget 1 it would leave pilot nothing to recommend.
    assert!(!change.keeps_past_budget(&changed_solved));
    assert!(!change.keeps_past_budget(&untouched_actionable));
}

/// #6943: the change's seams classified past the inventory seam limit join
/// the ranked population only when they are on a changed line and were not
/// already classified.
#[test]
fn change_seams_cut_by_the_inventory_limit_are_added_once() {
    let diff = format!(
        "{}diff --git a/README.md b/README.md\n--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-a\n+b\n",
        one_line_diff("src/a.rs", 10)
    );
    let change =
        PilotCurrentChange::from_diff_text(Path::new("."), Some("origin/main".to_string()), &diff);
    assert_eq!(change.changed_rust_files(), [PathBuf::from("src/a.rs")]);
    // Stable path text escapes `%`; the inventory needs the on-disk name.
    assert_eq!(
        changed("src/100%.rs", 1).changed_rust_files(),
        [PathBuf::from("src/100%.rs")]
    );
    assert!(
        PilotCurrentChange::from_diff_text(Path::new("."), None, "")
            .changed_rust_files()
            .is_empty()
    );

    let kept = classified_with(SeamGripClass::WeaklyGripped, "src/b.rs", 3, vec![], vec![]);
    let already = classified_with(SeamGripClass::WeaklyGripped, "src/a.rs", 10, vec![], vec![]);
    let mut classified = vec![kept, already.clone()];
    let cut_on_change = ClassifiedSeam {
        seam: RepoSeam::new(
            "src/a.rs",
            "pricing::other_total",
            SeamKind::PredicateBoundary,
            105,
            10,
            "other >= operand",
            RequiredDiscriminator::BoundaryValue {
                description: "other >= operand".to_string(),
            },
            ExpectedSink::ReturnValue,
        ),
        ..classified_with(SeamGripClass::Ungripped, "src/a.rs", 10, vec![], vec![])
    };
    let cut_off_change = classified_with(SeamGripClass::Ungripped, "src/a.rs", 40, vec![], vec![]);
    let added = change.add_cut_seams(
        &mut classified,
        vec![already, cut_on_change.clone(), cut_off_change],
    );
    assert_eq!(added, 1);
    assert_eq!(classified.len(), 3);
    assert_ne!(classified[1].seam.id(), cut_on_change.seam.id());
    assert_eq!(classified[2].seam.id(), cut_on_change.seam.id());
}

/// #6943: folding the change's own classification decides whether the
/// seam-limit caveat on the change still applies.
#[test]
fn folding_the_classified_change_keeps_the_caveat_only_when_it_can_hold() {
    let limit = || {
        Some(crate::analysis::SeamLimitInfo {
            analyzed: 1,
            total: 5,
            source: crate::analysis::SeamLimitSource::Default,
        })
    };
    let rust_change = changed("src/a.rs", 10);
    let on_change = classified_with(SeamGripClass::Ungripped, "src/a.rs", 10, vec![], vec![]);

    // Classified: the cut seam joins, counts as analyzed, and the caveat goes.
    let (mut classified, mut inventory_limit) = (Vec::new(), limit());
    let folded = rust_change.fold_classified_change(
        &mut classified,
        &mut inventory_limit,
        Some(Ok(vec![on_change])),
    );
    assert_eq!(
        (folded.added, folded.caveat_limit, folded.error),
        (1, None, None)
    );
    assert_eq!(classified.len(), 1);
    assert_eq!(
        inventory_limit.as_ref().map(|limit| limit.analyzed),
        Some(2)
    );

    // When every cut seam was on the change, nothing is left unanalyzed.
    let (mut classified, mut inventory_limit) = (Vec::new(), limit());
    if let Some(limit) = inventory_limit.as_mut() {
        limit.total = 2;
    }
    rust_change.fold_classified_change(
        &mut classified,
        &mut inventory_limit,
        Some(Ok(vec![classified_with(
            SeamGripClass::Ungripped,
            "src/a.rs",
            10,
            vec![],
            vec![],
        )])),
    );
    assert_eq!(inventory_limit, None);

    // Failed: nothing joins and the caveat cites the inventory's limit.
    let (mut classified, mut inventory_limit) = (Vec::new(), limit());
    let folded = rust_change.fold_classified_change(
        &mut classified,
        &mut inventory_limit,
        Some(Err("late".to_string())),
    );
    assert_eq!(folded.added, 0);
    assert_eq!(folded.caveat_limit.map(|limit| limit.analyzed), Some(1));
    assert_eq!(folded.error.as_deref(), Some("late"));
    assert!(classified.is_empty());

    // Not run on a Rust change (the limit did not fire): the caveat stays
    // whenever there is a limit to cite.
    let (mut classified, mut inventory_limit) = (Vec::new(), limit());
    let folded = rust_change.fold_classified_change(&mut classified, &mut inventory_limit, None);
    assert_eq!(folded.caveat_limit.map(|limit| limit.total), Some(5));

    // A change with no Rust file: pilot ranks Rust seams only, so the limit
    // cannot hide a seam on it.
    let docs_change = changed("README.md", 1);
    let (mut classified, mut inventory_limit) = (Vec::new(), limit());
    let folded = docs_change.fold_classified_change(&mut classified, &mut inventory_limit, None);
    assert_eq!(folded.caveat_limit, None);
}

#[test]
fn pilot_ranking_puts_seams_in_the_current_change_first() {
    // The untouched seam is the better class (weak beats ungripped), so the
    // repo-wide order puts it first.
    let untouched = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/a.rs",
        10,
        vec![missing()],
        vec![related_test()],
    );
    let touched = classified_with(
        SeamGripClass::Ungripped,
        "src/b.rs",
        20,
        Vec::new(),
        Vec::new(),
    );
    let touched_weaker = classified_with(
        SeamGripClass::ReachableUnrevealed,
        "src/b.rs",
        21,
        Vec::new(),
        Vec::new(),
    );
    let entries = [untouched, touched_weaker, touched];
    let order = |ranked: Vec<&ClassifiedSeam>| {
        ranked
            .iter()
            .map(|entry| {
                format!(
                    "{}:{}",
                    display_path(entry.seam.file()),
                    entry.seam.display_line()
                )
            })
            .collect::<Vec<_>>()
    };

    // A diff touching src/b.rs lines 20-21 puts both touched seams first and
    // keeps the existing class order inside that group.
    let diff = format!(
        "{}@@ -21 +21 @@\n-    a\n+    b\n",
        one_line_diff("src/b.rs", 20)
    );
    let change =
        PilotCurrentChange::from_diff_text(Path::new("."), Some("origin/main".to_string()), &diff);
    assert_eq!(
        order(top_actionable_seams(&entries, 5, Some(&change))),
        ["src/b.rs:20", "src/b.rs:21", "src/a.rs:10"]
    );

    // A change elsewhere, no change, an unavailable diff and no change data
    // all keep the repo-wide order.
    let repo_wide = ["src/a.rs:10", "src/b.rs:20", "src/b.rs:21"];
    for change in [
        Some(changed("src/other.rs", 20)),
        Some(PilotCurrentChange::from_diff_text(
            Path::new("."),
            Some("origin/main".to_string()),
            "",
        )),
        Some(PilotCurrentChange::from_diff_load(
            Path::new("."),
            Err("not a Git work tree"),
        )),
        None,
    ] {
        assert_eq!(
            order(top_actionable_seams(&entries, 5, change.as_ref())),
            repo_wide,
            "{change:?}"
        );
    }
}

#[test]
fn pilot_current_change_matches_the_seam_span_and_new_side_lines() {
    let multi_line = ClassifiedSeam {
        seam: seam("src/b.rs", 20, "amount\n        >= discount_threshold"),
        ..classified_with(
            SeamGripClass::Ungripped,
            "src/b.rs",
            20,
            Vec::new(),
            Vec::new(),
        )
    };
    // The seam's expression spans lines 20-21; a change on 21 touches it,
    // one on 22 or 19 does not.
    assert!(changed("src/b.rs", 21).touches(&multi_line));
    assert!(!changed("src/b.rs", 22).touches(&multi_line));
    assert!(!changed("src/b.rs", 19).touches(&multi_line));
    // A pure deletion anchors at the new-side position it was removed from.
    let deletion = PilotCurrentChange::from_diff_text(
        Path::new("."),
        None,
        "diff --git a/src/b.rs b/src/b.rs\n--- a/src/b.rs\n+++ b/src/b.rs\n@@ -30 +29,0 @@\n-    removed();\n",
    );
    let at_29 = classified_with(
        SeamGripClass::Ungripped,
        "src/b.rs",
        29,
        Vec::new(),
        Vec::new(),
    );
    assert!(deletion.touches(&at_29));
    // An absolute root is stripped from seam paths before matching.
    let absolute = PilotCurrentChange::from_diff_text(
        Path::new("/repo"),
        None,
        &one_line_diff("src/b.rs", 20),
    );
    let absolute_seam = classified_with(
        SeamGripClass::Ungripped,
        "/repo/src/b.rs",
        20,
        Vec::new(),
        Vec::new(),
    );
    assert!(absolute.touches(&absolute_seam));
    assert_eq!(absolute.state(), "changed");
}

/// #5309: a change whose seams pilot withholds, whose seams are already
/// gripped, or whose seams a seam limit left unanalyzed must not read as a
/// change with no seams. The counts come from the classified inventory
/// before the pilot budget cut drops the seams pilot cannot recommend.
#[test]
fn pilot_says_why_no_seam_on_the_change_ranks() -> Result<(), String> {
    let artifacts = pilot_artifacts();
    let ranked = classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    );
    let on_change = |class| classified_with(class, "src/other.rs", 3, Vec::new(), Vec::new());
    let limit = crate::analysis::SeamLimitInfo {
        analyzed: 2000,
        total: 10_000,
        source: crate::analysis::SeamLimitSource::Default,
    };
    let full = crate::analysis::SeamLimitInfo {
        analyzed: 3,
        total: 3,
        source: crate::analysis::SeamLimitSource::Default,
    };
    let render = |inventory: &[ClassifiedSeam],
                  limit: Option<&crate::analysis::SeamLimitInfo>|
     -> Result<(String, String, serde_json::Value), String> {
        let change = changed("src/other.rs", 3).with_seams_counted(inventory, limit);
        let context = PilotSummaryContext {
            current_change: Some(&change),
            ..pilot_context(&artifacts)
        };
        // The pilot budget has already cut the seams pilot cannot rank.
        let entries = [ranked.clone()];
        let json = render_pilot_summary_json(&entries, context);
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
        Ok((
            render_pilot_terminal(&entries, context),
            render_pilot_summary_md(&entries, context),
            parsed["current_change"].clone(),
        ))
    };
    let elsewhere = "This recommendation is elsewhere in the repo. For the change itself, run";
    let opaque = || on_change(SeamGripClass::Opaque);
    let unknown = || on_change(SeamGripClass::ActivationUnknown);
    let gripped = || on_change(SeamGripClass::StronglyGripped);
    let unseen = "the seam limit left 8000 of 10000 seams unanalyzed, so the change may have seams pilot did not see";
    for (inventory, limit, reason, withheld) in [
        (
            vec![ranked.clone(), opaque()],
            None,
            "Pilot withholds the analyzed seam on a line changed since origin/main: its static evidence is unknown or opaque, so it is a static limitation, not a gap.".to_string(),
            1,
        ),
        (
            vec![ranked.clone(), opaque(), unknown()],
            None,
            "Pilot withholds the 2 analyzed seams on lines changed since origin/main: their static evidence is unknown or opaque, so they are static limitations, not gaps.".to_string(),
            2,
        ),
        (
            vec![ranked.clone(), unknown(), gripped()],
            None,
            "Pilot withholds 1 of the 2 analyzed seams on lines changed since origin/main: its static evidence is unknown or opaque, so it is a static limitation, not a gap; the other is already gripped, intentional or suppressed.".to_string(),
            1,
        ),
        (
            vec![ranked.clone(), opaque(), unknown(), gripped(), gripped()],
            None,
            "Pilot withholds 2 of the 4 analyzed seams on lines changed since origin/main: their static evidence is unknown or opaque, so they are static limitations, not gaps; the others are already gripped, intentional or suppressed.".to_string(),
            2,
        ),
        (
            vec![ranked.clone(), gripped()],
            None,
            "The analyzed seam on a line changed since origin/main has no gap to rank: it is already gripped, intentional or suppressed.".to_string(),
            0,
        ),
        (
            vec![ranked.clone(), gripped(), gripped()],
            None,
            "The 2 analyzed seams on lines changed since origin/main have no gap to rank: they are already gripped, intentional or suppressed.".to_string(),
            0,
        ),
        // Past the inventory limit, a reason drawn from analyzed seams says
        // it may not be all of them.
        (
            vec![ranked.clone(), opaque()],
            Some(&limit),
            format!("Pilot withholds the analyzed seam on a line changed since origin/main: its static evidence is unknown or opaque, so it is a static limitation, not a gap, but {unseen}."),
            1,
        ),
        (
            vec![ranked.clone(), gripped()],
            Some(&limit),
            format!("The analyzed seam on a line changed since origin/main has no gap to rank: it is already gripped, intentional or suppressed, but {unseen}."),
            0,
        ),
        (
            vec![ranked.clone()],
            Some(&limit),
            format!("No analyzed seam is on a line changed since origin/main, but {unseen}."),
            0,
        ),
        // A limit that analyzed every seam is no limit.
        (
            vec![ranked.clone()],
            Some(&full),
            "No seam pilot analyzed is on a line changed since origin/main.".to_string(),
            0,
        ),
    ] {
        let (terminal, md, json) = render(&inventory, limit)?;
        assert!(
            terminal.contains(&format!(
                "current change: not part of it. {reason} {elsewhere}"
            )),
            "{terminal}"
        );
        let md_reason = reason.replace("since origin/main", "since `origin/main`");
        assert!(
            md.contains(&format!(
                "- Current change: not part of it. {md_reason} {elsewhere}"
            )),
            "{md}"
        );
        assert_eq!(json["withheld_seams_in_change"], withheld, "{json}");
        assert_eq!(json["actionable_seams_in_change"], 0, "{json}");
        assert_eq!(json["top_recommendation_in_change"], false, "{json}");
    }
    Ok(())
}

/// #5309: with nothing ranked, the pilot budget may already have dropped
/// the change's withheld seams, so the empty-ranking text cannot count them.
/// The current-change line still says why the change's seams are not ranked.
#[test]
fn pilot_explains_the_change_when_nothing_ranks() {
    let artifacts = pilot_artifacts();
    let opaque = classified_with(
        SeamGripClass::Opaque,
        "src/other.rs",
        3,
        Vec::new(),
        Vec::new(),
    );
    let change = changed("src/other.rs", 3).with_seams_counted(&[opaque], None);
    let context = PilotSummaryContext {
        current_change: Some(&change),
        ..pilot_context(&artifacts)
    };
    let reason = "Pilot withholds the analyzed seam on a line changed since origin/main: its static evidence is unknown or opaque, so it is a static limitation, not a gap.";
    // The budget cut every seam: nothing is left to rank or count.
    let terminal = render_pilot_terminal(&[], context);
    assert!(
        terminal.contains(&format!("  current change: {reason}\n")),
        "{terminal}"
    );
    let md = render_pilot_summary_md(&[], context);
    let md_reason = reason.replace("since origin/main", "since `origin/main`");
    assert!(
        md.contains(&format!("- Current change: {md_reason}\n")),
        "{md}"
    );
    // A change with no analyzed seam and no limit adds nothing.
    let bare = changed("src/other.rs", 3).with_seams_counted(&[], None);
    let context = PilotSummaryContext {
        current_change: Some(&bare),
        ..pilot_context(&artifacts)
    };
    assert!(!render_pilot_terminal(&[], context).contains("current change"));
    assert!(!render_pilot_summary_md(&[], context).contains("Current change"));
}

/// #6944: a change in files the repo inventory leaves out by design names
/// them, also when nothing ranks; the wording says whether they are the
/// whole change.
#[test]
fn pilot_names_changed_files_its_ranking_leaves_out() {
    use crate::analysis::DiffOnlySource;
    let artifacts = pilot_artifacts();
    let diff = |files: &[&str]| {
        files
            .iter()
            .map(|file| one_line_diff(file, 3))
            .collect::<String>()
    };
    let render = |change: &PilotCurrentChange| {
        let context = PilotSummaryContext {
            current_change: Some(change),
            ..pilot_context(&artifacts)
        };
        (
            render_pilot_terminal(&[], context),
            render_pilot_summary_md(&[], context),
        )
    };

    let only_build = PilotCurrentChange::from_diff_text(
        Path::new("."),
        Some("origin/main".to_string()),
        &diff(&["build.rs"]),
    )
    .with_diff_only_files(vec![(
        PathBuf::from("build.rs"),
        DiffOnlySource::BuildScript,
    )])
    .with_seams_counted(&[], None);
    let (terminal, md) = render(&only_build);
    assert!(
        terminal.contains("  current change: No seam pilot analyzed is on a line changed since origin/main: every changed Rust line is in build.rs, a Cargo build script, which pilot's repo-wide ranking leaves out.\n"),
        "{terminal}"
    );
    assert!(
        md.contains("- Current change: No seam pilot analyzed is on a line changed since `origin/main`: every changed Rust line is in `build.rs`, a Cargo build script, which pilot's repo-wide ranking leaves out.\n"),
        "{md}"
    );

    let partly = PilotCurrentChange::from_diff_text(
        Path::new("."),
        Some("origin/main".to_string()),
        &diff(&["xtask/src/main.rs", "src/lib.rs"]),
    )
    .with_diff_only_files(vec![(
        PathBuf::from("xtask/src/main.rs"),
        DiffOnlySource::RepoAutomation,
    )])
    .with_seams_counted(&[], None);
    let (terminal, _) = render(&partly);
    assert!(
        terminal.contains("the change includes xtask/src/main.rs, repository automation, which"),
        "{terminal}"
    );

    let several = PilotCurrentChange::from_diff_text(
        Path::new("."),
        Some("origin/main".to_string()),
        &diff(&["build.rs", "lib/odd.rs", "src/lib.rs"]),
    )
    .with_diff_only_files(vec![
        (PathBuf::from("build.rs"), DiffOnlySource::BuildScript),
        (
            PathBuf::from("lib/odd.rs"),
            DiffOnlySource::DeclaredOutsideSrc,
        ),
    ])
    .with_seams_counted(&[], None);
    let (terminal, _) = render(&several);
    assert!(
        terminal.contains("the change includes 2 files pilot's repo-wide ranking leaves out, such as build.rs (a Cargo build script)"),
        "{terminal}"
    );

    let only_several = PilotCurrentChange::from_diff_text(
        Path::new("."),
        Some("origin/main".to_string()),
        &diff(&["build.rs", "lib/odd.rs"]),
    )
    .with_diff_only_files(vec![
        (PathBuf::from("build.rs"), DiffOnlySource::BuildScript),
        (
            PathBuf::from("lib/odd.rs"),
            DiffOnlySource::DeclaredOutsideSrc,
        ),
    ])
    .with_seams_counted(&[], None);
    let (terminal, _) = render(&only_several);
    assert!(
        terminal.contains("every changed Rust line is in 2 files pilot's repo-wide ranking leaves out, such as build.rs (a Cargo build script)"),
        "{terminal}"
    );

    // A reason drawn from analyzed seams on the change still names the
    // build script it does not cover.
    let opaque = classified_with(
        SeamGripClass::Opaque,
        "src/lib.rs",
        3,
        Vec::new(),
        Vec::new(),
    );
    let with_seam = PilotCurrentChange::from_diff_text(
        Path::new("."),
        Some("origin/main".to_string()),
        &diff(&["build.rs", "src/lib.rs"]),
    )
    .with_diff_only_files(vec![(
        PathBuf::from("build.rs"),
        DiffOnlySource::BuildScript,
    )])
    .with_seams_counted(&[opaque], None);
    let (terminal, _) = render(&with_seam);
    assert!(
        terminal.contains("so it is a static limitation, not a gap; the change also includes build.rs, a Cargo build script, which pilot's repo-wide ranking leaves out.\n"),
        "{terminal}"
    );

    // #6987: with the seam-limit caveat as well, the diff-only clause comes
    // first and the caveat still closes the sentence.
    let opaque = classified_with(
        SeamGripClass::Opaque,
        "src/lib.rs",
        3,
        Vec::new(),
        Vec::new(),
    );
    let limit = crate::analysis::SeamLimitInfo {
        analyzed: 3,
        total: 7,
        source: crate::analysis::SeamLimitSource::Default,
    };
    let limited = PilotCurrentChange::from_diff_text(
        Path::new("."),
        Some("origin/main".to_string()),
        &diff(&["build.rs", "src/lib.rs"]),
    )
    .with_diff_only_files(vec![(
        PathBuf::from("build.rs"),
        DiffOnlySource::BuildScript,
    )])
    .with_seams_counted(&[opaque], Some(&limit));
    let (terminal, _) = render(&limited);
    assert!(
        terminal.contains("not a gap; the change also includes build.rs, a Cargo build script, which pilot's repo-wide ranking leaves out, but the seam limit left 4 of 7 seams unanalyzed, so the change may have seams pilot did not see.\n"),
        "{terminal}"
    );
}

#[test]
fn pilot_renderers_say_whether_the_top_recommendation_is_in_the_current_change()
-> Result<(), String> {
    let artifacts = pilot_artifacts();
    let entries = [classified_with(
        SeamGripClass::WeaklyGripped,
        "src/pricing.rs",
        88,
        vec![missing()],
        vec![related_test()],
    )];
    let render = |change: Option<&PilotCurrentChange>| -> Result<_, String> {
        let context = PilotSummaryContext {
            current_change: change,
            ..pilot_context(&artifacts)
        };
        let json = render_pilot_summary_json(&entries, context);
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .map_err(|err| format!("pilot summary JSON must parse: {err}\n{json}"))?;
        Ok((
            render_pilot_terminal(&entries, context),
            render_pilot_summary_md(&entries, context),
            parsed["current_change"].clone(),
        ))
    };

    // The change touches the top seam.
    let (terminal, md, json) = render(Some(&changed("src/pricing.rs", 88)))?;
    assert!(
        terminal.contains(
            "Top recommendation:\n  current change: part of it (this seam is on a line changed since origin/main)\n  inspected seam: "
        ),
        "{terminal}"
    );
    assert!(
        md.contains("- Current change: part of it (this seam is on a line changed since `origin/main`)\n- Inspected seam: "),
        "{md}"
    );
    assert!(
        md.contains("src/pricing.rs:88 `predicate_boundary` (in your current change)"),
        "{md}"
    );
    assert!(!terminal.contains("not part of it"), "{terminal}");
    assert_eq!(
        json,
        serde_json::json!({
            "state": "changed",
            "base": "origin/main",
            "reason": null,
            "actionable_seams_in_change": 1,
            "withheld_seams_in_change": 0,
            "top_recommendation_in_change": true
        })
    );

    // The change exists but no ranked seam is on it.
    let (terminal, md, json) = render(Some(&changed("src/other.rs", 3)))?;
    // The named root is bound like every other pilot command, so it pastes
    // from any directory.
    let bound = crate::agent::loop_commands::shell_path(
        &crate::agent::loop_commands::bound_root_path(Path::new(".")),
    );
    assert!(
        terminal.contains(&format!(
            "  current change: not part of it. No seam pilot analyzed is on a line changed since origin/main. This recommendation is elsewhere in the repo. For the change itself, run: ripr check --root {bound}\n"
        )),
        "{terminal}"
    );
    assert!(
        md.contains(&format!(
            "- Current change: not part of it. No seam pilot analyzed is on a line changed since `origin/main`. This recommendation is elsewhere in the repo. For the change itself, run `ripr check --root {bound}`."
        )),
        "{md}"
    );
    assert!(!md.contains("(in your current change)"), "{md}");
    assert_eq!(json["state"], "changed");
    assert_eq!(json["actionable_seams_in_change"], 0);
    assert_eq!(json["withheld_seams_in_change"], 0);
    assert_eq!(json["top_recommendation_in_change"], false);

    // An uncommitted change is invisible to plain `ripr check`, which reads
    // committed history, so the command pilot names selects the working tree.
    let uncommitted = changed("src/other.rs", 3).with_working_tree(true);
    let (worktree_terminal, worktree_md, _) = render(Some(&uncommitted))?;
    assert!(
        worktree_terminal.contains(&format!(
            "For the change itself, run: ripr check --root {bound} --worktree\n"
        )),
        "{worktree_terminal}"
    );
    assert!(
        worktree_md.contains(&format!(
            "For the change itself, run `ripr check --root {bound} --worktree`."
        )),
        "{worktree_md}"
    );

    // The Inspected block names the change-first scope when there is a change.
    assert!(
        terminal.contains(
            "  timeout: 30000 ms\n  scope: change-first (Rust seams on lines changed since origin/main rank first)\n\n"
        ),
        "{terminal}"
    );
    assert!(
        md.contains(
            "- Scope: change-first (Rust seams on lines changed since `origin/main` rank first)\n"
        ),
        "{md}"
    );

    // No change or an unavailable diff: the human output is what pilot printed
    // before current-change detection existed plus one scope line in the
    // Inspected block, and the JSON keeps an unavailable diff (with its
    // reason) distinct from no change. No change data adds no scope line.
    let (baseline_terminal, baseline_md, baseline_json) = render(None)?;
    assert_eq!(baseline_json, serde_json::Value::Null);
    assert!(
        !baseline_terminal.contains("current change") && !baseline_terminal.contains("scope:"),
        "{baseline_terminal}"
    );
    assert!(
        !baseline_md.contains("Current change") && !baseline_md.contains("- Scope:"),
        "{baseline_md}"
    );
    for (change, state, base, reason, scope) in [
        (
            PilotCurrentChange::from_diff_text(Path::new("."), Some("origin/main".to_string()), ""),
            "no_change",
            serde_json::json!("origin/main"),
            serde_json::Value::Null,
            "whole repository",
        ),
        (
            PilotCurrentChange::from_diff_load(Path::new("."), Err("not a Git work tree")),
            "unavailable",
            serde_json::Value::Null,
            serde_json::json!("not a Git work tree"),
            "whole repository (current change unavailable: not a Git work tree)",
        ),
    ] {
        let (terminal, md, json) = render(Some(&change))?;
        assert_eq!(
            terminal,
            baseline_terminal.replacen(
                "  timeout: 30000 ms\n",
                &format!("  timeout: 30000 ms\n  scope: {scope}\n"),
                1
            )
        );
        assert_eq!(
            md,
            baseline_md.replacen(
                "- Config: loaded `ripr.toml`\n",
                &format!("- Config: loaded `ripr.toml`\n- Scope: {scope}\n"),
                1
            )
        );
        assert_eq!(
            json,
            serde_json::json!({
                "state": state,
                "base": base,
                "reason": reason,
                "actionable_seams_in_change": null,
                "withheld_seams_in_change": null,
                "top_recommendation_in_change": null
            })
        );
    }
    Ok(())
}

/// The language route pilot prints for the `.` fixtures: `ripr check` on the
/// repository pilot analyzed, bound so it survives a paste elsewhere (#4000).
fn route_command() -> String {
    format!(
        "ripr check --root {}",
        crate::agent::loop_commands::shell_arg(&crate::agent::loop_commands::bound_root("."))
    )
}
