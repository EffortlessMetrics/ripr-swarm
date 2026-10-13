//! Guarded-arm activation evidence for the Python preview adapter (#7392).
//!
//! A changed line inside a guarding `if` arm executes only when the guard
//! accepts the call's inputs. When every strong related call provably leaves
//! the guard untaken, the changed line never runs under any observed input,
//! so a strong exact-value oracle still passes unchanged: reach plus a
//! strong oracle does not discriminate the change. This mirrors the
//! predicate boundary rule (`boundary.rs`) on the reachability side instead
//! of the equality side: the sibling predicate probe names the boundary it
//! misses, while this gate stops a return-value probe inside the same arm
//! from reading `exposed` off an oracle that never sees the new line run.
//!
//! The rule engages only when static evidence can read everything it needs:
//! the enclosing `if` conditions, their comparison operands, and the literal
//! arguments strong related tests bind. Anything unreadable (`elif`/`else`
//! chains, computed operands, non-literal rows) fails open and keeps the
//! existing verdict, like the boundary rule's `Unresolved` limitation.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    CallRow, canonical_decimal, literals_equal, resolve_operand, simple_comparison_operands,
    strong_owner_call_rows, unrebound_constants,
};
use super::module_constants::PythonModuleConstant;
use super::related_tests::PythonRelatedCandidate;
use crate::domain::{ValueContext, ValueFact};

/// Why a guarded changed line never executes under any observed input.
pub(super) struct GuardedArmGap {
    /// The guarding comparison a call input would need to satisfy
    /// (`total > 1000`), named as the repair hint.
    pub(super) discriminator: String,
    /// Literal owner-call arguments from the strong related tests, surfaced
    /// as observed values like the boundary rule does.
    pub(super) observed_values: Vec<ValueFact>,
    /// Why every observed input leaves the guard untaken.
    pub(super) reason: String,
}

