//! RIPR-SPEC-0231 rule 1 for boolean-valued operands (#7598): a comparison,
//! disjunction or `matches!` that an assertion only reads through a boolean
//! literal, a negation or an operator the classifier does not read pins no
//! value, so it never earns an exact kind.

use super::arguments::{
    AssertionComparison, assertion_comparison, boolean_assertion_macro_condition,
    is_complete_macro_invocation, split_top_level_commas,
};
use super::patterns::{
    boolean_literal_value, contains_negated_pattern, has_top_level_disjunction,
    has_top_level_exact_equality, pattern_conjunct_assertions, pattern_invocation_spans,
    split_top_level_conjuncts, strip_not_call, top_level_comparison_operator, unwrap_parentheses,
};
use crate::analysis::extract::mask_comments_and_strings;

/// How an assertion that compares a boolean-valued operand with a boolean
/// literal reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum BoolOperandReading {
    /// The assertion only states a relation, so it reads no stronger than
    /// the inequality ceiling: the operand is asserted false
    /// (`assert_eq!(r == Err(E::Bad), false)`), holds a deciding `||`, or
    /// compares with `!=`, `<`, `>`, `<=` or `>=`.
    Capped,
    /// The operand is an `&&` chain with a `matches!` conjunct and no pinning
    /// `==` conjunct asserted true, which reads like the same chain in
    /// `assert!` (rule 3): this assertion text.
    Chain(String),
    /// The operand is an `&&` chain with a pinning `==` conjunct and at
    /// least one conjunct that states only a relation
    /// (`a == b && !matches!(r, Err(E::Bad))`, `a == b && r != Err(E::Bad)`):
    /// the same equality assertion over the other conjuncts. The relation
    /// conjunct pins nothing, so it lends the reading no kind (rule 1;
    /// #7947). Only the equality-macro form produces it; condition macros
    /// (`assert!`, `ensure!`) read `None` here.
    Rest(String),
}

/// The reading of an equality macro with one boolean-literal operand
/// (`assert_eq!(X, true)`, `assert_ne!(false, X)` and the `debug_` and
/// `prop_` spellings), or a boolean assertion whose whole condition is such a
/// comparison (`ensure!((a == b) == false)`), when `X` is boolean-valued: an
/// `==`, `!=`, `<`, `>`, `<=` or `>=` comparison, an `||` or an `&&` at its
/// top level, through any `!` or `.not()`. `None` leaves the assertion to
/// the other rules: `X` is a bare value or call (`assert_eq!(is_valid(x),
/// true)` pins a bool result), a whole `matches!` (its polarity rule reads
/// it), or a comparison asserted true that may pin (`assert_eq!(a == b,
/// true)`).
pub(super) fn bool_literal_comparison_reading(line: &str) -> Option<BoolOperandReading> {
    if compares_test_with_non_literal(line) {
        return Some(BoolOperandReading::Capped);
    }
    let (operand, mut positive, in_condition) = compared_with_bool_literal(line)?;
    // An operand that feeds a `matches!` to an unread call or operator, in
    // any conjunct, may invert it (`assert_eq!(f(matches!(r, Ok(3))), true)`).
    // The whole operand caps: unlike a boolean assertion, no conjunct drops.
    let operand_assertion = format!("assert!({operand}\n);");
    if contains_unread_pattern_operand(&operand_assertion)
        || unread_pattern_conjunct_rest(&operand_assertion).is_some()
    {
        return Some(BoolOperandReading::Capped);
    }
    let (core, negated, literal_compared) = strip_negations(operand.trim());
    if negated {
        positive = !positive;
    }
    // A whole `matches!` asserted false through a peeled spelling
    // (`assert_eq!(matches!(..).eq(&true), false)`) excludes the pattern;
    // the polarity rule sees only `matches!(..)` compared directly.
    if !positive && is_whole_pattern(core) {
        return Some(BoolOperandReading::Capped);
    }
    // A single `&` requires both sides like `&&`, so the operand reads as
    // the same `&&` chain.
    let conjoined = conjoin_single_ampersands(core);
    let core = conjoined.as_str();
    let disjunction = has_top_level_disjunction(core);
    let conjunction = split_top_level_conjuncts(core).len() > 1;
    let comparison =
        top_level_comparison_operator(core).and_then(|(at, width)| core.get(at..at + width));
    let relational = has_top_level_relational_operator(core);
    // `^` and `|` bind tighter than a comparison, so they decide only
    // an operand with no top-level comparison, and may invert or discard a
    // pattern or comparison inside it (`matches!(r, Ok(3)) ^ flag`,
    // `(r == Err(E::Bad)) ^ true`). Over plain bool values
    // (`is_a(x) ^ true`) they still pin a bool result.
    let holds_test = {
        let masked = mask_comments_and_strings(core);
        masked.contains("matches!")
            || masked.contains("==")
            || masked.contains("!=")
            || holds_comparison_call(&masked)
    };
    let bitwise =
        comparison.is_none() && !relational && holds_test && has_top_level_bitwise_operator(core);
    if !(disjunction || conjunction || comparison.is_some() || relational || bitwise) {
        // A comparison spelled as a method call (`r.eq(&Err(E::Bad))`,
        // `score(2).ne(&0)`) reads like its operator: only `eq` asserted
        // true may pin (#7931).
        return method_comparison(core)
            .filter(|method| *method != "eq" || !positive)
            .map(|_| BoolOperandReading::Capped);
    }
    if !positive || disjunction || bitwise {
        return Some(BoolOperandReading::Capped);
    }
    if conjunction {
        // A whole condition compared with `true` is already capped by the
        // compared-pattern rule, so only the equality-macro form is read
        // as its chain here; the chain never raises that cap.
        if in_condition {
            return None;
        }
        let conjuncts = split_top_level_conjuncts(core);
        if conjuncts.iter().any(|conjunct| {
            has_top_level_exact_equality(conjunct) && !conjunct_states_relation(conjunct)
        }) {
            let rest: Vec<&str> = conjuncts
                .iter()
                .filter(|conjunct| !conjunct_states_relation(conjunct))
                .map(|conjunct| conjunct.trim())
                .collect();
            if rest.len() == conjuncts.len() {
                return None;
            }
            // The rest keeps the equality-macro form, so `a == b` reads as
            // `assert_eq!(a == b, true)` does.
            return Some(BoolOperandReading::Rest(format!(
                "assert_eq!({}\n, true\n);",
                rest.join("\n && ")
            )));
        }
        // With no pinning `==` conjunct, a negated, `!=`, relational or
        // false-compared conjunct states only a relation
        // (`score(2) != 0 && flag`, `flag && !matches!(r, Err(E::Bad))`).
        if conjuncts
            .iter()
            .any(|conjunct| conjunct_states_relation(conjunct))
        {
            return Some(BoolOperandReading::Capped);
        }
        let chain = format!("assert!({core}\n);");
        pattern_conjunct_assertions(&chain)?;
        // A chain compared with a literal inside the operand
        // (`(ready && matches!(r, Ok(3))) == true`) keeps the
        // compared-pattern cap, as in a whole condition.
        return Some(if literal_compared {
            BoolOperandReading::Capped
        } else {
            BoolOperandReading::Chain(chain)
        });
    }
    (comparison == Some("!=") || relational).then_some(BoolOperandReading::Capped)
}

