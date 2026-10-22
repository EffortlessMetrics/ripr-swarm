//! RIPR-SPEC-0243 rule 15: a changed arm of a returned conditional whose
//! condition did not change.
//!
//! `return level === Level.High ? 10 : 1;` becoming `... ? 11 : 1` leaves the
//! condition alone, so no boundary input is needed: an exact assertion on a
//! call whose argument selects the changed arm sees the new value, and one
//! that selects the other arm passes on both versions. The change reads as a
//! return-value change, credited only when a strong exact assertion calls the
//! owner with an argument that provably selects the changed arm.

use super::*;

/// A literal the condition compares the owner parameter with, or the literal
/// argument a test passes.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ArmConstant {
    Number(f64),
    Text(String),
    Boolean(bool),
    Null,
    Undefined,
    /// `Level.High`: a member path requiring declaration-backed enum identity.
    Member(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArmComparison {
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
}

/// The facts of a changed arm: which owner parameter the condition reads,
/// how it compares it, and the condition value that selects the changed arm.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TernaryArmChange {
    pub(crate) param_index: usize,
    /// The condition as `parameter <comparison> operand`.
    pub(crate) comparison: ArmComparison,
    pub(crate) operand: ArmConstant,
    /// `true` when the changed arm runs while the condition holds.
    pub(crate) arm_runs_when: bool,
    /// The changed arm before and after the edit, when each is a constant.
    old_arm: Option<ArmConstant>,
    new_arm: Option<ArmConstant>,
}

impl TernaryArmChange {
    /// Whether a call passing `argument` provably runs the changed arm.
    pub(crate) fn selects_changed_arm(
        &self,
        argument: &str,
        same_member: impl FnOnce(&str, &str) -> bool,
    ) -> bool {
        parse_arm_constant(argument)
            .and_then(|argument| match (&argument, &self.operand) {
                (ArmConstant::Member(argument), ArmConstant::Member(operand)) => {
                    if !same_member(argument, operand) {
                        return None;
                    }
                    match self.comparison {
                        ArmComparison::Equal => Some(true),
                        ArmComparison::NotEqual => Some(false),
                        _ => None,
                    }
                }
                _ => evaluate(self.comparison, &argument, &self.operand),
            })
            .is_some_and(|holds| holds == self.arm_runs_when)
    }

    /// Whether `expected` provably equals neither the old nor the new value
    /// of the changed arm, so an assertion on it fails on both versions.
    pub(crate) fn expected_is_dead(&self, expected: &str) -> bool {
        let (Some(expected), Some(old), Some(new)) =
            (parse_arm_constant(expected), &self.old_arm, &self.new_arm)
        else {
            return false;
        };
        evaluate(ArmComparison::Equal, &expected, old) == Some(false)
            && evaluate(ArmComparison::Equal, &expected, new) == Some(false)
    }
}

/// The `TernaryArmChange` when both lines are `return <cond> ? <a> : <b>;`
/// (or the body of a declared concise arrow) with the same condition, one arm
/// edited, and a condition that compares one stable owner parameter with a
/// literal or member path.
pub(crate) fn ts_ternary_arm_change(
    owner: &TypeScriptOwner,
    line_text: &str,
    old_line_text: Option<&str>,
) -> Option<TernaryArmChange> {
    let new = conditional_parts(line_text)?;
    let old = conditional_parts(old_line_text?)?;
    if new.condition != old.condition {
        return None;
    }
    let arm_runs_when = match (
        new.when_true != old.when_true,
        new.when_false != old.when_false,
    ) {
        (true, false) => true,
        (false, true) => false,
        _ => return None,
    };
    let (old_arm, new_arm) = if arm_runs_when {
        (&old.when_true_text, &new.when_true_text)
    } else {
        (&old.when_false_text, &new.when_false_text)
    };
    let (parameter, comparison, operand) = parameter_comparison(&new.condition_text, owner)?;
    let param_index = owner.params.iter().position(|param| *param == parameter)?;
    if !ts_parameter_is_stable(owner.source_text.as_deref()?, &parameter) {
        return None;
    }
    Some(TernaryArmChange {
        param_index,
        comparison,
        operand,
        arm_runs_when,
        old_arm: parse_arm_constant(old_arm),
        new_arm: parse_arm_constant(new_arm),
    })
}

struct ConditionalParts {
    /// The condition as written, for operand parsing.
    condition_text: String,
    /// The condition without whitespace, for comparing old with new.
    condition: String,
    when_true: String,
    when_false: String,
    /// The arms as written, trimmed, for constant parsing.
    when_true_text: String,
    when_false_text: String,
}

