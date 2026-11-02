//! Parser-backed match-arm selection witness (#5499).
//!
//! Answers which candidate-current arm an exact selector chooses under supported
//! Rust match patterns. This module does not change exposure classes, activation,
//! or discrimination; it only produces a typed, fail-closed selection witness.

use super::{
    ArmSelection, EnclosingMatch, PatternHead, arm_pattern_text, declared_enum_variants,
    enclosing_match, find_fat_arrow, input_head, judge_alternatives, pattern_head, same_pattern,
    top_level_alternatives, without_comments,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::FunctionSummary;
use crate::domain::{Probe, ProbeFamily};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct MatchArmSelectorValue<'a> {
    pub(in crate::analysis) text: &'a str,
    pub(in crate::analysis) provenance: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct MatchArmIdentity {
    pub(in crate::analysis) line: usize,
    pub(in crate::analysis) pattern: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::analysis) struct MatchArmSelectionWitnessV1 {
    pub(in crate::analysis) schema_version: u16,
    pub(in crate::analysis) owner_file: String,
    pub(in crate::analysis) owner_name: String,
    pub(in crate::analysis) probe_line: usize,
    pub(in crate::analysis) candidate_arm: MatchArmIdentity,
    pub(in crate::analysis) scrutinee: String,
    pub(in crate::analysis) selector_text: String,
    pub(in crate::analysis) selector_provenance: String,
    pub(in crate::analysis) selected_arm: Option<MatchArmIdentity>,
    pub(in crate::analysis) source_order_index: Option<usize>,
    pub(in crate::analysis) source_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::analysis) enum MatchArmSelectionUnresolvedReason {
    LexicalFallback,
    StaleProbeIdentity,
    AmbiguousCandidateArm,
    ArmGuard,
    UnsupportedPattern,
    UnsupportedEarlierArm,
    IncompleteVariantAuthority,
    NoMatchingArm,
    NonMatchArmProbe,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub(in crate::analysis) enum MatchArmSelectionOutcome {
    SelectedCurrent {
        witness: MatchArmSelectionWitnessV1,
    },
    SelectedSibling {
        witness: MatchArmSelectionWitnessV1,
    },
    Unresolved {
        reason: MatchArmSelectionUnresolvedReason,
        witness: MatchArmSelectionWitnessV1,
    },
}

pub(in crate::analysis) fn match_arm_selection_witness_schema_version() -> u16 {
    SCHEMA_VERSION
}

pub(in crate::analysis) fn match_arm_selection_witness_linkage() {
    std::hint::black_box(evaluate_match_arm_selection);
    std::hint::black_box(match_arm_selection_source_digest);
}

/// Evaluate which arm an exact selector chooses for one candidate-current probe.
pub(in crate::analysis) fn evaluate_match_arm_selection(
    owner: &FunctionSummary,
    probe: &Probe,
    selector: MatchArmSelectorValue<'_>,
    source_digest: &str,
    lexical_fallback: bool,
) -> MatchArmSelectionOutcome {
    let candidate_pattern = arm_pattern_text(&probe.expression).unwrap_or_default();
    let base_witness = || MatchArmSelectionWitnessV1 {
        schema_version: SCHEMA_VERSION,
        owner_file: owner.file.display().to_string(),
        owner_name: owner.name.clone(),
        probe_line: probe.location.line,
        candidate_arm: MatchArmIdentity {
            line: probe.location.line,
            pattern: candidate_pattern.clone(),
        },
        scrutinee: String::new(),
        selector_text: selector.text.to_string(),
        selector_provenance: selector.provenance.to_string(),
        selected_arm: None,
        source_order_index: None,
        source_digest: source_digest.to_string(),
    };
    if probe.family != ProbeFamily::MatchArm {
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::NonMatchArmProbe,
            witness: base_witness(),
        };
    }
    if lexical_fallback {
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::LexicalFallback,
            witness: base_witness(),
        };
    }
    if candidate_pattern.is_empty() {
        if probe_expression_has_guard(&probe.expression) {
            return MatchArmSelectionOutcome::Unresolved {
                reason: MatchArmSelectionUnresolvedReason::ArmGuard,
                witness: base_witness(),
            };
        }
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::UnsupportedPattern,
            witness: base_witness(),
        };
    }
    if !candidate_arm_identity_valid(owner, probe, &candidate_pattern) {
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::StaleProbeIdentity,
            witness: base_witness(),
        };
    }
    let Some(enclosing) = enclosing_match(owner, probe.location.line, true) else {
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::AmbiguousCandidateArm,
            witness: base_witness(),
        };
    };
    let Some(arms) = parsed_arms(probe.location.line, &enclosing, &candidate_pattern) else {
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::AmbiguousCandidateArm,
            witness: base_witness(),
        };
    };
    if arms.iter().any(|arm| arm.guarded) {
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::ArmGuard,
            witness: base_witness(),
        };
    }
    let input = resolve_input_head(owner, &enclosing.scrutinee, selector.text);
    if input == PatternHead::Opaque {
        return MatchArmSelectionOutcome::Unresolved {
            reason: MatchArmSelectionUnresolvedReason::IncompleteVariantAuthority,
            witness: base_witness(),
        };
    }
    let mut witness = base_witness();
    witness.scrutinee = enclosing.scrutinee.clone();
    match select_arm_in_source_order(&arms, &input) {
        Ok(index) => {
            let selected = arms[index].identity.clone();
            witness.selected_arm = Some(selected.clone());
            witness.source_order_index = Some(index);
            if same_pattern(&selected.pattern, &candidate_pattern) {
                MatchArmSelectionOutcome::SelectedCurrent { witness }
            } else {
                MatchArmSelectionOutcome::SelectedSibling { witness }
            }
        }
        Err(reason) => MatchArmSelectionOutcome::Unresolved { reason, witness },
    }
}

