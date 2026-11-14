use super::{
    ExposureClass, Finding, LanguageId, MissingDiscriminatorFact, RelatedTest, RelatedTestMiss,
};

/// The bounded, producer-backed witness projected by passive editor diagnostics.
///
/// This type deliberately contains no renderer-derived assertion or target.  A
/// missing field is retained as a named limitation so a client can distinguish
/// unavailable producer evidence from an empty or guessed value.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticWitness {
    pub kind: String,
    pub probe_family: String,
    pub changed_expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_sink: Option<String>,
    pub missing_discriminators: Vec<MissingDiscriminatorFact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_site: Option<DiagnosticFixSite>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_assertion: Option<String>,
    pub explain_command: String,
    pub confidence: DiagnosticConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<DiagnosticWitnessLimitation>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticFixSite {
    pub file: String,
    pub line: usize,
    pub test_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_oracle: Option<String>,
    pub oracle_kind: String,
    pub oracle_strength: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oracle_location: Option<DiagnosticSourceLocation>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticSourceLocation {
    pub file: String,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticConfidence {
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_contract_confidence"
    )]
    pub value: Option<f32>,
    pub basis: String,
}

/// A confidence at the two-decimal precision of the output contract
/// (`docs/OUTPUT_SCHEMA.md`), as the `f64` a JSON value carries.
///
/// `check` JSON and human output print `{:.2}`; widening the stored `f32`
/// straight to `f64` would serialize `0.4` as `0.4000000059604645`. The LSP
/// diagnostic payload, the context packet witness and SARIF
/// `properties.confidence` all go through this one helper (#10717).
pub(crate) fn contract_confidence(value: f32) -> f64 {
    format!("{value:.2}")
        .parse()
        .unwrap_or_else(|_| f64::from(value))
}