/// `text` through any leading `!`, trailing `.not()` or comparison with a
/// boolean literal (`X == false`, `X != true`, `X == true`), and whether
/// an odd number of negations applied. `!` binds tighter than a binary
/// operator, so it negates the whole only when the rest is one operand
/// (`!(a && b)`, not `!flag && b`). The third value says whether a literal
/// comparison was peeled.
fn strip_negations(text: &str) -> (&str, bool, bool) {
    let mut core = unwrap_parentheses(text);
    let mut negated = false;
    let mut literal_compared = false;
    loop {
        if let Some(rest) = core.strip_prefix('!') {
            let rest = rest.trim();
            let inner = unwrap_parentheses(rest);
            if inner.len() == rest.len() && !is_single_operand(rest) {
                break;
            }
            core = inner;
        } else if let Some(receiver) = strip_not_call(core) {
            core = unwrap_parentheses(receiver.trim());
        } else if let Some(inner) = unwrap_block(core) {
            // A block around one expression is that expression
            // (`{ r == Err(E::Bad) }`).
            core = unwrap_parentheses(inner);
            continue;
        } else if let Some(inner) = bool_preserving_operand(core) {
            core = inner;
            continue;
        } else if let Some((operand, flips)) =
            literal_compared_operand(core).or_else(|| method_literal_operand(core))
        {
            core = operand;
            literal_compared = true;
            if !flips {
                continue;
            }
        } else {
            break;
        }
        negated = !negated;
    }
    (core, negated, literal_compared)
}

/// The operand of a top-level `==` or `!=` whose other side is a boolean
/// literal, and whether the comparison negates it (`X == false`).
fn literal_compared_operand(text: &str) -> Option<(&str, bool)> {
    // `==` binds tighter than `&&` and `||`: in `a && x == false` the
    // literal compares only `x`.
    if split_top_level_conjuncts(text).len() > 1 || has_top_level_disjunction(text) {
        return None;
    }
    let (at, width) = top_level_comparison_operator(text)?;
    let not_equal = text.get(at..at + width)? == "!=";
    let (left, right) = (text.get(..at)?, text.get(at + width..)?);
    let literal = |side: &str| literal_spelling_value(&mask_comments_and_strings(side));
    let (operand, value) = match (literal(left), literal(right)) {
        (None, Some(value)) => (left, value),
        (Some(value), None) => (right, value),
        _ => return None,
    };
    Some((unwrap_parentheses(operand.trim()), value == not_equal))
}

/// The value of a boolean literal through the spellings that keep it: a
/// reference, a one-expression block, a `bool` cast or `bool::from`, each
/// through grouping parentheses (`&false`, `{ false }`, `(false as bool)`,
/// `bool::from(false)`; #7931), and through a `!` that negates it
/// (`!&true`; #7993). `side` is masked.
fn literal_spelling_value(side: &str) -> Option<bool> {
    let mut text = side.trim();
    let mut negated = false;
    loop {
        text = unwrap_parentheses(text);
        if let Some(value) = boolean_literal_value(text) {
            return Some(value != negated);
        }
        if let Some(rest) = text.strip_prefix('!') {
            negated = !negated;
            text = rest.trim();
            continue;
        }
        let cast = text.strip_suffix("bool").and_then(|rest| {
            rest.trim_end()
                .strip_suffix("as")
                .filter(|head| head.ends_with(char::is_whitespace))
        });
        let from = text
            .strip_prefix("bool::from")
            .and_then(|rest| super::arguments::parenthesized_contents(rest.trim_start()));
        text = text
            .strip_prefix('&')
            .or(cast)
            .or(from)
            .or_else(|| unwrap_block(text))?
            .trim();
    }
}

/// The expression inside a block that holds only that expression
/// (`{ r == Err(E::Bad) }`): the first `{` closes at the last `}` and no
/// top-level `;` ends a statement before it.
fn unwrap_block(text: &str) -> Option<&str> {
    let text = text.trim();
    let inner = text.strip_prefix('{')?.strip_suffix('}')?;
    let masked = mask_comments_and_strings(inner);
    let mut depth = 0usize;
    for byte in masked.bytes() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.checked_sub(1)?,
            b';' if depth == 0 => return None,
            _ => {}
        }
    }
    let trimmed = inner.trim();
    (depth == 0 && !trimmed.is_empty()).then_some(trimmed)
}

