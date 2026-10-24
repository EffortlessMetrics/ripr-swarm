//! One sentence per examined test saying why it would not notice the change.
//!
//! The analyzer records the fact as a [`RelatedTestMiss`]; this module is the
//! single place that turns it into prose, so human, JSON, LSP and agent output
//! say the same thing about the same test.

use crate::domain::{ProbeFamily, RelatedTest, RelatedTestMiss, RelationReason};

/// Short reason the test misses the changed behavior, or `None` when the
/// analyzer did not establish one.
///
/// Row-specific detail comes only from the row's own
/// [`RelatedTest::miss_support`] and the finding's typed `family` (#5564);
/// this never reads the finding-wide `missing_discriminators`, so a fact the
/// producer did not attach to this row cannot be borrowed. Absent support
/// renders the generic wording.
pub(crate) fn related_test_miss_reason(test: &RelatedTest, family: &ProbeFamily) -> Option<String> {
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
            "assertion not credited: ripr could not establish that this assertion counts here"
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
        // The wording of `missing_input` follows the finding's typed probe
        // family, never the punctuation of the support value: a guard the
        // input never enters (side-effect or call-deletion), a predicate or
        // returned-comparison boundary the input never reaches, or an arm no
        // input selects. A family with no input reading keeps the generic
        // sentence even when support is present.
        RelatedTestMiss::MissingInput => {
            let value = test
                .miss_support
                .as_ref()
                .map(|support| one_line(&support.value));
            match (family, value) {
                (ProbeFamily::SideEffect | ProbeFamily::CallDeletion, Some(value)) => {
                    format!("no test input enters `{value}`")
                }
                (ProbeFamily::Predicate | ProbeFamily::ReturnValue, Some(value)) => {
                    format!("no test input reaches `{value}`")
                }
                (ProbeFamily::MatchArm, Some(value)) => {
                    format!("no test input selects arm `{value} =>`")
                }
                _ => "no test input reaches the changed boundary".to_string(),
            }
        }
        RelatedTestMiss::MissingExactAssertion => match test.miss_support.as_ref() {
            Some(support) => format!("no assertion pins `{}`", one_line(&support.value)),
            None => "no assertion pins the exact changed value".to_string(),
        },
    })
}

