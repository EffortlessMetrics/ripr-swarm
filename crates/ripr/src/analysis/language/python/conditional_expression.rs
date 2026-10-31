//! Unreached conditional-expression arm evidence for the Python preview
//! adapter (#9184; extends RIPR-SPEC-0260).
//!
//! `return msg.upper() if loud else msg` -> `return msg.lower() if loud else msg`
//! changes only the arm taken when `loud` holds. When every strong related
//! call provably leaves `loud` false (or omits it and takes a falsy default),
//! the edited arm is never evaluated and a strong exact-value oracle passes
//! either way, exactly like a changed line in an untaken `if` arm.
//!
//! The rule engages only for one self-contained conditional expression on the
//! changed line whose condition is unchanged, whose other arm is unchanged,
//! and whose condition is either a bare owner parameter (read by literal
//! truthiness) or a comparison the guard reader already evaluates. Anything
//! else (nested expressions, comments, comprehensions, lambdas, a rebound
//! parameter, `or`/`not`, an unreadable row or an unbound strong call) fails
//! open and keeps the existing verdict.

use super::PythonOwner;
use super::assertion_selection::PythonAssertionFocus;
use super::boundary::{
    CallRow, canonical_decimal, strong_owner_call_rows_with_gaps, unrebound_constants,
};
use super::discriminators::python_string_literal_value;
use super::guard_activation::{GuardedArmGap, condition_holds, name_rebound_in_owner};
use super::module_constants::PythonModuleConstant;
use super::related_tests::PythonRelatedCandidate;
use crate::domain::ValueContext;
use crate::value_origins::LocatedValueFact;

/// `<true arm> if <condition> else <false arm>` split at its top level.
struct Ternary {
    true_arm: String,
    condition: String,
    false_arm: String,
}

/// Which arm of the conditional expression the edit changed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChangedArm {
    /// The arm taken when the condition holds.
    WhenTrue,
    /// The arm taken when the condition does not hold.
    WhenFalse,
}

