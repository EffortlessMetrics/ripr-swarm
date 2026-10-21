//! Same-file assertion macros called from tests (#9122).
//!
//! `assert_next_eq!(io, frame)` defined by a local `macro_rules!` hides its
//! assertion inside a token tree, so the test read as having no assertion.
//! This producer credits the call only from the macro's own transcriber text:
//!
//! - every arm of the unique, scoped, un-gated definition must itself show
//!   an assertion the ordinary extractor recognizes (`assert!`,
//!   `assert_eq!`, ...). A macro that only logs, or has one arm without such
//!   evidence, credits nothing. A hand-rolled `if bad { panic!() }` body
//!   shows no recognized assertion and stays the opaque custom assertion
//!   macro limit (#7089) rather than becoming an oracle here;
//! - the assertion must be attributable: in every arm, at least one
//!   recognized assertion names a `$fragment` that arm's matcher binds and is
//!   not a tautology of literals or of one operand against itself
//!   (`assert!(true)`, `assert_eq!(1, 1)`, `assert_eq!($x, $x)`). A
//!   transcriber that asserts nothing about its input proves nothing about
//!   the call. A macro that invokes itself, or whose matchers cannot take
//!   the call's argument count, credits nothing;
//! - the credit is capped at `unknown`: the call-site arguments, repetitions
//!   and conditional arms are not evaluated, so the transcriber cannot prove
//!   what the call pins and must never read as a weak/medium/strong oracle.
//!   It records that an assertion exists, the same grade the ordinary
//!   extractor gives a helper call whose callee it cannot see into;
//! - the call is a plain statement of the test's own body (see
//!   `syntax::local_macro_calls`), the test holds no `use` item, and the
//!   macro is defined in the test's own file. Macros from other files or
//!   crates (`tokio_test::assert_ready!`) contribute nothing.

use super::super::{OracleFact, RustIndex, TestFact};
use crate::analysis::extract::{
    extract_assertions, extract_identifier_tokens, mask_comments_and_strings,
    retain_reached_oracles,
};
use crate::analysis::syntax::{LocalMacroCall, MacroArm, local_macro_calls};
use crate::domain::{OracleKind, OracleStrength};
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) fn credit_same_file_assertion_macros(index: &mut RustIndex) {
    let candidate_files = index
        .tests()
        .iter()
        .map(|test| &test.file)
        .collect::<std::collections::BTreeSet<_>>();
    let files = index
        .files()
        .iter()
        .filter(|(file, facts)| {
            !facts.used_lexical_fallback
                && candidate_files.contains(file)
                && facts.source.contains("macro_rules")
        })
        .collect::<Vec<_>>();
    let calls_by_file: BTreeMap<PathBuf, Vec<LocalMacroCall>> = files
        .par_iter()
        .map(|(file, facts)| ((*file).clone(), local_macro_calls(&facts.source)))
        .filter(|(_, calls)| !calls.is_empty())
        .collect();
    if calls_by_file.is_empty() {
        return;
    }

    let mut credited: BTreeMap<(PathBuf, usize, String), Vec<OracleFact>> = BTreeMap::new();
    for test in index.tests().iter() {
        let Some(calls) = calls_by_file.get(&test.file) else {
            continue;
        };
        if test_holds_use_item(test) {
            continue;
        }
        let mut oracles = calls
            .iter()
            .filter(|call| {
                call.function == test.name
                    && test.start_line <= call.start_line
                    && call.end_line <= test.end_line
            })
            // Never for a call the ordinary extractor itself reads as an
            // assertion: a local macro that reuses a recognized assertion
            // macro's name keeps that macro's own grade. An unrelated
            // assertion sharing the line does not suppress the credit.
            .filter(|call| extract_assertions(&call.text, 1).is_empty())
            .filter_map(macro_call_oracle)
            .collect::<Vec<_>>();
        // The same never-run statements the ordinary extractor drops.
        retain_reached_oracles(&mut oracles, &test.body, test.start_line);
        if !oracles.is_empty() {
            credited.insert(
                (test.file.clone(), test.start_line, test.name.clone()),
                oracles,
            );
        }
    }
    if credited.is_empty() {
        return;
    }
    index.for_each_test_mut(|test| {
        if let Some(oracles) =
            credited.get(&(test.file.clone(), test.start_line, test.name.clone()))
        {
            test.assertions.extend(oracles.iter().cloned());
            test.assertions
                .sort_by(|left, right| left.line.cmp(&right.line).then(left.text.cmp(&right.text)));
        }
    });
}

/// A `use` item can import a same-named macro over the local definition.
fn test_holds_use_item(test: &TestFact) -> bool {
    mask_comments_and_strings(&test.body)
        .split(|character: char| !(character.is_alphanumeric() || character == '_'))
        .any(|word| word == "use")
}

