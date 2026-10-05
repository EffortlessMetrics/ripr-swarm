use crate::domain::{OracleKind, OracleStrength};

use super::arguments::{
    assertion_oracle_text, ensure_assertion_arguments, is_unguarded_wildcard_assertion,
    outer_assertion_condition,
};
use super::pattern_admission::pattern_assertion_classification;
use super::patterns::{
    contains_exact_comparison, is_broad_error_assertion, is_clear_exact_custom_assertion_helper,
    is_custom_assertion_helper, is_duplicative_comparison, is_duplicative_equality_assertion,
    is_effect_observer_subject_assertion, is_exact_error_variant_assertion,
    is_exact_value_assertion, is_mock_expectation_line, is_smoke_check, is_snapshot_assertion,
    is_whole_object_equality_assertion, tests_both_sides,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OracleClassification {
    pub(crate) kind: OracleKind,
    pub(crate) strength: OracleStrength,
}

pub(crate) fn classify_assertion(line: &str) -> OracleClassification {
    // Preserve the full outer-assertion boundary before semantic operand
    // projection removes wrappers or trailing tokens from the input.
    let scalar_integer_relation = outer_assertion_condition(line)
        .as_deref()
        .is_some_and(is_scalar_integer_relation);
    // Diagnostic expressions must not manufacture a trusted error kind before
    // reveal decides whether this oracle observes the changed error path.
    let oracle_text = assertion_oracle_text(line);
    let line = oracle_text.as_deref().unwrap_or(line);
    if is_unguarded_wildcard_assertion(line) {
        return OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        };
    }
    // RIPR-SPEC-0231 rules 2 and 3: a whole-pattern assertion is only as
    // strong as its pattern, ahead of every step that reads `matches!`.
    if let Some(classification) = pattern_assertion_classification(line) {
        return classification;
    }
    if let Some(classification) = classify_fallible_assertion(line) {
        return classification;
    }
    if is_exact_error_variant_assertion(line) {
        OracleClassification {
            kind: OracleKind::ExactErrorVariant,
            strength: OracleStrength::Strong,
        }
    } else if tests_both_sides(line) {
        // RIPR-SPEC-0231 rule 4: a condition that tests both sides accepts
        // every value, so it is neither a broad error nor a smoke check.
        OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        }
    } else if is_broad_error_assertion(line) {
        OracleClassification {
            kind: OracleKind::BroadError,
            strength: OracleStrength::Weak,
        }
    } else if is_duplicative_equality_assertion(line) {
        OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        }
    } else if is_whole_object_equality_assertion(line) {
        OracleClassification {
            kind: OracleKind::WholeObjectEquality,
            strength: OracleStrength::Strong,
        }
    } else if is_exact_value_assertion(line) {
        OracleClassification {
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
        }
    } else if is_snapshot_assertion(line) {
        OracleClassification {
            kind: OracleKind::Snapshot,
            strength: OracleStrength::Medium,
        }
    } else if is_smoke_check(line) {
        OracleClassification {
            kind: OracleKind::SmokeOnly,
            strength: OracleStrength::Smoke,
        }
    } else if scalar_integer_relation {
        OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        }
    } else if is_mock_expectation_line(line) || is_effect_observer_subject_assertion(line) {
        OracleClassification {
            kind: OracleKind::MockExpectation,
            strength: OracleStrength::Medium,
        }
    } else if is_clear_exact_custom_assertion_helper(line) {
        OracleClassification {
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
        }
    } else if is_custom_assertion_helper(line) {
        OracleClassification {
            kind: OracleKind::Unknown,
            strength: OracleStrength::Unknown,
        }
    } else if line.contains("> 0")
        || line.contains('<')
        || line.contains('>')
        || line.contains("is_empty")
        || line.contains("contains")
        || line.contains("assert!")
    {
        OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        }
    } else {
        OracleClassification {
            kind: OracleKind::Unknown,
            strength: OracleStrength::Unknown,
        }
    }
}

/// Recognize only one complete path/field versus decimal integer condition.
/// This is syntactic evidence, not integer type inference or call resolution.
fn is_scalar_integer_relation(condition: &str) -> bool {
    let mut expression = condition.trim();
    while let Some(inner) = super::arguments::parenthesized_contents(expression) {
        expression = inner.trim();
    }
    let mut comparator = None;
    for (index, byte) in expression.bytes().enumerate() {
        if matches!(byte, b'<' | b'>') {
            if comparator.is_some() {
                return false;
            }
            let width = usize::from(expression.as_bytes().get(index + 1) == Some(&b'=')) + 1;
            comparator = Some((index, width));
        }
    }
    let Some((index, width)) = comparator else {
        return false;
    };
    let left = expression.get(..index).unwrap_or_default().trim();
    let right = expression.get(index + width..).unwrap_or_default().trim();
    (is_simple_path(left) && is_decimal_integer(right))
        || (is_decimal_integer(left) && is_simple_path(right))
}