/// Splits `return c ? a : b;` into whitespace-free parts. Nested conditionals
/// and anything with a stray top-level `?` refuse.
fn conditional_parts(line_text: &str) -> Option<ConditionalParts> {
    let trimmed = line_text.trim_start();
    let leading = trimmed.strip_prefix("} ").unwrap_or(trimmed).trim_start();
    let expression = returned_expression(leading)?
        .trim()
        .trim_end_matches(';')
        .trim();
    let masked = ts_mask_literals_and_comments(expression);
    if masked.len() != expression.len() {
        return None;
    }
    let mut depth = 0usize;
    let mut question = None;
    let mut colon = None;
    for (at, c) in masked.char_indices() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            '?' if depth == 0 => {
                let next = masked.get(at + 1..)?.chars().next();
                if matches!(next, Some('?' | '.')) || masked[..at].ends_with('?') {
                    return None;
                }
                if question.is_some() {
                    return None;
                }
                question = Some(at);
            }
            ':' if depth == 0 && question.is_some() => {
                if colon.is_some() {
                    return None;
                }
                colon = Some(at);
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let (question, colon) = (question?, colon?);
    let normalize =
        |text: &str| -> String { text.chars().filter(|ch| !ch.is_whitespace()).collect() };
    let condition = expression.get(..question)?.trim();
    if condition.is_empty() || condition.contains(" => ") {
        return None;
    }
    Some(ConditionalParts {
        condition_text: condition.to_string(),
        condition: normalize(condition),
        when_true: normalize(expression.get(question + 1..colon)?),
        when_false: normalize(expression.get(colon + 1..)?),
        when_true_text: expression.get(question + 1..colon)?.trim().to_string(),
        when_false_text: expression.get(colon + 1..)?.trim().to_string(),
    })
}

/// `parameter <comparison> operand` from a condition that is exactly one
/// comparison of an owner parameter and a constant.
fn parameter_comparison(
    condition: &str,
    owner: &TypeScriptOwner,
) -> Option<(String, ArmComparison, ArmConstant)> {
    const OPERATORS: [(&str, ArmComparison); 8] = [
        ("===", ArmComparison::Equal),
        ("!==", ArmComparison::NotEqual),
        ("==", ArmComparison::Equal),
        ("!=", ArmComparison::NotEqual),
        (">=", ArmComparison::GreaterOrEqual),
        ("<=", ArmComparison::LessOrEqual),
        (">", ArmComparison::Greater),
        ("<", ArmComparison::Less),
    ];
    let without_inequality = condition.replace("!==", " ").replace("!=", " ");
    if without_inequality.contains(['&', '|', '!', '(', ')', '?']) {
        return None;
    }
    let mut found = None;
    let mut at = 0;
    while at < condition.len() {
        let rest = condition.get(at..)?;
        if let Some((text, comparison)) = OPERATORS.iter().find(|(text, _)| rest.starts_with(text))
        {
            if found.is_some() {
                return None;
            }
            found = Some((at, text.len(), *comparison));
            at += text.len();
        } else {
            at += rest.chars().next().map_or(1, char::len_utf8);
        }
    }
    let (at, len, comparison) = found?;
    let left = condition.get(..at)?.trim();
    let right = condition.get(at + len..)?.trim();
    let is_parameter = |name: &str| owner.params.iter().any(|param| param == name);
    if is_parameter(left) {
        Some((left.to_string(), comparison, parse_arm_constant(right)?))
    } else if is_parameter(right) {
        let flipped = match comparison {
            ArmComparison::Less => ArmComparison::Greater,
            ArmComparison::LessOrEqual => ArmComparison::GreaterOrEqual,
            ArmComparison::Greater => ArmComparison::Less,
            ArmComparison::GreaterOrEqual => ArmComparison::LessOrEqual,
            other => other,
        };
        Some((right.to_string(), flipped, parse_arm_constant(left)?))
    } else {
        None
    }
}

/// A number, plain string, `true`/`false`/`null`/`undefined`, or a dotted
/// member path of identifiers.
fn parse_arm_constant(text: &str) -> Option<ArmConstant> {
    let text = text.trim();
    match text {
        "true" => return Some(ArmConstant::Boolean(true)),
        "false" => return Some(ArmConstant::Boolean(false)),
        "null" => return Some(ArmConstant::Null),
        "undefined" => return Some(ArmConstant::Undefined),
        _ => {}
    }
    if let Some(number) = numeric_literal_value(text).and_then(|n| n.parse::<f64>().ok()) {
        return Some(ArmConstant::Number(number));
    }
    for quote in ['"', '\''] {
        if let Some(inner) = text
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return (!inner.contains([quote, '\\'])).then(|| ArmConstant::Text(inner.to_string()));
        }
    }
    let segments: Vec<&str> = text.split('.').collect();
    let identifier = |segment: &str| {
        segment
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '$'))
            && segment.chars().all(is_identifier_part)
    };
    (segments.len() >= 2 && segments.iter().all(|segment| identifier(segment)))
        .then(|| ArmConstant::Member(text.to_string()))
}

/// Whether `argument <comparison> operand` holds, or `None` when the two
/// constants cannot be compared without a type checker (different kinds, or
/// two distinct member paths that may share a value).
fn evaluate(
    comparison: ArmComparison,
    argument: &ArmConstant,
    operand: &ArmConstant,
) -> Option<bool> {
    use ArmComparison::{Equal, Greater, GreaterOrEqual, Less, LessOrEqual, NotEqual};
    match (argument, operand) {
        (ArmConstant::Number(a), ArmConstant::Number(b)) => {
            // `partial_cmp` keeps `-0 === 0` and refuses NaN.
            let order = a.partial_cmp(b)?;
            Some(match comparison {
                Equal => order.is_eq(),
                NotEqual => order.is_ne(),
                Less => order.is_lt(),
                LessOrEqual => order.is_le(),
                Greater => order.is_gt(),
                GreaterOrEqual => order.is_ge(),
            })
        }
        (ArmConstant::Text(a), ArmConstant::Text(b)) => match comparison {
            Equal => Some(a == b),
            NotEqual => Some(a != b),
            _ => None,
        },
        (ArmConstant::Boolean(a), ArmConstant::Boolean(b)) => match comparison {
            Equal => Some(a == b),
            NotEqual => Some(a != b),
            _ => None,
        },
        (ArmConstant::Null, ArmConstant::Null)
        | (ArmConstant::Undefined, ArmConstant::Undefined) => match comparison {
            Equal => Some(true),
            NotEqual => Some(false),
            _ => None,
        },
        _ => None,
    }
}