/// The comparison method (`eq`, `ne`, `lt`, `le`, `gt` or `ge`) when `text`
/// is one call to it (`r.eq(&Err(E::Bad))`, `PartialEq::eq(&r, &x)`; #7931).
fn method_comparison(text: &str) -> Option<&'static str> {
    comparison_call(text).map(|(name, _, _)| name)
}

/// A comparison spelled as one call: its method name and its two sides,
/// either a receiver and an argument (`score(2).ne(&0)`) or the two
/// arguments of a path call (`PartialEq::eq(&r, &x)`,
/// `<_ as PartialEq>::eq(&r, &x)`). A method call binds tighter than any
/// operator, so the receiver must be one operand (`flag && r.eq(&x)` calls
/// `eq` on `r` only) and the path call must be the whole text.
fn comparison_call(text: &str) -> Option<(&'static str, &str, &str)> {
    let text = text.trim();
    let masked = mask_comments_and_strings(text);
    // A trailing comment is masked to spaces; the call ends at its `)`.
    let masked = masked.trim_end();
    if !masked.ends_with(')') {
        return None;
    }
    let mut depth = 0usize;
    let mut open = None;
    for (index, byte) in masked.bytes().enumerate().rev() {
        match byte {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    open = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let open = open?;
    let head = masked.get(..open)?.trim_end();
    let arguments = text.get(open + 1..masked.len() - 1)?;
    const NAMES: [&str; 6] = ["eq", "ne", "lt", "le", "gt", "ge"];
    for name in NAMES {
        let Some(rest) = head.strip_suffix(name) else {
            continue;
        };
        if let Some(receiver) = rest.trim_end().strip_suffix('.') {
            let receiver = text.get(..receiver.len())?.trim();
            // A leading unary operator applies to the call's result
            // (`!x.eq(&y)` is `!(x.eq(&y))`), so it is not the receiver.
            let unary = receiver.starts_with(['!', '&', '*', '-']);
            if !receiver.is_empty() && !unary && is_single_operand(receiver) {
                return Some((name, receiver, arguments.trim()));
            }
        }
        if is_path_head(rest) {
            return split_two_arguments(arguments).map(|(left, right)| (name, left, right));
        }
    }
    None
}

/// Whether masked `head` is the whole path before an associated function
/// name: segments joined by `::`, through a qualified-self head or
/// generics (`PartialEq::`, `Result::`, `<_ as PartialEq>::`,
/// `<Result<u8, E> as PartialEq>::`; #7993). Any path's two-argument
/// `eq` reads like `PartialEq::eq`: a cap only lowers a reading, and `eq`
/// asserted true still pins. An operator or space outside the angles
/// (`flag && PartialEq::`, `a < b`) ends the path, so the call is not the
/// whole text.
fn is_path_head(head: &str) -> bool {
    let head = head.trim_end();
    if !head.ends_with("::") {
        return false;
    }
    let bytes = head.as_bytes();
    // Unformatted code may space a separator (`Result :: eq`); a space
    // anywhere else outside the angles ends the path.
    let beside_separator = |index: usize| {
        let before = bytes
            .get(..index)
            .and_then(|head| head.iter().rev().find(|byte| !byte.is_ascii_whitespace()));
        let after = bytes
            .get(index..)
            .and_then(|tail| tail.iter().find(|byte| !byte.is_ascii_whitespace()));
        before == Some(&b':') || after == Some(&b':')
    };
    let mut generics = 0usize;
    // A const-generic block inside the angles (`C::<{ A > B }>::ne`) is one
    // argument: its `<`, `>` and spaces are not the path's.
    let mut blocks = 0usize;
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b';' => return false,
            b'{' if generics > 0 => blocks += 1,
            b'}' if blocks > 0 => blocks -= 1,
            _ if blocks > 0 => {}
            b'<' if generics > 0
                || index == 0
                || (index >= 2 && bytes.get(index - 2..index) == Some(&b"::"[..])) =>
            {
                generics += 1;
            }
            b'>' if generics > 0 && (index == 0 || bytes.get(index - 1) != Some(&b'-')) => {
                generics -= 1;
            }
            b'{' | b'}' => return false,
            _ if generics > 0 => {}
            b':' | b'_' => {}
            _ if byte.is_ascii_alphanumeric() => {}
            _ if byte.is_ascii_whitespace() && beside_separator(index) => {}
            _ => return false,
        }
    }
    generics == 0 && blocks == 0
}

/// The two arguments of a call's argument list when it holds exactly two,
/// allowing a trailing comma (`PartialEq::eq(&r, &x,)`). Commas inside
/// brackets or turbofish generics (`::<K, V>`) do not separate arguments.
fn split_two_arguments(arguments: &str) -> Option<(&str, &str)> {
    let masked = mask_comments_and_strings(arguments);
    let trimmed = masked.trim_end();
    let end = trimmed.strip_suffix(',').map_or(trimmed.len(), str::len);
    let arguments = arguments.get(..end)?;
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut generics = 0usize;
    let mut comma = None;
    for (index, &byte) in bytes.iter().enumerate().take(end) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            // Inside a turbofish every `<` opens a nested generic
            // (`::<Vec<u8>, u8>`).
            b'<' if depth == 0
                && (generics > 0
                    || (index >= 2 && bytes.get(index - 2..index) == Some(&b"::"[..]))) =>
            {
                generics += 1;
            }
            // `->` inside generics (`Fn() -> T`) closes nothing.
            b'>' if depth == 0
                && generics > 0
                && (index == 0 || bytes.get(index - 1) != Some(&b'-')) =>
            {
                generics -= 1;
            }
            b',' if depth == 0 && generics == 0 => {
                if comma.is_some() {
                    return None;
                }
                comma = Some(index);
            }
            _ => {}
        }
    }
    let comma = comma?;
    let (left, right) = (
        arguments.get(..comma)?.trim(),
        arguments.get(comma + 1..)?.trim(),
    );
    (!left.is_empty() && !right.is_empty()).then_some((left, right))
}

