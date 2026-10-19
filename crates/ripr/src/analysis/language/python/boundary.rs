//! Predicate boundary activation evidence for the Python preview adapter.
//!
//! A changed relational predicate (`qty > on_hand` -> `qty >= on_hand`) only
//! behaves differently when its operands are EQUAL. A strong exact-value
//! assertion that calls the owner away from that boundary (`reserve(10, 3)`)
//! still passes after the change, so reach plus a strong oracle does not
//! discriminate it. The same rule covers an equality-to-equality constant
//! change (`n == 11` -> `n == 10`): input 20 takes the else branch both ways.
//! `!=` and `is` stay outside this gate.
//!
//! This mirrors the Rust activation rule
//! (`analysis/classify/activation.rs::missing_boundary_discriminator`):
//!
//! - the rule engages only when a strong related test calls the owner with at
//!   least one literal argument (Rust returns early when `call_values` is
//!   empty), because only then does static evidence see concrete owner inputs;
//! - the boundary is observed when some engaged call binds both comparison
//!   operands (literals, or owner parameters bound to literal arguments or
//!   literal defaults) to equal values;
//! - otherwise the boundary is missing. An operand that is neither a literal
//!   nor a parameter (`item.on_hand`, a computed `len(items)`, a comprehension
//!   local) is unresolved and never counts as observed, so this fails closed.
//!
//! A module-scope constant (`DISCOUNT_THRESHOLD = 10_000`) that nothing in the
//! module can rebind resolves to its literal, like a literal operand; a test
//! argument that names the constant imported from the owner's module binds to
//! the same value (#4227, `module_constants.rs`).
//!
//! A parametrized test's call (`sign(x)` under `@pytest.mark.parametrize("x",
//! [5, -3])`) expands into one call per statically certain case, binding the
//! case's literal argvalue (#4559, `parametrize.rs`). Without the expansion the
//! call binds nothing and a boundary no case reaches would keep `exposed`.
//!
//! When no strong call binds a literal argument (`Formatter()({...})`,
//! `reserve(item, qty)` with test locals), the rule cannot see the inputs in
//! either direction; the verdict stays with the existing oracle rules and the
//! finding carries a named `boundary_activation: unresolved` limitation.

use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::discriminators::{is_literal_python_model_field_value, python_string_literal_value};
use super::module_constants::PythonModuleConstant;
use super::no_behavior::{
    call_arglists_with_offsets, call_segment_keyword_name, matching_call_paren,
    split_top_level_args,
};
use super::related_tests::{
    PythonRelatedCandidate, import_source_module_matches_owner, is_python_identifier_char,
    owner_module_callees, owner_module_paths, python_text_hides_code, test_body_binds_local,
};
use super::{PythonOwner, PythonTest};
use crate::domain::{OracleStrength, OwnerKind, ValueContext, ValueFact};
use std::collections::BTreeMap;

/// One strong related call to the owner with the literal value each bound
/// parameter takes (an explicit argument or a literal default).
pub(super) struct PythonOwnerCallValues {
    pub(super) line: usize,
    pub(super) text: String,
    pub(super) values: BTreeMap<String, String>,
}

/// The owner calls the boundary rule reads, for another activation rule
/// (`term_activation.rs`): the same strong rows, the same constants.
///
/// None when some strong related test calls the owner in a way no row
/// binds, so the rows do not cover every strong input.
pub(super) fn strong_owner_call_values(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<Vec<PythonOwnerCallValues>> {
    let constants = unrebound_constants(owner, related_candidates);
    let (rows, gaps) =
        strong_owner_call_rows_with_gaps(owner, &constants, related_candidates, focus);
    if gaps {
        return None;
    }
    let rows = rows
        .into_iter()
        .map(|row| PythonOwnerCallValues {
            line: row.line,
            text: row.text,
            values: row
                .bindings
                .into_iter()
                .map(|(parameter, binding)| (parameter, binding.value))
                .collect(),
        })
        .collect();
    Some(rows)
}

/// The owner's module constants no related test file reassigns
/// (`pricing.DISCOUNT_THRESHOLD = 5` can change one at runtime).
pub(super) fn unrebound_constants(
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
) -> Vec<PythonModuleConstant> {
    owner
        .module_constants
        .iter()
        .filter(|constant| {
            !related_candidates.iter().any(|candidate| {
                candidate
                    .test
                    .constant_rebinding
                    .assigns_attribute(&constant.name)
            })
        })
        .cloned()
        .collect()
}

/// Whether static evidence places a strong related call on the boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoundaryActivation {
    /// A strong related call binds both operands to equal literal values.
    Observed,
    /// Strong related calls bind literal owner inputs, but none places the
    /// operands on the boundary (or an operand cannot be resolved).
    Missing,
    /// No strong related call binds a literal owner input, so static evidence
    /// cannot see the activating inputs either way.
    Unresolved,
}

/// Boundary activation evidence for one changed relational predicate.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PythonBoundaryEvidence {
    pub(super) activation: BoundaryActivation,
    /// The equality discriminator that pins the changed boundary
    /// (`qty == item.on_hand`), named only when both operands are simple
    /// names or literals.
    pub(super) discriminator: Option<String>,
    /// Literal owner-call arguments (and the boundary equality, when observed)
    /// from strong related tests.
    pub(super) observed_values: Vec<ValueFact>,
    /// Why the boundary is observed, missing, or unresolved.
    pub(super) reason: String,
}

