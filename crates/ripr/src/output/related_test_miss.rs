//! One sentence per examined test saying why it would not notice the change.
//!
//! The analyzer records the fact as a [`RelatedTestMiss`]; this module is the
//! single place that turns it into prose, so human, JSON, LSP and agent output
//! say the same thing about the same test.

use crate::domain::{
    MissingDiscriminatorFact, ProbeFamily, RelatedTest, RelatedTestMiss, RelationReason,
    exact_assertion_fact, input_boundary_fact,
};

/// Short reason the test misses the changed behavior, or `None` when the
/// analyzer did not establish one.
pub(crate) fn related_test_miss_reason(
    test: &RelatedTest,
    missing_discriminators: &[MissingDiscriminatorFact],
) -> Option<String> {
    Some(match test.miss? {
        RelatedTestMiss::NoCallPath => match test.relation_reason.and_then(link_label) {
            Some(link) => format!("no call to the changed code found; linked by {link} only"),
            None => "no call to the changed code found".to_string(),
        },
        RelatedTestMiss::NoAssertion => "has no assertion".to_string(),
        RelatedTestMiss::AssertionNotObserving => {
            "asserts, but not on the changed value".to_string()
        }
        RelatedTestMiss::AssertionNotCredited => {
            "assertion not credited: ripr could not establish that it runs as the standard macro"
                .to_string()
        }
        RelatedTestMiss::WeakAssertion => {
            "assertion too weak to tell the old behavior from the new".to_string()
        }
        // #5508: an unknown observation edge, not an established miss. Only
        // `assertion_not_observing` claims the assertion observes something
        // else.
        RelatedTestMiss::ObservationUnconfirmed => {
            "ripr could not confirm that this assertion observes the changed behavior".to_string()
        }
        // The analyzer assigns `missing_input` only for a predicate probe
        // with a boundary fact, and `missing_exact_assertion` only when no
        // boundary fact exists, so reading the facts as a predicate's picks
        // the fact each miss was assigned from.
        RelatedTestMiss::MissingInput => {
            match input_boundary_fact(missing_discriminators, &ProbeFamily::Predicate) {
                Some(fact) => format!("no test input reaches `{}`", one_line(&fact.value)),
                None => "no test input reaches the changed boundary".to_string(),
            }
        }
        RelatedTestMiss::MissingExactAssertion => {
            match exact_assertion_fact(missing_discriminators, &ProbeFamily::Predicate) {
                Some(fact) => format!("no assertion pins `{}`", one_line(&fact.value)),
                None => "no assertion pins the exact changed value".to_string(),
            }
        }
    })
}

/// The word placed before the reason. An unconfirmed observation is an
/// unknown, so it is not introduced as a miss (#5508).
pub(crate) fn related_test_miss_label(test: &RelatedTest) -> &'static str {
    match test.miss {
        Some(RelatedTestMiss::ObservationUnconfirmed) => "unconfirmed",
        _ => "misses",
    }
}

/// The assertion text a miss was judged by, on one line and without the
/// statement's trailing `;`, for quoting after the reason.
pub(crate) fn checked_assertion_text(oracle: &str) -> String {
    one_line(oracle)
        .trim_end_matches(';')
        .trim_end()
        .to_string()
}