/// Evidence that the edit sits on a conditional-expression arm no strong
/// related call evaluates. `None` when the line is not that shape, when any
/// observed input may evaluate the arm, or when static evidence cannot read it.
pub(super) fn python_conditional_expression_gap(
    owner: &PythonOwner,
    owner_source: &str,
    changed_line: usize,
    new_line: &str,
    old_line: Option<&str>,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<GuardedArmGap> {
    let (new, old) = (split_ternary(new_line)?, split_ternary(old_line?)?);
    let arm = changed_arm(&new, &old)?;
    let line_index = changed_line.checked_sub(1)?;
    let constants = unrebound_constants(owner, related_candidates);
    let (rows, gaps) =
        strong_owner_call_rows_with_gaps(owner, &constants, related_candidates, focus);
    if rows.is_empty() || gaps {
        // "EVERY strong call misses the arm" cannot be established from the
        // readable rows alone.
        return None;
    }
    let skipped = match arm {
        ChangedArm::WhenTrue => false,
        ChangedArm::WhenFalse => true,
    };
    let every_row_misses = rows.iter().all(|row| {
        condition_truth(
            &new.condition,
            row,
            &constants,
            owner,
            owner_source,
            line_index,
        ) == Some(skipped)
    });
    if !every_row_misses {
        return None;
    }
    let discriminator = match arm {
        ChangedArm::WhenTrue => new.condition.clone(),
        ChangedArm::WhenFalse => format!("not ({})", new.condition),
    };
    let bare = is_identifier(&new.condition);
    let mut observed: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            let binding = row.bindings.get(new.condition.as_str())?;
            bare.then(|| binding.value.clone())
        })
        .collect();
    observed.sort();
    observed.dedup();
    let mut observed_values: Vec<LocatedValueFact> = rows
        .iter()
        .flat_map(|row| {
            row.bindings
                .iter()
                .filter(|(parameter, binding)| {
                    !binding.from_default && (!bare || **parameter == new.condition)
                })
                .map(|(parameter, binding)| LocatedValueFact {
                    file: Some(row.file.clone()),
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
    let reason = format!(
        "the changed line is the {} arm of a conditional expression, and no strong related test input makes `{}` {}{}; the changed arm never executes under an observed input",
        match arm {
            ChangedArm::WhenTrue => "true",
            ChangedArm::WhenFalse => "false",
        },
        new.condition,
        match arm {
            ChangedArm::WhenTrue => "hold",
            ChangedArm::WhenFalse => "fail",
        },
        if observed.is_empty() {
            String::new()
        } else {
            format!(
                "; observed {} values: {}",
                new.condition,
                observed.join(", ")
            )
        }
    );
    Some(GuardedArmGap {
        discriminator,
        observed_values,
        reason,
        chained_receiver: false,
    })
}

/// The arm that differs between the old and the new expression: the condition
/// and the other arm must be textually unchanged, so a condition edit (a
/// predicate boundary question) and a two-arm edit both fail open.
fn changed_arm(new: &Ternary, old: &Ternary) -> Option<ChangedArm> {
    if new.condition != old.condition {
        return None;
    }
    match (new.true_arm != old.true_arm, new.false_arm != old.false_arm) {
        (true, false) => Some(ChangedArm::WhenTrue),
        (false, true) => Some(ChangedArm::WhenFalse),
        _ => None,
    }
}

/// Whether the condition holds for one row: `Some(true)` / `Some(false)` only
/// when static evidence reads it exactly.
fn condition_truth(
    condition: &str,
    row: &CallRow,
    constants: &[PythonModuleConstant],
    owner: &PythonOwner,
    owner_source: &str,
    line_index: usize,
) -> Option<bool> {
    if is_identifier(condition) {
        // A bare owner parameter, read from the call's binding only (never a
        // same-named module constant), and only when the owner never rebinds it.
        if name_rebound_in_owner(owner_source, owner, condition)
            || name_bound_by_statement(owner_source, owner, condition)
        {
            return None;
        }
        return literal_truthiness(&row.bindings.get(condition)?.value);
    }
    condition_holds(
        condition,
        line_index,
        row,
        constants,
        owner_source,
        owner.start_line,
    )
}

/// Whether an owner-body statement binds, scopes or removes `name` without an
/// assignment operator: `import x as name`, `from m import name`, a nested
/// `def name` / `class name`, `del name`, `global name`, `nonlocal name`. The
/// shared assignment reader does not see these, and after any of them the
/// call's default no longer predicts what the expression reads. Over-refuses
/// by design (any identifier-bounded mention on such a line).
fn name_bound_by_statement(owner_source: &str, owner: &PythonOwner, name: &str) -> bool {
    owner_source
        .lines()
        .skip(owner.start_line)
        .take(owner.end_line.saturating_sub(owner.start_line))
        .map(str::trim)
        .any(|line| {
            let mut words = line.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'));
            let first = words.find(|word| !word.is_empty());
            match first {
                Some("import" | "from" | "del" | "global" | "nonlocal") => {
                    words.any(|word| word == name)
                }
                Some("def" | "class" | "async") => {
                    words.find(|word| !word.is_empty() && !matches!(*word, "def")) == Some(name)
                }
                _ => false,
            }
        })
}

/// Python truthiness of a scalar literal, or `None` for anything else.
fn literal_truthiness(value: &str) -> Option<bool> {
    match value {
        "True" => return Some(true),
        "False" | "None" => return Some(false),
        _ => {}
    }
    if let Some(decimal) = canonical_decimal(value) {
        return Some(decimal != "0");
    }
    python_string_literal_value(value).map(|body| !body.is_empty())
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && !matches!(text, "True" | "False" | "None")
}

/// Split one line holding exactly one top-level conditional expression. The
/// line is refused when it carries a comment, a triple-quoted string, a
/// comprehension, a lambda, a walrus, unbalanced brackets, or more than one
/// `if` / `else` keyword outside brackets and quotes.
fn split_ternary(line: &str) -> Option<Ternary> {
    let text = line.trim();
    if ["if ", "elif ", "while ", "assert "]
        .iter()
        .any(|prefix| text.starts_with(prefix))
        || text.contains("\"\"\"")
        || text.contains("'''")
        || text.contains(":=")
        || text.contains("lambda")
        || text.contains(" for ")
        || text.contains("yield")
    {
        return None;
    }
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut depth = 0usize;
    let mut if_at: Option<usize> = None;
    let mut else_at: Option<usize> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' => quote = Some(byte),
            b'#' => return None,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b' ' if depth == 0 => {
                if text[index..].starts_with(" if ") {
                    if if_at.replace(index).is_some() {
                        return None;
                    }
                } else if text[index..].starts_with(" else ") && else_at.replace(index).is_some() {
                    return None;
                }
            }
            _ => {}
        }
        index += 1;
    }
    if quote.is_some() || depth != 0 {
        return None;
    }
    let (if_at, else_at) = (if_at?, else_at?);
    if if_at >= else_at {
        return None;
    }
    let ternary = Ternary {
        true_arm: text[..if_at].trim().to_string(),
        condition: text[if_at + " if ".len()..else_at].trim().to_string(),
        false_arm: text[else_at + " else ".len()..].trim().to_string(),
    };
    (!ternary.true_arm.is_empty() && !ternary.condition.is_empty() && !ternary.false_arm.is_empty())
        .then_some(ternary)
}