/// Boundary evidence for a changed Predicate line that contains a relational
/// comparison (`<`, `<=`, `>`, `>=`) or a same-operator equality (`==` ->
/// `==`). Returns None when the line has no such comparison (for example
/// `if flag:`, `if x != 5:`, or `if x is None:`), leaving the caller's
/// existing classification untouched. `relational_operators` still skips
/// `==`/`!=` so returned equalities stay return-value probes.
pub(super) fn python_boundary_evidence(
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<PythonBoundaryEvidence> {
    let condition = predicate_condition(line_text);
    let relational = relational_operators(&condition);
    let (operators, old_equality_operands) = if relational.is_empty() {
        let gate = equality_predicate_gate(&condition, old_line_text)?;
        (gate.operators, gate.old_operands)
    } else {
        (relational, None)
    };
    let operands = match operators.as_slice() {
        [(start, len)] => simple_comparison_operands(&condition, *start, *len),
        _ => None,
    };
    let discriminator = operands
        .as_ref()
        .map(|(left, right)| format!("{left} == {right}"));

    let constants = unrebound_constants(owner, related_candidates);
    let rows = strong_owner_call_rows(owner, &constants, related_candidates, focus);
    let mut observed_values: Vec<ValueFact> = rows
        .iter()
        .flat_map(|row| {
            row.bindings
                .iter()
                .filter(|(_, binding)| !binding.from_default)
                .map(|(parameter, binding)| ValueFact {
                    line: row.line,
                    text: row.text.clone(),
                    value: format!("{parameter} = {}", binding.value),
                    context: ValueContext::FunctionArgument,
                })
        })
        .collect();
    if observed_values.is_empty() {
        return Some(PythonBoundaryEvidence {
            activation: BoundaryActivation::Unresolved,
            discriminator,
            observed_values,
            reason: format!(
                "no strong related test call to `{}` binds a literal argument, so static evidence cannot place an input at the changed boundary of `{condition}`",
                owner.name
            ),
        });
    }

    let Some((left, right)) = operands else {
        let reason = if operators.len() > 1 {
            format!(
                "The changed predicate `{condition}` holds {} relational comparisons; static evidence cannot establish which boundary a related test call pins",
                operators.len()
            )
        } else {
            format!(
                "The changed predicate `{condition}` compares a computed operand; static evidence cannot establish that a related test call pins its boundary"
            )
        };
        return Some(PythonBoundaryEvidence {
            activation: BoundaryActivation::Missing,
            discriminator,
            observed_values,
            reason,
        });
    };

    let mut observed_new = false;
    let mut observed_old = false;
    let mut left_values = Vec::new();
    let mut right_values = Vec::new();
    for row in &rows {
        let left_value = resolve_operand(&left, row, &constants);
        let right_value = resolve_operand(&right, row, &constants);
        if let (Some(left_value), Some(right_value)) = (&left_value, &right_value)
            && literals_equal(left_value, right_value)
        {
            observed_new = true;
            observed_values.push(ValueFact {
                line: row.line,
                text: format!(
                    "{} | {}; {}",
                    row.text,
                    operand_provenance(&left, left_value, &constants),
                    operand_provenance(&right, right_value, &constants)
                ),
                value: format!("{left} == {right}"),
                context: ValueContext::FunctionArgument,
            });
        }
        left_values.extend(left_value);
        right_values.extend(right_value);
        if let Some((old_left, old_right)) = &old_equality_operands {
            let old_left_value = resolve_operand(old_left, row, &constants);
            let old_right_value = resolve_operand(old_right, row, &constants);
            if let (Some(old_left_value), Some(old_right_value)) =
                (&old_left_value, &old_right_value)
                && literals_equal(old_left_value, old_right_value)
            {
                observed_old = true;
                observed_values.push(ValueFact {
                    line: row.line,
                    text: format!(
                        "{} | {}; {}",
                        row.text,
                        operand_provenance(old_left, old_left_value, &constants),
                        operand_provenance(old_right, old_right_value, &constants)
                    ),
                    value: format!("{old_left} == {old_right}"),
                    context: ValueContext::FunctionArgument,
                });
            }
        }
    }
    let observed = observed_new || observed_old;
    observed_values.sort_by(|a, b| {
        a.line
            .cmp(&b.line)
            .then(a.value.cmp(&b.value))
            .then(a.text.cmp(&b.text))
    });
    observed_values.dedup();

    // An operand that no strong call resolves (`item.on_hand`, a test local)
    // leaves the boundary unknown rather than missed: the test input may sit
    // on it at runtime (`reserve(Item("a", 3), 3)`). The finding still fails
    // closed to weak, but the equality is not handed out as a repair target,
    // so no repair card asks for a test that may already exist.
    let unresolved_operand = [(&left, &left_values), (&right, &right_values)]
        .into_iter()
        .any(|(operand, values)| {
            static_operand(operand, &constants).is_none() && values.is_empty()
        });
    let discriminator = if observed || !unresolved_operand {
        discriminator
    } else {
        None
    };
    let reason = if observed_new {
        format!("A strong related test call places {left} equal to {right}")
    } else if let Some((old_left, old_right)) = &old_equality_operands
        && observed_old
    {
        format!("A strong related test call places {old_left} equal to {old_right}")
    } else {
        // A literal operand is its own value and a module constant names its
        // one value; only operands that vary with the test input are listed.
        let observed_lists = [(&left, left_values), (&right, right_values)]
            .into_iter()
            .filter(|(operand, _)| static_operand(operand, &constants).is_none())
            .map(|(operand, values)| {
                format!("observed {operand} values: {}", list_or_unresolved(values))
            })
            .collect::<Vec<_>>();
        let constants = [&left, &right]
            .into_iter()
            .filter_map(|operand| module_constant(operand, &constants))
            .map(|constant| format!(" (module constant {})", constant_provenance(constant)))
            .collect::<String>();
        let place = match &old_equality_operands {
            Some((old_left, old_right)) if *old_left != left || *old_right != right => {
                format!("{left} equal to {right} or {old_left} equal to {old_right}")
            }
            _ => format!("{left} equal to {right}"),
        };
        format!(
            "No strong related test call places {place}{constants}; {}",
            observed_lists.join("; ")
        )
    };
    Some(PythonBoundaryEvidence {
        activation: if observed {
            BoundaryActivation::Observed
        } else {
            BoundaryActivation::Missing
        },
        discriminator,
        observed_values,
        reason,
    })
}

fn operand_provenance(operand: &str, value: &str, constants: &[PythonModuleConstant]) -> String {
    if literal_value(operand).is_some() {
        format!("literal operand {operand}")
    } else if let Some(constant) = module_constant(operand, constants) {
        format!("module constant {}", constant_provenance(constant))
    } else {
        format!("{operand} = {value}")
    }
}

fn constant_provenance(constant: &PythonModuleConstant) -> String {
    format!(
        "{} = {} at line {}",
        constant.name, constant.value, constant.line
    )
}

/// The module constant a comparison operand names, when the owner sees one.
fn module_constant<'a>(
    operand: &str,
    constants: &'a [PythonModuleConstant],
) -> Option<&'a PythonModuleConstant> {
    constants.iter().find(|constant| constant.name == operand)
}

