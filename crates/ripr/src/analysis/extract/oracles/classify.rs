use crate::domain::{OracleKind, OracleStrength};

use super::arguments::{
    assertion_oracle_text, boolean_assertion_macro_condition, ensure_assertion_arguments,
    is_unguarded_wildcard_assertion, outer_assertion_condition,
};
use super::pattern_admission::pattern_assertion_classification;
use super::patterns::{
    PatternPolarity, contains_negated_pattern, deciding_disjunction_conjunct_rest,
    has_deciding_disjunction, has_top_level_exact_equality, inequality_has_struct_literal_operand,
    is_broad_error_assertion, is_clear_exact_custom_assertion_helper,
    is_comparison_shaped_custom_assertion, is_custom_assertion_helper, is_duplicative_comparison,
    is_duplicative_equality_assertion, is_effect_observer_subject_assertion,
    is_exact_error_variant_assertion, is_exact_membership_any_assertion, is_exact_value_assertion,
    is_inequality_macro_assertion, is_inequality_named_assertion_helper,
    is_inequality_named_assertion_macro, is_inequality_only_comparison, is_mock_expectation_line,
    is_negated_pattern_assertion, is_observer_named_assertion_helper, is_smoke_check,
    is_snapshot_assertion, is_whole_object_equality_assertion, negated_pattern_conjunct_rest,
    pattern_comparison_polarity, pattern_conjunct_assertions, tests_both_sides,
};

/// A relation that pins no value (RIPR-SPEC-0231 rules 1 and 4).
const RELATION: OracleClassification = OracleClassification {
    kind: OracleKind::RelationalCheck,
    strength: OracleStrength::Weak,
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
    // In a top-level `&&` chain the negated pattern pins nothing, so the
    // other conjuncts decide (`!matches!(r, Err(E::X)) && score == 4` reads
    // as `score == 4`). A negation anywhere else caps the whole reading.
    if let Some(rest) = negated_pattern_conjunct_rest(line) {
        return if rest.is_empty() {
            INEQUALITY
        } else {
            classify_assertion(&rest)
        };
    }
    // A parenthesized `&&` conjunct that a `||` decides pins nothing, so the
    // other conjuncts decide, as beside a negated pattern; with nothing else
    // left the group is only a relation (#7558). The rest's own reading is
    // kept, never lifted, so rule 1 never raises a strength.
    if let Some(rest) = deciding_disjunction_conjunct_rest(line) {
        return if rest.is_empty() {
            RELATION
        } else {
            classify_assertion(&rest)
        };
    }
    // A `matches!` compared with `==` or `!=` reads as the pattern only with
    // positive polarity against a boolean literal (`== true`, `!= false`);
    // any other comparison (`!= true`, `== !true`, `== flag`) excludes the
    // pattern or cannot be resolved, so it caps the reading like a negated
    // condition (`!matches!(..)` anywhere, or `matches!(..).not()`).
    let polarity = pattern_comparison_polarity(line);
    if contains_negated_pattern(line) || polarity == Some(PatternPolarity::NotPositive) {
        return capped_at_inequality(classify_non_negated_assertion(
            line,
            scalar_integer_relation,
        ));
    }
    // An inequality-named helper is recognized before any exact-kind step,
    // so a `matches!` or `Err(..)` among its arguments cannot lend it the
    // exact kind it denies (`assert_not_equal(matches!(r, Err(E::X)), true)`).
    // Capping, not replacing, keeps an unknown helper reading unknown, and a
    // mock or side-effect observer (`mock.assert_not_called()`,
    // `assert_not_sent(..)`) keeps its observer reading.
    if is_inequality_named_assertion_helper(line) || is_inequality_named_assertion_macro(line) {
        let classification = classify_non_negated_assertion(line, scalar_integer_relation);
        // A call-count or observation helper keeps its own reading: a mock
        // verification stays `mock_expectation` / medium, and a free helper
        // that rule 5 gives no observer credit (`assert_not_sent(..)`, like
        // `assert_sent(..)`) stays unknown rather than becoming a relation.
        if is_observer_named_assertion_helper(line) {
            return if classification.kind == OracleKind::MockExpectation {
                classification
            } else {
                capped_at_inequality(classification)
            };
        }
        // A comparison-shaped call (`assert_not_equal(a, b)`) is a relation
        // even though its name earns no exact-helper credit.
        if classification.kind == OracleKind::Unknown && is_comparison_shaped_custom_assertion(line)
        {
            return INEQUALITY;
        }
        return capped_at_inequality(classification);
    }
    classify_non_negated_assertion(line, scalar_integer_relation)
}

/// RIPR-SPEC-0231 rule 1: whether the assertion only shows its value differs
/// from one alternative (`assert_ne!`, `debug_assert_ne!`, a whole
/// `!matches!(..)` condition, a `matches!` comparison that is not positive
/// polarity, a negated pattern that caps a compound condition, an
/// inequality-named `assert_*` helper or macro that is not a mock or
/// side-effect observer, or a boolean condition whose only comparison is
/// `!=` and is not a positive pattern comparison). Reveal reads this so a probe-family override can never lift an
/// inequality above its classified strength.
pub(crate) fn is_inequality_assertion(line: &str) -> bool {
    let oracle_text = assertion_oracle_text(line);
    let line = oracle_text.as_deref().unwrap_or(line);
    let polarity = pattern_comparison_polarity(line);
    is_inequality_macro_assertion(line)
        || is_negated_pattern_assertion(line)
        || polarity == Some(PatternPolarity::NotPositive)
        || (contains_negated_pattern(line)
            && negated_pattern_conjunct_rest(line).is_none()
            && deciding_disjunction_conjunct_rest(line).is_none())
        || ((is_inequality_named_assertion_helper(line)
            || is_inequality_named_assertion_macro(line))
            && classify_assertion(line).kind != OracleKind::MockExpectation)
        || (polarity != Some(PatternPolarity::Positive)
            && boolean_assertion_macro_condition(line)
                .is_some_and(|(_, condition)| is_inequality_only_comparison(&condition)))
}

/// RIPR-SPEC-0231 rule 1 as a ceiling: an inequality never reads above weak,
/// and a reading already at or below weak keeps its kind and strength.
fn capped_at_inequality(classification: OracleClassification) -> OracleClassification {
    if classification.strength.rank() > INEQUALITY.strength.rank() {
        INEQUALITY
    } else {
        classification
    }
}

fn classify_non_negated_assertion(
    line: &str,
    scalar_integer_relation: bool,
) -> OracleClassification {
    let whole = classify_whole_condition(line, scalar_integer_relation);
    // RIPR-SPEC-0231 rule 3 (#7695): without a pinning `==` conjunct (rule
    // 1), a `&&` chain is no stronger than its strongest conjunct read alone,
    // so a `matches!` in the chain keeps its pattern's reading instead of
    // earning `exact_value` / strong whatever the pattern
    // (`assert!(ready && matches!(r, Ok(_)))`). A pinning `==` keeps the
    // chain's reading: the test does fix that value, and lowering it could
    // turn a real discriminator into an actionable gap. It only lowers.
    let pins_by_equality = boolean_assertion_macro_condition(line)
        .is_some_and(|(_, condition)| has_top_level_exact_equality(&condition));
    let Some(conjuncts) = pattern_conjunct_assertions(line).filter(|_| !pins_by_equality) else {
        return whole;
    };
    let readings = conjuncts
        .iter()
        .map(|conjunct| classify_assertion(conjunct))
        .collect::<Vec<_>>();
    let Some(rank) = readings.iter().map(|reading| reading.strength.rank()).max() else {
        return whole;
    };
    if rank > whole.strength.rank() {
        return whole;
    }
    // The chain keeps its kind only when a conjunct earns that kind at the
    // same strength alone, so an irrefutable `Ok(_) | Err(_)` conjunct
    // cannot lend the chain `broad_error` from its `Err` text.
    let strongest = readings
        .iter()
        .filter(|reading| reading.strength.rank() == rank)
        .collect::<Vec<_>>();
    if strongest.iter().any(|reading| **reading == whole) {
        return whole;
    }
    strongest
        .first()
        .map_or(whole.clone(), |reading| (*reading).clone())
}