/// The oracle one call earns: attributed in every arm, capped at unknown.
fn macro_call_oracle(call: &LocalMacroCall) -> Option<OracleFact> {
    if !call_arguments_fit_an_arm(call) {
        return None;
    }
    for arm in &call.arms {
        if !arm_attributes_an_assertion(&call.macro_name, arm) {
            return None;
        }
    }
    let text = call.text.trim().to_string();
    Some(OracleFact {
        line: call.start_line,
        kind: OracleKind::Unknown,
        strength: OracleStrength::Unknown,
        observed_tokens: extract_identifier_tokens(&text),
        text,
        ok_value_observed: None,
    })
}

/// Whether some arm's matcher can take the call's top-level argument count.
/// A matcher with a repetition or a non-comma separator is not counted and
/// fits; the every-arm attribution rule still applies to it.
fn call_arguments_fit_an_arm(call: &LocalMacroCall) -> bool {
    let Some(arguments) = call_arguments(&call.text) else {
        return false;
    };
    let given = top_level_arguments(arguments).len();
    call.arms.iter().any(|arm| {
        let Some(matcher) = delimited_inner(&arm.matcher) else {
            return false;
        };
        let masked = mask_comments_and_strings(matcher);
        if masked.contains("$(") || masked.contains(';') || masked.contains("=>") {
            return true;
        }
        top_level_arguments(matcher).len() == given
    })
}

/// The text between a call's outer delimiters.
fn call_arguments(call_text: &str) -> Option<&str> {
    let (_, rest) = call_text.trim().split_once('!')?;
    delimited_inner(rest.trim())
}

fn delimited_inner(text: &str) -> Option<&str> {
    let text = text.trim();
    let mut characters = text.chars();
    let open = characters.next()?;
    let close = characters.next_back()?;
    matches!((open, close), ('(', ')') | ('[', ']') | ('{', '}'))
        .then(|| &text[open.len_utf8()..text.len() - close.len_utf8()])
}

/// Comma-split at bracket depth zero; an empty list is zero arguments.
fn top_level_arguments(text: &str) -> Vec<String> {
    let masked = mask_comments_and_strings(text);
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for character in masked.chars() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .collect()
}

/// Whether an arm holds a recognized assertion over one of its own fragments.
fn arm_attributes_an_assertion(macro_name: &str, arm: &MacroArm) -> bool {
    let masked = mask_comments_and_strings(&arm.transcriber);
    if invokes_macro(&masked, macro_name) {
        return false;
    }
    let fragments = matcher_fragments(&arm.matcher);
    if fragments.is_empty() {
        return false;
    }
    if may_not_run_every_time(&masked) {
        return false;
    }
    let mut assertions = extract_assertions(&arm.transcriber, 1);
    retain_reached_oracles(&mut assertions, &arm.transcriber, 1);
    assertions.iter().any(|assertion| {
        rank(&assertion.strength) > rank(&OracleStrength::Unknown)
            && assertion_uses_fragment(&assertion.text, &fragments)
            && !is_tautology(&assertion.text)
    })
}

/// Fail-closed: a transcriber with a branch, loop, match, closure, early
/// exit or repetition may skip its assertion, so none of it is credited.
fn may_not_run_every_time(masked: &str) -> bool {
    const KEYWORDS: [&str; 10] = [
        "if", "else", "while", "for", "loop", "match", "return", "break", "continue", "async",
    ];
    if masked.contains("$(") {
        return true;
    }
    if masked
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
        .any(|word| KEYWORDS.contains(&word))
    {
        return true;
    }
    // A closure opens with `|` where no operand can precede it.
    masked.match_indices('|').any(|(index, _)| {
        let before = masked[..index].trim_end();
        before.is_empty()
            || before.ends_with("move")
            || before.ends_with(['(', ',', '=', '{', ';', ':'])
    })
}

/// A transcriber that calls its own macro recurses into arms read nowhere.
fn invokes_macro(masked: &str, name: &str) -> bool {
    let needle = format!("{name}!");
    masked.match_indices(&needle).any(|(index, _)| {
        !masked[..index]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_')
    })
}

/// `$name` fragments an arm's matcher binds (`$name:kind`).
fn matcher_fragments(matcher: &str) -> Vec<String> {
    let masked = mask_comments_and_strings(matcher);
    let characters = masked.chars().collect::<Vec<_>>();
    let mut fragments = Vec::new();
    for (index, character) in characters.iter().enumerate() {
        if *character != '$' {
            continue;
        }
        let name = characters[index + 1..]
            .iter()
            .take_while(|c| c.is_alphanumeric() || **c == '_')
            .collect::<String>();
        let after = index + 1 + name.chars().count();
        if !name.is_empty() && characters.get(after) == Some(&':') {
            fragments.push(name);
        }
    }
    fragments
}