/// An operand's value independent of any test call: its own literal, or the
/// literal of the module constant it names.
pub(super) fn static_operand(operand: &str, constants: &[PythonModuleConstant]) -> Option<String> {
    if let Some((base, offset)) = integer_offset(operand) {
        return static_operand(base, constants).and_then(|value| shifted_integer(&value, offset));
    }
    literal_value(operand)
        .or_else(|| module_constant(operand, constants).map(|constant| constant.value.clone()))
}

fn list_or_unresolved(mut values: Vec<String>) -> String {
    values.sort();
    values.dedup();
    if values.is_empty() {
        "unresolved".to_string()
    } else {
        values.join(", ")
    }
}

/// The boolean condition of a changed predicate line: the control prefix
/// (`if`/`elif`/`while`) and trailing `:` stripped, or the condition between
/// ` if ` and ` else ` of a conditional expression.
fn predicate_condition(line_text: &str) -> String {
    if let Some(condition) = python_return_comparison(line_text) {
        return condition.to_string();
    }
    let text = line_text.trim().trim_end_matches(':').trim();
    for prefix in ["if ", "elif ", "while "] {
        if let Some(stripped) = text.strip_prefix(prefix) {
            return stripped.trim().to_string();
        }
    }
    if let Some((_, after_if)) = text.split_once(" if ")
        && let Some((condition, _)) = after_if.split_once(" else ")
    {
        return condition.trim().to_string();
    }
    text.to_string()
}

/// The comparison of a `return <a> <op> <b>` line whose returned expression
/// is exactly one relational comparison between simple operands. The
/// returned bool changes only at the boundary, so the line is a predicate:
/// an exact oracle on an input far from `a == b` (`is_large(500) == True`)
/// returns the same value before and after a `>=` -> `>` change.
pub(super) fn python_return_comparison(line_text: &str) -> Option<&str> {
    let expression = line_text.trim().strip_prefix("return ")?.trim();
    let [(start, len)] = relational_operators(expression)[..] else {
        return None;
    };
    let (left, right) = simple_comparison_operands(expression, start, len)?;
    // An offset operand (`total + 1 > limit`) is a boundary inside an `if`,
    // but a returned arithmetic expression keeps its return-value family.
    let whole_sides = is_simple_operand(&left)
        && is_simple_operand(&right)
        && expression.get(..start)?.trim() == left
        && expression.get(start + len..)?.trim() == right;
    whole_sides.then_some(expression)
}

/// Byte offset and length of every relational comparison operator outside
/// string literals. Shift (`<<`, `>>`), arrow (`->`) and comparison-equality
/// (`==`, `!=`) operators are not relational boundaries.
fn relational_operators(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut operators = Vec::new();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut idx = 0usize;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if escaped {
            escaped = false;
            idx += 1;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            idx += 1;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            idx += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'#' => break,
            b'<' | b'>' => {
                let next = bytes.get(idx + 1).copied();
                let prev = idx.checked_sub(1).map(|p| bytes[p]);
                if next == Some(byte) || prev == Some(b'-') {
                    // `<<` / `>>` shift (and `<<=` / `>>=`), or the `->` arrow.
                    idx += 2;
                    continue;
                }
                let len = if next == Some(b'=') { 2 } else { 1 };
                operators.push((idx, len));
                idx += len;
                continue;
            }
            _ => {}
        }
        idx += 1;
    }
    operators
}

struct EqualityGate {
    operators: Vec<(usize, usize)>,
    old_operands: Option<(String, String)>,
}

/// `==` -> `==` only. Mixed old forms (`!=`, `is`, relational) stay outside
/// the gate so complementary mutations keep today's verdict.
fn equality_predicate_gate(condition: &str, old_line_text: Option<&str>) -> Option<EqualityGate> {
    if contains_ungated_identity_or_inequality(condition) {
        return None;
    }
    let operators = equality_operators(condition);
    if operators.len() != 1 {
        return None;
    }
    // `==` -> `==` only: an added line has no old equality to compare.
    let old = old_line_text?;
    let old_condition = predicate_condition(old);
    if contains_ungated_identity_or_inequality(&old_condition)
        || !relational_operators(&old_condition).is_empty()
    {
        return None;
    }
    let old_eq = equality_operators(&old_condition);
    let [(start, len)] = old_eq.as_slice() else {
        return None;
    };
    let old_operands = simple_comparison_operands(&old_condition, *start, *len);
    Some(EqualityGate {
        operators,
        old_operands,
    })
}