/// The operand of an `eq` or `ne` call whose other side is a boolean
/// literal, and whether the call negates it (`x.eq(&false)`; #7931).
fn method_literal_operand(text: &str) -> Option<(&str, bool)> {
    let (name, left, right) = comparison_call(text)?;
    if !matches!(name, "eq" | "ne") {
        return None;
    }
    let literal = |side: &str| literal_spelling_value(&mask_comments_and_strings(side));
    let (operand, value) = match (literal(left), literal(right)) {
        (None, Some(value)) => (left, value),
        (Some(value), None) => (right, value),
        _ => return None,
    };
    Some((unwrap_parentheses(operand), value == (name == "ne")))
}

/// The operand inside a spelling that keeps its bool value: a reference to
/// a single operand, `bool::from(..)` or a single operand cast `as bool`
/// (`&(r == Err(E::Bad))`, `(r == Err(E::Bad)) as bool`; #7931).
fn bool_preserving_operand(text: &str) -> Option<&str> {
    let text = text.trim();
    let single = |operand: &str| {
        let operand = operand.trim();
        unwrap_parentheses(operand).len() < operand.len() || is_single_operand(operand)
    };
    if let Some(rest) = text.strip_prefix('&').filter(|rest| single(rest)) {
        return Some(unwrap_parentheses(rest.trim()));
    }
    if let Some(inner) = text
        .strip_prefix("bool::from")
        .and_then(|rest| super::arguments::parenthesized_contents(rest.trim_start()))
    {
        return Some(unwrap_parentheses(inner.trim()));
    }
    let masked = mask_comments_and_strings(text);
    let head = masked.strip_suffix("bool")?.trim_end().strip_suffix("as")?;
    if !head.ends_with(char::is_whitespace) {
        return None;
    }
    let operand = text.get(..head.len())?;
    single(operand).then(|| unwrap_parentheses(operand.trim()))
}

/// Whether `text` has no top-level binary operator the bool rules read.
fn is_single_operand(text: &str) -> bool {
    split_top_level_conjuncts(text).len() == 1
        && !has_top_level_disjunction(text)
        && top_level_comparison_operator(text).is_none()
        && !has_top_level_relational_operator(text)
        && !has_top_level_bitwise_operator(text)
        && conjoin_single_ampersands(text) == text
}

/// Whether one conjunct of a chain asserted true states only a relation: a
/// negated test (`!matches!(..)`, `!(a == b)`, `(a == b) == false`) or a
/// `!=` or relational comparison, through any literal comparison
/// (`(r != Err(E::Bad)) == true`). A negated bool value (`x == false`,
/// `!x`) pins it, like `!is_valid(x)`.
fn conjunct_states_relation(conjunct: &str) -> bool {
    let (core, negated, _) = strip_negations(conjunct.trim());
    let relational = has_top_level_relational_operator(core);
    let masked = mask_comments_and_strings(core);
    let holds_test = relational
        || masked.contains("matches!")
        || masked.contains("==")
        || masked.contains("!=")
        || holds_comparison_call(&masked);
    (negated && holds_test)
        || relational
        // `^` and `|` may invert or discard a comparison call
        // (`flag ^ r.ne(&x)`), as they do an operator comparison.
        || (holds_comparison_call(&masked)
            && top_level_comparison_operator(core).is_none()
            && has_top_level_bitwise_operator(core))
        || method_comparison(core).is_some_and(|method| method != "eq" || negated)
        || top_level_comparison_operator(core).and_then(|(at, width)| core.get(at..at + width))
            == Some("!=")
}

/// Whether an equality macro, or a boolean assertion whose whole condition
/// is an `==` or `!=`, compares a boolean-valued test with a side that is no
/// boolean literal (`assert_eq!(r == Err(E::Bad), expected)`,
/// `assert_eq!(matches!(r, Err(E::Bad)).eq(&true), expected)`; #8006). The
/// other side may be `false`, so the test may be asserted false: like a
/// `matches!` compared with a non-literal, it fails closed.
fn compares_test_with_non_literal(line: &str) -> bool {
    let literal = |side: &str| literal_spelling_value(&mask_comments_and_strings(side)).is_some();
    let sides = match assertion_comparison(line) {
        Some(AssertionComparison::Operands { left, right, .. }) => (left, right),
        Some(AssertionComparison::Condition(condition)) => {
            let condition = unwrap_parentheses(condition.trim());
            if has_top_level_disjunction(condition)
                || split_top_level_conjuncts(condition).len() > 1
            {
                return false;
            }
            let Some((at, width)) = top_level_comparison_operator(condition) else {
                return false;
            };
            match (condition.get(..at), condition.get(at + width..)) {
                (Some(left), Some(right)) => (left.to_string(), right.to_string()),
                _ => return false,
            }
        }
        None => return false,
    };
    let (left, right) = (sides.0.trim(), sides.1.trim());
    !literal(left) && !literal(right) && (is_bool_test(left) || is_bool_test(right))
}

