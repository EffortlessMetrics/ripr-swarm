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
            // Never beside an assertion the ordinary extractor already read
            // on the call's lines: a local macro that reuses a recognized
            // assertion macro's name keeps that macro's own grade.
            .filter(|call| {
                !test
                    .assertions
                    .iter()
                    .any(|own| call.start_line <= own.line && own.line <= call.end_line)
            })
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
    let mut assertions = extract_assertions(&arm.transcriber, 1);
    retain_reached_oracles(&mut assertions, &arm.transcriber, 1);
    assertions.iter().any(|assertion| {
        rank(&assertion.strength) > rank(&OracleStrength::Unknown)
            && assertion_uses_fragment(&assertion.text, &fragments)
            && !is_tautology(&assertion.text)
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
    let masked = mask_comments_and_strings(assertion);
    fragments.iter().any(|name| {
        let needle = format!("${name}");
        masked.match_indices(&needle).any(|(index, _)| {
            !masked[index + needle.len()..]
                .chars()
                .next()
                .is_some_and(|after| after.is_alphanumeric() || after == '_')
        })
    })
}

/// `assert!(true)`-style conditions and `assert_eq!(a, a)`-style comparisons
/// of one operand with itself, or of two literals, prove nothing.
fn is_tautology(assertion: &str) -> bool {
    let masked = mask_comments_and_strings(assertion);
    let Some((_, rest)) = masked.trim().split_once('!') else {
        return false;
    };
    let Some(inner) = delimited_inner(rest.trim().trim_end_matches(';')) else {
        return false;
    };
    let arguments = top_level_arguments(inner);
    let is_literal = |argument: &str| {
        let argument = argument.trim();
        argument == "true"
            || argument == "false"
            || (!argument.is_empty()
                && argument
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '_' || c == '.'))
    };
    match arguments.as_slice() {
        [condition] => is_literal(condition),
        [left, right, ..] => left == right || (is_literal(left) && is_literal(right)),
        [] => true,
    }
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