/// Byte offset and length of every `==` comparison outside string literals.
/// `!=` is not an equality boundary (handle `==` only).
fn equality_operators(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut operators = Vec::new();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut idx = 0usize;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if escaped {
            escaped = false;
            idx += 1;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            idx += 1;
            continue;
        }
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            idx += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'#' => break,
            b'=' => {
                let next = bytes.get(idx + 1).copied();
                let prev = idx.checked_sub(1).map(|p| bytes[p]);
                if next == Some(b'=') && prev != Some(b'!') && prev != Some(b'=') {
                    operators.push((idx, 2));
                    idx += 2;
                    continue;
                }
            }
            _ => {}
        }
        idx += 1;
    }
    operators
}

fn contains_ungated_identity_or_inequality(text: &str) -> bool {
    live_token(text, "!=") || live_keyword(text, "is")
}

fn live_token(text: &str, token: &str) -> bool {
    text.match_indices(token)
        .any(|(idx, _)| !python_text_hides_code(text, idx))
}

fn live_keyword(text: &str, keyword: &str) -> bool {
    text.match_indices(keyword).any(|(idx, _)| {
        let end = idx + keyword.len();
        !python_text_hides_code(text, idx)
            && !text[..idx]
                .chars()
                .next_back()
                .is_some_and(is_python_identifier_char)
            && !text[end..]
                .chars()
                .next()
                .is_some_and(is_python_identifier_char)
    })
}

/// The two operands of the only relational comparison in `condition`, when
/// both are complete simple operands: a name, dotted attribute path, or
/// scalar literal. Returns None for a computed operand (`len(items)`,
/// `a + b`, `values[0]`), so the caller never binds a partial operand.
pub(super) fn simple_comparison_operands(
    condition: &str,
    operator_start: usize,
    operator_len: usize,
) -> Option<(String, String)> {
    let left_side = condition.get(..operator_start)?.trim_end();
    let right_side = condition.get(operator_start + operator_len..)?.trim_start();

    let left_start = trailing_operand_start(left_side);
    let before_left = left_side[..left_start].trim_end();

    let mut left_start = left_start;
    let mut before_left = before_left;
    // `C - 1 < x`: a trailing integer literal may close a `base +/- N` operand.
    if let Some(extended) = leading_offset_start(left_side, left_start) {
        left_start = extended;
        before_left = left_side[..left_start].trim_end();
    }
    let left = &left_side[left_start..];
    let left_boundary_ok = before_left.is_empty()
        || before_left.ends_with('(')
        || ["not", "and", "or", "if"]
            .into_iter()
            .any(|keyword| ends_with_keyword(before_left, keyword));

    let mut right_end = leading_operand_end(right_side);
    // `x > C - 1`: a simple operand followed by `+ N` or `- N`.
    if let Some(extended) = trailing_offset_end(right_side, right_end) {
        right_end = extended;
    }
    let right = &right_side[..right_end];
    let after_right = right_side[right_end..].trim_start();
    let right_boundary_ok = after_right.is_empty()
        || after_right.starts_with(')')
        || ["and", "or", "for", "if", "else"]
            .into_iter()
            .any(|keyword| starts_with_keyword(after_right, keyword));

    let operand_ok = |operand: &str| {
        is_simple_operand(operand)
            || integer_offset(operand).is_some_and(|(base, _)| is_simple_operand(base))
    };
    (left_boundary_ok && right_boundary_ok && operand_ok(left) && operand_ok(right))
        .then(|| (left.to_string(), right.to_string()))
}

/// A `base + N` / `base - N` operand with an integer literal offset, the
/// rewritten form of an inclusive bound (`x >= C` as `x > C - 1`). Returns the
/// base text and the signed offset.
fn integer_offset(operand: &str) -> Option<(&str, i128)> {
    let idx = operand.rfind(['+', '-'])?;
    let base = operand[..idx].trim_end();
    let digits = operand[idx + 1..].trim_start();
    if base.is_empty() || digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let magnitude: i128 = digits.parse().ok()?;
    let offset = if operand[idx..].starts_with('-') {
        -magnitude
    } else {
        magnitude
    };
    Some((base, offset))
}

/// End of `text[..end]` extended over a following `+ N` / `- N` integer offset.
fn trailing_offset_end(text: &str, end: usize) -> Option<usize> {
    let rest = &text[end..];
    let after_space = rest.trim_start();
    let sign = after_space
        .chars()
        .next()
        .filter(|ch| matches!(ch, '+' | '-'))?;
    let digits = after_space[sign.len_utf8()..].trim_start();
    let digit_len = digits.chars().take_while(char::is_ascii_digit).count();
    if digit_len == 0 || digits[digit_len..].starts_with(|ch: char| is_operand_char(ch)) {
        return None;
    }
    Some(text.len() - digits.len() + digit_len)
}