/// Whether `side`, through the spellings `strip_negations` peels, is a
/// test: a top-level comparison, `||` or `&&`, a comparison call, a whole
/// `matches!`, or a `^`, `|` or `&` over a comparison or pattern
/// (`(r == Err(E::Bad)) ^ flag`). A bitwise operator over plain values is
/// no test (`flags & MASK`, `n & (n - 1)` pin a value).
fn is_bool_test(side: &str) -> bool {
    let (core, _, _) = strip_negations(side);
    let masked = mask_comments_and_strings(core);
    let holds_test = masked.contains("matches!")
        || masked.contains("==")
        || masked.contains("!=")
        || holds_comparison_call(&masked);
    top_level_comparison_operator(core).is_some()
        || has_top_level_relational_operator(core)
        || has_top_level_disjunction(core)
        || split_top_level_conjuncts(core).len() > 1
        || method_comparison(core).is_some()
        || is_whole_pattern(core)
        || (holds_test && has_top_level_bitwise_operator(core))
}

/// The non-literal operand of a comparison with a boolean literal, whether
/// the assertion states that operand is true, and whether the comparison is
/// a boolean assertion's whole condition rather than an equality macro's
/// operands.
fn compared_with_bool_literal(line: &str) -> Option<(String, bool, bool)> {
    let literal = |side: &str| literal_spelling_value(&mask_comments_and_strings(side));
    let pick = |left: &str, right: &str, not_equal: bool| match (literal(left), literal(right)) {
        (None, Some(value)) => Some((left.to_string(), value != not_equal)),
        (Some(value), None) => Some((right.to_string(), value != not_equal)),
        _ => None,
    };
    match assertion_comparison(line)? {
        AssertionComparison::Operands {
            not_equal,
            left,
            right,
        } => pick(&left, &right, not_equal).map(|(operand, positive)| (operand, positive, false)),
        AssertionComparison::Condition(condition) => {
            let condition = unwrap_parentheses(condition.trim());
            // `a && b == true` compares only `b`; the chain rules read it.
            if has_top_level_disjunction(condition)
                || split_top_level_conjuncts(condition).len() > 1
            {
                return None;
            }
            let (at, width) = top_level_comparison_operator(condition)?;
            let not_equal = condition.get(at..at + width) == Some("!=");
            pick(
                condition.get(..at)?,
                condition.get(at + width..)?,
                not_equal,
            )
            .map(|(operand, positive)| (operand, positive, true))
        }
    }
}

/// Whether a `<`, `>`, `<=` or `>=` comparison sits outside every
/// delimiter, string and comment. A turbofish (`::<u8>`), `->`, `=>`, `<-`
/// and the shifts `<<` and `>>` are not comparisons.
fn has_top_level_relational_operator(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut generics = 0usize;
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let previous = index.checked_sub(1).and_then(|at| bytes.get(at)).copied();
        let next = bytes.get(index + 1).copied();
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'<' | b'>' if depth == 0 && generics == 0 && next == Some(byte) => {
                // A shift, not a comparison.
                index += 2;
                continue;
            }
            // Inside a turbofish every `<` opens and every `>` closes a
            // generic, so `Vec::<Vec<u8>>` closes both levels.
            // A `<` after `::`, at the start, or after an operator or `(`
            // opens a turbofish or qualified path (`<T as Tr>::f`).
            b'<' if depth == 0 && (generics > 0 || opens_generics(text, &masked, index)) => {
                generics += 1;
            }
            b'<' if depth == 0 && generics == 0 && next != Some(b'-') => return true,
            // The `>` of a `->` inside generics (`fn() -> u8`) closes nothing.
            b'>' if depth == 0 && generics > 0 && previous != Some(b'-') => generics -= 1,
            b'>' if depth == 0 && !matches!(previous, Some(b'-' | b'=')) => return true,
            _ => {}
        }
        index += 1;
    }
    false
}

/// Whether the `<` at `index` opens generics: it follows `::`, starts the
/// text, or follows an operator, `(` or `,` rather than an operand. A string
/// or char literal masks to spaces, so `text` decides whether one ends the
/// head (`"x+" < s` compares).
fn opens_generics(text: &str, masked: &str, index: usize) -> bool {
    let literal_operand = text
        .get(..index)
        .unwrap_or_default()
        .trim_end()
        .ends_with(['"', '\'', '#']);
    let head = masked.get(..index).unwrap_or_default().trim_end();
    if literal_operand {
        return false;
    }
    head.is_empty()
        || head.ends_with("::")
        || head.ends_with(['=', '!', '&', '|', '(', ',', '^', '+', '-', '*', '/', '%'])
}

/// Whether `^` or `|` (not `||`) sits outside every delimiter, string and
/// comment. A closure parameter list is not an operator.
fn has_top_level_bitwise_operator(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    let trimmed = masked.trim_start();
    // A closure parameter list is not an operator.
    let is_move_closure = trimmed
        .strip_prefix("move")
        .is_some_and(|rest| rest.trim_start().starts_with('|'));
    if trimmed.starts_with('|') || is_move_closure {
        return false;
    }
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let previous = index.checked_sub(1).and_then(|at| bytes.get(at)).copied();
        let next = bytes.get(index + 1).copied();
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'&' | b'|' if depth == 0 && next == Some(byte) => {
                index += 2;
                continue;
            }
            b'^' if depth == 0 => return true,
            // A single `&` is a non-short-circuit conjunction: like `&&` it
            // requires both sides, so it neither inverts nor discards one;
            // `conjoin_single_ampersands` reads it as one.
            b'|' if depth == 0
                && previous.is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b')' | b']' | b' ' | b'_')
                })
                && masked
                    .get(..index)
                    .is_some_and(|head| !head.trim().is_empty()) =>
            {
                return true;
            }
            _ => {}
        }
        index += 1;
    }
    false
}