/// The oracle phrase of a related-test row, after "uses". A strength of
/// `unknown` means the assertion was not graded against this probe (the
/// probe family has no typed oracle relationships), not that the assertion
/// itself is of unknown quality (#8388); `none` means the test has no oracle
/// at all (#8747).
pub(crate) fn oracle_projection(
    strength: &crate::domain::OracleStrength,
    kind: &crate::domain::OracleKind,
    has_assertion: bool,
) -> String {
    use crate::domain::{OracleKind, OracleStrength};
    match (strength, kind) {
        (OracleStrength::None, _) => "no oracle".to_string(),
        // A row with no selected assertion has no oracle to describe.
        (OracleStrength::Unknown, OracleKind::Unknown) if !has_assertion => "no oracle".to_string(),
        (OracleStrength::Unknown, OracleKind::Unknown) => {
            "an unclassified oracle, not graded against this probe".to_string()
        }
        (OracleStrength::Unknown, kind) => format!(
            "{} oracle, not graded against this probe",
            kind.as_str().replace('_', " ")
        ),
        (strength, kind) => format!(
            "{} oracle",
            crate::output::oracle_phrase::oracle_phrase(
                strength.as_str(),
                &kind.as_str().replace('_', " ")
            )
        ),
    }
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

/// Human-output spelling of assertion or value text that must stay on one
/// evidence row: text that spans lines (a snapshot body, a closure test
/// body, a multi-line string literal) collapses its whitespace the way the
/// `checked` row does; single-line text is returned byte-for-byte (#9165).
pub(crate) fn single_line_evidence_text(value: &str) -> String {
    if value.contains(['\n', '\r']) {
        one_line(value)
    } else {
        value.to_string()
    }
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        MissingDiscriminatorFact, OracleKind, OracleStrength, RelatedTestMissSupport,
    };
    use crate::output::perl_preview_card::tests::sample_perl_finding as sample_finding;
    use std::path::PathBuf;

    #[test]
    fn oracle_projection_separates_ungraded_assertions_from_absent_ones() {
        let unknown = (&OracleStrength::Unknown, &OracleKind::Unknown);
        assert_eq!(
            oracle_projection(unknown.0, unknown.1, true),
            "an unclassified oracle, not graded against this probe"
        );
        assert_eq!(oracle_projection(unknown.0, unknown.1, false), "no oracle");
        assert_eq!(
            oracle_projection(&OracleStrength::Unknown, &OracleKind::ExactValue, true),
            "exact value oracle, not graded against this probe"
        );
        // A row with no oracle strength reads "no oracle" whatever its kind.
        assert_eq!(
            oracle_projection(&OracleStrength::None, &OracleKind::ExactValue, false),
            "no oracle"
        );
        // A kind that already leads with its strength word is not doubled.
        assert_eq!(
            oracle_projection(&OracleStrength::Smoke, &OracleKind::SmokeOnly, true),
            "smoke only oracle"
        );
        // A graded pair keeps the "<strength> <kind> oracle" spelling.
        assert_eq!(
            oracle_projection(&OracleStrength::Strong, &OracleKind::ExactValue, true),
            "strong exact value oracle"
        );
    }

    #[test]
    fn single_line_evidence_text_collapses_only_multi_line_text() {
        // #9165: a closure or snapshot oracle spans lines; the row keeps one.
        assert_eq!(
            single_line_evidence_text("scope(|scope| {\n    scope.spawn(f);\n})\n.unwrap();"),
            "scope(|scope| { scope.spawn(f); }) .unwrap();"
        );
        assert_eq!(single_line_evidence_text("a\r\nb"), "a b");
        // Negative controls: single-line text is untouched, including its
        // inner spacing and trailing `;`.
        assert_eq!(
            single_line_evidence_text("assert_eq!(a,  b);"),
            "assert_eq!(a,  b);"
        );
        assert_eq!(single_line_evidence_text(""), "");
    }

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
            miss_support: None,
        }
    }

    fn support(value: &str) -> RelatedTestMissSupport {
        RelatedTestMissSupport {
            value: value.to_string(),
            flow_sink: None,
        }
    }

    fn row_named(
        name: &str,
        miss: RelatedTestMiss,
        miss_support: Option<RelatedTestMissSupport>,
    ) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            miss_support,
            ..test_with(Some(miss), None)
        }
    }

    fn reason(test: &RelatedTest, family: &ProbeFamily) -> Option<String> {
        related_test_miss_reason(test, family)
    }

    #[test]
    fn a_test_without_a_recorded_miss_gets_no_reason() {
        let mut test = test_with(None, None);
        // Support without a miss is not rendered.
        test.miss_support = Some(support("len == 0"));
        assert_eq!(reason(&test, &ProbeFamily::Predicate), None);
    }

    #[test]
    fn each_miss_names_a_checkable_fact() {
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
        for (miss, relation, expected) in cases {
            let mut test = test_with(Some(miss), relation);
            test.miss_support = Some(support("len == 0"));
            assert_eq!(
                reason(&test, &ProbeFamily::Predicate).as_deref(),
                Some(expected),
                "{miss:?}"
            );
        }
    }

    /// Control 1: two rows, two predicate facts, each renders its own.
    #[test]
    fn two_rows_with_different_boundaries_each_render_their_own() {
        let a = row_named(
            "a",
            RelatedTestMiss::MissingInput,
            Some(support("amount == threshold")),
        );
        let b = row_named(
            "b",
            RelatedTestMiss::MissingInput,
            Some(support("qty == limit")),
        );
        assert_eq!(
            reason(&a, &ProbeFamily::Predicate).as_deref(),
            Some("no test input reaches `amount == threshold`")
        );
        assert_eq!(
            reason(&b, &ProbeFamily::Predicate).as_deref(),
            Some("no test input reaches `qty == limit`")
        );
    }

    /// Controls 2, 3, 6 and 10: a row without its own support never borrows
    /// the finding-wide fact that sits beside it.
    #[test]
    fn a_row_without_support_renders_generic_wording_not_a_finding_wide_fact() {
        let mut finding = sample_finding();
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "amount == threshold".to_string(),
            reason: "no related test passes the boundary".to_string(),
            flow_sink: None,
        }];
        let owner = row_named(
            "misses",
            RelatedTestMiss::MissingInput,
            Some(support("amount == threshold")),
        );
        let bystander = row_named("hits", RelatedTestMiss::MissingInput, None);
        let family = &finding.probe.family;
        assert_eq!(
            reason(&owner, family).as_deref(),
            Some("no test input reaches `amount == threshold`")
        );
        for family in [&ProbeFamily::Predicate, &ProbeFamily::ReturnValue] {
            assert_eq!(
                reason(&bystander, family).as_deref(),
                Some("no test input reaches the changed boundary")
            );
        }
        let exact = row_named("exact", RelatedTestMiss::MissingExactAssertion, None);
        assert_eq!(
            reason(&exact, &ProbeFamily::ErrorPath).as_deref(),
            Some("no assertion pins the exact changed value")
        );
    }

    /// Control 4: a wrong-owner fact in the finding-wide list is ignored; the
    /// row renders its own support.
    #[test]
    fn a_wrong_owner_finding_wide_fact_is_ignored() {
        let mut finding = sample_finding();
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "other == wrong".to_string(),
            reason: "another row's fact".to_string(),
            flow_sink: None,
        }];
        let row = row_named(
            "mine",
            RelatedTestMiss::MissingExactAssertion,
            Some(support("CalcError::TooLarge")),
        );
        assert_eq!(
            reason(&row, &finding.probe.family).as_deref(),
            Some("no assertion pins `CalcError::TooLarge`")
        );
    }

    /// Control 5: wording follows the typed family, not punctuation in the
    /// support value.
    #[test]
    fn wording_follows_the_typed_family_not_equality_punctuation() {
        let row = row_named(
            "r",
            RelatedTestMiss::MissingInput,
            Some(support("if x == 10")),
        );
        assert_eq!(
            reason(&row, &ProbeFamily::SideEffect).as_deref(),
            Some("no test input enters `if x == 10`")
        );
        assert_eq!(
            reason(&row, &ProbeFamily::CallDeletion).as_deref(),
            Some("no test input enters `if x == 10`")
        );
        // A family with no input reading never treats ` == ` as a boundary.
        for family in [
            ProbeFamily::ErrorPath,
            ProbeFamily::FieldConstruction,
            ProbeFamily::StaticUnknown,
        ] {
            assert_eq!(
                reason(&row, &family).as_deref(),
                Some("no test input reaches the changed boundary"),
                "{family:?}"
            );
        }
        // An exact-assertion value containing ` == ` is still pinned, not
        // read as an input boundary.
        let exact = row_named(
            "e",
            RelatedTestMiss::MissingExactAssertion,
            Some(support("out.total == 5")),
        );
        assert_eq!(
            reason(&exact, &ProbeFamily::FieldConstruction).as_deref(),
            Some("no assertion pins `out.total == 5`")
        );
    }

    #[test]
    fn an_unselected_arm_miss_names_the_arm_as_a_missing_input() {
        let row = row_named(
            "r",
            RelatedTestMiss::MissingInput,
            Some(support("Kind::Beta")),
        );
        assert_eq!(
            reason(&row, &ProbeFamily::MatchArm).as_deref(),
            Some("no test input selects arm `Kind::Beta =>`")
        );
    }

    /// Control 7: the reason follows the row, not its position.
    #[test]
    fn row_order_does_not_move_support_between_tests() {
        let mut rows = vec![
            row_named("a", RelatedTestMiss::MissingInput, Some(support("a == 1"))),
            row_named("b", RelatedTestMiss::MissingInput, Some(support("b == 2"))),
            row_named("c", RelatedTestMiss::MissingInput, None),
        ];
        let render = |rows: &[RelatedTest]| {
            let mut out: Vec<(String, Option<String>)> = rows
                .iter()
                .map(|row| (row.name.clone(), reason(row, &ProbeFamily::Predicate)))
                .collect();
            out.sort();
            out
        };
        let forward = render(&rows);
        rows.reverse();
        assert_eq!(render(&rows), forward);
        rows.swap(0, 1);
        assert_eq!(render(&rows), forward);
    }

    /// Control 8: support ownership survives a serde round trip, and legacy
    /// artifacts without the field deserialize to `None`.
    #[test]
    fn support_survives_serialization_and_legacy_artifacts_stay_absent() -> Result<(), String> {
        let row = row_named(
            "r",
            RelatedTestMiss::MissingInput,
            Some(RelatedTestMissSupport {
                value: "amount == threshold".to_string(),
                flow_sink: None,
            }),
        );
        let json = serde_json::to_value(&row).map_err(|error| error.to_string())?;
        assert_eq!(json["miss"], "missing_input");
        assert_eq!(json["miss_support"]["value"], "amount == threshold");
        assert!(json["miss_support"].get("flow_sink").is_none());
        let back: RelatedTest =
            serde_json::from_value(json.clone()).map_err(|error| error.to_string())?;
        assert_eq!(back, row);
        let mut legacy = json;
        legacy
            .as_object_mut()
            .ok_or("row is an object")?
            .remove("miss_support");
        let legacy: RelatedTest =
            serde_json::from_value(legacy).map_err(|error| error.to_string())?;
        assert_eq!(legacy.miss_support, None);
        assert_eq!(
            reason(&legacy, &ProbeFamily::Predicate).as_deref(),
            Some("no test input reaches the changed boundary")
        );
        // A row with no support omits the field entirely.
        let bare = serde_json::to_value(row_named("b", RelatedTestMiss::MissingInput, None))
            .map_err(|error| error.to_string())?;
        assert!(bare.get("miss_support").is_none());
        Ok(())
    }

    #[test]
    fn probe_family_wire_tokens_round_trip() {
        for family in [
            ProbeFamily::Predicate,
            ProbeFamily::ReturnValue,
            ProbeFamily::ErrorPath,
            ProbeFamily::CallDeletion,
            ProbeFamily::FieldConstruction,
            ProbeFamily::SideEffect,
            ProbeFamily::MatchArm,
            ProbeFamily::StaticUnknown,
        ] {
            assert_eq!(ProbeFamily::from_wire(family.as_str()), Some(family));
        }
        assert_eq!(ProbeFamily::from_wire("nope"), None);
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
                ignored_working_tree_source_paths: Vec::new(),
                suppression: None,
                analysis_outcome: None,
                partial_scope: None,
                unlinked_python_tests: None,
                analyzed_revisions: None,
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
            let why = related_test_miss_reason(direct, &finding.probe.family)
                .ok_or("the direct row should have a reason")?;
            assert_eq!(
                related_test_miss_reason(advisory, &finding.probe.family),
                None
            );
            let label = related_test_miss_label(direct);
            let output = report_with(vec![finding.clone()]);

            let json: serde_json::Value =
                serde_json::from_str(&crate::output::json::render(&output))
                    .map_err(|error| format!("parse check JSON: {error}"))?;
            let context: serde_json::Value =
                serde_json::from_str(&crate::output::json::render_context_packet(
                    &finding,
                    8,
                    &crate::config::RiprConfig::default(),
                    std::path::Path::new("."),
                ))
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
                std::path::Path::new("."),
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
                    serde_json::from_str(&crate::output::json::render_context_packet(
                        finding,
                        8,
                        &crate::config::RiprConfig::default(),
                        std::path::Path::new("."),
                    ))
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
                // Not vacuous: the currentness-filtered SARIF projection emits a
                // result exactly for the candidate-current copies.
                let sarif: serde_json::Value =
                    serde_json::from_str(&crate::output::sarif::render_findings_sarif(
                        &report_with(vec![finding.clone()]),
                        &crate::config::RiprConfig::default(),
                        &[],
                    ))
                    .map_err(|error| format!("parse SARIF: {error}"))?;
                let results = sarif["runs"][0]["results"]
                    .as_array()
                    .map_or(0, |results| results.len());
                assert_eq!(results > 0, finding.is_candidate_actionable(), "{sarif}");

                let mut full = crate::output::human::render_finding(&finding);
                for row in &finding.related_tests {
                    if let Some(why) = related_test_miss_reason(row, &finding.probe.family) {
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