/// Start of a `base + N` / `base - N` operand whose integer literal begins at
/// `start` in `text`, when the text before it is `base +` or `base -`.
fn leading_offset_start(text: &str, start: usize) -> Option<usize> {
    let literal = &text[start..];
    if literal.is_empty() || !literal.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let before = text[..start].trim_end();
    let sign = before
        .chars()
        .next_back()
        .filter(|ch| matches!(ch, '+' | '-'))?;
    let before_sign = before[..before.len() - sign.len_utf8()].trim_end();
    let base_start = trailing_operand_start(before_sign);
    let base = &before_sign[base_start..];
    is_simple_operand(base).then_some(base_start)
}

/// An integer value shifted by `offset`; None for any non-integer value.
fn shifted_integer(value: &str, offset: i128) -> Option<String> {
    let canonical = canonical_decimal(value)?;
    let integer: i128 = canonical.parse().ok()?;
    Some(integer.checked_add(offset)?.to_string())
}

/// Byte offset where the operand that ends `text` starts: a quoted string
/// literal, or a run of name/number/attribute characters.
fn trailing_operand_start(text: &str) -> usize {
    if let Some(quote) = text
        .chars()
        .next_back()
        .filter(|ch| matches!(ch, '"' | '\''))
    {
        let body_end = text.len() - quote.len_utf8();
        return text[..body_end].rfind(quote).unwrap_or(body_end);
    }
    text.char_indices()
        .rev()
        .find(|(_, ch)| !is_operand_char(*ch))
        .map_or(0, |(idx, ch)| idx + ch.len_utf8())
}

/// Byte offset where the operand that starts `text` ends: a quoted string
/// literal, an optionally negative number, or a run of name/attribute
/// characters.
fn leading_operand_end(text: &str) -> usize {
    if let Some(quote) = text.chars().next().filter(|ch| matches!(ch, '"' | '\'')) {
        let body_start = quote.len_utf8();
        return text[body_start..]
            .find(quote)
            .map_or(text.len(), |idx| body_start + idx + quote.len_utf8());
    }
    let sign =
        usize::from(text.starts_with('-') && text[1..].starts_with(|ch: char| ch.is_ascii_digit()));
    text[sign..]
        .char_indices()
        .find(|(_, ch)| !is_operand_char(*ch))
        .map_or(text.len(), |(idx, _)| sign + idx)
}

fn is_operand_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'
}

fn is_simple_operand(operand: &str) -> bool {
    let unsigned = operand.strip_prefix('-').unwrap_or(operand);
    !unsigned.is_empty()
        && !unsigned.starts_with('.')
        && !unsigned.ends_with('.')
        && (literal_value(operand).is_some()
            || unsigned
                .split('.')
                .all(super::static_limits::is_simple_python_identifier))
}

fn ends_with_keyword(text: &str, keyword: &str) -> bool {
    text.strip_suffix(keyword).is_some_and(|before| {
        before.is_empty() || before.ends_with(|ch: char| ch.is_whitespace() || ch == '(')
    })
}

fn starts_with_keyword(text: &str, keyword: &str) -> bool {
    text.strip_prefix(keyword)
        .is_some_and(|after| after.is_empty() || after.starts_with(char::is_whitespace))
}

/// Owner parameters paired with the parametrize argname their argument names.
type CaseArguments = Vec<(String, String)>;

/// A literal binding for one owner parameter in one test call.
#[derive(Clone, Debug)]
pub(super) struct Binding {
    pub(super) value: String,
    pub(super) from_default: bool,
}

/// One analyzable call to the owner from a strong related test.
#[derive(Clone, Debug)]
pub(super) struct CallRow {
    pub(super) line: usize,
    pub(super) text: String,
    pub(super) bindings: BTreeMap<String, Binding>,
}