fn classify_whole_condition(line: &str, scalar_integer_relation: bool) -> OracleClassification {
    // RIPR-SPEC-0231 rules 2 and 3: a whole-pattern assertion is only as
    // strong as its pattern, ahead of every step that reads `matches!`.
    if let Some(classification) = pattern_assertion_classification(line) {
        return classification;
    }
    if let Some(classification) = classify_fallible_assertion(line) {
        return classification;
    }
    // RIPR-SPEC-0231 rule 1 (#7558): a boolean assertion decided by an `||`
    // passes whenever one arm does, so no `matches!`, error variant or
    // equality in one arm earns an exact kind.
    let disjunction = boolean_assertion_macro_condition(line)
        .is_some_and(|(_, condition)| has_deciding_disjunction(&condition));
    let inequality_macro = is_inequality_macro_assertion(line);
    if inequality_macro && is_exact_error_variant_assertion(line) {
        INEQUALITY
    } else if is_exact_error_variant_assertion(line) && !disjunction {
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
        // `ready || r.is_err()` passes without an error: only a relation.
        if disjunction {
            RELATION
        } else {
            OracleClassification {
                kind: OracleKind::BroadError,
                strength: OracleStrength::Weak,
            }
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
    } else if is_whole_object_equality_assertion(line) && !disjunction {
        OracleClassification {
            kind: OracleKind::WholeObjectEquality,
            strength: OracleStrength::Strong,
        }
    } else if inequality_macro {
        INEQUALITY
    } else if is_exact_value_assertion(line) && !disjunction {
        OracleClassification {
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
        }
    } else if is_snapshot_assertion(line) {
        // `assert!(ready || { assert_snapshot!(v); true })` may never run
        // the snapshot, so a deciding `||` earns no snapshot credit either.
        if disjunction {
            RELATION
        } else {
            OracleClassification {
                kind: OracleKind::Snapshot,
                strength: OracleStrength::Medium,
            }
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
        // `assert!(ready || events_sent.contains(&id))` passes without the
        // observation, so a deciding `||` earns no observer credit either.
        if disjunction {
            RELATION
        } else {
            OracleClassification {
                kind: OracleKind::MockExpectation,
                strength: OracleStrength::Medium,
            }
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
    } else if is_exact_membership_any_assertion(line) {
        // RIPR-SPEC-0231 rule 7 (#6991): an exact-equality `.any()`
        // membership check fails for any wrong member value, so it pins
        // an exact value. It sits after the custom-helper steps and
        // before the bare-`assert!` catch-all below, which keeps every
        // wider `.any()` shape (and `.contains()`) weak.
        OracleClassification {
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
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
    // against an error variant or a `matches!` operand it excludes. A
    // positive-polarity pattern comparison (`matches!(..) != false`) asserts
    // the pattern and is not an inequality.
    if is_inequality_only_comparison(&condition)
        && pattern_comparison_polarity(line) != Some(PatternPolarity::Positive)
    {
        return Some(INEQUALITY);
    }
    // A deciding `||` passes whenever one arm does, so neither an error
    // variant nor an `==` in one arm pins the value (#7558).
    let disjunction = has_deciding_disjunction(&condition);
    if is_exact_error_variant_assertion(&condition) && !disjunction {
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
        Some(if disjunction {
            RELATION
        } else {
            OracleClassification {
                kind: OracleKind::BroadError,
                strength: OracleStrength::Weak,
            }
        })
    } else if ((is_exact_value_assertion(&condition) && !disjunction)
        || has_top_level_exact_equality(&condition))
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

    fn require_classification(
        text: &str,
        expected_kind: OracleKind,
        expected_strength: OracleStrength,
    ) -> Result<(), String> {
        let actual = classify_assertion(text);
        if actual.kind != expected_kind || actual.strength != expected_strength {
            return Err(format!(
                "oracle classification mismatch for {text}: got {actual:?}, want {expected_kind:?}/{expected_strength:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn exact_membership_any_assertion_admits_exact_equality() -> Result<(), String> {
        for text in [
            // Issue #6991 repro: exact membership over a captured log buffer.
            r#"assert!(lines.iter().any(|l| l == "audited 42"), "{lines:?}");"#,
            "assert!(xs.iter().any(|x| x == Config::LIMIT));",
            "assert!(xs.iter().any(|x| x == None));",
            "assert!(counts.into_iter().any(|n| n == 42));",
            "debug_assert!(buf.iter_mut().any(|b| *b == 0xFF));",
            "assert!(vals.iter().any(|v| v == 0x1E));",
            "assert!(chars.iter().any(|c| c == 'a'));",
            // Operator-lookalikes inside the literal must not defeat the shape.
            r#"assert!(xs.iter().any(|x| x == "a>b"));"#,
            r#"assert!(xs.iter().any(|x| x == "a|b"));"#,
            r##"assert!(xs.iter().any(|x| x == r#"raw"#));"##,
            "assert!(rows.iter().any(|r| r.total == 33));",
            "assert!(xs.iter().any(|x| x == -1));",
            "assert!(xs.iter().any(|x| x == 1e-3));",
            "assert!(xs.iter().any(|x| x == 1.5E+6));",
            r#"assert!(lines.iter().any(|l: &String| l == "audited 42"));"#,
            // A comma nested in the type ascription is not a second parameter.
            "assert!(pairs.iter().any(|pair: &(u32, u32)| pair.0 == 7));",
        ] {
            require_classification(text, OracleKind::ExactValue, OracleStrength::Strong)?;
        }
        Ok(())
    }

    #[test]
    fn exact_membership_any_assertion_rejects_non_exact_shapes() -> Result<(), String> {
        for text in [
            "assert!(xs.iter().any(|x| x > 1));",
            "assert!(xs.iter().any(|x| x != 1));",
            "assert!(!xs.iter().any(|x| x == 1));",
            "assert!(ready && xs.iter().any(|x| x == 1));",
            "assert!(xs.iter().any(|x| x == 1 || x == 2));",
            "assert!(xs.iter().any(|x| x == 1 && ready));",
            "assert!(xs.iter().any(|x| x == expected()));",
            "assert!(xs.iter().any(|x| x == other));",
            "assert!(ready.any(|x| x == 1));",
            "assert!(xs.any(|x| x == 1));",
            "assert!(xs.iter().any(|x| x.len() == 1));",
            "assert!(xs.iter().all(|x| x == 1));",
            // Rule 12 keeps `.contains()` weak; out of scope for #6991.
            "assert!(lines.contains(&expected));",
            // The shape inside a string literal is not an assertion shape.
            r#"assert!(x == ".any(|l| l == 1");"#,
            // Constructor calls stay weak: only literals and const paths pin.
            "assert!(xs.iter().any(|x| x == Some(1)));",
            // Bare uppercase names may be locals (`let EXPECTED = xs[0]`),
            // so only `::` paths and `None` pin.
            "assert!(ids.iter().any(|id| id == EXPECTED_ID));",
            "assert!(xs.iter().any(|x| x == Some));",
            // Malformed exponents never pin.
            "assert!(xs.iter().any(|x| x == 1e));",
            // Angle-nested commas stay rejected (fail-closed residual).
            "assert!(pairs.iter().any(|pair: Pair<u32, u32>| pair.0 == 7));",
            // A comment inside the operand stays weak (fail-closed residual).
            "assert!(xs.iter().any(|x| x == /* budget */ 7));",
            "assert!(xs.iter().any(|a, b| a == b));",
            "assert!(xs.iter().any(move |x| x == 1));",
            "assert!(xs.iter().any(|x| { x == 1 }));",
            "assert!(a.any(b).iter().any(|x| x == 1));",
        ] {
            require_classification(text, OracleKind::RelationalCheck, OracleStrength::Weak)?;
        }
        Ok(())
    }

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
            ("std::assert_ne!(score(2), 0);", RelationalCheck, Weak),
            ("core::assert_ne!(score(2), 0);", RelationalCheck, Weak),
            (
                "pretty_assertions::assert_ne!(score(2), 0);",
                RelationalCheck,
                Weak,
            ),
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

    /// #6670 review: an inequality-named helper is recognized before the
    /// exact-kind steps, so a `matches!(.., Err(..))` argument cannot lend
    /// it `exact_error_variant` / strong.
    #[test]
    fn spec_0231_rule_1_inequality_helper_precedes_exact_kinds() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, RelationalCheck, Unknown};
        use OracleStrength::{Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            (
                "assert_not_equal(matches!(result, Err(E::Bad)), true);",
                RelationalCheck,
                Weak,
            ),
            (
                "helpers::assert_ne(matches!(check(20), Err(E::Bad)), true);",
                RelationalCheck,
                Weak,
            ),
            (
                "checker.assert_not_matches(matches!(result, Err(E::Bad)), true);",
                RelationalCheck,
                Weak,
            ),
            // Capped, not replaced: an unknown helper stays unknown.
            ("assert_not_empty(items);", Unknown, OracleStrength::Unknown),
            // Controls: the equality helper keeps the exact kind.
            (
                "assert_equal(matches!(result, Err(E::Bad)), true);",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert_json_eq(actual, expected);",
                OracleKind::ExactValue,
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

    /// #6670 review: only a negated `matches!` that is the whole condition
    /// is an inequality. In a top-level `&&` chain the other conjuncts
    /// decide; any other compound use caps the reading at weak.
    #[test]
    fn spec_0231_rule_1_negated_pattern_must_be_the_whole_condition() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, ExactValue, RelationalCheck, SmokeOnly};
        use OracleStrength::{Smoke, Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            // The whole condition: an inequality.
            ("assert!(!matches!(r, Err(E::Bad)));", RelationalCheck, Weak),
            (
                "assert!(!(matches!(r, Err(E::Bad))));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(!matches!(r, Err(E::Bad)) && !matches!(r, Err(E::Other)));",
                RelationalCheck,
                Weak,
            ),
            // `&&` chains: the remaining conjuncts carry the reading.
            (
                "ensure!(!matches!(r, Err(E::Bad)) && score == 4);",
                ExactValue,
                Strong,
            ),
            (
                "assert!(!matches!(r, Err(E::Bad)) && matches!(r, Err(E::Other)));",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert!(!matches!(r, Err(E::Bad)) && r.is_ok());",
                SmokeOnly,
                Smoke,
            ),
            (
                "assert!(!matches!(r, Err(E::Bad)) && score == 4);",
                RelationalCheck,
                Weak,
            ),
            // Any other compound negation fails closed at weak.
            (
                "assert!(!matches!(r, Err(E::Bad)) || matches!(r, Err(E::Other)));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(ready && (!matches!(r, Err(E::Bad)) || matches!(r, Err(E::Other))));",
                RelationalCheck,
                Weak,
            ),
            // A double negation is not read through; it fails closed.
            (
                "assert!(!!matches!(r, Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            // Control: a positive pattern beside a `!=` keeps its kind.
            (
                "assert!(matches!(r, Err(E::Bad)) && score != 4);",
                ExactErrorVariant,
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

    /// #6670 review (round 2): a `matches!` operand that is negated, or
    /// compared against `false`, inside any equality form excludes the
    /// pattern rather than naming it, so it never reads as the variant.
    #[test]
    fn spec_0231_rule_1_negated_matches_operand_in_equality_is_weak() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, RelationalCheck};
        use OracleStrength::{Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            (
                "assert_eq!(!matches!(r, Err(E::Bad)), true);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert_eq!(matches!(r, Err(E::Bad)), false);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert_eq!(false, matches!(r, Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            (
                "debug_assert_eq!(matches!(r, Err(E::Bad)), false);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) == false);",
                RelationalCheck,
                Weak,
            ),
            (
                "ensure!(matches!(r, Err(E::Bad)) == false);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(!matches!(r, Err(E::Bad)) == true);",
                RelationalCheck,
                Weak,
            ),
            (
                "ensure!(false == matches!(r, Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            // Positive controls: the pattern itself is asserted.
            (
                "assert_eq!(matches!(r, Err(E::Bad)), true);",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)));",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) == true);",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert_eq!(check(20), Err(E::Bad));",
                ExactErrorVariant,
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

    /// #6670 review (round 2): `&&` binds tighter than `||`, so a condition
    /// with a top-level `||` is not a conjunct chain and a negated pattern
    /// in it caps the whole reading.
    #[test]
    fn spec_0231_rule_1_negated_conjunct_respects_or_precedence() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, RelationalCheck};
        use OracleStrength::{Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            (
                "assert!(!matches!(r, Err(E::Bad)) && ready || matches!(r, Err(E::Other)));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(ready || !matches!(r, Err(E::Bad)) && matches!(r, Err(E::Other)));",
                RelationalCheck,
                Weak,
            ),
            // A `||` inside a parenthesized conjunct still decides the
            // condition, so the group caps the reading (rule 1, #7558);
            // a literal `|| false` arm is not read through.
            (
                "assert!(!matches!(r, Err(E::Bad)) && (matches!(r, Err(E::Other)) || false));",
                RelationalCheck,
                Weak,
            ),
            // Control: with no deciding `||`, the positive conjunct decides.
            (
                "assert!(!matches!(r, Err(E::Bad)) && matches!(r, Err(E::Other)));",
                ExactErrorVariant,
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

    /// #6670 review (round 2): every inequality spelling of a helper name,
    /// and any `*assert_ne!` macro, caps at weak; an inequality-named helper
    /// never earns the clear-exact helper credit.
    #[test]
    fn spec_0231_rule_1_inequality_spellings_are_weak() -> Result<(), String> {
        use OracleKind::{ExactValue, RelationalCheck};
        use OracleStrength::{Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            ("assert_unequal(score(2), 0);", RelationalCheck, Weak),
            ("assert_noteq(a, b);", RelationalCheck, Weak),
            ("assert_inequal(a, b);", RelationalCheck, Weak),
            ("assert_different(a, b);", RelationalCheck, Weak),
            ("assert_differs_eq(a, b);", RelationalCheck, Weak),
            ("assert_neq(a, b);", RelationalCheck, Weak),
            (
                "helpers::assert_values_unequal(a, b);",
                RelationalCheck,
                Weak,
            ),
            ("soft_assert_ne!(score(2), 0);", RelationalCheck, Weak),
            ("prop_assert_ne!(score(2), 0);", RelationalCheck, Weak),
            (
                "soft_assert_ne!(build(3), Config { retries: 9 });",
                RelationalCheck,
                Weak,
            ),
            // Controls: equality spellings keep their exact credit.
            ("assert_equal(score(2), 4);", ExactValue, Strong),
            ("assert_json_eq(actual, expected);", ExactValue, Strong),
            ("assert_eq!(score(2), 4);", ExactValue, Strong),
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

    /// #6670 review (round 2): the inequality-helper cap must not pre-empt
    /// the mock-expectation or side-effect observer readings main gave
    /// these lines.
    #[test]
    fn spec_0231_rule_1_inequality_helper_keeps_observer_readings() -> Result<(), String> {
        use OracleKind::{MockExpectation, RelationalCheck};
        use OracleStrength::{Medium, Weak};
        for (text, expected_kind, expected_strength) in [
            ("mock.assert_not_called();", MockExpectation, Medium),
            // Rule 5 gives a free helper call no observer credit, so this
            // reads like `assert_sent(&outbox, msg)`: unknown.
            (
                "assert_not_sent(&outbox, msg);",
                OracleKind::Unknown,
                OracleStrength::Unknown,
            ),
            // The observer token must sit in the helper name, not an argument.
            ("assert_not_equal(state, 0);", RelationalCheck, Weak),
            (
                "mock.assert_ne(matches!(r, Err(E::Bad)), true);",
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

    /// #6670 review (round 3): one structural rule resolves the polarity of a
    /// `matches!` compared with `==` or `!=` against a boolean literal (or a
    /// negated literal), in either operand order and in every assertion form.
    /// Only positive polarity keeps the pattern reading; negative polarity and
    /// any comparison against a non-literal fail closed at weak.
    #[test]
    fn spec_0231_rule_1_matches_comparison_polarity_is_structural() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, RelationalCheck};
        use OracleStrength::{Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            // Negative polarity: the pattern is excluded, not named.
            (
                "assert!(matches!(r, Err(E::Bad)) != true);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(true != matches!(r, Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            (
                "debug_assert!(matches!(r, Err(E::Bad)) != true);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert_eq!(matches!(r, Err(E::Bad)), !true);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) == !true);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) != !false);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert_eq!(!true, matches!(r, Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert_ne!(matches!(r, Err(E::Bad)), true);",
                RelationalCheck,
                Weak,
            ),
            // `assert_ne!` stays an inequality macro whatever the literal.
            (
                "assert_ne!(matches!(r, Err(E::Bad)), false);",
                RelationalCheck,
                Weak,
            ),
            // A comparison against anything but a boolean literal fails closed.
            (
                "assert_eq!(matches!(r, Err(E::Bad)), expected);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) == ready);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) == matches!(s, Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(score == 4 && matches!(r, Err(E::Bad)) != true);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(ready && (matches!(r, Err(E::Bad))) == false);",
                RelationalCheck,
                Weak,
            ),
            // A negated pattern inside a comparison is not read through.
            (
                "assert_eq!(!matches!(r, Err(E::Bad)), false);",
                RelationalCheck,
                Weak,
            ),
            // Positive polarity keeps today's reading.
            (
                "assert!(matches!(r, Err(E::Bad)));",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert_eq!(matches!(r, Err(E::Bad)), true);",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert_eq!(true, matches!(r, Err(E::Bad)));",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) == true);",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert!(true == matches!(r, Err(E::Bad)));",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)) != false);",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert_eq!(matches!(r, Err(E::Bad)), !false);",
                ExactErrorVariant,
                Strong,
            ),
            // A pattern that is not itself an operand is not compared.
            (
                "assert_eq!(rs.iter().filter(|r| matches!(r, Err(E::Bad))).count(), 2);",
                ExactErrorVariant,
                Strong,
            ),
        ] {
            let actual = classify_assertion(text);
            if actual.kind != expected_kind || actual.strength != expected_strength {
                return Err(format!(
                    "{text}: expected {expected_kind:?}/{expected_strength:?}, got {actual:?}"
                ));
            }
            let inequality = super::is_inequality_assertion(text);
            if inequality != (expected_strength == Weak) {
                return Err(format!("{text}: is_inequality_assertion was {inequality}"));
            }
        }
        Ok(())
    }

    /// #6670 review (round 3): the `prop_assert*` macros, a negation-named
    /// assertion macro and a `.not()` on the pattern carry the same polarity
    /// as their `assert*` and `!` spellings.
    #[test]
    fn spec_0231_rule_1_negation_spellings_follow_the_polarity_rule() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, RelationalCheck};
        use OracleStrength::{Strong, Weak};
        for (text, expected_kind, expected_strength) in [
            (
                "prop_assert!(!matches!(r, Err(E::Bad)));",
                RelationalCheck,
                Weak,
            ),
            (
                "prop_assert_eq!(matches!(r, Err(E::Bad)), false);",
                RelationalCheck,
                Weak,
            ),
            (
                "prop_assert!(matches!(r, Err(E::Bad)) != true);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert_not_matches!(r, Err(E::Bad));",
                RelationalCheck,
                Weak,
            ),
            (
                "debug_assert_not_matches!(r, Err(E::Bad));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(matches!(r, Err(E::Bad)).not());",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(ready || matches!(r, Err(E::Bad)).not());",
                RelationalCheck,
                Weak,
            ),
            // Positive controls.
            (
                "prop_assert!(matches!(r, Err(E::Bad)));",
                ExactErrorVariant,
                Strong,
            ),
            (
                "prop_assert_eq!(matches!(r, Err(E::Bad)), true);",
                ExactErrorVariant,
                Strong,
            ),
            (
                "assert_matches!(r, Err(E::Bad));",
                ExactErrorVariant,
                Strong,
            ),
        ] {
            let actual = classify_assertion(text);
            if actual.kind != expected_kind || actual.strength != expected_strength {
                return Err(format!(
                    "{text}: expected {expected_kind:?}/{expected_strength:?}, got {actual:?}"
                ));
            }
            let inequality = super::is_inequality_assertion(text);
            if inequality != (expected_strength == Weak) {
                return Err(format!("{text}: is_inequality_assertion was {inequality}"));
            }
        }
        Ok(())
    }

    /// #6670 review (round 3): the observer exemption for an inequality-named
    /// helper reads only the receiver and the helper name, never arguments,
    /// and a value-comparison name (`equal`, `eq`, `ne`) is never exempt.
    #[test]
    fn spec_0231_rule_1_observer_exemption_reads_receiver_and_name_only() -> Result<(), String> {
        use OracleKind::{MockExpectation, RelationalCheck};
        use OracleStrength::{Medium, Weak};
        for (text, expected_kind, expected_strength) in [
            (
                "assert_not_equal(expect_value(), 3);",
                RelationalCheck,
                Weak,
            ),
            ("mock.assert_not_equal(x, 3);", RelationalCheck, Weak),
            ("mock.assert_ne(x, 3);", RelationalCheck, Weak),
            ("mock_store.assert_neq(x, 3);", RelationalCheck, Weak),
            (
                "assert_ne_state(machine, State::Idle);",
                RelationalCheck,
                Weak,
            ),
            ("assert_ne_event(bus, Event::Stop);", RelationalCheck, Weak),
            // Call-count and observation helpers keep their own reading; a
            // free helper call has no observer credit under rule 5.
            ("mock.assert_not_called();", MockExpectation, Medium),
            (
                "assert_not_sent(&outbox, msg);",
                OracleKind::Unknown,
                OracleStrength::Unknown,
            ),
            (
                "mock_bus.assert_not_published(msg);",
                MockExpectation,
                Medium,
            ),
        ] {
            let actual = classify_assertion(text);
            if actual.kind != expected_kind || actual.strength != expected_strength {
                return Err(format!(
                    "{text}: expected {expected_kind:?}/{expected_strength:?}, got {actual:?}"
                ));
            }
            let inequality = super::is_inequality_assertion(text);
            if inequality != (expected_kind != MockExpectation) {
                return Err(format!("{text}: is_inequality_assertion was {inequality}"));
            }
        }
        Ok(())
    }

    /// #6670 review: `debug_assert_eq!` carries the same asserted relation as
    /// `assert_eq!` (the operand projection and the owner and arm readers
    /// already treat them alike), so it earns the same exact credit.
    #[test]
    fn debug_assert_eq_earns_the_assert_eq_exact_credit() -> Result<(), String> {
        use OracleKind::{ExactErrorVariant, ExactValue, WholeObjectEquality};
        use OracleStrength::Strong;
        for (text, expected_kind) in [
            ("debug_assert_eq!(score(2), 4);", ExactValue),
            ("assert_eq!(score(2), 4);", ExactValue),
            (
                "debug_assert_eq!(build(3), Config { retries: 3 });",
                WholeObjectEquality,
            ),
            (
                "debug_assert_eq!(check(20), Err(E::Bad));",
                ExactErrorVariant,
            ),
        ] {
            let actual = classify_assertion(text);
            if actual.kind != expected_kind || actual.strength != Strong {
                return Err(format!(
                    "{text}: expected {expected_kind:?}/Strong, got {actual:?}"
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
            // Both sides only on one receiver; a field is not a method. Rule 4
            // does not apply, but rule 1's disjunction does: the condition
            // passes with no error at all, so it is only a relation (#7558).
            (
                "assert!(left.is_ok() || right.is_err());",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(get(1).is_some() || get(1).is_none());",
                RelationalCheck,
                Weak,
            ),
            ("assert!(status.is_err);", RelationalCheck, Weak),
            // `|| fallback` passes with no error, so rule 1's disjunction
            // reads it as a relation (#7558).
            (
                "assert!(r.is_ok() && r.is_err() || fallback);",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(ready && (r.is_ok() || r.is_err()));",
                RelationalCheck,
                Weak,
            ),
            (
                "assert!(r?.is_some() || r?.is_none());",
                RelationalCheck,
                Weak,
            ),
            // Rule 5: whole identifier segments, outside free calls.
            (
                "assert!(sentEvents.contains(&id));",
                MockExpectation,
                Medium,
            ),
            ("assert!(state.ready);", MockExpectation, Medium),
            ("assert!(statement.is_empty());", RelationalCheck, Weak),
            ("assert!(consent_given());", RelationalCheck, Weak),
            ("assert!(ready /* events_sent */);", RelationalCheck, Weak),
            (
                "assert!(ready, \"events_sent was {}\", n);",
                RelationalCheck,
                Weak,
            ),
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

    /// RIPR-SPEC-0227 decision 2: an `unwrap_err()` whose value is never
    /// compared reads `broad_error` / weak on the line scanner and the
    /// parser path; a binding a later assertion compares adds nothing.
    #[test]
    fn spec_0227_uncompared_unwrap_err_is_a_weak_broad_error_oracle() -> Result<(), String> {
        use OracleKind::{BroadError, ExactErrorVariant};
        use OracleStrength::{Strong, Weak};
        let cases = [
            (
                "underscore_binding",
                "let _e = check(300).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "discarded",
                "check(300).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "unused_binding",
                "let e = check(300).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "compared_binding",
                "let e = check(300).unwrap_err();\n    assert_eq!(e, E::TooLong);",
                vec![(ExactErrorVariant, Strong)],
            ),
            (
                "method_on_the_error",
                "let k = check(300).unwrap_err().kind();",
                vec![],
            ),
            (
                "compared_chain_tail",
                "let e = check(300)\n        .unwrap_err();\n    assert_eq!(e, E::TooLong);",
                vec![(ExactErrorVariant, Strong)],
            ),
            (
                "compared_split_let",
                "let e =\n        check(300).unwrap_err();\n    assert_eq!(e, E::TooLong);",
                vec![(ExactErrorVariant, Strong)],
            ),
            (
                "compared_argument_tail",
                "let e = check(\n        300,\n    ).unwrap_err();\n    assert_eq!(e, E::TooLong);",
                vec![(ExactErrorVariant, Strong)],
            ),
            // An `Ok` the adapter turns into `Err` passes whatever the call
            // returned; only adapters that keep an `Ok` an `Ok` may sit
            // between the call and `unwrap_err()`.
            (
                "fallback_error",
                "check(300).and(Err(E::Other)).unwrap_err();",
                vec![],
            ),
            (
                "option_converted",
                "let _e = find(3).ok_or(E::Missing).unwrap_err();",
                vec![],
            ),
            // A grouping is not the call: the adapter inside it still
            // replaces the call's result (#7872).
            (
                "grouped_fallback_error",
                "(check(300).and(Err(E::Other))).unwrap_err();",
                vec![],
            ),
            (
                "nested_grouped_fallback_error",
                "let _e = ((check(300).and(Err(E::Other)))).unwrap_err();",
                vec![],
            ),
            (
                "grouped_or_else",
                "(check(300).or(Err(E::Other))).unwrap_err();",
                vec![],
            ),
            (
                "grouped_option_converted",
                "(find(3).ok_or(E::Missing)).unwrap_err();",
                vec![],
            ),
            // A grouping is looked through: around the call itself, or
            // around Ok-preserving adapters, it still reads the call.
            (
                "grouped_call",
                "(check(300)).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "grouped_error_mapped",
                "let _e = (check(300).map_err(Wrap)).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "grouped_then_mapped",
                "((check(300))).map_err(Wrap).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "raw_identifier_call",
                "r#check(300).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "grouped_mapped_then_fallback",
                "(check(300).map_err(Wrap)).and(Err(E::Other)).unwrap_err();",
                vec![],
            ),
            // A fallback inside the group is not hidden by an Ok-preserving
            // adapter on either side of the closing paren.
            (
                "grouped_fallback_then_mapped",
                "(check(300).and(Err(E::Other)).map_err(Wrap)).unwrap_err();",
                vec![],
            ),
            (
                "grouped_fallback_mapped_outside",
                "(check(300).and(Err(E::Other))).map_err(Wrap).unwrap_err();",
                vec![],
            ),
            // A group around a comparison, jump, range or tuple is not a
            // call.
            (
                "grouped_comparison",
                "(a < check(300)).unwrap_err();",
                vec![],
            ),
            (
                "grouped_return",
                "(return check(300)).unwrap_err();",
                vec![],
            ),
            ("grouped_range", "(a..check(300)).unwrap_err();", vec![]),
            ("grouped_tuple", "(x < y, check(300)).unwrap_err();", vec![]),
            (
                "turbofish_path",
                "(Api::<A, B>::check(300)).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            ("negated_call", "let _e = !check(300).unwrap_err();", vec![]),
            (
                "turbofish_call",
                "check::<u8>(300).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "error_mapped",
                "check(300).map_err(Wrap).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
            (
                "method_receiver",
                "svc.check(300).unwrap_err();",
                vec![(BroadError, Weak)],
            ),
        ];
        assert_both_adapters_read(&cases)
    }

    /// RIPR-SPEC-0227 decision 2 lists `r.expect_err(..)` beside
    /// `r.unwrap_err()`: whose value is never compared, it observes the `Err`
    /// side and reads `broad_error` / weak, not a medium mock expectation
    /// (#7604). A mock's own `expect_*` call still reads as one.
    #[test]
    fn spec_0227_uncompared_expect_err_reads_like_unwrap_err() -> Result<(), String> {
        use OracleKind::{BroadError, ExactErrorVariant, MockExpectation};
        use OracleStrength::{Medium, Strong, Weak};
        let cases = [
            (
                "expect_err_underscore_binding",
                "let _e = check(300).expect_err(\"too long\");",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_discarded",
                "check(300).expect_err(\"too long\");",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_unused_binding",
                "let e = check(300).expect_err(\"too long\");",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_formatted_message",
                "check(300).expect_err(&format!(\"len {}\", 300));",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_grouped_call",
                "(check(300)).expect_err(\"too long\");",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_compared_binding",
                "let e = check(300).expect_err(\"too long\");\n    assert_eq!(e, E::TooLong);",
                vec![(ExactErrorVariant, Strong)],
            ),
            (
                "expect_err_method_on_the_error",
                "let k = check(300).expect_err(\"too long\").kind();",
                vec![],
            ),
            (
                "expect_err_fallback_error",
                "check(300).and(Err(E::Other)).expect_err(\"too long\");",
                vec![],
            ),
            (
                "mock_expectation_still_reads_medium",
                "mock.expect_publish().times(1);",
                vec![(MockExpectation, Medium)],
            ),
            (
                "mock_expect_err_prefixed_name",
                "mock.expect_err_report().times(1);",
                vec![(MockExpectation, Medium)],
            ),
            // mockall's `expect_<method>` for a method named `err` takes no
            // message: it stays a mock expectation.
            (
                "mock_expectation_for_err_method",
                "mock.expect_err();",
                vec![(MockExpectation, Medium)],
            ),
            (
                "mock_expectation_for_err_method_configured",
                "mock.expect_err().returning(|| ());",
                vec![(MockExpectation, Medium)],
            ),
            // `expect_` text in the message is not a mock expectation name.
            (
                "expect_err_expect_word_message",
                "check(300).expect_err(\"expect_error\");",
                vec![(BroadError, Weak)],
            ),
            // An effect word in the message is not an effect observer.
            (
                "expect_err_effect_word_message",
                "check(300).expect_err(\"state not sent\");",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_observer_and_expect_words_message",
                "check(300).expect_err(\"state expect\");",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_mock_verification_message",
                "check(300).expect_err(\"mock.verify() state\");",
                vec![(BroadError, Weak)],
            ),
            (
                "expect_err_nested_in_message",
                "check(300).expect_err(&check(400).expect_err(\"x\").to_string());",
                vec![(BroadError, Weak)],
            ),
        ];
        assert_both_adapters_read(&cases)
    }

    /// A named test body and the oracle kinds both adapters must read.
    type ParityCase<'a> = (&'a str, &'a str, Vec<(OracleKind, OracleStrength)>);

    /// Each body read by the line scanner and, as its own test, by the
    /// parser path, must yield exactly the expected oracle kinds.
    fn assert_both_adapters_read(cases: &[ParityCase<'_>]) -> Result<(), String> {
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
        let mut source = String::new();
        for (name, body, expected) in cases {
            let lexical = crate::analysis::extract::extract_assertions(body, 1)
                .iter()
                .map(|fact| (fact.kind.clone(), fact.strength.clone()))
                .collect::<Vec<_>>();
            if &lexical != expected {
                return Err(format!("{name}: line scanner read {lexical:?}"));
            }
            source.push_str(&format!("#[test]\nfn {name}() {{\n    {body}\n}}\n\n"));
        }
        let facts =
            RaRustSyntaxAdapter.summarize_file(std::path::Path::new("src/lib.rs"), &source)?;
        for (name, _, expected) in cases {
            let test = facts
                .tests
                .iter()
                .find(|test| test.name == *name)
                .ok_or_else(|| format!("parser lost test {name}"))?;
            let parsed = test
                .assertions
                .iter()
                .map(|fact| (fact.kind.clone(), fact.strength.clone()))
                .collect::<Vec<_>>();
            if &parsed != expected {
                return Err(format!("{name}: parser read {parsed:?}"));
            }
        }
        Ok(())
    }

    // Independent literal #7955 controls; original proposal SHA-256:
    // 681e7150b792e0ca65c401da3c5a1f6934bb81e786d4008ac4b403c1d7234968.
    type Literal7955Expectation<'a> = (usize, OracleKind, OracleStrength, &'a str);

    fn literal_7955_text_key(text: &str) -> String {
        text.trim()
            .strip_suffix(';')
            .unwrap_or(text.trim())
            .split_whitespace()
            .collect()
    }

    fn literal_7955_require_facts(
        context: &str,
        facts: &[crate::analysis::facts::OracleFact],
        expected: &[Literal7955Expectation<'_>],
    ) -> Result<(), String> {
        if !expected.is_empty() && facts.is_empty() {
            return Err(format!("{context}: no oracle subjects were extracted"));
        }
        if facts.len() != expected.len() {
            return Err(format!(
                "{context}: got {} facts, wanted {}: {facts:?}",
                facts.len(),
                expected.len()
            ));
        }
        let mut unmatched = facts.iter().collect::<Vec<_>>();
        for (line, kind, strength, text) in expected {
            let key = literal_7955_text_key(text);
            let position = unmatched
                .iter()
                .position(|fact| literal_7955_text_key(&fact.text) == key)
                .ok_or_else(|| {
                    format!("{context}: missing complete expression {text:?}: {facts:?}")
                })?;
            let fact = unmatched.remove(position);
            if fact.line != *line || &fact.kind != kind || &fact.strength != strength {
                return Err(format!(
                    "{context}: {text:?} got {fact:?}, wanted line {line}, {kind:?}/{strength:?}"
                ));
            }
            if fact.observed_tokens.is_empty() {
                return Err(format!(
                    "{context}: expression has no observed identifier tokens"
                ));
            }
        }
        Ok(())
    }

    fn literal_7955_read_both(
        name: &str,
        body: &str,
        expected: &[Literal7955Expectation<'_>],
    ) -> Result<(), String> {
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};

        // Two fixed wrapper rows make the first body row line 3 on BOTH paths.
        // Expected rows below are independently counted from the literal source.
        let lexical = crate::analysis::extract::extract_assertions(body, 3);
        let mut failures = Vec::new();
        if let Err(error) =
            literal_7955_require_facts(&format!("{name}/lexical"), &lexical, expected)
        {
            failures.push(error);
        }

        let source = format!("#[test]\nfn {name}() {{\n{body}\n}}\n");
        match RaRustSyntaxAdapter.summarize_file(std::path::Path::new("src/lib.rs"), &source) {
            Err(error) => failures.push(format!("{name}/parser SETUP: {error}")),
            Ok(file) => {
                if file.used_lexical_fallback || file.tests.len() != 1 {
                    failures.push(format!(
                        "{name}/parser SETUP: fallback={}, tests={}",
                        file.used_lexical_fallback,
                        file.tests.len()
                    ));
                } else if let Some(test) = file.tests.first() {
                    if test.name != name {
                        failures.push(format!("{name}/parser SETUP: wrong test {}", test.name));
                    } else if let Err(error) = literal_7955_require_facts(
                        &format!("{name}/parser"),
                        &test.assertions,
                        expected,
                    ) {
                        failures.push(error);
                    }
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

    #[test]
    fn spec_0227_7955_complete_error_statements_keep_literal_oracles() -> Result<(), String> {
        use OracleKind::{BroadError, ExactErrorVariant, MockExpectation, SmokeOnly};
        use OracleStrength::{Medium, Smoke, Strong, Weak};

        let cases = [
            // Unchanged cheap positive: establishes the retained basic authority.
            (
                "literal_7955_direct",
                "probe(700).unwrap_err();",
                vec![(3, BroadError, Weak, "probe(700).unwrap_err()")],
            ),
            // Blank first body row also distinguishes the caller's line origin.
            // A semicolon/closing paren in a message must not end a statement.
            (
                "literal_7955_wrapped_message",
                r#"
    let _message = probe(711).expect_err(
        &format!("tag-7955-a ; ) {}", 711),
    );"#,
                vec![(
                    4,
                    BroadError,
                    Weak,
                    r#"probe(711).expect_err(&format!("tag-7955-a ; ) {}", 711),)"#,
                )],
            ),
            (
                "literal_7955_expect_chain",
                r#"probe(722)
        .map_err(Wrap)
        .expect_err("tag-7955-b");"#,
                vec![(
                    3,
                    BroadError,
                    Weak,
                    r#"probe(722).map_err(Wrap).expect_err("tag-7955-b")"#,
                )],
            ),
            // The method expression starts on line 4; the statement starts on 3.
            // Parser method_call.range.start() alone cannot satisfy this pin.
            (
                "literal_7955_unwrap_split_let",
                "let _split =\n    probe(733)\n        .map_err(Wrap)\n        .unwrap_err();",
                vec![(3, BroadError, Weak, "probe(733).map_err(Wrap).unwrap_err()")],
            ),
            (
                "literal_7955_wrapped_arguments",
                "probe(\n    744,\n)\n    .unwrap_err();",
                vec![(3, BroadError, Weak, "probe(744,).unwrap_err()")],
            ),
            (
                "literal_7955_expect_split_let",
                r#"let _split =
        probe(755)
            .expect_err(
                "tag-7955-e",
            );"#,
                vec![(
                    3,
                    BroadError,
                    Weak,
                    r#"probe(755).expect_err("tag-7955-e",)"#,
                )],
            ),
            // Exact count + two exact expression identities defeats a whole-line
            // SmokeOnly fake and a two-copy whole-line fake, regardless of order.
            (
                "literal_7955_same_row_expect",
                r#"probe(766).expect_err("tag-7955-f"); companion(767).unwrap();"#,
                vec![
                    (
                        3,
                        BroadError,
                        Weak,
                        r#"probe(766).expect_err("tag-7955-f")"#,
                    ),
                    (3, SmokeOnly, Smoke, "companion(767).unwrap()"),
                ],
            ),
            (
                "literal_7955_same_row_unwrap",
                r#"probe(777).unwrap_err(); companion(778).expect("tag-7955-g");"#,
                vec![
                    (3, BroadError, Weak, "probe(777).unwrap_err()"),
                    (
                        3,
                        SmokeOnly,
                        Smoke,
                        r#"companion(778).expect("tag-7955-g")"#,
                    ),
                ],
            ),
            // Reversing source order must not let smoke admission consume the
            // entire row and hide the following error-side observer.
            (
                "literal_7955_same_row_smoke_first",
                r#"companion(788).unwrap(); probe(789).expect_err("tag-7955-reverse");"#,
                vec![
                    (3, SmokeOnly, Smoke, "companion(788).unwrap()"),
                    (
                        3,
                        BroadError,
                        Weak,
                        r#"probe(789).expect_err("tag-7955-reverse")"#,
                    ),
                ],
            ),
            // A wrapped diagnostic string contains no executable observers.
            // Count=1 and the full outer expression reject manufactured smoke
            // or mock facts from message text or a detached message-only row.
            (
                "literal_7955_wrapped_fake_observer_message",
                r#"probe(790).expect_err(
        "tag-7955-message companion(999).unwrap(); expect_publish",
    );"#,
                vec![(
                    3,
                    BroadError,
                    Weak,
                    r#"probe(790).expect_err("tag-7955-message companion(999).unwrap(); expect_publish",)"#,
                )],
            ),
            // Existing ceilings: a method on the extracted Error is not the
            // whole statement's error-side result observer.
            (
                "literal_7955_error_method",
                r#"let _kind = probe(800)
        .expect_err("tag-7955-method")
        .kind();"#,
                vec![],
            ),
            // Unsupported adapters can turn Ok into Err, so do not observe
            // whether the original call returned Err. No weak or mock credit.
            (
                "literal_7955_fallback_receiver",
                "probe(801)\n    .and(Err(E::Other))\n    .unwrap_err();",
                vec![],
            ),
            (
                "literal_7955_option_receiver",
                r#"lookup(802)
        .ok_or(E::Missing)
        .expect_err("tag-7955-option");"#,
                vec![],
            ),
            // The later comparison owns the sole exact oracle: no added weak
            // side check. A scanner must examine text after the COMPLETE binding.
            (
                "literal_7955_compared_binding",
                r#"let err = probe(811)
        .expect_err(
            "tag-7955-compared",
        );
    assert_eq!(err, E::Rejected);"#,
                vec![(7, ExactErrorVariant, Strong, "assert_eq!(err, E::Rejected)")],
            ),
            // The rest-of-statement cursor includes a same-row later comparison.
            (
                "literal_7955_compared_same_row",
                r#"let err = probe(822).expect_err("tag-7955-compared-row"); assert_eq!(err, E::Rejected);"#,
                vec![(3, ExactErrorVariant, Strong, "assert_eq!(err, E::Rejected)")],
            ),
            (
                "literal_7955_mock_prefix",
                "mock.expect_err_report().times(2);",
                vec![(
                    3,
                    MockExpectation,
                    Medium,
                    "mock.expect_err_report().times(2)",
                )],
            ),
            // Zero arguments identify mockall's expect_<method> for method err.
            (
                "literal_7955_mock_zero_arg",
                "mock.expect_err();",
                vec![(3, MockExpectation, Medium, "mock.expect_err()")],
            ),
            // Valid Rust tail expressions, but not complete semicolon-terminated
            // statements admitted by this narrow existing authority.
            (
                "literal_7955_unterminated_unwrap_statement",
                "probe(833)\n    .unwrap_err()",
                vec![],
            ),
            (
                "literal_7955_unterminated_expect_statement",
                r#"probe(844)
        .expect_err("tag-7955-tail")"#,
                vec![],
            ),
        ];

        // Run every case even when an earlier case returns a behavioral mismatch.
        let failures = cases
            .iter()
            .filter_map(|(name, body, expected)| literal_7955_read_both(name, body, expected).err())
            .collect::<Vec<_>>();
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

    #[test]
    fn spec_0227_7955_lexical_adapter_routes_to_complete_statement_scanner() -> Result<(), String> {
        use crate::analysis::syntax::{LexicalRustSyntaxAdapter, RustSyntaxAdapter};

        let source = r#"#[test]
    fn literal_7955_lexical_route() {
    probe(955).expect_err(
        "tag-7955-route",
    );
    }
    "#;
        let file =
            LexicalRustSyntaxAdapter.summarize_file(std::path::Path::new("src/lib.rs"), source)?;
        if !file.used_lexical_fallback || file.tests.len() != 1 {
            return Err(format!(
                "lexical routing SETUP: fallback={}, tests={}",
                file.used_lexical_fallback,
                file.tests.len()
            ));
        }
        let test = file
            .tests
            .first()
            .ok_or("lexical routing SETUP: missing test")?;
        if test.name != "literal_7955_lexical_route" {
            return Err(format!("lexical routing SETUP: wrong test {}", test.name));
        }
        literal_7955_require_facts(
            "lexical adapter routing",
            &test.assertions,
            &[(
                3,
                OracleKind::BroadError,
                OracleStrength::Weak,
                r#"probe(955).expect_err("tag-7955-route",)"#,
            )],
        )
    }

    #[test]
    fn spec_0227_7955_malformed_source_does_not_gain_error_or_mock_credit() -> Result<(), String> {
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};

        // These inputs are intentionally parser-invalid. Do not describe their
        // parser rejection as an executed empty-oracle behavioral negative.
        for body in [
            "probe(899)\n    .expect_err(\n        \"tag-7955-open\"",
            "let _unfinished = probe(900)\n    .unwrap_err(",
        ] {
            let lexical = crate::analysis::extract::extract_assertions(body, 3);
            // A literal ceiling assertion: malformed extraction must not acquire
            // BroadError or Medium/Strong strength. This does not
            // invent an unobserved exact baseline reading for arbitrary malformed
            // input; another existing Smoke/Weak reading is outside this claim.
            if lexical.iter().any(|fact| {
                fact.kind == OracleKind::BroadError
                    || matches!(
                        fact.strength,
                        OracleStrength::Medium | OracleStrength::Strong
                    )
            }) {
                return Err(format!(
                    "malformed source exceeded existing ceiling: {lexical:?}"
                ));
            }
            let source = format!("#[test]\nfn literal_7955_malformed() {{\n{body}\n}}\n");
            if RaRustSyntaxAdapter
                .summarize_file(std::path::Path::new("src/lib.rs"), &source)
                .is_ok()
            {
                return Err("malformed parser setup unexpectedly succeeded".to_string());
            }
        }
        Ok(())
    }

    #[test]
    fn spec_0227_7955_executed_diagnostic_observer_survives_outer_credit() -> Result<(), String> {
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};

        let body = r#"let _diagnostic =
        probe(901).expect_err(
            &companion(902).unwrap().to_string(),
        );"#;
        let source = format!("#[test]\nfn literal_7955_nested_observer() {{\n{body}\n}}\n");
        let lexical = crate::analysis::extract::extract_assertions(body, 3);
        let parsed =
            RaRustSyntaxAdapter.summarize_file(std::path::Path::new("src/lib.rs"), &source)?;
        if parsed.used_lexical_fallback || parsed.tests.len() != 1 {
            return Err(format!(
                "nested observer SETUP: fallback={}, tests={}",
                parsed.used_lexical_fallback,
                parsed.tests.len()
            ));
        }
        let test = parsed
            .tests
            .first()
            .ok_or("nested observer SETUP: missing test")?;
        if test.name != "literal_7955_nested_observer" {
            return Err(format!("nested observer SETUP: wrong test {}", test.name));
        }

        let mut failures = Vec::new();
        for (adapter, facts) in [
            ("lexical", lexical.as_slice()),
            ("parser", test.assertions.as_slice()),
        ] {
            // Literal, independently chosen readings: exactly one of each grade.
            let outer = facts
                .iter()
                .filter(|fact| {
                    fact.kind == OracleKind::BroadError && fact.strength == OracleStrength::Weak
                })
                .collect::<Vec<_>>();
            let inner = facts
                .iter()
                .filter(|fact| {
                    fact.kind == OracleKind::SmokeOnly && fact.strength == OracleStrength::Smoke
                })
                .collect::<Vec<_>>();
            if facts.len() != 2 || outer.len() != 1 || inner.len() != 1 {
                failures.push(format!(
                    "{adapter}: wanted two nonempty subjects, BroadError/Weak and SmokeOnly/Smoke: {facts:?}"
                ));
                continue;
            }
            let outer_text = outer[0].text.split_whitespace().collect::<String>();
            let inner_text = inner[0].text.split_whitespace().collect::<String>();
            if outer[0].line != 3 || !outer_text.contains("probe(901).expect_err(") {
                failures.push(format!(
                    "{adapter}: wrong outer identity/first statement line: {:?}",
                    outer[0]
                ));
            }
            // Identity only: leave the established lexical/AST text boundaries
            // and the inner line coordinate outside this supplemental claim.
            if !inner_text.contains("companion(902).unwrap()") {
                failures.push(format!(
                    "{adapter}: missing executed inner observer identity: {:?}",
                    inner[0]
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

    #[test]
    fn spec_0227_7955_malformed_inherited_context_adds_no_error_credit() -> Result<(), String> {
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};

        let cases = [
            (
                "open_call_after_closed_block",
                "consume(\n    {}\n    probe(903)\n        .unwrap_err();",
            ),
            (
                "open_array_after_closed_block",
                "let values = [\n    {}\n    probe(904)\n        .unwrap_err();",
            ),
            (
                "unmatched_closer_before_new_block",
                "];\n{}\nprobe(905)\n    .unwrap_err();",
            ),
            (
                "crossed_closers_before_new_block",
                "consume([\n)];\n{}\nprobe(906)\n    .unwrap_err();",
            ),
        ];
        let mut failures = Vec::new();
        for (name, body) in cases {
            let lexical = crate::analysis::extract::extract_assertions(body, 3);
            if !lexical.is_empty() {
                failures.push(format!("{name}: wanted zero lexical facts: {lexical:?}"));
            }
            let source = format!("#[test]\nfn literal_7955_malformed_context() {{\n{body}\n}}\n");
            if RaRustSyntaxAdapter
                .summarize_file(std::path::Path::new("src/lib.rs"), &source)
                .is_ok()
            {
                failures.push(format!(
                    "{name}: malformed parser setup unexpectedly succeeded"
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }

    #[test]
    fn spec_0227_7955_valid_block_context_keeps_error_credit() -> Result<(), String> {
        use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};

        // Both fixtures put the probe statement on literal source line 4.
        // The invoked closure also has an enclosing `(` outside its block: using
        // "any open paren exists" instead of the innermost context would lose it.
        let cases = [
            (
                "block",
                "{\n    probe(907)\n        .unwrap_err();\n}",
                "probe(907).unwrap_err()",
            ),
            (
                "invoked_closure",
                "(|| {\n    probe(908)\n        .unwrap_err();\n})();",
                "probe(908).unwrap_err()",
            ),
        ];
        let mut failures = Vec::new();
        for (name, body, identity) in cases {
            let lexical = crate::analysis::extract::extract_assertions(body, 3);
            let source = format!("#[test]\nfn literal_7955_valid_context() {{\n{body}\n}}\n");
            let parsed =
                RaRustSyntaxAdapter.summarize_file(std::path::Path::new("src/lib.rs"), &source)?;
            if parsed.used_lexical_fallback || parsed.tests.len() != 1 {
                return Err(format!("{name}: parser SETUP lost the one expected test"));
            }
            let test = parsed
                .tests
                .first()
                .ok_or("valid context SETUP: missing test")?;
            if test.name != "literal_7955_valid_context" {
                return Err(format!(
                    "{name}: parser SETUP found wrong test {}",
                    test.name
                ));
            }
            for (adapter, facts) in [
                ("lexical", lexical.as_slice()),
                ("parser", test.assertions.as_slice()),
            ] {
                if facts.len() != 1 {
                    failures.push(format!(
                        "{name}/{adapter}: wanted one nonempty fact: {facts:?}"
                    ));
                    continue;
                }
                let fact = &facts[0];
                let text = fact.text.split_whitespace().collect::<String>();
                if fact.line != 4
                    || fact.kind != OracleKind::BroadError
                    || fact.strength != OracleStrength::Weak
                    || !text.contains(identity)
                {
                    failures.push(format!(
                        "{name}/{adapter}: wrong literal line/grade/identity: {fact:?}"
                    ));
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("\n"))
        }
    }
}