/// Evidence that a changed line sits inside guarding arm(s) no strong
/// related call enters. Returns `None` when the line is unguarded, when any
/// observed input may enter the arm, or when static evidence cannot read the
/// enclosing structure — every one keeps the caller's existing verdict.
pub(super) fn python_guarded_arm_gap(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<GuardedArmGap> {
    let conditions = guarding_conditions(owner, owner_source, changed_line)?;
    let constants = unrebound_constants(owner, related_candidates);
    let rows = strong_owner_call_rows(owner, &constants, related_candidates, focus);
    if rows.is_empty() {
        // No strong related test binds a readable literal input, so static
        // evidence cannot see reachability either way; the existing oracle
        // verdict stands (the boundary rule engages under the same rule).
        return None;
    }

    // A row reaches the changed line only when EVERY guard holds; it is
    // provably excluded when ANY guard is provably false. A row with an
    // unevaluable guard is never excluded, so one unreadable input keeps
    // the verdict.
    let every_row_excluded = rows.iter().all(|row| {
        conditions
            .iter()
            .any(|condition| condition_holds(condition, row, &constants) == Some(false))
    });
    if !every_row_excluded {
        return None;
    }

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
    observed_values.sort_by(|left, right| {
        left.line
            .cmp(&right.line)
            .then(left.value.cmp(&right.value))
            .then(left.text.cmp(&right.text))
    });
    observed_values.dedup();

    let mut parts = Vec::new();
    for condition in &conditions {
        let observed = condition_observed_values(condition, &rows);
        parts.push(format!(
            "no strong related test input satisfies `{condition}`{}",
            if observed.is_empty() {
                String::new()
            } else {
                format!(
                    "; observed {} values: {}",
                    condition_lhs(condition),
                    observed.join(", ")
                )
            }
        ));
    }
    let reason = format!(
        "{}; the changed line never executes under an observed input",
        parts.join(" and ")
    );
    Some(GuardedArmGap {
        discriminator: conditions.join(" and "),
        observed_values,
        reason,
    })
}

/// The text of the left operand of a condition's first comparison, used to
/// name the observed values the strong related calls bind to it.
fn condition_lhs(condition: &str) -> String {
    comparison_operator(condition)
        .map(|(start, _)| condition[..start].trim_end().to_string())
        .unwrap_or_else(|| condition.to_string())
}

/// The literal values the rows bind to the condition's left operand.
fn condition_observed_values(condition: &str, rows: &[CallRow]) -> Vec<String> {
    let lhs = condition_lhs(condition);
    let mut values: Vec<String> = rows
        .iter()
        .filter_map(|row| row.bindings.get(&lhs).map(|binding| binding.value.clone()))
        .collect();
    values.sort();
    values.dedup();
    values
}

/// The `if`/`while` conditions enclosing `changed_line` inside its owner,
/// outermost first. Returns `None` when the enclosing structure cannot be
/// read honestly: an `else` arm can never be excluded by evidence, an
/// unrecognized dedented line means the source does not parse the way this
/// reader expects, and the owner bounds keep the walk inside one body.
pub(super) fn guarding_conditions(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
) -> Option<Vec<String>> {
    let lines: Vec<&str> = owner_source.lines().collect();
    let changed_index = changed_line.checked_sub(1)?;
    if owner.start_line == 0 || changed_index >= lines.len() {
        return None;
    }
    let changed = lines.get(changed_index)?;
    if changed.trim().is_empty() {
        return None;
    }
    let mut threshold = leading_whitespace(changed);
    let mut conditions = Vec::new();
    for index in (owner.start_line..changed_index).rev() {
        let raw = lines[index];
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = leading_whitespace(raw);
        if indent >= threshold {
            // A body line (or this guard's own continuation): the nesting
            // question lives further up.
            continue;
        }
        if let Some(condition) = trimmed
            .strip_prefix("if ")
            .or_else(|| trimmed.strip_prefix("elif "))
        {
            // An `elif X:` arm is taken by exactly the rows `X` accepts (a
            // row an earlier arm caught is a fail-open over-approximation,
            // never a false exclusion), so it reads like an `if`.
            let condition = condition.trim_end().strip_suffix(':')?.trim();
            conditions.push(condition.to_string());
            threshold = indent;
            continue;
        }
        if trimmed.starts_with("else") {
            // An `else` arm is taken whenever the chain is reached, so no
            // row can ever be provably excluded: fail open.
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix("while ") {
            let condition = rest.trim_end().strip_suffix(':')?.trim();
            conditions.push(condition.to_string());
            threshold = indent;
            continue;
        }
        if is_block_opener(trimmed) {
            // `for` / `with` / `try` / `except` / `finally` / `def` / `class`
            // open blocks this reader adds no condition for; the walk
            // continues at their indent. The owner header bounds the walk.
            threshold = indent;
            continue;
        }
        // A dedented line that opens nothing readable: the source does not
        // nest the way this reader expects (a continuation, a broken
        // dedent), so no honest exclusion is possible.
        return None;
    }
    conditions.reverse();
    (!conditions.is_empty()).then_some(conditions)
}

fn leading_whitespace(text: &str) -> usize {
    text.chars().take_while(|ch| ch.is_whitespace()).count()
}

fn is_block_opener(trimmed: &str) -> bool {
    [
        "for ",
        "with ",
        "try:",
        "except",
        "finally:",
        "def ",
        "async def ",
        "class ",
        "@",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}

/// Whether one guarding condition holds for one call row: every `and`
/// conjunct must be a comparison static evidence can evaluate to true.
/// `Some(false)` means the row provably fails the guard; `None` means the
/// guard (or one conjunct) is unreadable for this row, which never excludes
/// the row.
fn condition_holds(
    condition: &str,
    row: &CallRow,
    constants: &[PythonModuleConstant],
) -> Option<bool> {
    for conjunct in condition.split(" and ") {
        match eval_comparison(conjunct.trim(), row, constants) {
            Some(true) => {}
            Some(false) => return Some(false),
            None => return None,
        }
    }
    Some(true)
}

/// One `left <op> right` comparison under a row's literal bindings, or None
/// when the comparison or an operand is not statically readable.
fn eval_comparison(text: &str, row: &CallRow, constants: &[PythonModuleConstant]) -> Option<bool> {
    let (start, len) = comparison_operator(text)?;
    let (left, right) = simple_comparison_operands(text, start, len)?;
    let left = resolve_operand(&left, row, constants)?;
    let right = resolve_operand(&right, row, constants)?;
    compare_literal_values(&left, &text[start..start + len], &right)
}

/// The span of the single comparison operator in `text` (outside quotes),
/// or None for zero or several operators — several cannot be evaluated
/// honestly, and zero means there is no comparison to read.
fn comparison_operator(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut found: Option<(usize, usize)> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'=' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((index, 2));
                    index += 2;
                    continue;
                }
                // A bare `=` is an assignment, not a comparison.
                return None;
            }
            b'!' => {
                if bytes.get(index + 1) == Some(&b'=') {
                    if found.is_some() {
                        return None;
                    }
                    found = Some((index, 2));
                    index += 2;
                    continue;
                }
                index += 1;
            }
            b'<' | b'>' => {
                if bytes.get(index + 1) == Some(&byte)
                    || index.checked_sub(1).is_some_and(|prev| bytes[prev] == b'-')
                {
                    // `<<` / `>>` shifts and the `->` arrow.
                    index += 2;
                    continue;
                }
                if found.is_some() {
                    return None;
                }
                let len = usize::from(bytes.get(index + 1) == Some(&b'=')) + 1;
                found = Some((index, len));
                index += len;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    found
}

/// Compare two resolved literal values with `op`. Equality uses Python
/// scalar semantics (`literals_equal`); ordering parses the canonical
/// decimals and never orders across kinds (a string against a number is
/// unreadable, which fails open).
fn compare_literal_values(left: &str, op: &str, right: &str) -> Option<bool> {
    match op {
        "==" => Some(literals_equal(left, right)),
        "!=" => Some(!literals_equal(left, right)),
        "<" | "<=" | ">" | ">=" => {
            let left: f64 = canonical_decimal(left)?.parse().ok()?;
            let right: f64 = canonical_decimal(right)?.parse().ok()?;
            Some(match op {
                "<" => left < right,
                "<=" => left <= right,
                ">" => left > right,
                _ => left >= right,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::owners_tests::extract_owners;
    use super::*;

    fn quote_owner(source: &str) -> Result<PythonOwner, String> {
        extract_owners(std::path::Path::new("src/subject.py"), source)
            .into_iter()
            .find(|owner| owner.name == "quote")
            .ok_or_else(|| "quote owner must parse".to_string())
    }

    #[test]
    fn guarding_conditions_reads_enclosing_if_arm() -> Result<(), String> {
        let source = "def quote(total):\n    if total > 1000:\n        return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n";
        let owner = quote_owner(source)?;
        let conditions = guarding_conditions(&owner, source, 3)
            .ok_or_else(|| "guarded line must yield conditions".to_string())?;
        assert_eq!(conditions, vec!["total > 1000".to_string()]);
        Ok(())
    }

    #[test]
    fn guarding_conditions_walks_nested_arms_outermost_first() -> Result<(), String> {
        let source = "def quote(total):\n    if total > 0:\n        if total > 1000:\n            return round(total * 1.02, 2)\n    return round(total * 1.05, 2)\n";
        let owner = quote_owner(source)?;
        let conditions = guarding_conditions(&owner, source, 4)
            .ok_or_else(|| "nested guarded line must yield conditions".to_string())?;
        assert_eq!(
            conditions,
            vec!["total > 0".to_string(), "total > 1000".to_string()]
        );
        Ok(())
    }

    #[test]
    fn guarding_conditions_reads_elif_arm_without_its_chain_head() -> Result<(), String> {
        // The chain head at the same indent is a sibling line for the walk:
        // the elif arm's reach is approximated by its own condition only,
        // which never excludes a row the arm would genuinely take.
        let source = "def quote(total):\n    if total > 1000:\n        return round(total * 1.02, 2)\n    elif total > 0:\n        return round(total * 1.05, 2)\n    return total\n";
        let owner = quote_owner(source)?;
        let conditions = guarding_conditions(&owner, source, 5)
            .ok_or_else(|| "elif arm line must yield conditions".to_string())?;
        assert_eq!(conditions, vec!["total > 0".to_string()]);
        Ok(())
    }

    #[test]
    fn guarding_conditions_fails_open_on_else_and_unguarded_lines() -> Result<(), String> {
        let source = "def quote(total):\n    if total > 1000:\n        return round(total * 1.02, 2)\n    else:\n        return round(total * 1.05, 2)\n";
        let owner = quote_owner(source)?;
        assert!(guarding_conditions(&owner, source, 3).is_some());
        // An `else` arm is taken whenever the chain is reached, so no row can
        // ever be provably excluded.
        assert!(guarding_conditions(&owner, source, 5).is_none());
        let unguarded = "def quote(total):\n    return round(total * 1.05, 2)\n";
        let owner = quote_owner(unguarded)?;
        assert!(guarding_conditions(&owner, unguarded, 2).is_none());
        Ok(())
    }

    #[test]
    fn compare_literal_values_uses_python_scalar_semantics() {
        assert_eq!(compare_literal_values("100", "==", "100.0"), Some(true));
        assert_eq!(compare_literal_values("100", ">", "1000"), Some(false));
        assert_eq!(compare_literal_values("2000", ">", "1000"), Some(true));
        assert_eq!(compare_literal_values("1", "<=", "1.0"), Some(true));
        assert_eq!(compare_literal_values("\"a\"", "==", "\"a\""), Some(true));
        assert_eq!(compare_literal_values("1", "!=", "2"), Some(true));
        // A number against a string is unreadable, which fails open.
        assert_eq!(compare_literal_values("100", ">", "\"a\""), None);
        assert_eq!(compare_literal_values("\"a\"", "<", "\"b\""), None);
    }
}