struct ParsedArm {
    identity: MatchArmIdentity,
    alternatives: Vec<PatternHead>,
    guarded: bool,
}

fn candidate_arm_identity_valid(
    owner: &FunctionSummary,
    probe: &Probe,
    candidate_pattern: &str,
) -> bool {
    if probe
        .owner
        .as_ref()
        .is_some_and(|owner_id| owner_id != &owner.id)
    {
        return false;
    }
    if probe.after.is_none() {
        let Some(relative) = probe.location.line.checked_sub(owner.start_line) else {
            return false;
        };
        let Some(current) = owner.body.lines().nth(relative) else {
            return false;
        };
        return arm_pattern_text(current)
            .is_some_and(|current| same_pattern(&current, candidate_pattern));
    }
    true
}

fn parsed_arms(
    candidate_line: usize,
    enclosing: &EnclosingMatch,
    candidate_pattern: &str,
) -> Option<Vec<ParsedArm>> {
    let mut patterns = enclosing.earlier_patterns.clone();
    match &enclosing.arms_from_changed {
        Some(later) => patterns.extend(later.iter().map(|(pattern, _)| pattern.clone())),
        None => patterns.push(candidate_pattern.to_string()),
    }
    let candidate_index = enclosing.earlier_patterns.len();
    patterns
        .into_iter()
        .enumerate()
        .map(|(index, pattern)| {
            let line = candidate_line.saturating_sub(candidate_index.saturating_sub(index));
            let guarded = arm_has_guard(&pattern);
            let alternatives = if guarded {
                Vec::new()
            } else {
                top_level_alternatives(&pattern)?
                    .into_iter()
                    .map(pattern_head)
                    .collect::<Vec<_>>()
            };
            Some(ParsedArm {
                identity: MatchArmIdentity { line, pattern },
                alternatives,
                guarded,
            })
        })
        .collect()
}

fn probe_expression_has_guard(expression: &str) -> bool {
    let Some(arrow) = find_fat_arrow(expression) else {
        return false;
    };
    without_comments(&expression[..arrow])
        .map(|pattern| arm_has_guard(pattern.trim()))
        .unwrap_or(false)
}

fn arm_has_guard(pattern: &str) -> bool {
    let masked = mask_comments_and_strings(pattern);
    super::whole_word_offsets(&masked, "if")
        .into_iter()
        .any(|offset| {
            masked[offset..].starts_with("if ")
                || masked[offset..].starts_with("if(")
                || masked
                    .get(offset + 2..)
                    .is_some_and(|rest| rest.starts_with('('))
        })
}

fn resolve_input_head(owner: &FunctionSummary, scrutinee: &str, selector: &str) -> PatternHead {
    match input_head(selector) {
        PatternHead::UncasedVariant { name, .. } => {
            declared_enum_variants(&owner.body, scrutinee, &owner.name)
                .filter(|variants| variants.contains(&name))
                .map(|_| PatternHead::Variant {
                    name,
                    irrefutable: true,
                })
                .unwrap_or(PatternHead::Opaque)
        }
        head => head,
    }
}

fn select_arm_in_source_order(
    arms: &[ParsedArm],
    input: &PatternHead,
) -> Result<usize, MatchArmSelectionUnresolvedReason> {
    for (index, arm) in arms.iter().enumerate() {
        if arm.guarded {
            return Err(MatchArmSelectionUnresolvedReason::ArmGuard);
        }
        if arm.alternatives.is_empty() {
            return Err(MatchArmSelectionUnresolvedReason::UnsupportedPattern);
        }
        match judge_alternatives(&arm.alternatives, input) {
            ArmSelection::Selects => return Ok(index),
            ArmSelection::Unknown => {
                if arms[..index].iter().any(|earlier| {
                    judge_alternatives(&earlier.alternatives, input) == ArmSelection::Unknown
                }) {
                    return Err(MatchArmSelectionUnresolvedReason::UnsupportedEarlierArm);
                }
                return Err(MatchArmSelectionUnresolvedReason::UnsupportedPattern);
            }
            ArmSelection::SelectsOther => {}
        }
    }
    Err(MatchArmSelectionUnresolvedReason::NoMatchingArm)
}