/// Owner calls from related tests whose strongest assertion is strong: the
/// same tests that could credit `exposed`. Each analyzable call binds owner
/// parameters to literal arguments (or literal defaults when omitted).
pub(super) fn strong_owner_call_rows(
    owner: &PythonOwner,
    constants: &[PythonModuleConstant],
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Vec<CallRow> {
    strong_owner_call_rows_with_gaps(owner, constants, related_candidates, focus).0
}

/// The strong owner-call rows, and whether some strong related test has an
/// owner call that binds no row (a `*args` unpack, a comment inside the
/// argument list) or no owner call this reader recognizes at all. A rule
/// that needs every strong input (`term_activation.rs`, `guard_activation.rs`)
/// treats that as unresolved; the boundary rule reads the rows it has.
pub(super) fn strong_owner_call_rows_with_gaps(
    owner: &PythonOwner,
    constants: &[PythonModuleConstant],
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> (Vec<CallRow>, bool) {
    let (method_call, skip) = match owner.owner_kind {
        Some(OwnerKind::Method) => (true, 1),
        Some(OwnerKind::ClassMethod) => (
            true,
            usize::from(
                !owner
                    .decorators
                    .iter()
                    .any(|decorator| decorator.ends_with("staticmethod")),
            ),
        ),
        Some(OwnerKind::Function) => (false, 0),
        _ => return (Vec::new(), true),
    };
    let mut rows = Vec::new();
    let mut gaps = false;
    for candidate in related_candidates {
        if !candidate.relation.uses_oracle() {
            continue;
        }
        // A strong row is one whose family-selected assertion (#5572) is
        // strong: the same assertion the finding's row displays.
        let strong = select_relevant_assertion(&candidate.test.assertions, Some(focus))
            .assertion()
            .is_some_and(|assertion| {
                assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
            });
        if !strong {
            continue;
        }
        // The test body, plus the call each callable-form exception assertion
        // makes (RIPR-SPEC-0238 rule 13: `assertRaisesRegex(E, 'm', owner, 20)`
        // calls `owner(20)`).
        let mut sources = vec![(candidate.test.line, candidate.test.body_text.as_str())];
        sources.extend(candidate.test.assertions.iter().filter_map(|assertion| {
            assertion
                .callable_call
                .as_deref()
                .map(|call| (assertion.line, call))
        }));
        let mut candidate_rows = 0usize;
        for name in owner_call_names(owner, candidate.test, method_call) {
            for (first_line, text) in &sources {
                for (offset, arglist) in call_arglists_with_offsets(text, &name, method_call) {
                    let Some((bindings, case_arguments)) =
                        bind_call_arguments(owner, constants, candidate.test, skip, arglist)
                    else {
                        gaps = true;
                        continue;
                    };
                    candidate_rows += 1;
                    let line = first_line + text[..offset].matches('\n').count();
                    // The call runs from its name to its closing parenthesis,
                    // the byte after the arglist; whitespace between the name
                    // and `(` is part of it.
                    let arglist_end =
                        arglist.as_ptr().addr() - text.as_ptr().addr() + arglist.len();
                    let text = call_line_text(text, offset, arglist_end + 1 - offset);
                    match (&candidate.test.parametrize, case_arguments.is_empty()) {
                        (Some(parametrize), false) => {
                            rows.extend(parametrize.cases.iter().map(|case| {
                                parametrize_case_row(line, &text, &bindings, &case_arguments, case)
                            }));
                        }
                        _ => rows.push(CallRow {
                            line,
                            text,
                            bindings,
                        }),
                    }
                }
            }
        }
        gaps |= candidate_rows == 0;
    }
    (rows, gaps)
}

/// The owner's own name plus any `from M import owner as alias` alias and
/// module-qualified spelling (`utils.sign`, #4567); a method is only called
/// through its own attribute name. Module-qualified calls must bind here too,
/// or a call the alignment credits would skip the boundary gate.
fn owner_call_names(owner: &PythonOwner, test: &PythonTest, method_call: bool) -> Vec<String> {
    let mut names = vec![owner.name.clone()];
    if !method_call {
        names.extend(
            test.imports
                .iter()
                .filter(|import| import.imported == owner.name && import.alias != owner.name)
                .map(|import| import.alias.clone()),
        );
        names.extend(
            owner_module_callees(test, owner)
                .into_iter()
                .filter(|callee| callee.contains('.')),
        );
    }
    names.sort();
    names.dedup();
    names
}

/// The longest source excerpt, in bytes, a call row keeps as evidence text.
/// A row is kept per owner call and its text is cloned into every value fact
/// of that call, so an unbounded physical line (`assert f(0) == 1 and f(1) == 1
/// and ...`) would cost O(calls x line length) memory. An ordinary line is
/// shown whole; only a longer one is cut to a window of at most this many
/// bytes around the call (plus up to six bytes of `...` cut markers).
const CALL_ROW_TEXT_LIMIT: usize = 240;
/// How much of a cut line is kept before the call; the rest follows it.
const CALL_ROW_TEXT_LEAD: usize = 60;

fn call_line_text(body: &str, offset: usize, call_len: usize) -> String {
    let start = body[..offset].rfind('\n').map_or(0, |idx| idx + 1);
    let end = body[offset..]
        .find('\n')
        .map_or(body.len(), |idx| offset + idx);
    let line = body[start..end].trim();
    if line.len() <= CALL_ROW_TEXT_LIMIT {
        return line.to_string();
    }
    // Cut to a window that starts a little before the call, on character
    // boundaries, and mark each cut side.
    // A call that fits the window is kept whole: the lead shrinks so its
    // closing parenthesis stays inside.
    let lead = CALL_ROW_TEXT_LEAD.min(CALL_ROW_TEXT_LIMIT.saturating_sub(call_len));
    // Round the start forward (toward the call) so the window never loses
    // trailing bytes of a call that fits it.
    let mut from = offset.saturating_sub(lead).max(start);
    while !body.is_char_boundary(from) {
        from += 1;
    }
    let mut to = (from + CALL_ROW_TEXT_LIMIT).min(end);
    while !body.is_char_boundary(to) {
        to -= 1;
    }
    let window = body[from..to].trim();
    format!(
        "{}{window}{}",
        // A marker only where visible text was cut, not bare indentation or
        // trailing whitespace.
        if body[start..from].trim().is_empty() {
            ""
        } else {
            "..."
        },
        if body[to..end].trim().is_empty() {
            ""
        } else {
            "..."
        }
    )
}

#[cfg(test)]
mod call_line_text_tests {
    use super::{CALL_ROW_TEXT_LIMIT, call_line_text};

    #[test]
    fn indentation_only_cut_emits_no_leading_marker() {
        let body = format!("{}price(1) {}", " ".repeat(70), "x".repeat(250));
        let text = call_line_text(&body, 70, "price(1)".len());
        assert!(text.starts_with("price(1)"), "{text}");
        assert!(text.ends_with("..."), "{text}");
    }

    #[test]
    fn cut_content_still_emits_leading_marker() {
        let body = format!("{} price(1) {}", "y".repeat(100), "x".repeat(250));
        let text = call_line_text(&body, 101, "price(1)".len());
        assert!(text.starts_with("..."), "{text}");
    }

    #[test]
    fn multibyte_start_keeps_the_whole_call() {
        // The lead lands inside a 3-byte char; a 238-byte call must still end
        // with its closing parenthesis.
        let call = format!("price({})", "a".repeat(238 - "price()".len()));
        assert_eq!(call.len(), 238);
        for pad in 0..4 {
            let body = format!(
                "{}{}{call} {}",
                "x".repeat(pad),
                "\u{20ac}".repeat(30),
                "z".repeat(50)
            );
            let offset = body.find("price").unwrap_or(0);
            let text = call_line_text(&body, offset, call.len());
            assert!(text.contains(&call), "pad {pad}: {text}");
        }
    }

    #[test]
    fn exact_limit_line_is_whole_and_one_over_is_cut() {
        let call = "price(1)";
        let whole = format!("{call}{}", "x".repeat(CALL_ROW_TEXT_LIMIT - call.len()));
        assert_eq!(whole.len(), CALL_ROW_TEXT_LIMIT);
        assert_eq!(call_line_text(&whole, 0, call.len()), whole);
        let over = format!("{whole}x");
        let cut = call_line_text(&over, 0, call.len());
        assert!(cut.starts_with(call) && cut.ends_with("..."), "{cut}");
        assert!(!cut.contains(&over));
    }

    #[test]
    fn exact_call_length_with_space_before_paren_keeps_the_closing_paren() {
        let spaces = " ".repeat(30);
        let argument = "a".repeat(200);
        let body = format!(
            "{}price{spaces}({argument}) tail {}",
            "p".repeat(80),
            "t".repeat(60)
        );
        let offset = body.find("price").unwrap_or(0);
        let call_len = "price".len() + spaces.len() + argument.len() + 2;
        let text = call_line_text(&body, offset, call_len);
        assert!(text.contains(&format!("({argument})")), "{text}");
    }
}

/// Bind a call's literal arguments to owner parameters. Positional arguments
/// bind in declaration order after `skip` implicit receivers; keyword
/// arguments bind by name; an omitted parameter with a literal default binds
/// to that default. An argument naming an owner-module constant the test
/// imports binds to that constant's literal. Returns None for a `*args` /
/// `**kwargs` unpack, whose binding is undecidable. Other non-literal
/// arguments stay unbound (unresolved).
///
/// The second value pairs each owner parameter whose argument is a bare
/// parametrize argname of the test (`sign(x)` under `parametrize("x", ...)`)
/// with that argname, for per-case expansion (#4559). An argname the test body
/// rebinds is not paired.
fn bind_call_arguments(
    owner: &PythonOwner,
    constants: &[PythonModuleConstant],
    test: &PythonTest,
    skip: usize,
    arglist: &str,
) -> Option<(BTreeMap<String, Binding>, CaseArguments)> {
    let positional: Vec<&str> = owner
        .parameters
        .iter()
        .filter(|parameter| !parameter.keyword_only)
        .skip(skip)
        .map(|parameter| parameter.name.as_str())
        .collect();
    let mut bound_names = Vec::new();
    let mut bindings = BTreeMap::new();
    let mut case_arguments = Vec::new();
    let mut position = 0usize;
    for segment in split_top_level_args(arglist) {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        if segment.starts_with('*') || segment.contains('#') {
            return None;
        }
        let (name, value) = match call_segment_keyword_name(segment) {
            Some(name) => {
                let value = segment
                    .split_once('=')
                    .map(|(_, value)| value.trim())
                    .unwrap_or_default();
                (Some(name.to_string()), value)
            }
            None => {
                let name = positional.get(position).map(|name| (*name).to_string());
                position += 1;
                (name, segment)
            }
        };
        let Some(name) = name else {
            continue;
        };
        bound_names.push(name.clone());
        if let Some(value) =
            literal_value(value).or_else(|| imported_constant_value(owner, constants, test, value))
        {
            bindings.insert(
                name,
                Binding {
                    value,
                    from_default: false,
                },
            );
        } else if test
            .parametrize
            .as_ref()
            .is_some_and(|parametrize| parametrize.binds(value))
            && !test_body_binds_local(test, value)
            && !case_name_may_be_rebound(&test.body_text, value)
        {
            case_arguments.push((name, value.to_string()));
        }
    }
    for parameter in owner.parameters.iter().skip(skip) {
        if bound_names.contains(&parameter.name) {
            continue;
        }
        if let Some(value) = parameter.default.as_deref().and_then(literal_value) {
            bindings.insert(
                parameter.name.clone(),
                Binding {
                    value,
                    from_default: true,
                },
            );
        }
    }
    Some((bindings, case_arguments))
}

/// One call row per parametrize case: each paired owner parameter binds the
/// case's argvalue when it is a literal and stays unresolved otherwise.
fn parametrize_case_row(
    line: usize,
    text: &str,
    bindings: &BTreeMap<String, Binding>,
    case_arguments: &[(String, String)],
    case: &BTreeMap<String, String>,
) -> CallRow {
    let mut bindings = bindings.clone();
    let mut shown = Vec::new();
    for (parameter, argname) in case_arguments {
        let Some(value) = case.get(argname) else {
            continue;
        };
        shown.push(format!("{argname}={value}"));
        if let Some(value) = literal_value(value) {
            bindings.insert(
                parameter.clone(),
                Binding {
                    value,
                    from_default: false,
                },
            );
        }
    }
    CallRow {
        line,
        text: format!("{text} [parametrize {}]", shown.join(", ")),
        bindings,
    }
}

/// The literal of an owner-module constant that a test-call argument names
/// through the test's own import of it: `DISCOUNT_THRESHOLD` or an alias after
/// `from pricing import DISCOUNT_THRESHOLD [as T]`, or `pricing.DISCOUNT_THRESHOLD`
/// after `import pricing` / `from pkg import pricing`. The argument is then the
/// boundary value by identity. A name the test did not import from the
/// owner's module, or that the test or its module rebinds, stays unresolved.
fn imported_constant_value(
    owner: &PythonOwner,
    constants: &[PythonModuleConstant],
    test: &PythonTest,
    argument: &str,
) -> Option<String> {
    let owner_modules = owner_module_paths(&owner.file);
    let constant = constants.iter().find(|constant| {
        test.imports.iter().any(|import| {
            if test.constant_rebinding.rebinds(&import.alias) {
                return false;
            }
            if import.alias == argument {
                return import.imported == constant.name
                    && import_source_module_matches_owner(import, owner, &test.file);
            }
            let module = if import.source_module.is_empty() {
                import.imported.clone()
            } else {
                format!("{}.{}", import.source_module, import.imported)
            };
            argument
                .strip_prefix(import.alias.as_str())
                .and_then(|rest| rest.strip_prefix('.'))
                .is_some_and(|name| name == constant.name)
                && owner_modules.contains(&module)
        })
    })?;
    Some(constant.value.clone())
}

/// A comparison operand's value in one call row: its own literal, the literal
/// of the module constant it names, or the literal bound to the owner parameter
/// it names. Anything else is unresolved.
pub(super) fn resolve_operand(
    operand: &str,
    row: &CallRow,
    constants: &[PythonModuleConstant],
) -> Option<String> {
    if let Some((base, offset)) = integer_offset(operand) {
        return resolve_operand(base, row, constants)
            .and_then(|value| shifted_integer(&value, offset));
    }
    static_operand(operand, constants)
        .or_else(|| row.bindings.get(operand).map(|b| b.value.clone()))
}

/// The canonical text of a scalar Python literal (number with `_` separators
/// removed, string, `True`/`False`/`None`), or None for anything else.
pub(super) fn literal_value(text: &str) -> Option<String> {
    let text = text.trim();
    let without_separators: String = text.chars().filter(|ch| *ch != '_').collect();
    let numeric = !without_separators.is_empty()
        && without_separators
            .chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '-');
    if numeric && is_literal_python_model_field_value(&without_separators) {
        return Some(without_separators);
    }
    if is_literal_python_model_field_value(text) {
        return Some(text.to_string());
    }
    None
}

/// Literal equality as Python compares the scalar values: decimals by value
/// (`100 == 100.0`), strings by content regardless of quote style, and
/// `True`/`False`/`None` by identity. Mixed kinds never compare equal here
/// (for example `True == 1`), which fails closed toward "not observed".
pub(super) fn literals_equal(left: &str, right: &str) -> bool {
    match (canonical_decimal(left), canonical_decimal(right)) {
        (Some(left), Some(right)) => return left == right,
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }
    match (
        python_string_literal_value(left),
        python_string_literal_value(right),
    ) {
        (Some(left), Some(right)) => left == right,
        (None, None) => left == right,
        _ => false,
    }
}

/// Canonical text of a plain decimal literal so values Python compares equal
/// render identically: `100`, `100.0` and `0100.00` all become `100`, and
/// `-0.0` becomes `0`. None for anything that is not a plain decimal.
pub(super) fn canonical_decimal(text: &str) -> Option<String> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (integer, fraction) = digits.split_once('.').unwrap_or((digits, ""));
    if integer.is_empty() && fraction.is_empty()
        || !integer
            .chars()
            .chain(fraction.chars())
            .all(|ch| ch.is_ascii_digit())
    {
        return None;
    }
    let integer = integer.trim_start_matches('0');
    let fraction = fraction.trim_end_matches('0');
    let magnitude = match (integer.is_empty(), fraction.is_empty()) {
        (true, true) => return Some("0".to_string()),
        (false, true) => integer.to_string(),
        (true, false) => format!("0.{fraction}"),
        (false, false) => format!("{integer}.{fraction}"),
    };
    Some(if negative {
        format!("-{magnitude}")
    } else {
        magnitude
    })
}