fn is_simple_path(text: &str) -> bool {
    if text.is_empty() || text.starts_with('.') || text.ends_with('.') || text.ends_with(':') {
        return false;
    }
    text.replace("::", ".").split('.').all(|segment| {
        let mut bytes = segment.bytes();
        bytes
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
            && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    })
}

fn is_decimal_integer(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

fn classify_fallible_assertion(line: &str) -> Option<OracleClassification> {
    let condition = ensure_assertion_arguments(line)?.into_iter().next()?;
    if is_exact_error_variant_assertion(&condition) {
        Some(OracleClassification {
            kind: OracleKind::ExactErrorVariant,
            strength: OracleStrength::Strong,
        })
    } else if tests_both_sides(&condition) {
        Some(OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        })
    } else if is_broad_error_assertion(&condition) {
        Some(OracleClassification {
            kind: OracleKind::BroadError,
            strength: OracleStrength::Weak,
        })
    } else if (is_exact_value_assertion(&condition) || contains_exact_comparison(&condition))
        && !is_duplicative_comparison(&condition)
    {
        Some(OracleClassification {
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
        })
    } else if is_smoke_check(&condition) {
        Some(OracleClassification {
            kind: OracleKind::SmokeOnly,
            strength: OracleStrength::Smoke,
        })
    } else {
        Some(OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::classify_assertion;
    use crate::domain::{OracleKind, OracleStrength};

    #[test]
    fn exact_fallible_oracle_requires_an_explicit_value_comparison() -> Result<(), String> {
        let exact = classify_assertion(
            "ensure!(state == TerminalState::Pass, \"message mentions is_err\");",
        );
        if exact.kind != OracleKind::ExactValue || exact.strength != OracleStrength::Strong {
            return Err(format!("exact ensure condition was not strong: {exact:?}"));
        }

        for (text, expected_kind, expected_strength) in [
            (
                "ensure!(matches!(result, Err(AuthError::Denied)), \"exact error\");",
                OracleKind::ExactErrorVariant,
                OracleStrength::Strong,
            ),
            (
                "ensure!(matches!(value, Some(7)), \"exact option\");",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            ),
            (
                "ensure!(result.is_err(), \"expected failure\");",
                OracleKind::BroadError,
                OracleStrength::Weak,
            ),
            (
                "ensure!(result.is_ok(), \"expected success\");",
                OracleKind::SmokeOnly,
                OracleStrength::Smoke,
            ),
            (
                "ensure!(result.is_some(), \"expected value\");",
                OracleKind::SmokeOnly,
                OracleStrength::Smoke,
            ),
            (
                "ensure!(result.is_none(), \"expected absence\");",
                OracleKind::SmokeOnly,
                OracleStrength::Smoke,
            ),
            (
                "ensure!(result.unwrap(), \"expected unwrap\");",
                OracleKind::SmokeOnly,
                OracleStrength::Smoke,
            ),
            (
                "ensure!(result.expect(\"value\"), \"expected value\");",
                OracleKind::SmokeOnly,
                OracleStrength::Smoke,
            ),
            (
                "ensure!(state != TerminalState::Pending, \"not pending\");",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            ),
            (
                "ensure!(ready, \"expected == ready\");",
                OracleKind::RelationalCheck,
                OracleStrength::Weak,
            ),
            (
                "ensure!(rendered == rendered, \"self comparison\");",
                OracleKind::RelationalCheck,
                OracleStrength::Weak,
            ),
        ] {
            let actual = classify_assertion(text);
            if actual.kind != expected_kind || actual.strength != expected_strength {
                return Err(format!(
                    "fallible oracle classification mismatch for {text}: {actual:?}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn spec_0231_rules_4_to_6_match_whole_names() -> Result<(), String> {
        use OracleKind::{BroadError, ExactValue, MockExpectation, RelationalCheck, SmokeOnly};
        use OracleStrength::{Medium, Smoke, Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            // Acceptance examples 8 (not mock), 9, 10, 12, 15, 16 and 26.
            ("assert!(is_present());", RelationalCheck, Weak),
            ("assert_eq!(events_sent.len(), 1);", ExactValue, Strong),
            (
                "assert!(events_sent.contains(&id));",
                MockExpectation,
                Medium,
            ),
            ("assert_json_eq(actual, expected);", ExactValue, Strong),
            ("assert!(opt.is_some_and(|v| v > 1));", SmokeOnly, Smoke),
            ("assert!(!events.is_empty());", MockExpectation, Medium),
            ("assert!(r.is_ok() || r.is_err());", RelationalCheck, Weak),
            // Rule 4: whole method names only.
            ("assert!(r.is_okay());", RelationalCheck, Weak),
            ("assert!(this_errs(r));", RelationalCheck, Weak),
            ("assert!(r.is_err());", BroadError, Weak),
            ("assert!(Option::is_some(&r));", SmokeOnly, Smoke),
            (
                "assert!(o.is_some() || o.is_none());",
                RelationalCheck,
                Weak,
            ),
            ("ensure!(r.is_ok() || r.is_err());", RelationalCheck, Weak),
            ("assert!(o.is_none_or(|v| v > 1));", SmokeOnly, Smoke),
            // Rule 5: whole identifier segments, outside free calls.
            (
                "assert!(sentEvents.contains(&id));",
                MockExpectation,
                Medium,
            ),
            ("assert!(state.ready);", MockExpectation, Medium),
            ("assert!(statement.is_empty());", RelationalCheck, Weak),
            ("assert!(consent_given());", RelationalCheck, Weak),
            (
                "assert!(store.saved().is_empty());",
                MockExpectation,
                Medium,
            ),
            // Rule 6: equality names only.
            (
                "assert_equalish(score(2), 4);",
                OracleKind::Unknown,
                OracleStrength::Unknown,
            ),
            (
                "assert_eqv(score(2), 4);",
                OracleKind::Unknown,
                OracleStrength::Unknown,
            ),
            ("assert_values_equals(score(2), 4);", ExactValue, Strong),
            (
                "assert_snapshot_matches(actual, expected);",
                ExactValue,
                Strong,
            ),
        ] {
            let actual = classify_assertion(text);
            if actual.kind != expected_kind || actual.strength != expected_strength {
                return Err(format!(
                    "{text}: expected {expected_kind:?}/{expected_strength:?}, got {actual:?}"
                ));
            }
        }
        Ok(())
    }

    /// Rules 2 and 3 through the classifier and the scanner that feed
    /// related tests, not only through the pattern reader.
    #[test]
    fn spec_0231_pattern_readings_reach_the_classifier_and_scanner() -> Result<(), String> {
        use OracleKind::{BroadError, ExactErrorVariant, ExactValue, RelationalCheck, SmokeOnly};
        use OracleStrength::{Smoke, Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            ("assert!(matches!(check(20), Err(_e)));", BroadError, Weak),
            ("assert!(matches!(check(5), Ok(_)));", SmokeOnly, Smoke),
            (
                "assert!(matches!(lookup(1), Some(_) | None));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(lookup(1), Some(1..=5)));",
                RelationalCheck,
                Weak,
            ),
            ("ensure!(matches!(check(5), Ok(_)));", SmokeOnly, Smoke),
            (
                "assert_matches!(check(20), Err(ref e) if e.len() > 1);",
                BroadError,
                Weak,
            ),
            ("assert!(matches!(check(5), Ok(5)));", ExactValue, Strong),
            (
                "assert!(matches!(check(20), Err(e @ E::Bad)));",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert!(matches!(value, _ if value == 2));",
                ExactValue,
                Strong,
            ),
        ] {
            let actual = classify_assertion(text);
            let facts = crate::analysis::extract::extract_assertions(text, 7);
            let scanned = facts
                .iter()
                .map(|fact| (fact.kind.clone(), fact.strength.clone()))
                .collect::<Vec<_>>();
            if actual.kind != expected_kind
                || actual.strength != expected_strength
                || scanned != vec![(expected_kind.clone(), expected_strength.clone())]
            {
                return Err(format!(
                    "{text}: expected {expected_kind:?}/{expected_strength:?}, classifier {actual:?}, scanner {scanned:?}"
                ));
            }
        }
        Ok(())
    }

    /// The parser path (`syntax/ra.rs`) that builds related tests reads the
    /// same pattern strengths.
    #[test]
    fn spec_0231_pattern_readings_reach_the_parser_path() -> Result<(), String> {
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        let source = r#"
#[test]
fn err_binding() {
    assert!(matches!(check(20), Err(_e)));
}

#[test]
fn ok_wildcard() {
    assert!(matches!(check(5), Ok(_)));
}

#[test]
fn ok_literal() {
    assert!(matches!(check(5), Ok(5)));
}
"#;
        let facts =
            RaRustSyntaxAdapter.summarize_file(std::path::Path::new("src/lib.rs"), source)?;
        let readings = facts
            .tests
            .iter()
            .map(|test| {
                let oracle = test.assertions.first();
                (
                    test.name.as_str(),
                    oracle.map(|fact| (fact.kind.clone(), fact.strength.clone())),
                )
            })
            .collect::<Vec<_>>();
        let expected = vec![
            (
                "err_binding",
                Some((OracleKind::BroadError, OracleStrength::Weak)),
            ),
            (
                "ok_wildcard",
                Some((OracleKind::SmokeOnly, OracleStrength::Smoke)),
            ),
            (
                "ok_literal",
                Some((OracleKind::ExactValue, OracleStrength::Strong)),
            ),
        ];
        if readings != expected {
            return Err(format!("parser readings {readings:?}"));
        }
        Ok(())
    }
}