/// Whether a boolean assertion's condition uses a `matches!` as an operand
/// the classifier does not read: of `^`, `|`, `<`, `>`, `<=` or `>=`
/// (`matches!(..) ^ true`), as an argument of a call that is the whole
/// condition (`Not::not(matches!(..))`, `check::<bool>(1, matches!(..))`,
/// directly or through nested calls), or in an `if`, `while` or `match`
/// condition anywhere in it. Each can invert or discard the pattern, so the caller caps the
/// reading. A `matches!` in a closure body, compared with `==` or `!=`,
/// negated, joined by `&&` or `||`, or whose call result is read again
/// (`foo(matches!(..)).is_ok()`, `matches!(build(matches!(..)), Ok(_))`) is
/// read by the other rules.
pub(super) fn contains_unread_pattern_operand(line: &str) -> bool {
    let Some((_, condition)) = boolean_assertion_macro_condition(line) else {
        return false;
    };
    let masked = mask_comments_and_strings(&condition);
    let spans = pattern_invocation_spans(&condition);
    if spans.is_empty() {
        return false;
    }
    // A negation of the whole condition, through any grouping, block,
    // `.not()` or negating literal call, excludes whatever pattern it holds
    // (`!(flag || matches!(..))`, `!{ matches!(..) }`,
    // `Some(matches!(..)).unwrap().not()`, `(matches!(..),).0.eq(&false)`;
    // #8006). A `!` or `.not()` directly on the pattern is the
    // negated-pattern rules' to read, so a negated conjunct still drops.
    let (_, negated, _) = strip_negations(&condition);
    if negated && !contains_negated_pattern(line) {
        return true;
    }
    let head = unwrap_parentheses(masked.trim());
    if ["if", "while", "match"].iter().any(|keyword| {
        head.strip_prefix(keyword)
            .is_some_and(|rest| rest.starts_with(|ch: char| ch.is_whitespace() || ch == '('))
    }) {
        return true;
    }
    if call_argument_pattern(&masked) {
        return true;
    }
    spans.into_iter().any(|(start, end)| {
        let before = masked.get(..start).unwrap_or_default().trim_end();
        // Grouping parentheses and one-expression blocks pass the pattern
        // through (`flag ^ { matches!(..) }`; #8006).
        let unopened =
            before.trim_end_matches(|ch: char| ch.is_whitespace() || ch == '(' || ch == '{');
        let after = masked
            .get(end..)
            .unwrap_or_default()
            .trim_start_matches(|ch: char| ch.is_whitespace() || ch == ')' || ch == '}');
        unread_operator_before(unopened)
            || binary_bar_before(unopened)
            || in_keyword_condition(before)
            || after.starts_with(['^', '<', '>'])
            || (after.starts_with('|') && !after.starts_with("||"))
            // A cast reads the bool as a number (`matches!(..) as u8 == 0`).
            || after
                .strip_prefix("as")
                .is_some_and(|rest| rest.starts_with(char::is_whitespace))
            || method_on_pattern(before, masked.get(end..).unwrap_or_default())
    })
}

/// Whether `text` is one `matches!` invocation and nothing else but
/// comments and whitespace (`/* c */ matches!(..)`). A nested `matches!`
/// inside its arguments still leaves it whole.
fn is_whole_pattern(text: &str) -> bool {
    let masked = mask_comments_and_strings(text);
    pattern_invocation_spans(text).iter().any(|&(start, end)| {
        masked
            .get(..start)
            .is_some_and(|head| head.trim().is_empty())
            && masked.get(end..).is_some_and(|tail| tail.trim().is_empty())
    })
}

/// Whether masked `text` spells a comparison as a call anywhere
/// (`r.eq(&Err(E::Bad)) ^ true`, `PartialOrd::lt(&a, &b) | flag`; #7931),
/// including through any path (`Result::ne(&r, &x) ^ flag`,
/// `<_ as PartialEq>::ne(..)`; #7993).
fn holds_comparison_call(text: &str) -> bool {
    text.contains("PartialEq::")
        || text.contains("PartialOrd::")
        || ["eq", "ne", "lt", "le", "gt", "ge"].iter().any(|name| {
            // A method call or its turbofish follows the name; after a path
            // `::name` only a call does, since `helpers::eq::value()` names
            // a module.
            let called = |rest: &str, turbofish: bool| {
                let rest = rest.trim_start();
                rest.starts_with('(') || (turbofish && rest.starts_with(':'))
            };
            text.match_indices(&format!(".{name}"))
                .any(|(at, matched)| {
                    text.get(at + matched.len()..)
                        .is_some_and(|rest| called(rest, true))
                })
                || text
                    .match_indices(&format!("::{name}"))
                    .any(|(at, matched)| {
                        text.get(at + matched.len()..)
                            .is_some_and(|rest| called(rest, false))
                    })
        })
}

