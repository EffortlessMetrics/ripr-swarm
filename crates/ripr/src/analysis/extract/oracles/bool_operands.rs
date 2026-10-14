//! RIPR-SPEC-0231 rule 1 for boolean-valued operands (#7598): a comparison,
//! disjunction or `matches!` that an assertion only reads through a boolean
//! literal, a negation or an operator the classifier does not read pins no
//! value, so it never earns an exact kind.

use super::arguments::{
    AssertionComparison, assertion_comparison, boolean_assertion_macro_condition,
    is_complete_macro_invocation, split_top_level_commas,
};
use super::patterns::{
    boolean_literal_value, has_top_level_disjunction, has_top_level_exact_equality,
    pattern_conjunct_assertions, pattern_invocation_spans, split_top_level_conjuncts,
    strip_not_call, top_level_comparison_operator, unwrap_parentheses,
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
        masked.contains("matches!") || masked.contains("==") || masked.contains("!=")
    };
    let bitwise =
        comparison.is_none() && !relational && holds_test && has_top_level_bitwise_operator(core);
    if !(disjunction || conjunction || comparison.is_some() || relational || bitwise) {
        return None;
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
            return None;
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
        } else if let Some((operand, flips)) = literal_compared_operand(core) {
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
    let literal = |side: &str| boolean_literal_value(mask_comments_and_strings(side).trim());
    let (operand, value) = match (literal(left), literal(right)) {
        (None, Some(value)) => (left, value),
        (Some(value), None) => (right, value),
        _ => return None,
    };
    Some((unwrap_parentheses(operand.trim()), value == not_equal))
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
    let holds_test =
        relational || masked.contains("matches!") || masked.contains("==") || masked.contains("!=");
    (negated && holds_test)
        || relational
        || top_level_comparison_operator(core).and_then(|(at, width)| core.get(at..at + width))
            == Some("!=")
}

/// The non-literal operand of a comparison with a boolean literal, whether
/// the assertion states that operand is true, and whether the comparison is
/// a boolean assertion's whole condition rather than an equality macro's
/// operands.
fn compared_with_bool_literal(line: &str) -> Option<(String, bool, bool)> {
    let literal = |side: &str| boolean_literal_value(mask_comments_and_strings(side).trim());
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
        let after = masked
            .get(end..)
            .unwrap_or_default()
            .trim_start_matches(|ch: char| ch.is_whitespace() || ch == ')');
        unread_operator_before(before)
            || in_keyword_condition(before)
            || after.starts_with(['^', '<', '>'])
            || (after.starts_with('|') && !after.starts_with("||"))
    })
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