/// Whether a parametrize argname may name something other than the case
/// value where the owner call reads it, beyond the statement bindings the
/// recorded cases already exclude (`parametrize.rs`, #4612 review): a
/// `lambda` may shadow it with a parameter, and a comprehension's `for`
/// target may name it. Textual and fail-closed: any live `lambda`, or any
/// live `for ... in` whose target (across lines) mentions the name, counts.
fn case_name_may_be_rebound(test_text: &str, name: &str) -> bool {
    let Some(body) = test_body_after_header(test_text) else {
        return true;
    };
    let live_words = |word: &str| -> Vec<usize> {
        body.match_indices(word)
            .map(|(idx, _)| idx)
            .filter(|&idx| {
                let end = idx + word.len();
                !body[..idx]
                    .chars()
                    .next_back()
                    .is_some_and(|ch| ch == '.' || is_python_identifier_char(ch))
                    && !body[end..]
                        .chars()
                        .next()
                        .is_some_and(is_python_identifier_char)
                    && !python_text_hides_code(body, idx)
            })
            .collect()
    };
    if !live_words("lambda").is_empty() {
        return true;
    }
    let mentions = live_words(name);
    live_words("for").into_iter().any(|for_idx| {
        let target_start = for_idx + "for".len();
        let target_end = body[target_start..]
            .find(" in ")
            .map_or(body.len(), |offset| target_start + offset);
        mentions
            .iter()
            .any(|&idx| idx > target_start && idx < target_end)
    })
}

/// The test function's text after its `def ...(...):` header.
fn test_body_after_header(text: &str) -> Option<&str> {
    let def = text.find("def ")?;
    let open = def + text[def..].find('(')?;
    let close = matching_call_paren(text, open)?;
    let colon = close + text[close..].find(':')?;
    text.get(colon + 1..)
}