/// Whether a method is called on the `matches!` result itself, through only
/// the parentheses or one-expression blocks that wrap it (`matches!(..).eq(&false)`,
/// `(matches!(..)).then(|| false)`; #7931). A method can compare, invert or
/// discard the bool, so the pattern is not read. A method after a call that
/// takes the `matches!` as an argument (`xs.iter().filter(|x| matches!(..))
/// .count()`) is the call's, not the pattern's.
fn method_on_pattern(before: &str, after: &str) -> bool {
    // Only grouping parentheses and one-expression blocks wrap the pattern
    // (`({ matches!(..) }).then(..)`). A `(` after a name, `!` or closing
    // delimiter opens a call (`foo(matches!(..)).is_ok()`), and a `{` after
    // one opens a closure, `if` or `match` body (`|| { matches!(..) }`).
    let mut closers = Vec::new();
    let mut head = before.trim_end();
    while let Some((rest, closer)) = head
        .strip_suffix('(')
        .map(|rest| (rest, ')'))
        .or_else(|| head.strip_suffix('{').map(|rest| (rest, '}')))
    {
        let rest = rest.trim_end();
        if rest.ends_with(|ch: char| {
            ch.is_alphanumeric() || matches!(ch, '_' | '!' | ')' | ']' | '>' | '|')
        }) {
            break;
        }
        closers.push(closer);
        head = rest;
    }
    let mut rest = after.trim_start();
    let mut open = closers.as_slice();
    while let Some((closer, outer)) = open.split_first() {
        match rest.strip_prefix(*closer) {
            Some(next) => {
                rest = next.trim_start();
                open = outer;
            }
            None => break,
        }
    }
    rest.starts_with('.')
        && !rest.starts_with("..")
        && !(head.is_empty() && asserts_pattern_through_literal_calls(rest, open))
}

/// Whether `rest`, the method chain on a `matches!` result that starts the
/// condition, is only `eq` or `ne` calls against boolean literals that keep
/// the pattern's polarity (`.eq(&true)`, `.ne(&false)`,
/// `.eq(&false).eq(&false)`), followed by nothing but the `open` wrappers'
/// closers. Such a chain is the whole condition and asserts the pattern; any
/// other method, operator, enclosing call or chain (`flag && ..`,
/// `Not::not(..)`), or odd number of negations leaves it unread.
fn asserts_pattern_through_literal_calls(mut rest: &str, open: &[char]) -> bool {
    let mut negated = false;
    let mut calls = 0;
    while let Some(method) = rest.strip_prefix('.') {
        let Some((name, tail)) = ["eq", "ne"].iter().find_map(|name| {
            method
                .strip_prefix(name)
                .map(|tail| (*name, tail.trim_start()))
        }) else {
            return false;
        };
        let Some(close) = closing_parenthesis(tail) else {
            return false;
        };
        let Some(value) = tail.get(1..close).and_then(literal_spelling_value) else {
            return false;
        };
        negated ^= value == (name == "ne");
        calls += 1;
        rest = tail.get(close + 1..).unwrap_or_default().trim_start();
    }
    for closer in open {
        match rest.strip_prefix(*closer) {
            Some(next) => rest = next.trim_start(),
            None => return false,
        }
    }
    calls > 0 && !negated && rest.trim().is_empty()
}