pub(in crate::analysis) fn match_arm_selection_source_digest(source: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(source.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeltaKind, ProbeFamily, ProbeId, SourceLocation, SymbolId};

    fn owner(body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::reason".into()),
            name: "reason".into(),
            file: "src/lib.rs".into(),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.to_string().into(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: crate::analysis::facts::FunctionSourceRole::Production,
            attrs: Vec::new(),
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn match_probe(line: usize, expression: &str) -> Probe {
        Probe {
            id: ProbeId(format!("probe:src_lib_rs:{line}:match_arm")),
            location: SourceLocation::new("src/lib.rs", line, 1),
            owner: Some(SymbolId("src/lib.rs::reason".into())),
            family: ProbeFamily::MatchArm,
            delta: DeltaKind::Value,
            before: None,
            after: Some(expression.into()),
            expression: expression.into(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn evaluate(body: &str, line: usize, arm: &str, selector: &str) -> MatchArmSelectionOutcome {
        evaluate_match_arm_selection(
            &owner(body),
            &match_probe(line, arm),
            MatchArmSelectorValue {
                text: selector,
                provenance: "test",
            },
            &match_arm_selection_source_digest(body),
            false,
        )
    }

    #[test]
    fn match_arm_selection_witness_some5_selects_sibling_arm() {
        let body = "fn reason(x: Option<i32>) -> i32 {\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}";
        let outcome = evaluate(body, 4, "None => 0", "Some(5)");
        assert!(matches!(
            outcome,
            MatchArmSelectionOutcome::SelectedSibling { .. }
        ));
    }

    #[test]
    fn match_arm_selection_witness_none_selects_current_arm() {
        let body = "fn reason(x: Option<i32>) -> i32 {\n    match x {\n        Some(v) => v + 1,\n        None => 0,\n    }\n}";
        let outcome = evaluate(body, 4, "None => 0", "None");
        assert!(matches!(
            outcome,
            MatchArmSelectionOutcome::SelectedCurrent { .. }
        ));
    }

    #[test]
    fn match_arm_selection_witness_guard_fails_closed() {
        let body = "fn route(x: i32) -> i32 {\n    match x {\n        2 if x > 10 => 1,\n        2 => 2,\n        _ => 0,\n    }\n}";
        let outcome = evaluate(body, 3, "2 if x > 10 => 1", "2");
        assert!(matches!(
            outcome,
            MatchArmSelectionOutcome::Unresolved {
                reason: MatchArmSelectionUnresolvedReason::ArmGuard,
                ..
            }
        ));
    }

    #[test]
    fn match_arm_selection_witness_lexical_fallback_is_unresolved() {
        let body = "fn reason(x: Option<i32>) -> i32 {\n    match x {\n        None => 0,\n        Some(v) => v,\n    }\n}";
        let outcome = evaluate_match_arm_selection(
            &owner(body),
            &match_probe(3, "None => 0"),
            MatchArmSelectorValue {
                text: "None",
                provenance: "test",
            },
            &match_arm_selection_source_digest(body),
            true,
        );
        assert!(matches!(
            outcome,
            MatchArmSelectionOutcome::Unresolved {
                reason: MatchArmSelectionUnresolvedReason::LexicalFallback,
                ..
            }
        ));
    }

    #[test]
    fn match_arm_selection_witness_integer_literal_selects_matching_arm() {
        let body = "fn class(code: u16) -> u16 {\n    match code {\n        2 => 20,\n        3 => 30,\n        _ => 0,\n    }\n}";
        let outcome = evaluate(body, 3, "2 => 20", "2");
        assert!(matches!(
            outcome,
            MatchArmSelectionOutcome::SelectedCurrent { .. }
        ));
    }

    #[test]
    fn match_arm_selection_witness_tuple_match_decision_is_representable() {
        let body = "fn relation(a: bool, b: bool) -> bool {\n    match (a, b) {\n        (true, true) => true,\n        _ => false,\n    }\n}";
        let outcome = evaluate(body, 3, "(true, true) => true", "(true, true)");
        assert!(matches!(
            outcome,
            MatchArmSelectionOutcome::Unresolved { .. }
                | MatchArmSelectionOutcome::SelectedCurrent { .. }
        ));
    }
}
