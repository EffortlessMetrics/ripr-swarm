use crate::domain::{OracleKind, OracleStrength};

use super::arguments::{
    assertion_oracle_text, ensure_assertion_arguments, is_unguarded_wildcard_assertion,
    outer_assertion_condition,
};
use super::patterns::{
    contains_exact_comparison, inequality_has_struct_literal_operand, is_broad_error_assertion,
    is_clear_exact_custom_assertion_helper, is_custom_assertion_helper, is_duplicative_comparison,
    is_duplicative_equality_assertion, is_exact_error_variant_assertion, is_exact_value_assertion,
    is_inequality_macro_assertion, is_inequality_named_custom_helper,
    is_inequality_only_comparison, is_mock_expectation_line, is_negated_pattern_assertion,
    is_side_effect_observer_assertion, is_snapshot_assertion, is_whole_object_equality_assertion,
};

/// RIPR-SPEC-0231 rule 1: an inequality shows only that the value is not one
/// alternative, so it never earns an exact kind or strong strength.
const INEQUALITY: OracleClassification = OracleClassification {
    kind: OracleKind::RelationalCheck,
    strength: OracleStrength::Weak,
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
    // A negated pattern takes rule 1 before any pattern-reading step, so
    // `!matches!(r, Err(E::X))` never reads as the variant it excludes.
    if is_negated_pattern_assertion(line) {
        return INEQUALITY;
    }
    if let Some(classification) = classify_fallible_assertion(line) {
        return classification;
    }
    let inequality_macro = is_inequality_macro_assertion(line);
    if inequality_macro && is_exact_error_variant_assertion(line) {
        INEQUALITY
    } else if is_exact_error_variant_assertion(line) {
        OracleClassification {
            kind: OracleKind::ExactErrorVariant,
            strength: OracleStrength::Strong,
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
    } else if inequality_macro && is_whole_object_equality_assertion(line) {
        // A struct literal keeps its kind with no field credit (RIPR-SPEC-0225);
        // a closure or block operand is only a relation.
        if inequality_has_struct_literal_operand(line) {
            OracleClassification {
                kind: OracleKind::WholeObjectEquality,
                strength: OracleStrength::Weak,
            }
        } else {
            INEQUALITY
        }
    } else if is_whole_object_equality_assertion(line) {
        OracleClassification {
            kind: OracleKind::WholeObjectEquality,
            strength: OracleStrength::Strong,
        }
    } else if inequality_macro {
        INEQUALITY
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
    } else if line.contains(".unwrap(")
        || line.contains(".expect(")
        || line.contains("is_ok")
        || line.contains("is_some")
        || line.contains("is_none")
    {
        OracleClassification {
            kind: OracleKind::SmokeOnly,
            strength: OracleStrength::Smoke,
        }
    } else if scalar_integer_relation {
        OracleClassification {
            kind: OracleKind::RelationalCheck,
            strength: OracleStrength::Weak,
        }
    } else if is_mock_expectation_line(line) || is_side_effect_observer_assertion(line) {
        OracleClassification {
            kind: OracleKind::MockExpectation,
            strength: OracleStrength::Medium,
        }
    } else if is_clear_exact_custom_assertion_helper(line) {
        if is_inequality_named_custom_helper(line) {
            return INEQUALITY;
        }
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
    // Rule 1 at step 0: `!=` as the only comparison is a relation, even
    // against an error variant or a `matches!` operand it excludes.
    if is_inequality_only_comparison(&condition) {
        return Some(INEQUALITY);
    }
    if is_exact_error_variant_assertion(&condition) {
        Some(OracleClassification {
            kind: OracleKind::ExactErrorVariant,
            strength: OracleStrength::Strong,
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
    } else if condition.contains(".unwrap(")
        || condition.contains(".expect(")
        || condition.contains("is_ok")
        || condition.contains("is_some")
        || condition.contains("is_none")
    {
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
                OracleKind::RelationalCheck,
                OracleStrength::Weak,
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
    fn spec_0231_rule_1_inequality_is_never_exact() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, ExactValue, RelationalCheck, WholeObjectEquality};
        use OracleStrength::{Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            // Acceptance examples 1, 2, 11, 13, 14, 19, 20 and 26.
            ("assert_ne!(score(2), 0);", RelationalCheck, Weak),
            ("assert_ne!(check(20), Ok(20));", RelationalCheck, Weak),
            ("assert_not_equal(score(2), 0);", RelationalCheck, Weak),
            ("ensure!(score(2) != 0);", RelationalCheck, Weak),
            (
                "assert_ne!(build(3), Config { retries: 9 });",
                WholeObjectEquality,
                Weak,
            ),
            (
                "assert!(!matches!(check(20), Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            ("assert_ne!(check(20), Err(E::Bad));", RelationalCheck, Weak),
            (
                "assert!(!matches!(check(5), Ok(_)));",
                RelationalCheck,
                Weak,
            ),
            // The other macro and helper spellings of the same inequality.
            ("debug_assert_ne!(score(2), 0);", RelationalCheck, Weak),
            (
                "debug_assert_ne!(build(3), Config { retries: 9 });",
                WholeObjectEquality,
                Weak,
            ),
            (
                "assert!((!matches!(check(20), Err(E::Bad))));",
                RelationalCheck,
                Weak,
            ),
            (
                "ensure!(matches!(check(20), Err(E::Bad)) != true);",
                RelationalCheck,
                Weak,
            ),
            (
                "ensure!(score(2) != 0 /* == placeholder */);",
                RelationalCheck,
                Weak,
            ),
            (
                "debug_assert!(!matches!(check(20), Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            (
                "ensure!(!matches!(check(20), Err(E::Bad)), \"bad\");",
                RelationalCheck,
                Weak,
            ),
            ("ensure!(check(20) != Err(E::Bad));", RelationalCheck, Weak),
            ("assert_ne_eq(score(2), 0);", RelationalCheck, Weak),
            (
                "helpers::assert_neq_matches(score(2), 0);",
                RelationalCheck,
                Weak,
            ),
            // A closure or block operand is not a struct literal.
            (
                "assert_ne!(apply(|x| { x + 1 }), 3);",
                RelationalCheck,
                Weak,
            ),
            ("assert_ne!(score(2), { 0 });", RelationalCheck, Weak),
            // Controls: equality and positive patterns keep their readings.
            ("assert_eq!(score(2), 4);", ExactValue, Strong),
            (
                "assert_eq!(build(3), Config { retries: 3 });",
                WholeObjectEquality,
                Strong,
            ),
            ("assert!(matches!(check(5), Ok(5)));", ExactValue, Strong),
            (
                "assert!(matches!(check(20), Err(E::Bad)));",
                ExactErrorVariant,
                Strong,
            ),
            ("assert_json_eq(actual, expected);", ExactValue, Strong),
            ("ensure!(score(2) == 4);", ExactValue, Strong),
            (
                "ensure!(score(2) == 4 && score(0) != 1);",
                ExactValue,
                Strong,
            ),
            (
                "assert!(score(2) != 0, \"not {}\", \"assert_eq!\");",
                RelationalCheck,
                Weak,
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
}
