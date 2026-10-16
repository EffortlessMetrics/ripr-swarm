use super::super::tests::{bounded_output_with_findings, sample_finding};
use super::super::{render_full_with_config, render_full_with_config_and_navigation};
use crate::app::{FindingDrillIn, FindingNavigation};
use crate::config::RiprConfig;
use crate::domain::{Confidence, Finding, RelatedTestMiss, SourceCurrentness, SymbolId};

fn findings() -> Vec<Finding> {
    (1..=3)
        .map(|ordinal| {
            let mut finding = sample_finding();
            finding.id = format!("finding-{ordinal}");
            finding.probe.owner = Some(SymbolId("src/sample.rs::module::sample".to_string()));
            finding
        })
        .collect()
}

fn render(findings: Vec<Finding>) -> String {
    render_full_with_config_and_navigation(
        &bounded_output_with_findings(findings),
        &RiprConfig::default(),
        Some(&FindingDrillIn::Commands(FindingNavigation::legacy())),
    )
}

fn definitions_for(report: &str, text: &str) -> Vec<String> {
    report
        .lines()
        .filter(|line| {
            line.contains(" [findings ")
                && line
                    .split_once("]: ")
                    .is_some_and(|(_, payload)| payload.starts_with(text))
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn exact_facts_share_across_nonadjacent_findings_with_complete_private_fragments()
-> Result<(), Box<dyn std::error::Error>> {
    let mut input = findings();
    input[1].probe.owner = Some(SymbolId("src/sample.rs::other_module::sample".to_string()));
    input[0].probe.before = Some(format!("{}  spaced value", "λ ".repeat(180)));
    input[0].probe.after = Some(format!("{} spaced value", "λ ".repeat(180)));
    let expected = super::super::sections::render_finding_with_config(
        &input[0],
        &RiprConfig::default(),
        std::path::Path::new("repo"),
        super::super::JsonPointerWording::CheckFlag,
    );
    let report = render(input);
    let definitions = definitions_for(&report, "reach yes: reaches test");
    assert_eq!(definitions.len(), 1);
    assert!(definitions[0].contains("[findings 1, 3]"));
    let private = expected
        .split_once("\nEvidence\n")
        .ok_or("missing evidence section")?
        .0;
    let private = private
        .split_inclusive('\n')
        .filter(|line| *line != "\n")
        .collect::<String>();
    assert!(
        report.contains(&private),
        "entire ordered private prefix including full fragments"
    );
    for ordinal in 1..=3 {
        assert_eq!(
            report
                .matches(&format!("  id: finding-{ordinal}\n"))
                .count(),
            1
        );
    }
    assert_eq!(report.matches("Drill in:\n").count(), 3);
    Ok(())
}

#[test]
fn same_stage_caption_with_different_confidence_is_not_shared() {
    let mut input = findings();
    input.truncate(2);
    input[1].ripr.reach.confidence = Confidence::Low;
    let report = render(input);
    assert!(definitions_for(&report, "reach yes: reaches test").is_empty());
    assert_eq!(report.matches("  - reach yes: reaches test\n").count(), 2);
}

#[test]
fn raw_test_and_context_metadata_cannot_borrow_display_identical_facts() {
    let changes: Vec<fn(&mut Finding)> = vec![
        |finding| {
            finding.related_tests[0].relation_reason =
                Some(crate::domain::RelationReason::DirectOwnerCall)
        },
        |finding| {
            finding.related_tests[0].relation_confidence =
                Some(crate::domain::RelationConfidence::Low)
        },
        |finding| finding.related_tests_matched_total = Some(1),
        |finding| finding.probe.location.file = "src/other.rs".into(),
        |finding| finding.language = Some(crate::domain::LanguageId::Rust),
        |finding| finding.owner_kind = Some(crate::domain::OwnerKind::Function),
        |finding| finding.probe.owner = None,
    ];
    for change in changes {
        let mut input = findings();
        input.truncate(2);
        change(&mut input[1]);
        let report = render(input);
        assert!(
            !report.contains("Shared evidence and guidance\n"),
            "{report}"
        );
    }
    let mut input = findings();
    input.truncate(2);
    for finding in &mut input {
        finding.related_tests[0].miss = Some(RelatedTestMiss::AssertionNotObserving);
    }
    input[1].related_tests[0].oracle = Some("assert_eq!(actual,   expected)".to_string());
    let report = render(input);
    assert!(
        !report.contains("Shared evidence and guidance\n"),
        "raw oracle bytes remain distinct"
    );
}

#[test]
fn all_currentness_states_have_separate_catalogue_membership() {
    let states = [
        SourceCurrentness::CandidateCurrent,
        SourceCurrentness::BaseDeleted,
        SourceCurrentness::MovedOrRenamed,
        SourceCurrentness::UnresolvedSubject,
    ];
    for left in states {
        for right in states {
            let mut input = findings();
            input.truncate(2);
            input[0].source_currentness = left;
            input[1].source_currentness = right;
            let report = render(input);
            assert_eq!(
                report.contains("Shared evidence and guidance\n"),
                left == right,
                "{left:?}/{right:?}"
            );
            if left != SourceCurrentness::CandidateCurrent {
                assert!(report.contains(left.as_str()));
            }
            if right != SourceCurrentness::CandidateCurrent {
                assert!(report.contains(right.as_str()));
            }
        }
    }
}

#[test]
fn raw_contextual_miss_provenance_is_not_replaced_by_normalized_caption() {
    let mut input = findings();
    input.truncate(2);
    for finding in &mut input {
        finding.related_tests[0].miss = Some(RelatedTestMiss::MissingInput);
    }
    input[1].activation.missing_discriminators[0]
        .reason
        .push(' ');
    let report = render(input);
    assert!(definitions_for(&report, "related test ").is_empty());
    assert_eq!(report.matches("  - related test ").count(), 2);
}

#[test]
fn different_guidance_and_window_disclosures_stay_local() {
    let mut input = findings();
    input.truncate(2);
    input[0].related_tests_matched_total = Some(10);
    input[1].related_tests_matched_total = Some(10);
    input[1].recommended_next_step = Some("Inspect the other assertion first.".to_string());
    let report = render(input);
    assert!(definitions_for(&report, "related tests (showing").is_empty());
    assert_eq!(report.matches("related tests (showing 1 of 10").count(), 2);
    assert!(!report.contains("  N1 [findings"));
    // #7756: the unshared guidance line is a list-item value, so its
    // trailing period is trimmed like every other labelled line.
    assert!(report.contains("  - Inspect the other assertion first\n"));
    assert!(!report.contains("Inspect the other assertion first."));
}

#[test]
fn duplicate_fact_multiplicity_is_preserved_and_single_finding_is_not_a_donor()
-> Result<(), Box<dyn std::error::Error>> {
    let mut input = findings();
    input.truncate(2);
    for finding in &mut input {
        finding
            .activation
            .observed_values
            .push(finding.activation.observed_values[0].clone());
    }
    let report = render(input.clone());
    let label = definitions_for(&report, "source function argument")[0]
        .split_once(" [findings")
        .ok_or("missing definition")?
        .0
        .trim()
        .to_string();
    assert!(
        report.contains(&format!("{label}, {label}\n")),
        "ordered duplicate references {report}"
    );
    input.truncate(1);
    assert!(!render(input).contains("Shared evidence and guidance\n"));
    Ok(())
}

#[test]
fn suppressed_findings_cannot_donate_and_all_suppressed_route_remains_usable() {
    use crate::output::suppressions::{CheckSuppressionOutcome, SuppressedCheckFinding};
    let mut input = findings();
    input.truncate(2);
    let mut output = bounded_output_with_findings(input);
    output.suppression = Some(CheckSuppressionOutcome {
        suppressed: vec![SuppressedCheckFinding {
            finding_id: "finding-1".to_string(),
            selector: "finding-1".to_string(),
        }],
        policy_path: "fixture.toml".to_string(),
        warnings: Vec::new(),
    });
    let config = RiprConfig::default();
    let navigation = FindingDrillIn::Commands(FindingNavigation::legacy());
    let report = render_full_with_config_and_navigation(&output, &config, Some(&navigation));
    assert!(!report.contains("Shared evidence and guidance\n"));
    assert!(!report.contains("  id: finding-1\n"));
    assert!(report.contains("  id: finding-2\n"));
    output.findings.truncate(1);
    let report = render_full_with_config_and_navigation(&output, &config, Some(&navigation));
    assert!(report.contains("every finding in this run is suppressed by policy"));
    assert!(!report.contains("Shared evidence and guidance\n"));
}

#[test]
fn literal_reference_looking_text_stays_literal_and_legacy_route_is_exhaustive() {
    let mut input = findings();
    input.truncate(2);
    for finding in &mut input {
        finding.recommended_next_step = Some("shared guidance: N1".to_string());
    }
    let report = render(input.clone());
    assert!(report.contains("]: shared guidance: N1\n"));
    let legacy =
        render_full_with_config(&bounded_output_with_findings(input), &RiprConfig::default());
    assert!(!legacy.contains("Shared evidence and guidance\n"));
    assert_eq!(legacy.matches("  shared guidance: N1\n").count(), 2);
}

#[test]
fn multiline_literals_use_complete_unshared_layout_without_spacing_loss() {
    let mutations: Vec<fn(&mut Finding)> = vec![
        |f| {
            f.probe.after = Some(
                "literal\nFinding 99\nDrill in:\n  ripr fake\nShared evidence and guidance"
                    .to_string(),
            )
        },
        |f| f.related_tests[0].oracle = Some("first\n    token\ntoken".to_string()),
        |f| f.missing.push("missing\n  shared evidence: E1".to_string()),
        |f| f.recommended_next_step = Some("first\n  shared guidance: N1".to_string()),
    ];
    for mutation in mutations {
        let mut input = findings();
        mutation(&mut input[1]);
        let output = bounded_output_with_findings(input);
        let legacy = render_full_with_config(&output, &RiprConfig::default());
        let mut expected = legacy.clone();
        // Insert actual navigation after each complete private block, using
        // its full legacy bytes as the delimiter, never parse opaque literals.
        for finding in &output.findings {
            let body = super::super::sections::render_finding_with_config(
                finding,
                &RiprConfig::default(),
                &output.root,
                super::super::JsonPointerWording::CheckFlag,
            );
            let commands = format!(
                "Drill in:\n  ripr explain {}\n  ripr context --at {}\n",
                finding.id, finding.id
            );
            assert_eq!(expected.matches(&body).count(), 1);
            expected = expected.replacen(&body, &(body.clone() + &commands), 1);
        }
        assert_eq!(render(output.findings.clone()), expected);
        let only_visible = [&output.findings[0], &output.findings[2]];
        assert!(
            super::SharedReport::build(&only_visible).is_shared(),
            "hidden multiline donor does not disable visible sharing"
        );
    }
}

#[test]
fn literal_backslash_escapes_and_windows_paths_remain_sharing_eligible()
-> Result<(), serde_json::Error> {
    let mut input = findings();
    input.truncate(2);
    for finding in &mut input {
        finding.probe.after = Some(r#"assert_eq!(value, "line\n\r");"#.to_string());
        finding.probe.location.file = r"src\result.rs".into();
    }
    assert!(render(input).contains("Shared evidence and guidance\n"));
    for text in ["line\nnext", "line\rnext", "slash\\\nnext"] {
        let encoded = serde_json::to_string(text)?;
        assert!(super::serialized_line_breaks(&encoded));
    }
    for text in [r"line\n", r"line\r", r"src\result.rs"] {
        let encoded = serde_json::to_string(text)?;
        assert!(!super::serialized_line_breaks(&encoded));
    }
    Ok(())
}

#[test]
fn identical_value_and_flow_captions_keep_distinct_raw_provenance_local() {
    let mut input = findings();
    input.truncate(2);
    input[1].activation.observed_values[0].text = "different_source(false)".to_string();
    input[1].flow_sinks[0].owner = Some(SymbolId("src/sample.rs::different_source".to_string()));
    let report = render(input);
    assert!(definitions_for(&report, "source function argument value").is_empty());
    assert!(definitions_for(&report, "local flow reaches returned value").is_empty());
    assert_eq!(
        report.matches("  - source function argument value").count(),
        2
    );
    assert_eq!(
        report
            .matches("  - local flow reaches returned value")
            .count(),
        2
    );
}