/// The index of the `)` that closes the `(` starting `text`.
fn closing_parenthesis(text: &str) -> Option<usize> {
    if !text.starts_with('(') {
        return None;
    }
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (byte == b')').then_some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `before` (masked) ends inside the condition or scrutinee of an
/// `if`, `while` or `match` keyword whose condition is still open there
/// (`flag == if ready && matches!(..) { false } else { true }`).
fn in_keyword_condition(before: &str) -> bool {
    let is_word = |ch: char| ch.is_alphanumeric() || ch == '_';
    // `#` joins `r#match`, a raw identifier, not a keyword.
    let joins = |ch: char| is_word(ch) || ch == '#';
    ["if", "while", "match"].iter().any(|keyword| {
        before.match_indices(keyword).any(|(at, _)| {
            let starts_word = before.get(..at).is_none_or(|head| !head.ends_with(joins));
            let ends_word = before
                .get(at + keyword.len()..)
                .is_some_and(|rest| !rest.starts_with(is_word));
            starts_word
                && ends_word
                && condition_still_open(before.get(at + keyword.len()..).unwrap_or_default())
        })
    })
}

/// Whether `rest`, the text after a keyword, stays inside that keyword's
/// condition: it never closes the group holding the keyword (a guard in an
/// earlier `matches!(..)`) and opens no `{` block at the keyword's level.
fn condition_still_open(rest: &str) -> bool {
    let mut depth = 0usize;
    for byte in rest.bytes() {
        match byte {
            b'{' if depth == 0 => return false,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => match depth.checked_sub(1) {
                Some(next) => depth = next,
                None => return false,
            },
            _ => {}
        }
    }
    true
}

/// `text` with each top-level binary `&` (not `&&`, not a borrow) spelled
/// `&&`, so the conjunct rules read it. `&` binds tighter than a comparison,
/// so beside a top-level comparison it is a bitwise operand (`a & mask ==
/// b`) and stays.
fn conjoin_single_ampersands(text: &str) -> String {
    if top_level_comparison_operator(text).is_some() || has_top_level_relational_operator(text) {
        return text.to_string();
    }
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let mut out = String::with_capacity(text.len() + 4);
    let mut depth = 0usize;
    let mut copied = 0;
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let next = bytes.get(index + 1).copied();
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'&' if next == Some(b'&') => {
                index += 2;
                continue;
            }
            b'&' if depth == 0 => {
                let head = text.get(..index).unwrap_or_default();
                let binary = head.trim_end().ends_with(|ch: char| {
                    ch.is_alphanumeric() || matches!(ch, '_' | ')' | ']' | '}' | '?' | '"' | '\'')
                });
                if binary {
                    out.push_str(text.get(copied..index).unwrap_or_default());
                    out.push_str("&&");
                    copied = index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    out.push_str(text.get(copied..).unwrap_or_default());
    out
}

/// Whether `before` (masked) ends with a binary single `|`
/// (`ready | matches!(..)`), not `||` or the closing bar of a closure
/// parameter list (`|x| matches!(..)`). An opening closure bar follows an
/// operator or group start; a binary bar follows an operand.
fn binary_bar_before(before: &str) -> bool {
    let unopened = before.trim_end_matches(|ch: char| ch.is_whitespace() || ch == '(');
    if !unopened.ends_with('|') || unopened.ends_with("||") {
        return false;
    }
    // Scan only the innermost group still open at the pattern.
    let mut depth = 0usize;
    let mut start = 0;
    for (at, byte) in unopened.bytes().enumerate().rev() {
        match byte {
            b')' | b']' | b'}' => depth += 1,
            b'(' | b'[' | b'{' if depth == 0 => {
                start = at + 1;
                break;
            }
            b'(' | b'[' | b'{' => depth -= 1,
            _ => {}
        }
    }
    let group = unopened.get(start..).unwrap_or_default();
    let mut depth = 0usize;
    let mut in_params = false;
    let mut binary = false;
    let mut index = 0;
    let bytes = group.as_bytes();
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'|' if depth == 0 && !in_params && bytes.get(index + 1) == Some(&b'|') => {
                binary = false;
                index += 2;
                continue;
            }
            b'|' if depth == 0 && in_params => {
                in_params = false;
                binary = false;
            }
            b'|' if depth == 0 => {
                let head = group.get(..index).unwrap_or_default().trim_end();
                let operand_end = head.ends_with(|ch: char| {
                    ch.is_alphanumeric() || matches!(ch, '_' | ')' | ']' | '}' | '?')
                }) && !head.ends_with("move")
                    && !head.ends_with("return");
                binary = operand_end;
                in_params = !operand_end;
            }
            _ => {}
        }
        index += 1;
    }
    binary
}

fn unread_operator_before(before: &str) -> bool {
    match before.as_bytes().last() {
        Some(b'^' | b'<') => true,
        Some(b'>') => !(before.ends_with("->") || before.ends_with("=>")),
        Some(b'=') => before.ends_with("<=") || before.ends_with(">="),
        _ => false,
    }
}

/// Whether `expression` (masked) is a call, through parentheses, `!` and
/// `&`, with a whole `matches!` among its arguments, directly or as an
/// argument of a nested call. The callee is a path, method chain, turbofish
/// (`check::<bool>`) or call or index result (`make()`, `checks[0]`), never a
/// macro (`matches!(build(..), ..)`) or an expression with an operator.
fn call_argument_pattern(expression: &str) -> bool {
    let mut text = unwrap_parentheses(expression.trim());
    while let Some(rest) = text.strip_prefix(['!', '&']) {
        text = unwrap_parentheses(rest.trim());
    }
    let Some(open) = final_group_open(text) else {
        return false;
    };
    let (Some(callee), Some(arguments)) = (
        text.get(..open),
        text.get(open + 1..text.len().saturating_sub(1)),
    ) else {
        return false;
    };
    if !is_callee(callee) {
        return false;
    }
    split_top_level_commas(arguments).iter().any(|argument| {
        let mut argument = unwrap_parentheses(argument.trim());
        while let Some(rest) = argument.strip_prefix(['!', '&']) {
            argument = unwrap_parentheses(rest.trim());
        }
        is_complete_macro_invocation(argument, "matches!") || call_argument_pattern(argument)
    })
}

/// Where the parenthesis group that ends `text` opens, when `text` ends with
/// `)`.
fn final_group_open(text: &str) -> Option<usize> {
    if !text.ends_with(')') {
        return None;
    }
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for index in (0..bytes.len()).rev() {
        match bytes.get(index) {
            Some(b')' | b']' | b'}') => depth += 1,
            Some(b'(' | b'[' | b'{') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (bytes.get(index) == Some(&b'(')).then_some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `callee` (masked) is a call target with no operator at its top
/// level: identifiers, `::`, `.`, a turbofish, and call or index groups.
fn is_callee(callee: &str) -> bool {
    let callee = callee.trim_end();
    if callee.is_empty() || callee.ends_with('!') || callee.ends_with("->") {
        return false;
    }
    let mut depth = 0usize;
    callee.chars().all(|ch| match ch {
        '(' | '[' | '{' => {
            depth += 1;
            true
        }
        ')' | ']' | '}' => {
            depth = depth.saturating_sub(1);
            true
        }
        _ if depth > 0 => true,
        _ => ch.is_alphanumeric() || matches!(ch, '_' | ':' | '.' | '<' | '>' | ','),
    })
}

/// For a boolean assertion whose condition is a top-level `&&` chain with at
/// least one conjunct holding an unread `matches!` operand, the same
/// assertion over only the other conjuncts (`None` when the condition is not
/// such a chain, and an empty string when no other conjunct remains). The
/// conjunct with the unread pattern pins nothing, but the others are still
/// required, so they decide the reading, as beside a negated pattern.
pub(super) fn unread_pattern_conjunct_rest(line: &str) -> Option<String> {
    let (macro_name, condition) = boolean_assertion_macro_condition(line)?;
    let condition = unwrap_parentheses(condition.trim());
    if has_top_level_disjunction(condition) {
        return None;
    }
    let conjuncts = split_top_level_conjuncts(condition);
    let unread = |conjunct: &str| {
        contains_unread_pattern_operand(&format!("{macro_name}({}\n);", conjunct.trim()))
    };
    if conjuncts.len() < 2 || !conjuncts.iter().any(|conjunct| unread(conjunct)) {
        return None;
    }
    let rest = conjuncts
        .iter()
        .filter(|conjunct| !unread(conjunct))
        .map(|conjunct| conjunct.trim())
        .collect::<Vec<_>>();
    if rest.is_empty() {
        return Some(String::new());
    }
    // Each conjunct ends its own line, so a trailing `//` comment on one
    // cannot swallow the next conjunct or the closing `);`.
    Some(format!("{macro_name}({}\n);", rest.join("\n && ")))
}