fn serialize_contract_confidence<S: serde::Serializer>(
    value: &Option<f32>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_f64(contract_confidence(*value)),
        None => serializer.serialize_none(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DiagnosticWitnessLimitation {
    pub kind: String,
    pub detail: String,
}

impl DiagnosticWitness {
    /// Build a witness only for findings that represent an unresolved or
    /// weakly observed behavior.  A fully exposed finding is not relabeled as
    /// a gap merely because it has a probe.
    pub fn from_finding(finding: &Finding) -> Option<Self> {
        if matches!(finding.class, ExposureClass::Exposed)
            && finding.activation.missing_discriminators.is_empty()
        {
            return None;
        }

        let best_test = Self::selected_related_test(finding);
        // `observed_values` is finding-wide and does not identify the
        // selected related test.  Do not guess an assertion location by
        // matching text; the test's own line remains the only producer-owned
        // fix-site location available until the analyzer supplies oracle
        // identity.
        let oracle_location = None;
        let fix_site = best_test.map(|test| DiagnosticFixSite {
            file: display_path(&test.file),
            line: test.line,
            test_name: test.name.clone(),
            current_oracle: test
                .oracle
                .clone()
                .filter(|oracle| !oracle.trim().is_empty()),
            oracle_kind: test.oracle_kind.as_str().to_string(),
            oracle_strength: test.oracle_strength.as_str().to_string(),
            oracle_location: oracle_location.clone(),
        });

        let mut limitations = Vec::new();
        if finding.activation.missing_discriminators.is_empty() {
            limitations.push(limitation(
                "missing_discriminator_unavailable",
                "The producer did not resolve an exact missing discriminator.",
            ));
        }
        match best_test {
            None => limitations.push(limitation(
                "fix_site_unavailable",
                "The producer did not identify a related test target.",
            )),
            Some(test) if test.oracle.as_deref().is_none_or(str::is_empty) => {
                limitations.push(limitation(
                    "oracle_text_unavailable",
                    "The producer identified a test target but did not provide its oracle text.",
                ));
            }
            Some(_) if oracle_location.is_none() => limitations.push(limitation(
                "oracle_source_location_unavailable",
                "The producer did not provide an exact assertion source location.",
            )),
            Some(_) => {}
        }
        // No Finding field currently carries a canonical, symbol-resolved
        // assertion template.  Keep the limitation explicit until a producer
        // supplies one; do not synthesize it from the missing value or prose.
        limitations.push(limitation(
            "suggested_assertion_unavailable",
            "No producer-owned symbol-resolved assertion template is available.",
        ));

        Some(Self {
            kind: "static_discriminator_gap".to_string(),
            probe_family: finding.probe.family.as_str().to_string(),
            changed_expression: finding.probe.expression.clone(),
            before: finding.probe.before.clone(),
            after: finding.probe.after.clone(),
            expected_sink: finding
                .flow_sinks
                .first()
                .map(|sink| sink.kind.as_str().to_string())
                .or_else(|| finding.probe.expected_sinks.first().cloned()),
            missing_discriminators: finding.activation.missing_discriminators.clone(),
            fix_site,
            suggested_assertion: None,
            explain_command: format!(
                "ripr explain --root . {}",
                crate::agent::loop_commands::shell_arg(&finding.id)
            ),
            confidence: DiagnosticConfidence {
                value: finding.confidence.is_finite().then_some(finding.confidence),
                basis: "static_only".to_string(),
            },
            limitations,
        })
    }

    /// Keep typed oracle metadata bound to the same test as the public witness.
    /// Renderers must not reconstruct this selection from its string DTO fields.
    pub(crate) fn selected_related_test(finding: &Finding) -> Option<&RelatedTest> {
        // The analyzer's suggested repair test, when it is one of the
        // related tests, is the fix site: `check`, `explain` and the repair
        // card already name it, so every surface points at the same test.
        // Tests listed only as examined misses (#5344) supplied no oracle and
        // are never a fix site: a fix site claims "this test runs the change
        // and has this oracle".
        if finding.language == Some(LanguageId::Python)
            && finding.evidence.iter().any(|entry| {
                [
                    "suggested_test_file:",
                    "suggested_test_name:",
                    "suggested_test_line:",
                ]
                .iter()
                .any(|prefix| entry.starts_with(prefix))
            })
        {
            // A declared repair target cannot borrow another test's oracle
            // when its complete identity is missing or ambiguous.
            python_suggested_related_test(finding)
        } else if finding.language == Some(LanguageId::Perl) {
            perl_suggested_related_test(finding)
        } else {
            suggested_related_test(finding)
                .or_else(|| best_related_test(finding.oracle_related_tests()))
        }
    }
}

/// Resolve the Python producer's selected row once for cards and diagnostic
/// witnesses. Older declarations without a line still require one unique row.
pub(crate) fn python_suggested_related_test(finding: &Finding) -> Option<&RelatedTest> {
    if finding.language != Some(LanguageId::Python) {
        return None;
    }
    let evidence = |prefix: &str| {
        let mut values = finding
            .evidence
            .iter()
            .filter_map(|entry| entry.strip_prefix(prefix));
        let value = values.next()?.trim();
        (values.next().is_none() && !value.is_empty()).then_some(value)
    };
    let file = evidence("suggested_test_file:")?.replace('\\', "/");
    let name = evidence("suggested_test_name:")?;
    let line = if finding
        .evidence
        .iter()
        .any(|entry| entry.starts_with("suggested_test_line:"))
    {
        let line = evidence("suggested_test_line:")?.parse::<usize>().ok()?;
        if line == 0 {
            return None;
        }
        Some(line)
    } else {
        None
    };
    let mut matches = finding.oracle_related_tests().filter(|test| {
        test.name == name
            && display_path(&test.file).trim_start_matches("./") == file.trim_start_matches("./")
            && line.is_none_or(|line| test.line == line)
    });
    let selected = matches.next()?;
    matches.next().is_none().then_some(selected)
}

/// Resolve the Perl producer's complete declared test identity once for all
/// consumers. Missing, unknown or ambiguous declarations cannot borrow a
/// differently ranked test's location or oracle.
pub(crate) fn perl_suggested_related_test(finding: &Finding) -> Option<&RelatedTest> {
    if finding.language != Some(LanguageId::Perl) {
        return None;
    }
    let mut declarations = finding.evidence.iter().filter_map(|line| {
        line.strip_prefix("perl_suggested_test_location: ")
            .filter(|value| !value.is_empty())
    });
    let declaration = declarations.next()?;
    if declarations.next().is_some() {
        return None;
    }
    let mut matches = finding.oracle_related_tests().filter(|test| {
        // Normalize only the file. Test descriptions may themselves contain
        // backslashes, whitespace or `::`; their identity stays exact.
        let name_suffix = format!("::{}", test.name);
        declaration
            .strip_suffix(name_suffix.as_str())
            .is_some_and(|file| file.replace('\\', "/") == display_path(&test.file))
    });
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn suggested_related_test(finding: &Finding) -> Option<&RelatedTest> {
    let evidence = |prefix: &str| {
        finding
            .evidence
            .iter()
            .find_map(|line| line.strip_prefix(prefix))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let name = evidence("suggested_test_name: ")?;
    // The file, when the producer names one, disambiguates same-named tests.
    let file = evidence("suggested_test_file: ");
    // A proximity-only row is never the fix site, even when named (#5354).
    let mut matches = finding.oracle_related_tests().filter(|test| {
        is_call_bearing(test)
            && test.name == name
            && file.is_none_or(|file| test.file.to_string_lossy().replace('\\', "/") == file)
    });
    let first = matches.next()?;
    // Two related tests with the same identity are ambiguous: fall back.
    matches.next().is_none().then_some(first)
}

/// Whether a row may be a fix site: any relation except proximity alone.
/// This is weaker than `RelationReason::establishes_reach` (a name or import
/// affinity still qualifies); it only removes rows that share nothing with
/// the owner but a location or a token. Proximity-only links
/// (`same_test_file`, `same_module`, `weak_token_substring`) never run the
/// change, so naming one as the fix site would send the reader to an
/// unrelated test (#5354). Rows with no recorded relation (legacy producers)
/// stay eligible. The row's `miss` annotation is explanatory and decides
/// nothing here (`output/related_test_role_tests.rs`).
fn is_call_bearing(test: &RelatedTest) -> bool {
    test.relation_reason
        .is_none_or(|reason| !reason.is_proximity_only())
}

/// Whether a row's relation shows the test runs the owner
/// (`RelationReason::establishes_reach`: a direct call, a helper chain, a
/// followed re-export). Rows with no recorded relation count too. A name,
/// import or fixture affinity is still eligible as a fix site, but ranks
/// after every reaching row: otherwise a name-matched test with a stronger
/// oracle on other code would win over the test that calls the change, the
/// #5335 defect. The owner is treated as a function, so an assertion-target
/// affinity is a fallback here; that only orders candidates.
fn reaches_owner(test: &RelatedTest) -> bool {
    test.relation_reason
        .is_none_or(|reason| reason.establishes_reach(true))
}

/// Rows that reach the owner first, then rows whose assertion can observe the
/// change (#14454), then oracle strength, then relation confidence, over
/// call-bearing rows only; when none exists there is no fix
/// site rather than a nearby test.
fn best_related_test<'a>(
    related_tests: impl Iterator<Item = &'a RelatedTest>,
) -> Option<&'a RelatedTest> {
    // `min_by` over the reversed ranking keeps the earliest of equally
    // ranked rows: the producer lists related tests by relation priority, so
    // a tie resolves to the row `check` shows first, and the lens and the
    // hover open the same test.
    //
    // #14454: among rows of equal reach, a row whose assertion cannot observe
    // the change is never the fix site while another row can, however strong
    // its oracle. A row with no call path to the changed code reaches it
    // least of all, so it ranks below a direct caller whose assertion pins
    // only the success arm.
    let observes = |test: &RelatedTest| match test.miss {
        Some(RelatedTestMiss::NoCallPath) => 0u8,
        Some(RelatedTestMiss::AssertionNotObserving) => 1,
        _ => 2,
    };
    related_tests
        .filter(|test| is_call_bearing(test))
        .min_by(|left, right| {
            reaches_owner(right)
                .cmp(&reaches_owner(left))
                .then_with(|| observes(right).cmp(&observes(left)))
                .then_with(|| {
                    right
                        .oracle_strength
                        .rank()
                        .cmp(&left.oracle_strength.rank())
                })
                .then_with(|| {
                    left.relation_confidence
                        .map_or(3, |confidence| confidence.rank())
                        .cmp(
                            &right
                                .relation_confidence
                                .map_or(3, |confidence| confidence.rank()),
                        )
                })
        })
}

fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn limitation(kind: &str, detail: &str) -> DiagnosticWitnessLimitation {
    DiagnosticWitnessLimitation {
        kind: kind.to_string(),
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, DiscriminatorShape, FindingCanonicalGap,
        FlowSinkFact, FlowSinkKind, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId,
        RevealEvidence, RiprEvidence, SourceLocation, StageEvidence, StageState, SymbolId,
        ValueContext, ValueFact,
    };
    use std::path::PathBuf;

    #[test]
    fn witness_preserves_exact_facts_and_named_missing_assertion_limitations() -> Result<(), String>
    {
        let mut finding = sample_finding();
        finding.canonical_gap = Some(FindingCanonicalGap {
            id: "gap:rust:pricing:error_path:error_variant:amount".to_string(),
            language: "rust".to_string(),
            file: "src/pricing.rs".to_string(),
            owner: "pricing::discount".to_string(),
            behavior_kind: "error_path".to_string(),
            probe_kind: "error_path".to_string(),
            normalized_discriminator: "PricingError::Boundary".to_string(),
        });
        finding.probe.family = ProbeFamily::ErrorPath;
        finding.probe.before = Some("Err(PricingError::Old)".to_string());
        finding.probe.after = Some("Err(PricingError::Boundary)".to_string());
        finding.probe.expected_sinks = vec!["error_variant".to_string()];
        finding.flow_sinks = vec![FlowSinkFact {
            kind: FlowSinkKind::ErrorVariant,
            text: "PricingError::Boundary".to_string(),
            line: 88,
            owner: Some(SymbolId("pricing::discount".to_string())),
        }];
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "PricingError::Boundary".to_string(),
            reason: "the broad error oracle does not distinguish the variant".to_string(),
            flow_sink: None,
            shape: DiscriminatorShape::Other,
            drafted_boundary: None,
        }];
        finding.activation.observed_values = vec![ValueFact {
            line: 12,
            text: "assert!(result.is_err())".to_string(),
            value: "result.is_err()".to_string(),
            context: ValueContext::AssertionArgument,
        }];
        finding.related_tests = vec![
            RelatedTest {
                name: "rejects_boundary".to_string(),
                file: PathBuf::from("tests/pricing.rs"),
                line: 10,
                oracle: Some("assert!(result.is_err())".to_string()),
                oracle_kind: OracleKind::BroadError,
                oracle_strength: OracleStrength::Weak,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
                miss_support: None,
                role: crate::domain::RelatedTestRole::Contributing,
            },
            RelatedTest {
                name: "rejects_boundary_in_other_fixture".to_string(),
                file: PathBuf::from("tests/other_pricing.rs"),
                line: 22,
                oracle: Some("assert!(result.is_err())".to_string()),
                oracle_kind: OracleKind::BroadError,
                oracle_strength: OracleStrength::Weak,
                relation_reason: None,
                relation_confidence: None,
                miss: None,
                miss_support: None,
                role: crate::domain::RelatedTestRole::Contributing,
            },
        ];

        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or_else(|| "expected weak finding witness".to_string())?;
        assert_eq!(witness.kind, "static_discriminator_gap");
        assert_eq!(witness.expected_sink.as_deref(), Some("error_variant"));
        assert_eq!(
            witness.missing_discriminators[0].value,
            "PricingError::Boundary"
        );
        // Equally ranked rows resolve to the earliest listed (#5354).
        assert_eq!(witness.fix_site.as_ref().map(|site| site.line), Some(10));
        assert!(
            witness
                .fix_site
                .as_ref()
                .is_some_and(|site| site.oracle_location.is_none())
        );
        assert!(
            witness
                .limitations
                .iter()
                .any(|item| item.kind == "oracle_source_location_unavailable")
        );
        assert!(witness.suggested_assertion.is_none());
        assert!(
            witness
                .limitations
                .iter()
                .any(|item| item.kind == "suggested_assertion_unavailable")
        );
        Ok(())
    }

    /// #14454: a strong row that cannot observe the change (an `Ok`-only pin
    /// on a changed error variant) is never the fix site over a weaker row
    /// that does; without that miss, oracle strength still leads.
    #[test]
    fn fix_site_skips_a_strong_row_that_cannot_observe_the_change() {
        let related = |name: &str, strength: OracleStrength, miss: RelatedTestMiss| RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/existing.rs"),
            line: 4,
            oracle: Some(format!("{name}_assertion")),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: strength,
            relation_reason: None,
            relation_confidence: None,
            miss: Some(miss),
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        };
        let mut finding = sample_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.related_tests = vec![
            related(
                "accepts_short_names",
                OracleStrength::Strong,
                RelatedTestMiss::AssertionNotObserving,
            ),
            related(
                "rejects_long_names",
                OracleStrength::Weak,
                RelatedTestMiss::WeakAssertion,
            ),
        ];
        let fix_test = |finding: &Finding| {
            DiagnosticWitness::from_finding(finding)
                .and_then(|witness| witness.fix_site)
                .map(|site| site.test_name)
        };
        assert_eq!(fix_test(&finding).as_deref(), Some("rejects_long_names"));
        // A strong row with no call path to the change ranks below the
        // Ok-only direct caller, never above it.
        let mut no_call = finding.clone();
        no_call.related_tests[1] = related(
            "checks_other",
            OracleStrength::Strong,
            RelatedTestMiss::NoCallPath,
        );
        assert_eq!(fix_test(&no_call).as_deref(), Some("accepts_short_names"));
        finding.related_tests[0].miss = Some(RelatedTestMiss::MissingExactAssertion);
        assert_eq!(fix_test(&finding).as_deref(), Some("accepts_short_names"));
    }

    /// RC walk (py-pricing): `check` and the repair card named
    /// `test_discount_far_above_threshold` while the context witness named the
    /// first strong related test. The suggested repair test is the fix site
    /// when it is one of the related tests; otherwise the ranking decides.
    #[test]
    fn fix_site_follows_the_suggested_repair_test() {
        let related = |name: &str, line: usize| RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("tests/test_pricing.py"),
            line,
            oracle: Some(format!("assert {name}()")),
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        };
        let mut finding = sample_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "amount == DISCOUNT_THRESHOLD".to_string(),
            reason: "no related input at the boundary".to_string(),
            flow_sink: None,
            shape: DiscriminatorShape::Boundary,
            drafted_boundary: None,
        }];
        finding.related_tests = vec![
            related("test_no_discount_below_threshold", 4),
            related("test_discount_far_above_threshold", 8),
        ];
        let fix_test = |finding: &Finding| {
            DiagnosticWitness::from_finding(finding)
                .and_then(|witness| witness.fix_site)
                .map(|site| site.test_name)
        };
        let ranked = fix_test(&finding);

        finding.evidence =
            vec!["suggested_test_name: test_discount_far_above_threshold".to_string()];
        assert_eq!(
            fix_test(&finding).as_deref(),
            Some("test_discount_far_above_threshold")
        );
        finding.evidence =
            vec!["suggested_test_name: test_no_discount_below_threshold".to_string()];
        assert_eq!(
            fix_test(&finding).as_deref(),
            Some("test_no_discount_below_threshold")
        );
        // A suggestion that names no related test falls back to the ranking.
        finding.evidence = vec!["suggested_test_name: test_new_boundary".to_string()];
        assert_eq!(fix_test(&finding), ranked);
    }

    fn rust_related(
        name: &str,
        line: usize,
        reason: crate::domain::RelationReason,
        oracle_strength: OracleStrength,
        miss: crate::domain::RelatedTestMiss,
    ) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            line,
            oracle: (oracle_strength != OracleStrength::None)
                .then(|| "assert!(discount(200, true) < 200)".to_string()),
            oracle_kind: OracleKind::Unknown,
            oracle_strength,
            relation_reason: Some(reason),
            relation_confidence: Some(reason.confidence()),
            miss: Some(miss),
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        }
    }

    /// #5354: a call-bearing test with no assertion beats a proximity-only
    /// test whose oracle belongs to a different function.
    #[test]
    fn fix_site_prefers_a_call_bearing_test_over_a_proximity_only_oracle() -> Result<(), String> {
        use crate::domain::{RelatedTestMiss, RelationReason};
        let mut finding = sample_finding();
        finding.class = ExposureClass::ReachableUnrevealed;
        finding.related_tests = vec![
            rust_related(
                "member_gets_discount",
                17,
                RelationReason::SameTestFile,
                OracleStrength::Weak,
                RelatedTestMiss::NoCallPath,
            ),
            rust_related(
                "shipping_runs",
                19,
                RelationReason::DirectOwnerCall,
                OracleStrength::None,
                RelatedTestMiss::NoAssertion,
            ),
        ];
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or("a reachable_unrevealed finding builds a witness")?;
        let fix_site = witness
            .fix_site
            .ok_or("a call-bearing test is the fix site")?;
        assert_eq!(fix_site.test_name, "shipping_runs");
        assert_eq!(fix_site.line, 19);
        assert_eq!(fix_site.current_oracle, None);
        Ok(())
    }

    /// #5354: a test that only carries the owner's name, with a strong
    /// oracle on other code, ranks after a weaker test that calls the owner.
    #[test]
    fn fix_site_prefers_a_calling_test_over_a_stronger_name_only_test() -> Result<(), String> {
        use crate::domain::{RelatedTestMiss, RelationReason};
        let mut finding = sample_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.related_tests = vec![
            rust_related(
                "discount_named_elsewhere",
                9,
                RelationReason::OwnerNamedTest,
                OracleStrength::Strong,
                RelatedTestMiss::NoCallPath,
            ),
            rust_related(
                "checkout_applies_discount",
                21,
                RelationReason::DirectOwnerCall,
                OracleStrength::Weak,
                RelatedTestMiss::WeakAssertion,
            ),
        ];
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or("a weakly_exposed finding builds a witness")?;
        let fix_site = witness.fix_site.ok_or("the calling test is the fix site")?;
        assert_eq!(fix_site.test_name, "checkout_applies_discount");
        assert_eq!(fix_site.line, 21);

        // With no calling test the name-matched one is still offered.
        finding.related_tests.truncate(1);
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or("a weakly_exposed finding builds a witness")?;
        let fix_site = witness
            .fix_site
            .ok_or("a name-matched test remains a fallback fix site")?;
        assert_eq!(fix_site.test_name, "discount_named_elsewhere");
        Ok(())
    }

    /// #5354: a producer-named suggested test that is linked only by
    /// proximity does not override the calling test.
    #[test]
    fn a_suggested_proximity_only_test_is_not_the_fix_site() -> Result<(), String> {
        use crate::domain::{RelatedTestMiss, RelationReason};
        let mut finding = sample_finding();
        finding.class = ExposureClass::ReachableUnrevealed;
        finding.evidence = vec!["suggested_test_name: member_gets_discount".to_string()];
        finding.related_tests = vec![
            rust_related(
                "member_gets_discount",
                17,
                RelationReason::SameTestFile,
                OracleStrength::Weak,
                RelatedTestMiss::NoCallPath,
            ),
            rust_related(
                "shipping_runs",
                19,
                RelationReason::DirectOwnerCall,
                OracleStrength::None,
                RelatedTestMiss::NoAssertion,
            ),
        ];
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or("a reachable_unrevealed finding builds a witness")?;
        let fix_site = witness.fix_site.ok_or("the calling test is the fix site")?;
        assert_eq!(fix_site.test_name, "shipping_runs");
        Ok(())
    }

    /// #5354: with only proximity-linked tests there is no fix site, and the
    /// witness says so instead of naming a nearby test.
    #[test]
    fn proximity_only_related_tests_never_become_the_fix_site() -> Result<(), String> {
        use crate::domain::{RelatedTestMiss, RelationReason};
        let mut finding = sample_finding();
        finding.class = ExposureClass::ReachableUnrevealed;
        finding.related_tests = vec![
            rust_related(
                "member_gets_discount",
                17,
                RelationReason::SameTestFile,
                OracleStrength::Strong,
                RelatedTestMiss::NoCallPath,
            ),
            rust_related(
                "token_neighbour",
                30,
                RelationReason::WeakTokenSubstring,
                OracleStrength::Strong,
                RelatedTestMiss::WeakAssertion,
            ),
        ];
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or("a reachable_unrevealed finding builds a witness")?;
        assert_eq!(witness.fix_site, None);
        assert!(
            witness
                .limitations
                .iter()
                .any(|limitation| limitation.kind == "fix_site_unavailable"),
            "{:?}",
            witness.limitations
        );
        Ok(())
    }

    /// #5344 review: a test listed only because ripr examined it (none of its
    /// assertions matched the change) is not a fix site, even when it is the
    /// only related test or the suggested one.
    #[test]
    fn an_examined_miss_is_never_the_fix_site() -> Result<(), String> {
        let miss = RelatedTest {
            name: "check_runs".to_string(),
            file: PathBuf::from("tests/run.rs"),
            line: 4,
            oracle: Some("assert_eq!(names.len(), 2);".to_string()),
            oracle_kind: OracleKind::Unknown,
            oracle_strength: OracleStrength::None,
            relation_reason: None,
            relation_confidence: None,
            miss: Some(crate::domain::RelatedTestMiss::AssertionNotObserving),
            miss_support: None,
            role: crate::domain::RelatedTestRole::ExaminedOnly,
        };
        let mut finding = sample_finding();
        finding.class = ExposureClass::ReachableUnrevealed;
        finding.related_tests = vec![miss];
        finding.evidence = vec!["suggested_test_name: check_runs".to_string()];
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or("a reachable_unrevealed finding builds a witness")?;
        assert!(witness.fix_site.is_none(), "{:?}", witness.fix_site);
        assert!(
            witness
                .limitations
                .iter()
                .any(|limitation| limitation.kind == "fix_site_unavailable"),
            "{:?}",
            witness.limitations
        );
        Ok(())
    }

    #[test]
    fn fix_site_matches_the_suggested_test_by_file_and_name() {
        let related = |file: &str, line: usize| RelatedTest {
            name: "test_boundary".to_string(),
            file: PathBuf::from(file),
            line,
            oracle: None,
            oracle_kind: OracleKind::ExactValue,
            oracle_strength: OracleStrength::Strong,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        };
        let mut finding = sample_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.activation.missing_discriminators = vec![MissingDiscriminatorFact {
            value: "amount == DISCOUNT_THRESHOLD".to_string(),
            reason: "no related input at the boundary".to_string(),
            flow_sink: None,
            shape: DiscriminatorShape::Boundary,
            drafted_boundary: None,
        }];
        finding.related_tests = vec![related("tests/test_a.py", 4), related("tests/test_b.py", 8)];
        let fix_file = |finding: &Finding| {
            DiagnosticWitness::from_finding(finding)
                .and_then(|witness| witness.fix_site)
                .map(|site| site.file)
        };
        for file in ["tests/test_a.py", "tests/test_b.py"] {
            finding.evidence = vec![
                "suggested_test_name: test_boundary".to_string(),
                format!("suggested_test_file: {file}"),
            ];
            assert_eq!(fix_file(&finding).as_deref(), Some(file));
        }
    }

    #[test]
    fn exposed_finding_without_missing_fact_does_not_become_a_gap_witness() {
        let finding = sample_finding();
        assert!(DiagnosticWitness::from_finding(&finding).is_none());
    }

    fn python_duplicate_finding() -> Finding {
        let mut finding = sample_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.language = Some(LanguageId::Python);
        finding.evidence = vec![
            "suggested_test_file: tests/test_mixed.v2.py".to_string(),
            "suggested_test_name: test_same".to_string(),
            "suggested_test_line: 7".to_string(),
        ];
        finding.related_tests = [
            (
                4,
                OracleKind::SmokeOnly,
                OracleStrength::Smoke,
                Some("assertTrue(value)"),
            ),
            (7, OracleKind::Unknown, OracleStrength::Unknown, None),
        ]
        .into_iter()
        .map(|(line, oracle_kind, oracle_strength, oracle)| RelatedTest {
            name: "test_same".to_string(),
            file: PathBuf::from("./tests/test_mixed.v2.py"),
            line,
            oracle: oracle.map(ToString::to_string),
            oracle_kind,
            oracle_strength,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            miss_support: None,
            role: crate::domain::RelatedTestRole::Contributing,
        })
        .collect();
        finding
    }

    #[test]
    fn python_selected_line_preserves_unknown_row_over_stronger_duplicate() -> Result<(), String> {
        let mut finding = python_duplicate_finding();
        for reverse in [false, true] {
            if reverse {
                finding.related_tests.reverse();
                finding.evidence[0] = "suggested_test_file: .\\tests\\test_mixed.v2.py".to_string();
            }
            let site = DiagnosticWitness::from_finding(&finding)
                .and_then(|witness| witness.fix_site)
                .ok_or("selected Unknown row must remain a fix site")?;
            assert_eq!(site.line, 7);
            assert_eq!(site.test_name, "test_same");
            assert_eq!(site.oracle_strength, "unknown");
            assert_eq!(site.oracle_kind, "unknown");
            assert!(site.current_oracle.is_none());
        }
        Ok(())
    }

    #[test]
    fn python_selected_identity_legacy_unique_and_absent_declaration_stay_distinct() {
        let mut finding = python_duplicate_finding();
        finding.evidence.pop();
        assert!(python_suggested_related_test(&finding).is_none());
        assert!(DiagnosticWitness::from_finding(&finding).is_some_and(|w| w.fix_site.is_none()));

        finding.related_tests.remove(0);
        assert_eq!(
            python_suggested_related_test(&finding).map(|test| test.line),
            Some(7)
        );
        assert_eq!(
            DiagnosticWitness::from_finding(&finding)
                .and_then(|w| w.fix_site)
                .map(|s| s.line),
            Some(7)
        );

        let mut finding = python_duplicate_finding();
        finding.evidence.clear();
        assert_eq!(
            DiagnosticWitness::from_finding(&finding)
                .and_then(|w| w.fix_site)
                .map(|s| s.line),
            Some(4)
        );
    }

    #[test]
    fn python_declared_unusable_identity_never_borrows_a_competing_row() {
        let original = python_duplicate_finding();
        let mut variants = Vec::new();
        for bad_line in ["", "not-a-line", "0", "99"] {
            let mut finding = original.clone();
            finding.evidence[2] = format!("suggested_test_line: {bad_line}");
            variants.push(finding);
        }
        for index in [0, 1] {
            let mut finding = original.clone();
            finding.evidence.remove(index);
            variants.push(finding);
        }
        for (index, entry) in [
            (0, "suggested_test_file: tests/other.py"),
            (1, "suggested_test_name: test_other"),
        ] {
            let mut finding = original.clone();
            finding.evidence[index] = entry.to_string();
            variants.push(finding);
        }
        for entry in [
            "suggested_test_file: tests/other.py",
            "suggested_test_name: test_other",
            "suggested_test_line: 7",
        ] {
            let mut finding = original.clone();
            finding.evidence.push(entry.to_string());
            variants.push(finding);
        }
        let mut unmatched = original.clone();
        unmatched.related_tests[1].miss =
            Some(crate::domain::RelatedTestMiss::AssertionNotObserving);
        unmatched.related_tests[1].role = crate::domain::RelatedTestRole::ExaminedOnly;
        variants.push(unmatched);
        let mut duplicate = original.clone();
        duplicate
            .related_tests
            .push(duplicate.related_tests[1].clone());
        variants.push(duplicate);
        for finding in variants {
            assert!(
                python_suggested_related_test(&finding).is_none(),
                "{:?}",
                finding.evidence
            );
            assert!(
                DiagnosticWitness::from_finding(&finding).is_some_and(|w| w.fix_site.is_none()),
                "{:?}",
                finding.evidence
            );
        }
    }

    #[test]
    fn explain_command_quotes_a_finding_id_that_embeds_a_hostile_path() -> Result<(), String> {
        let mut finding = sample_finding();
        finding.class = ExposureClass::WeaklyExposed;
        finding.id = "probe:src_we ird_it's;x.rs:predicate:ec6d6f91".to_string();
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or_else(|| "sample finding should produce a witness".to_string())?;
        // Host-aware quoting (#7729): bash single quotes off Windows, double
        // quotes on it.
        let expected = format!(
            "ripr explain --root . {}",
            crate::agent::loop_commands::shell_arg("probe:src_we ird_it's;x.rs:predicate:ec6d6f91")
        );
        assert_eq!(witness.explain_command, expected);
        Ok(())
    }

    #[test]
    fn explain_command_leaves_a_plain_finding_id_bare() -> Result<(), String> {
        let mut finding = sample_finding();
        finding.class = ExposureClass::WeaklyExposed;
        let witness = DiagnosticWitness::from_finding(&finding)
            .ok_or_else(|| "sample finding should produce a witness".to_string())?;
        assert_eq!(
            witness.explain_command,
            "ripr explain --root . probe:pricing:88:error_path"
        );
        Ok(())
    }

    fn sample_finding() -> Finding {
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "probe:pricing:88:error_path".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:pricing:88:error_path".to_string()),
                location: SourceLocation::new("src/pricing.rs", 88, 1),
                owner: Some(SymbolId("pricing::discount".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "amount >= threshold".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::Exposed,
            ripr: RiprEvidence {
                reach: StageEvidence::new(StageState::Yes, Confidence::High, "reachable"),
                infect: StageEvidence::new(StageState::Yes, Confidence::High, "infectable"),
                propagate: StageEvidence::new(StageState::Yes, Confidence::Medium, "propagatable"),
                reveal: RevealEvidence {
                    observe: StageEvidence::new(StageState::Yes, Confidence::Medium, "observed"),
                    discriminate: StageEvidence::new(
                        StageState::Yes,
                        Confidence::Medium,
                        "discriminated",
                    ),
                },
            },
            confidence: 0.75,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: Vec::new(),
            recommended_next_step: None,
            language: None,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }
}