fn assertion_uses_fragment(assertion: &str, fragments: &[String]) -> bool {
    let Some((_, operands)) = leading_operands(assertion) else {
        return false;
    };
    let asserted = operands.join(" , ");
    fragments.iter().any(|name| {
        let needle = format!("${name}");
        asserted.match_indices(&needle).any(|(index, _)| {
            !asserted[index + needle.len()..]
                .chars()
                .next()
                .is_some_and(|after| after.is_alphanumeric() || after == '_')
        })
    })
}

/// The macro name and its asserted operands: one for `assert!`, two for the
/// rest. Later arguments are diagnostic messages and prove nothing. `None`
/// when the shape is not read (generic turbofish commas are not split).
fn leading_operands(assertion: &str) -> Option<(String, Vec<String>)> {
    let masked = mask_comments_and_strings(assertion);
    if masked.contains("::<") {
        return None;
    }
    let (name, rest) = masked.trim().split_once('!')?;
    let name = name.rsplit("::").next().unwrap_or(name).trim();
    let name = name.strip_prefix("debug_").unwrap_or(name).to_string();
    let inner = delimited_inner(rest.trim().trim_end_matches(';'))?;
    let count = if name == "assert" { 1 } else { 2 };
    let operands = top_level_arguments(inner).into_iter().take(count).collect();
    Some((name, operands))
}

/// `assert!(true)`-style conditions and `assert_eq!(a, a)`-style comparisons
/// of one operand with itself, or of two literals, prove nothing.
fn is_tautology(assertion: &str) -> bool {
    let Some((_, operands)) = leading_operands(assertion) else {
        return true;
    };
    match operands.as_slice() {
        [condition] => constant_condition(condition),
        [left, right] => same_operand(left, right) || (is_literal(left) && is_literal(right)),
        _ => true,
    }
}

fn is_literal(argument: &str) -> bool {
    let argument = argument.trim();
    argument == "true"
        || argument == "false"
        || (!argument.is_empty()
            && argument
                .chars()
                .all(|c| c.is_ascii_digit() || c == '_' || c == '.'))
}

/// Whitespace-free operand with trailing `.clone()` calls removed.
fn normalized_operand(operand: &str) -> String {
    let mut text = operand
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    while let Some(stripped) = text.strip_suffix(".clone()") {
        text = stripped.to_string();
    }
    text
}

fn same_operand(left: &str, right: &str) -> bool {
    normalized_operand(left) == normalized_operand(right)
}

/// Split at a top-level operator outside brackets.
fn split_top<'a>(text: &'a str, operator: &str) -> Vec<&'a str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            _ if depth == 0 && bytes[index..].starts_with(operator.as_bytes()) => {
                parts.push(&text[start..index]);
                index += operator.len();
                start = index;
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(&text[start..]);
    parts
}

/// The inside of `( .. )` when the opening paren closes at the last char.
fn parenthesized_whole(text: &str) -> Option<&str> {
    if !text.starts_with('(') || !text.ends_with(')') {
        return None;
    }
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return (index + 1 == text.len()).then(|| &text[1..index]);
                }
            }
            _ => {}
        }
    }
    None
}

/// A condition that is constant whatever the fragments hold: a literal,
/// `x == x`, `x || true`, `true || x`, `!false`.
fn constant_condition(condition: &str) -> bool {
    let condition = condition.trim();
    if is_literal(condition) {
        return true;
    }
    if let Some(inner) = parenthesized_whole(condition) {
        return constant_condition(inner);
    }
    let alternatives = split_top(condition, "||");
    if alternatives.len() > 1 {
        return alternatives.iter().any(|part| constant_condition(part));
    }
    let conjuncts = split_top(condition, "&&");
    if conjuncts.len() > 1 {
        return conjuncts.iter().all(|part| constant_condition(part));
    }
    if let Some(negated) = condition.strip_prefix('!') {
        return is_literal(negated.trim());
    }
    for operator in ["==", "<=", ">="] {
        if let [left, right] = split_top(condition, operator).as_slice() {
            return same_operand(left, right);
        }
    }
    false
}

/// Evidence order: `Unknown` is the floor of what counts as evidence.
fn rank(strength: &OracleStrength) -> u8 {
    match strength {
        OracleStrength::Strong => 5,
        OracleStrength::Medium => 4,
        OracleStrength::Weak => 3,
        OracleStrength::Smoke => 2,
        OracleStrength::Unknown => 1,
        OracleStrength::None => 0,
    }
}