fn link_label(reason: RelationReason) -> Option<&'static str> {
    match reason {
        RelationReason::SameTestFile => Some("file location"),
        RelationReason::SameModule => Some("module location"),
        RelationReason::OwnerNamedTest | RelationReason::WeakTokenSubstring => Some("name"),
        _ => None,
    }
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{MissingDiscriminatorFact, OracleKind, OracleStrength};
    use crate::output::perl_preview_card::tests::sample_perl_finding as sample_finding;
    use std::path::PathBuf;

    fn test_with(miss: Option<RelatedTestMiss>, reason: Option<RelationReason>) -> RelatedTest {
        RelatedTest {
            name: "parses_ge".to_string(),
            file: PathBuf::from("tests/parse.rs"),
            line: 3,
            oracle: Some("assert_eq!(op(\">=1\"), Op::GreaterEq)".to_string()),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: reason,
            relation_confidence: None,
            miss,
        }
    }

    #[test]
    fn a_test_without_a_recorded_miss_gets_no_reason() {
        let finding = sample_finding();
        assert_eq!(
            related_test_miss_reason(
                &test_with(None, None),
                &finding.activation.missing_discriminators
            ),
            None
        );
    }

    #[test]
    fn each_miss_names_a_checkable_fact() {
        let mut finding = sample_finding();
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "len == 0".to_string(),
            reason: "no related test passes an empty input".to_string(),
            flow_sink: None,
        }];
        let cases = [
            (
                RelatedTestMiss::NoCallPath,
                Some(RelationReason::SameTestFile),
                "no call to the changed code found; linked by file location only",
            ),
            (
                RelatedTestMiss::NoCallPath,
                Some(RelationReason::DirectOwnerCall),
                "no call to the changed code found",
            ),
            (RelatedTestMiss::NoAssertion, None, "has no assertion"),
            (
                RelatedTestMiss::AssertionNotObserving,
                None,
                "asserts, but not on the changed value",
            ),
            (
                RelatedTestMiss::WeakAssertion,
                None,
                "assertion too weak to tell the old behavior from the new",
            ),
            (
                RelatedTestMiss::MissingInput,
                None,
                "no test input reaches `len == 0`",
            ),
            (
                RelatedTestMiss::ObservationUnconfirmed,
                None,
                "ripr could not confirm that this assertion observes the changed behavior",
            ),
        ];
        for (miss, reason, expected) in cases {
            assert_eq!(
                related_test_miss_reason(
                    &test_with(Some(miss), reason),
                    &finding.activation.missing_discriminators
                )
                .as_deref(),
                Some(expected),
                "{miss:?}"
            );
        }
    }

    #[test]
    fn only_an_unconfirmed_observation_drops_the_misses_label() {
        let established = [
            RelatedTestMiss::NoCallPath,
            RelatedTestMiss::NoAssertion,
            RelatedTestMiss::AssertionNotObserving,
            RelatedTestMiss::AssertionNotCredited,
            RelatedTestMiss::WeakAssertion,
            RelatedTestMiss::MissingInput,
            RelatedTestMiss::MissingExactAssertion,
        ];
        for miss in established {
            assert_eq!(
                related_test_miss_label(&test_with(Some(miss), None)),
                "misses",
                "{miss:?}"
            );
        }
        assert_eq!(
            related_test_miss_label(&test_with(
                Some(RelatedTestMiss::ObservationUnconfirmed),
                None
            )),
            "unconfirmed"
        );
    }

    #[test]
    fn an_exact_assertion_miss_names_the_variant_not_an_input() {
        let mut finding = sample_finding();
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "CalcError::TooLarge".to_string(),
            reason: "No exact error variant assertion for CalcError::TooLarge".to_string(),
            flow_sink: None,
        }];
        let test = test_with(Some(RelatedTestMiss::MissingExactAssertion), None);
        assert_eq!(
            related_test_miss_reason(&test, &finding.activation.missing_discriminators).as_deref(),
            Some("no assertion pins `CalcError::TooLarge`")
        );
    }

    /// #5510: packet-backed Perl findings through every report surface this
    /// module's callers own. The findings come from frozen packets through
    /// the production Perl mapper (`analysis::perl_*` test exports).
    #[cfg(feature = "lang-perl")]
    mod perl_packet {
        use super::*;
        use crate::domain::Finding;

        fn report_with(findings: Vec<Finding>) -> crate::app::CheckOutput {
            crate::app::CheckOutput {
                harness_projections: Vec::new(),
                schema_version: "0.2".to_string(),
                tool: "ripr".to_string(),
                mode: crate::app::Mode::Draft,
                root: PathBuf::from("."),
                base: None,
                summary: crate::domain::Summary::default(),
                findings,
                preview_language_advisories: Vec::new(),
                language_runs: Vec::new(),
                no_scope_provided: false,
                unanalyzed_working_tree: false,
                untracked_working_tree_source_paths: Vec::new(),
                suppression: None,
                analysis_outcome: None,
                partial_scope: None,
            }
        }

        /// The JSON row named `name` among `rows`.
        fn json_row<'a>(
            rows: &'a serde_json::Value,
            name: &str,
        ) -> Result<&'a serde_json::Map<String, serde_json::Value>, String> {
            rows.as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_object)
                .find(|row| row.get("name").and_then(serde_json::Value::as_str) == Some(name))
                .ok_or_else(|| format!("no JSON row for `{name}` in {rows}"))
        }

        /// The one line of `text` that names `name` and starts with `prefix`.
        fn line_naming<'a>(text: &'a str, prefix: &str, name: &str) -> Result<&'a str, String> {
            let mut lines = text
                .lines()
                .filter(|line| line.trim_start().starts_with(prefix) && line.contains(name));
            match (lines.next(), lines.next()) {
                (Some(line), None) => Ok(line),
                _ => Err(format!(
                    "expected one `{prefix}` line for `{name}` in:\n{text}"
                )),
            }
        }

        /// Surface parity: the finding rows, the check JSON, the context
        /// packet, `human-full` and `ripr explain` carry the same row, token
        /// and shared sentence; the advisory row carries none. The human
        /// digest, LSP and MCP checks sit beside their private projections.
        #[test]
        fn perl_packet_backed_rows_agree_across_report_surfaces() -> Result<(), String> {
            let finding = crate::analysis::perl_direct_and_advisory_finding()?;
            let [direct, advisory] = finding.related_tests.as_slice() else {
                return Err(format!("expected two rows: {:?}", finding.related_tests));
            };
            assert_eq!(direct.miss, Some(RelatedTestMiss::ObservationUnconfirmed));
            assert_eq!(advisory.miss, None);
            let why = related_test_miss_reason(direct, &finding.activation.missing_discriminators)
                .ok_or("the direct row should have a reason")?;
            assert_eq!(
                related_test_miss_reason(advisory, &finding.activation.missing_discriminators),
                None
            );
            let label = related_test_miss_label(direct);
            let output = report_with(vec![finding.clone()]);

            let json: serde_json::Value =
                serde_json::from_str(&crate::output::json::render(&output))
                    .map_err(|error| format!("parse check JSON: {error}"))?;
            let context: serde_json::Value =
                serde_json::from_str(&crate::output::json::render_context_packet(&finding, 8))
                    .map_err(|error| format!("parse context packet: {error}"))?;
            for rows in [
                &json["findings"][0]["related_tests"],
                &context["related_tests"],
            ] {
                let row = json_row(rows, &direct.name)?;
                assert_eq!(
                    row.get("miss").and_then(serde_json::Value::as_str),
                    Some("observation_unconfirmed")
                );
                assert_eq!(
                    row.get("why").and_then(serde_json::Value::as_str),
                    Some(why.as_str())
                );
                let row = json_row(rows, &advisory.name)?;
                assert!(!row.contains_key("miss") && !row.contains_key("why"));
            }

            let full = crate::render_check(&output, &crate::OutputFormat::HumanFull)?;
            let explain = crate::output::human::render_finding_with_context_command(
                &finding,
                &crate::config::RiprConfig::default(),
                "ripr explain probe",
            );
            let (evidence, verdict) = explain
                .split_once("Why this verdict")
                .ok_or("explain should add the verdict section")?;
            for (text, prefix, separator) in [
                (full.as_str(), "- related test", "; "),
                (evidence, "- related test", "; "),
                (verdict, "- t/app.t", ": "),
            ] {
                let line = line_naming(text, prefix, &direct.name)?;
                assert!(
                    line.ends_with(&format!("{separator}{label}: {why}")),
                    "{line}"
                );
                let line = line_naming(text, prefix, &advisory.name)?;
                assert!(
                    !line.contains(&why) && !line.contains(&format!("{label}:")),
                    "{line}"
                );
            }
            Ok(())
        }

        /// Verdict invariance, consumer side: across the packet-backed
        /// matrix, clearing every row's miss leaves each projected decision
        /// unchanged — the check JSON and context packet once `miss`/`why`
        /// are dropped, the diagnostic witness (fix site), preview
        /// actionability and card, the reconciled next step, the oracle rows,
        /// and `human-full` once the shared reason suffix is removed. This
        /// proves these consumers do not read `miss`; strict actionability
        /// reads the packet, not the finding, and is not covered here. The
        /// producer side is the Perl test
        /// `perl_related_test_miss_is_the_only_field_the_rule_writes`.
        #[test]
        fn perl_packet_backed_miss_changes_no_projected_decision() -> Result<(), String> {
            fn without_miss_keys(value: &mut serde_json::Value) {
                match value {
                    serde_json::Value::Object(map) => {
                        map.remove("miss");
                        map.remove("why");
                        map.values_mut().for_each(without_miss_keys);
                    }
                    serde_json::Value::Array(items) => items.iter_mut().for_each(without_miss_keys),
                    _ => {}
                }
            }
            fn decisions(finding: &Finding) -> Result<String, String> {
                let mut report: serde_json::Value =
                    serde_json::from_str(&crate::output::json::render(&report_with(vec![
                        finding.clone(),
                    ])))
                    .map_err(|error| format!("parse check JSON: {error}"))?;
                without_miss_keys(&mut report);
                let mut context: serde_json::Value =
                    serde_json::from_str(&crate::output::json::render_context_packet(finding, 8))
                        .map_err(|error| format!("parse context packet: {error}"))?;
                without_miss_keys(&mut context);
                // The currentness-filtered projections (#6586): SARIF results,
                // GitHub annotations and the diff badge only see
                // candidate-current findings.
                let output = report_with(vec![finding.clone()]);
                let config = crate::config::RiprConfig::default();
                let mut sarif: serde_json::Value = serde_json::from_str(
                    &crate::output::sarif::render_findings_sarif(&output, &config, &[]),
                )
                .map_err(|error| format!("parse SARIF: {error}"))?;
                without_miss_keys(&mut sarif);
                Ok(format!(
                    "{report}\n{context}\n{sarif}\n{}\n{:?}\n{:?}\n{:?}\n{:?}\n{}\n{:?}",
                    crate::output::github::render_with_config(&output, &config),
                    crate::output::badge::ripr_badge_summary(
                        &output,
                        crate::output::badge::BadgePolicy::default()
                    ),
                    crate::domain::DiagnosticWitness::from_finding(finding),
                    crate::output::preview_actionability::preview_actionability_for(finding),
                    crate::output::perl_preview_card::perl_preview_card_json(finding),
                    crate::output::next_step::reconcile_next_step(finding),
                    finding
                        .oracle_related_tests()
                        .map(|test| &test.name)
                        .collect::<Vec<_>>(),
                ))
            }
            let mut explained_rows = 0;
            // Every matrix finding is checked as the fixture-only unknown and
            // as the candidate-current finding an observed change produces.
            let findings = crate::analysis::perl_miss_matrix_findings()?;
            let current = findings.iter().cloned().map(|mut finding| {
                finding.source_currentness = crate::domain::SourceCurrentness::CandidateCurrent;
                finding
            });
            let findings = findings
                .clone()
                .into_iter()
                .chain(current)
                .collect::<Vec<_>>();
            for finding in findings {
                let mut cleared = finding.clone();
                for row in &mut cleared.related_tests {
                    row.miss = None;
                }
                assert_eq!(decisions(&finding)?, decisions(&cleared)?);

                let mut full = crate::output::human::render_finding(&finding);
                for row in &finding.related_tests {
                    if let Some(why) =
                        related_test_miss_reason(row, &finding.activation.missing_discriminators)
                    {
                        explained_rows += 1;
                        full =
                            full.replace(&format!("; {}: {why}", related_test_miss_label(row)), "");
                    }
                }
                assert_eq!(full, crate::output::human::render_finding(&cleared));
            }
            // Not vacuous: five of the six findings carry one explained row,
            // checked under both currentness values.
            assert_eq!(explained_rows, 10);
            Ok(())
        }
    }
}
