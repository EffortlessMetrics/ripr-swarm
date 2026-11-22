use super::super::rust_index::{FunctionSummary, TestSummary, extract_literals};
use super::activation::{
    boundary_constant_operand_name, comparison_operands, has_observed_boundary_equality,
    is_computed_value_expression, list_or_unknown, owner_input_values, returned_comparison_view,
};
use super::owner_pin::short_circuit::ShortCircuitTail;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::probes::same_connectives;
use crate::domain::*;
use crate::value_origins::LocatedActivationEvidence as ActivationEvidence;
#[cfg(test)]
use crate::value_origins::LocatedValueFact as ValueFact;
use ra_ap_syntax::{AstNode, ast};
use std::collections::BTreeMap;

/// #7970: whether ripr can show that no related test input evaluates the
/// changed operands of a pure `&&`/`||` predicate. Each run is the literal
/// `name = value` arguments of one owner call line. Fail closed toward
/// `false` (the ordinary reading stays): a table, builder or other input
/// source ripr cannot place on a call line, an unreadable argument, an
/// operand outside the pure-chain grammar, or a changed operator or operand
/// count all decline.
///
/// The owner's body must be exactly the chain (an earlier statement could
/// rebind a parameter), and every related test must reach the owner only by
/// direct calls whose arguments were read: a test that calls the owner
/// through a wrapper or a helper, or at all differently, may evaluate the
/// operand on an input this reading never saw.
///
/// Ladder order (#7970): the caller tries this after the missing-equality
/// rung and before the observed-boundary-equality rung. Moving it below that
/// rung lets an unchanged operand's boundary input read `Yes` again; moving
/// it above the missing-equality rung would hide a missing discriminator.
fn changed_operands_are_never_evaluated(
    probe: &Probe,
    activation: &ActivationEvidence,
    related_tests: &[&TestSummary],
    owner: &FunctionSummary,
) -> bool {
    let Some(before) = probe.before.as_deref() else {
        return false;
    };
    if !same_connectives(&probe.expression, before) {
        return false;
    }
    let Some(after) = ShortCircuitTail::parse(&owner.body, &probe.expression) else {
        return false;
    };
    let Some(prior) = after.with_chain(before) else {
        return false;
    };
    let parameters = after.parameters();
    let Some(runs) = owner_literal_runs(activation, related_tests, owner, parameters, &[]) else {
        return false;
    };
    after.changed_operands_never_evaluated(&prior, &runs)
}

/// One run per owner call written in a related test: the literal argument
/// of each owner parameter on that call line, `None` where it cannot be
/// read. `None` overall when a table, builder or other input source ripr
/// cannot place on a call line may feed the owner, or a related test reaches
/// the owner other than by direct calls with plain literal arguments.
fn owner_literal_runs(
    activation: &ActivationEvidence,
    related_tests: &[&TestSummary],
    owner: &FunctionSummary,
    parameters: &[String],
    table_tests: &[&TestSummary],
) -> Option<Vec<Vec<Option<String>>>> {
    // A related test that never writes the owner's name cannot call it when
    // no other function holds the owner (the caller's `owner_has_other_callers`
    // check covers helpers and macros), unless it calls another related test.
    // Such a bystander contributes no run, and its own table rows and
    // arguments are not inputs to the owner.
    // Any whole-word mention counts, not only a call: `let g = owner;` holds
    // the owner as a value and a later `g(..)` is a call this scan cannot see.
    let reaches = |test: &TestSummary| mentions_word(&test.body, &owner.name);
    let bystander = |test: &TestSummary| {
        // A constant-row table test is read row by row by
        // `table_test_runs`; its argument facts are not call-line inputs.
        table_tests
            .iter()
            .any(|table| table.name == test.name && table.file == test.file)
            || !reaches(test)
                && !test.calls.iter().any(|call| {
                    related_tests
                        .iter()
                        .any(|other| other.name == call.name && other.name != test.name)
                })
    };
    let readers = related_tests
        .iter()
        .copied()
        .filter(|test| !bystander(test))
        .collect::<Vec<_>>();
    if readers.is_empty()
        || !readers
            .iter()
            .all(|test| test_reaches_owner_only_directly(test, &owner.name))
    {
        return None;
    }
    let in_bystander = |fact: &crate::value_origins::LocatedValueFact| {
        related_tests.iter().any(|test| {
            bystander(test)
                && (test.start_line..=test.end_line).contains(&fact.line)
                && fact.file.as_ref().is_none_or(|file| *file == test.file)
        })
    };
    // Keyed by file too: two test files can put an owner call on the same line.
    let mut lines: BTreeMap<(Option<std::path::PathBuf>, usize), BTreeMap<String, Option<String>>> =
        BTreeMap::new();
    for fact in &activation.observed_values {
        if in_bystander(fact) {
            continue;
        }
        match fact.context {
            ValueContext::FunctionArgument => {}
            ValueContext::AssertionArgument => continue,
            // Inputs ripr cannot place on a call line may evaluate the operand.
            _ => return None,
        }
        // A derived fact such as `x == 0 || y == 1` is not an argument.
        let Some((name, value)) = fact.value.split_once(" = ") else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return None;
        }
        // The same parameter twice on a line is two calls; neither is read.
        let line = lines.entry((fact.file.clone(), fact.line)).or_default();
        let read = if line.contains_key(name) {
            None
        } else {
            Some(value.trim().to_string())
        };
        line.insert(name.to_string(), read);
    }
    // One run per owner call written in a related test; a call line with no
    // readable arguments is a run that cannot be shown to short-circuit.
    let empty = BTreeMap::new();
    let runs = readers
        .iter()
        .flat_map(|test| test.body_calls().map(move |call| (*test, call)))
        .filter(|(_, call)| call.name == owner.name)
        .map(|(test, call)| {
            let line = lines
                .get(&(Some(test.file.clone()), call.line))
                .or_else(|| lines.get(&(None, call.line)))
                .unwrap_or(&empty);
            parameters
                .iter()
                .map(|name| line.get(name).cloned().flatten())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    Some(runs)
}

/// #6488: whether the related tests call the owner, directly and with plain
/// literal arguments only, on rows that tell its all-bool `&&`/`||` body
/// from each one-step mutation (see
/// `ShortCircuitTail::runs_tell_apart_one_step_mutants`). The chain must be
/// the owner's whole body, so the arguments are the values it reads.
fn boolean_truth_table_is_exercised(
    probe: &Probe,
    activation: &ActivationEvidence,
    related_tests: &[&TestSummary],
    owner: &FunctionSummary,
) -> bool {
    let Some(chain) = ShortCircuitTail::parse(&owner.body, &probe.expression) else {
        return false;
    };
    // A row counts only when it is live and its result is asserted bare: a
    // call under `#[ignore]`, an `if`, a closure or a tautology (`r == r`,
    // `|| true`) separates nothing at runtime. A test that feeds its rows
    // through one constant `for` table (`table_test_runs`) is read row by row.
    let mentioning = related_tests
        .iter()
        .copied()
        .filter(|test| !word_call_offsets(&test.body, &owner.name).is_empty())
        .collect::<Vec<_>>();
    if mentioning.is_empty() {
        return false;
    }
    let mut table_tests = Vec::new();
    let mut runs = Vec::new();
    for test in &mentioning {
        if let Some(rows) = table_test_runs(test, &owner.name, chain.parameters()) {
            table_tests.push(*test);
            runs.extend(rows);
        } else if !only_bare_row_assertions(test, &owner.name) {
            return false;
        }
    }
    if table_tests.len() < mentioning.len() {
        let Some(direct) = owner_literal_runs(
            activation,
            related_tests,
            owner,
            chain.parameters(),
            &table_tests,
        ) else {
            return false;
        };
        runs.extend(direct);
    }
    chain.runs_tell_apart_one_step_mutants(&runs)
}

/// RIPR-SPEC-0353: the rows of a test whose whole body is one constant
/// `for` table asserting the owner against a boolean column,
///
/// ```text
/// let rows = [(true, true, true), (true, false, false)];
/// for (paid, in_stock, want) in rows {
///     assert_eq!(owner(paid, in_stock), want);
/// }
/// ```
///
/// (the `let` is optional; the table may be written inline, but a `let` must be
/// the table the loop iterates). One run per
/// row, in the owner's parameter order. `None` unless the test is a plain
/// `#[test]` holding nothing else, every argument is a loop variable or a
/// boolean literal, the expected value is a loop variable, and every cell of every column read is a boolean literal. A
/// table that `constant_table_column` cannot establish (a `continue`, an
/// `if`, a non-constant row, a rebound name) declines.
fn table_test_runs(
    test: &TestSummary,
    owner: &str,
    parameters: &[String],
) -> Option<Vec<Vec<Option<String>>>> {
    if test.attrs.iter().any(|attr| attr.trim() != "#[test]") {
        return None;
    }
    let body = test.body.as_ref();
    let (open, close) = (body.find('{')?, body.rfind('}')?);
    if open >= close {
        return None;
    }
    let inner = &body[open + 1..close];
    let masked = mask_comments_and_strings(inner);
    if masked.len() != inner.len() || inner.contains("//") || inner.contains("/*") {
        return None;
    }
    let mut rest = masked.trim();
    let mut bound_table = None;
    if let Some(after_let) = rest.strip_prefix("let ") {
        let (name, table) = after_let.split_once('=')?;
        let name = name.trim();
        if !is_plain_identifier(name) {
            return None;
        }
        bound_table = Some(name);
        let end = matching_close(table.trim_start(), '[', ']')?;
        let table = table.trim_start();
        rest = table[end + 1..]
            .trim_start()
            .strip_prefix(';')?
            .trim_start();
    }
    let after_for = rest.strip_prefix("for ")?;
    let (pattern, after_in) = after_for.split_once(" in ")?;
    let pattern = pattern.trim().strip_prefix('(')?.strip_suffix(')')?;
    let variables = pattern
        .split(',')
        .map(str::trim)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if variables.len() < 2 || variables.iter().any(|name| !is_plain_identifier(name)) {
        return None;
    }
    let after_in = after_in.trim_start();
    // A `let` is read only as the table the loop iterates, so its
    // initializer is validated as constant rows and cannot hide a `return`
    // or a panic ahead of an inline table.
    let body_open = match bound_table {
        Some(name) => {
            let iterable = &after_in[..after_in.find(char::is_whitespace)?];
            if iterable != name {
                return None;
            }
            iterable.len()
        }
        None if after_in.starts_with('[') => matching_close(after_in, '[', ']')? + 1,
        None => return None,
    };
    let loop_body = after_in[body_open..]
        .trim_start()
        .strip_prefix('{')?
        .trim_end()
        .strip_suffix('}')?
        .trim();
    let statement = loop_body.strip_suffix(';')?.trim();
    if statement.contains(';') {
        return None;
    }
    let arguments = statement.strip_prefix("assert_eq!(")?.strip_suffix(')')?;
    let parts = split_top_level_commas(arguments)?;
    // An optional message must be a single string literal (its contents are
    // masked to spaces), so no call hides in a format argument.
    let plain_message = |part: &str| {
        let part = part.trim();
        let inside = part
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or(part);
        inside.trim().is_empty()
    };
    if !(parts.len() == 2 || (parts.len() == 3 && plain_message(parts[2]))) {
        return None;
    }
    let call = parts[0].trim();
    let want = parts[1].trim();
    let call_arguments = call
        .strip_prefix(owner)?
        .trim_start()
        .strip_prefix('(')?
        .strip_suffix(')')?;
    let call_arguments = split_top_level_commas(call_arguments)?;
    if call_arguments.len() != parameters.len() {
        return None;
    }
    let column = |name: &str| -> Option<Vec<String>> {
        let column = crate::analysis::syntax::constant_table_column(&test.body, name)?;
        column
            .iter()
            .all(|cell| matches!(cell.as_str(), "true" | "false"))
            .then_some(column)
    };
    let value_of = |text: &str| -> Option<Result<Vec<String>, String>> {
        let text = text.trim();
        if matches!(text, "true" | "false") {
            return Some(Err(text.to_string()));
        }
        variables.iter().any(|name| name == text).then_some(())?;
        column(text).map(Ok)
    };
    let want_values = match value_of(want)? {
        Ok(values) => values,
        Err(_) => return None,
    };
    let rows = want_values.len();
    if rows == 0 {
        return None;
    }
    let argument_values = call_arguments
        .iter()
        .map(|argument| value_of(argument))
        .collect::<Option<Vec<_>>>()?;
    if argument_values
        .iter()
        .any(|value| matches!(value, Ok(values) if values.len() != rows))
    {
        return None;
    }
    Some(
        (0..rows)
            .map(|row| {
                argument_values
                    .iter()
                    .map(|value| match value {
                        Ok(values) => Some(values[row].clone()),
                        Err(literal) => Some(literal.clone()),
                    })
                    .collect()
            })
            .collect(),
    )
}

/// The byte offset of the bracket closing the `open` that `text` starts with.
fn matching_close(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (at, character) in text.char_indices() {
        if character == open {
            depth += 1;
        } else if character == close {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(at);
            }
        }
    }
    None
}

fn is_plain_identifier(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|character: char| character.is_ascii_digit())
        && text.bytes().all(is_ident_byte)
}

/// `text` split at its top-level commas (outside any bracket).
fn split_top_level_commas(text: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0usize);
    for (at, character) in text.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    Some(parts)
}

/// `test` is a plain `#[test]` whose body is nothing but top-level
/// `assert!([!]owner(..))`, `assert_eq!`/`assert_ne!` of an owner call
/// against `true`/`false` (either order), each ending in `;`. The call's
/// arguments are plain literals (`owner_call_has_plain_literal_arguments`).
/// Anything else (a `let`, a branch, a message argument, a path-qualified
/// call, a trailing expression, an attribute besides `#[test]`) declines.
fn only_bare_row_assertions(test: &TestSummary, owner: &str) -> bool {
    if test.attrs.iter().any(|attr| attr.trim() != "#[test]") {
        return false;
    }
    let body = test.body.as_ref();
    let (Some(open), Some(close)) = (body.find('{'), body.rfind('}')) else {
        return false;
    };
    if open >= close {
        return false;
    }
    let inner = &body[open + 1..close];
    let masked = mask_comments_and_strings(inner);
    if masked.len() != inner.len() || inner.contains("//") || inner.contains("/*") {
        return false;
    }
    let mut statements = masked.split(';').collect::<Vec<_>>();
    if statements.pop().is_none_or(|rest| !rest.trim().is_empty()) || statements.is_empty() {
        return false;
    }
    let is_bool = |text: &str| matches!(text.trim(), "true" | "false");
    let is_call = |text: &str| {
        let text = text.trim();
        text.strip_prefix(owner).is_some_and(closes_at_end)
            && owner_call_has_plain_literal_arguments(text, owner)
    };
    statements.iter().all(|statement| {
        let statement = statement.trim();
        if let Some(argument) = statement
            .strip_prefix("assert!(")
            .and_then(|rest| rest.strip_suffix(')'))
        {
            return is_call(argument.trim().strip_prefix('!').unwrap_or(argument));
        }
        ["assert_eq!(", "assert_ne!("].iter().any(|macro_open| {
            statement
                .strip_prefix(macro_open)
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|arguments| {
                    // The call holds a comma, so split on the last comma
                    // when the call is first and the first when it is not.
                    let (first, last) = arguments.rsplit_once(',')?;
                    if is_call(first) && is_bool(last) {
                        return Some(());
                    }
                    let (first, rest) = arguments.split_once(',')?;
                    (is_bool(first) && is_call(rest)).then_some(())
                })
                .is_some()
        })
    })
}

/// RIPR-SPEC-0353: the returned-value sink of an all-bool `&&`/`||` owner
/// body whose related tests exercise its truth table, for the
/// `return_value` probe on that chain. `None` for anything else, so the
/// ordinary sink reading (unknown for a chain with no value marker) stays.
pub(in crate::analysis) fn exercised_bool_chain_return_sink(
    probe: &Probe,
    activation: &ActivationEvidence,
    related_tests: &[&TestSummary],
    owner: &FunctionSummary,
    owner_has_other_callers: bool,
    assertion_macros_rebound: bool,
) -> Option<FlowSinkFact> {
    (probe.family == ProbeFamily::ReturnValue
        && !assertion_macros_rebound
        && probe.owner.as_ref() == Some(&owner.id)
        && !owner_has_other_callers
        && probe
            .before
            .as_deref()
            .is_none_or(|before| same_connectives(&probe.expression, before))
        && boolean_truth_table_is_exercised(probe, activation, related_tests, owner))
    .then(|| FlowSinkFact {
        kind: FlowSinkKind::ReturnValue,
        text: probe.expression.trim().to_string(),
        line: probe.location.line,
        owner: Some(owner.id.clone()),
    })
}

/// Whether `test` calls the owner directly and routes nowhere else that
/// could reach it: it has an owner call in its own body, no call lifted
/// from a helper outside its body, no body call other than assertion
/// macros and plain constructors, and no macro that could expand to an
/// owner call. Every owner call is the only one on its line and has plain
/// decimal or boolean literal arguments: the argument extractor reads a hex
/// or suffixed literal as another number, and two calls on one line collapse
/// into one call fact.
fn test_reaches_owner_only_directly(test: &TestSummary, owner: &str) -> bool {
    const INERT: &[&str] = &["vec", "Some", "Ok", "Err", "format", "matches"];
    let inert = |name: &str| {
        name.starts_with("assert") || name.starts_with("debug_assert") || INERT.contains(&name)
    };
    let mut owner_calls = 0;
    for call in test.body_calls().filter(|call| call.name == owner) {
        owner_calls += 1;
        if !owner_call_has_plain_literal_arguments(&call.text, owner) {
            return false;
        }
    }
    owner_calls > 0
        && word_call_offsets(&test.body, owner).len() == owner_calls
        && only_std_assertion_macros(&test.body)
        && test.calls.iter().all(|call| {
            if (test.start_line..=test.end_line).contains(&call.line) {
                // The facts list the test's own declaration as a call.
                call.name == owner || call.name == test.name || inert(&call.name)
            } else {
                false
            }
        })
}

/// `text` holds a whole-word `name` (an identifier boundary on both sides).
fn mentions_word(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    text.match_indices(name).any(|(at, _)| {
        (at == 0 || !is_ident_byte(bytes[at - 1]))
            && bytes
                .get(at + name.len())
                .is_none_or(|byte| !is_ident_byte(*byte))
    })
}

/// `rest` is one parenthesized group whose closing parenthesis is its last
/// byte: `(true, false)` but not `(false, true) || (true)`.
fn closes_at_end(rest: &str) -> bool {
    if !rest.starts_with('(') {
        return false;
    }
    let mut depth = 0usize;
    for (index, byte) in rest.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return index + 1 == rest.len();
                }
            }
            _ => {}
        }
    }
    false
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Byte offsets of each `name(` call-shaped use of `name` in `text`.
fn word_call_offsets(text: &str, name: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    text.match_indices(name)
        .filter(|(at, _)| *at == 0 || !is_ident_byte(bytes[at - 1]))
        .filter(|(at, _)| {
            bytes[at + name.len()..]
                .iter()
                .find(|byte| !byte.is_ascii_whitespace())
                == Some(&b'(')
        })
        .map(|(at, _)| at)
        .collect()
}

/// `text` holds exactly one `owner(...)` call whose arguments are all plain
/// decimal integers or booleans and whose parenthesis closes on the line.
fn owner_call_has_plain_literal_arguments(text: &str, owner: &str) -> bool {
    let &[at] = word_call_offsets(text, owner).as_slice() else {
        return false;
    };
    let rest = &text[at + owner.len()..];
    let Some(open) = rest.find('(') else {
        return false;
    };
    let mut depth = 0usize;
    let mut close = None;
    for (index, byte) in rest.bytes().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return false;
    };
    let arguments = &rest[open + 1..close];
    !arguments.trim().is_empty()
        && arguments.split(',').all(|argument| {
            let argument = argument.trim();
            let digits = argument.strip_prefix('-').unwrap_or(argument);
            argument == "true"
                || argument == "false"
                || (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
        })
}

/// Every `name!(`/`name![`/`name!{` in `body` is a std assertion or
/// constructor macro. A user macro, even one defined elsewhere in the test
/// file, may expand to an owner call whose arguments this reading never saw.
fn only_std_assertion_macros(body: &str) -> bool {
    const ALLOWED: &[&str] = &[
        "assert",
        "assert_eq",
        "assert_ne",
        "debug_assert",
        "debug_assert_eq",
        "debug_assert_ne",
        "matches",
        "vec",
        "format",
    ];
    let bytes = body.as_bytes();
    bytes.iter().enumerate().all(|(at, byte)| {
        if *byte != b'!' || at == 0 || !is_ident_byte(bytes[at - 1]) {
            return true;
        }
        let invoked = bytes[at + 1..]
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            .is_some_and(|byte| matches!(byte, b'(' | b'[' | b'{'));
        let start = bytes[..at]
            .iter()
            .rposition(|byte| !is_ident_byte(*byte))
            .map_or(0, |index| index + 1);
        !invoked || ALLOWED.contains(&&body[start..at])
    })
}

#[cfg(test)]
pub(in crate::analysis) fn infection_evidence(
    probe: &Probe,
    related_tests: &[&TestSummary],
    activation: &ActivationEvidence,
) -> StageEvidence {
    infection_evidence_with_boundary_input(probe, related_tests, activation, None)
}

/// `infection_evidence` given the activation authority's reason, if any,
/// why the changed boundary's inputs cannot be read (#6674, #6693,
/// #6672). Such a boundary is unknown, not a missing input: it never
/// reads `weak` from a literal mismatch it cannot interpret.
#[cfg(test)]
pub(in crate::analysis) fn infection_evidence_with_boundary_input(
    probe: &Probe,
    related_tests: &[&TestSummary],
    activation: &ActivationEvidence,
    unresolved_boundary: Option<&str>,
) -> StageEvidence {
    infection_evidence_for_owner(
        probe,
        related_tests,
        activation,
        unresolved_boundary,
        OwnerInputs::default(),
        true,
        false,
    )
}

/// The changed owner and the literal byte-length read of its boundary, which
/// travel together: both come from the owner's own source.
#[derive(Clone, Copy, Default)]
pub(in crate::analysis) struct OwnerInputs<'a> {
    pub(in crate::analysis) owner: Option<&'a FunctionSummary>,
    pub(in crate::analysis) literal_length_boundary:
        Option<&'a super::literal_length_boundary::LiteralLengthBoundary>,
    pub(in crate::analysis) slice_counter_boundary:
        Option<&'a super::slice_counter_boundary::SliceCounterBoundary>,
    /// #14815: the owner's whole tail gained or lost a prefix `-`/`!`, with
    /// the base-side owner text the diff establishes.
    pub(in crate::analysis) unary_tail: Option<&'a super::UnaryTailChange>,
    /// #14816: one `if` condition replaced by a bool literal, with the
    /// base-side owner text the diff establishes.
    pub(in crate::analysis) literal_condition: Option<&'a super::LiteralConditionChange>,
}

const RSTEST_FIXTURE_INPUT: &str = "A related test passes the rstest fixture";
const UNREAD_BOUNDARY_LITERAL: &str = "A related test passes the changed boundary literal [";

/// Whether `infect` is an [`opaque_boundary_input`] reading (#15343,
/// #15344): its summary already names why the input is unknown, so a
/// generic "no detected boundary input" line would contradict it.
pub(super) fn is_opaque_boundary_input(infect: &StageEvidence) -> bool {
    infect.state == StageState::Unknown
        && (infect.summary.starts_with(RSTEST_FIXTURE_INPUT)
            || infect.summary.starts_with(UNREAD_BOUNDARY_LITERAL))
}

/// Why a related test's input to `owner` is unreadable when no boundary
/// input was found: a boundary literal passed to a macro whose expansion
/// ripr does not read (a test-local wrapper such as `check!(100, 0)` may
/// pass it to the owner, #15344), or an owner call fed by an rstest
/// fixture parameter, whose value the `#[fixture]` supplies (#15343).
fn opaque_boundary_input(
    related_tests: &[&TestSummary],
    owner: &str,
    probe_literals: &[String],
) -> Option<String> {
    for test in related_tests {
        // Only a macro the test itself defines: any other unread macro
        // (`info!`, `assert_that!`) is no evidence that its arguments feed
        // the owner.
        let unread = unread_macro_arguments(&test.body);
        for (name, _, arguments) in &unread {
            if !local_macro_calls(&test.body, name, owner) {
                continue;
            }
            let literals = extract_literals(arguments);
            if let Some(boundary) = probe_literals
                .iter()
                .find(|literal| literals.contains(literal))
            {
                return Some(format!(
                    "{UNREAD_BOUNDARY_LITERAL}{boundary}] to `{name}!`, whose expansion ripr does not read, so whether it reaches the changed owner as an input is unknown"
                ));
            }
        }
        // A same-file wrapper macro's spliced expansion (#9122) shows the
        // owner call in the body text, but activation does not read its
        // arguments as owner inputs. An owner call the call facts hold with
        // the boundary is one activation saw and declined (an ignored test,
        // another receiver), so its reason stands.
        let declined = test
            .calls
            .iter()
            .filter(|call| call.name == owner)
            .flat_map(|call| call_arguments(call.text.as_ref(), owner))
            .flat_map(|arguments| extract_literals(&arguments))
            .any(|literal| probe_literals.contains(&literal));
        if declined {
            continue;
        }
        // An owner call inside an unread macro's arguments is no evidence
        // of an input: the macro may discard it (`discard!(pick(100))`,
        // which call extraction masks), and a local macro that does pass
        // its arguments to the owner was read above.
        let mut last_end = 0;
        let outermost = unread
            .iter()
            .map(|(_, range, _)| range.clone())
            .filter(|range| {
                let keep = range.start >= last_end
                    && test.body.is_char_boundary(range.start)
                    && test.body.is_char_boundary(range.end);
                if keep {
                    last_end = range.end;
                }
                keep
            })
            .collect::<Vec<_>>();
        let outside_macros =
            crate::analysis::extract::property_macros::mask_evidence_ranges(&test.body, outermost);
        if let Some(boundary) = call_arguments(&outside_macros, owner)
            .iter()
            .flat_map(|arguments| extract_literals(arguments))
            .find(|literal| probe_literals.contains(literal))
        {
            return Some(format!(
                "{UNREAD_BOUNDARY_LITERAL}{boundary}] to `{owner}` in a form ripr does not read as an owner input (for example a wrapper macro's expansion), so whether it activates the change is unknown"
            ));
        }
        let fixtures = crate::analysis::value_resolution::rstest_fixture_parameters(test);
        if fixtures.is_empty() {
            continue;
        }
        for call in test.calls.iter().filter(|call| call.name == owner) {
            // Only an owner call whose arguments are all unread: beside a
            // visible literal (`pick(&ctx, 50)`) the fixture may fill another
            // parameter, so the readable input's gap stands.
            if let Some(fixture) = fixtures.iter().find(|fixture| {
                call_arguments(call.text.as_ref(), owner)
                    .iter()
                    .any(|arguments| {
                        extract_literals(arguments).is_empty()
                            && super::activation::free_identifiers(arguments).contains(fixture)
                    })
            }) {
                return Some(format!(
                    "{RSTEST_FIXTURE_INPUT} `{fixture}` to `{owner}`; ripr does not read the fixture's value, so whether it sits on the changed boundary [{}] is unknown",
                    list_or_unknown(probe_literals)
                ));
            }
        }
    }
    None
}

/// The argument text of each invocation in `body` of a macro other than the
/// standard ones ripr reads ([`super::activation::READ_MACROS`]) and
/// `macro_rules!`, with its byte range in `body`; comments and strings are
/// masked for the scan.
fn unread_macro_arguments(body: &str) -> Vec<(String, std::ops::Range<usize>, String)> {
    let masked = mask_comments_and_strings(body);
    let bytes = masked.as_bytes();
    let mut out = Vec::new();
    for (bang, _) in masked.match_indices('!') {
        let Some(open) = bytes[bang + 1..]
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .map(|skip| bang + 1 + skip)
            .filter(|open| matches!(bytes[*open], b'(' | b'[' | b'{'))
        else {
            continue;
        };
        let name_end = masked[..bang].trim_end().len();
        let name_start = masked[..name_end]
            .char_indices()
            .rev()
            .find(|(_, ch)| !(ch.is_ascii_alphanumeric() || *ch == '_'))
            .map_or(0, |(index, ch)| index + ch.len_utf8());
        let name = &masked[name_start..name_end];
        if name.is_empty()
            || RUST_KEYWORDS.contains(&name)
            || name == "macro_rules"
            || super::activation::READ_MACROS.contains(&name)
        {
            continue;
        }
        let Some(close) = balanced_close(bytes, open) else {
            continue;
        };
        if let Some(arguments) = body.get(open + 1..close) {
            out.push((name.to_string(), open + 1..close, arguments.to_string()));
        }
    }
    out
}

/// Keywords that can precede a `!` negation (`if !(a == b)`).
const RUST_KEYWORDS: &[&str] = &[
    "if", "while", "match", "return", "in", "let", "else", "loop", "break", "yield", "move",
];

/// Whether `body` holds a `macro_rules! name` definition whose rules call
/// `owner`: a local macro that never calls it (`approx!(pick(50), 100)`)
/// cannot pass the boundary to the owner, so its literals stay expected
/// values.
fn local_macro_calls(body: &str, name: &str, owner: &str) -> bool {
    let masked = mask_comments_and_strings(body);
    let bytes = masked.as_bytes();
    masked.match_indices("macro_rules").any(|(at, _)| {
        let after = at + "macro_rules".len();
        let rest = masked[after..].trim_start();
        let Some(rest) = rest.strip_prefix('!').map(str::trim_start) else {
            return false;
        };
        let Some(tail) = rest.strip_prefix(name) else {
            return false;
        };
        if tail
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return false;
        }
        let tail_at = masked.len() - tail.len();
        let Some(open) = bytes[tail_at..]
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .map(|skip| tail_at + skip)
            .filter(|open| matches!(bytes[*open], b'(' | b'[' | b'{'))
        else {
            return false;
        };
        balanced_close(bytes, open)
            .and_then(|close| body.get(open + 1..close))
            .is_some_and(|rules| !call_arguments(rules, owner).is_empty())
    })
}

/// Offset of the delimiter closing the one at `open` in masked text.
fn balanced_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// The argument text of each `owner(..)` call in `text`.
fn call_arguments(text: &str, owner: &str) -> Vec<String> {
    let masked = mask_comments_and_strings(text);
    let bytes = masked.as_bytes();
    let is_word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    masked
        .match_indices(owner)
        .filter_map(|(at, _)| {
            let end = at + owner.len();
            if at > 0 && is_word(bytes[at - 1]) {
                return None;
            }
            let open = bytes[end..]
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .map(|skip| end + skip)
                .filter(|open| bytes[*open] == b'(')?;
            let close = balanced_close(bytes, open)?;
            text.get(open + 1..close).map(str::to_string)
        })
        .collect()
}

/// [`infection_evidence_with_boundary_input`] given the changed owner, which
/// the `&&`/`||` skipped-operand reading (#7970) needs to know the owner's
/// parameters and that its body is the whole chain. Without an owner, or when
/// any function other than the related tests calls the owner by name
/// (`owner_has_other_callers`: a wrapper or helper may feed it other inputs),
/// that reading declines. The truth-table reading (#6488) also declines when
/// the workspace rebinds `assert`, `assert_eq` or `assert_ne`
/// (`assertion_macros_rebound`, #10386): it trusts those names to compare, and
/// a shadowing `macro_rules!` compares nothing.
pub(in crate::analysis) fn infection_evidence_for_owner(
    probe: &Probe,
    related_tests: &[&TestSummary],
    activation: &ActivationEvidence,
    unresolved_boundary: Option<&str>,
    owner_inputs: OwnerInputs<'_>,
    owner_has_other_callers: bool,
    assertion_macros_rebound: bool,
) -> StageEvidence {
    let OwnerInputs {
        owner,
        literal_length_boundary,
        slice_counter_boundary,
        unary_tail,
        literal_condition,
    } = owner_inputs;
    if !matches!(probe.family, ProbeFamily::Predicate)
        && let Some(reason) = unresolved_boundary.filter(|reason| {
            reason.starts_with(super::activation::ERROR_RETURN_GUARD_UNRESOLVED_MARKER)
        })
    {
        return StageEvidence::new(StageState::Unknown, Confidence::Low, reason.to_string());
    }
    // A returned bare comparison with a resolved missing boundary
    // discriminator reads the predicate lens's gap, never a better reading:
    // Weak or unresolved boundary evidence also applies to the returned
    // comparison (#9094, #10046).
    // #13040: an unseen input (a plain identifier bound to an env var or a
    // local helper call) yields neither a missing discriminator nor an
    // unresolved boundary, because the argument is not a computed
    // expression ripr flags as unreadable. The predicate lens still reads
    // `Unknown` there (no comparable literal input), so an `Unknown`
    // predicate reading applies when no owner input is readable at all
    // (`owner_input_values` empty: a visible input, even one the literal
    // reading cannot compare, keeps the return-value reading); a `Weak`
    // reading (a visible input that merely misses the boundary, including
    // an intermediate input that separates a multi-value move) keeps the
    // existing resolved-gap gate, so the return-value lens can still
    // credit separation the literal reading cannot see. The `Unknown`
    // delegation only applies when related tests reach the owner
    // (`related_tests` non-empty): with no related tests the return-value
    // summary stands, not the predicate one. The empty-input `Unknown`
    // arm is further scoped to a related test that directly calls the
    // owner (`test.calls` naming `owner`): a test linked only by a name
    // token or an unexpanded macro invocation (for example `check_small!`
    // mentioning `bulk`) does not delegate on that arm, and when the
    // owner is unknown (`owner` is `None`) that arm does not delegate at
    // all. The missing-discriminator and unresolved-boundary arms keep
    // their prior scope.
    if let Some(mut predicate) = returned_comparison_view(probe) {
        predicate.family = ProbeFamily::Predicate;
        let evidence = infection_evidence_for_owner(
            &predicate,
            related_tests,
            activation,
            unresolved_boundary,
            owner_inputs,
            owner_has_other_callers,
            assertion_macros_rebound,
        );
        let directly_calls_owner = owner.is_some_and(|owner| {
            related_tests
                .iter()
                .any(|test| test.calls.iter().any(|call| call.name == owner.name))
        });
        if (evidence.state == StageState::Unknown
            && !related_tests.is_empty()
            && ((owner_input_values(activation).is_empty() && directly_calls_owner)
                || !activation.missing_discriminators.is_empty()
                || unresolved_boundary.is_some()))
            || (matches!(evidence.state, StageState::Weak)
                && (!activation.missing_discriminators.is_empty() || unresolved_boundary.is_some()))
        {
            return evidence;
        }
    }
    if let Some(evidence) = literal_length_boundary
        .zip(owner)
        .and_then(|(boundary, owner)| boundary.infection(probe, owner))
    {
        return evidence;
    }
    if let Some(evidence) = slice_counter_boundary
        .zip(owner)
        .and_then(|(boundary, owner)| boundary.infection(probe, owner))
    {
        return evidence;
    }
    match probe.family {
        ProbeFamily::Predicate => {
            let probe_literals = boundary_literals(&probe.expression);
            // #11519: a predicate that spells no literal can still compare
            // against a local whose live initializer does
            // (`val >= surrogate_start` after `let surrogate_start =
            // 0xd800u32;`). Only a binding the predicate directly uses
            // supplies a boundary; no resolvable initializer keeps the
            // boundary unknown.
            let probe_literals = if probe_literals.is_empty() {
                local_initializer_boundary_literals(probe, owner).unwrap_or(probe_literals)
            } else {
                probe_literals
            };
            // A number, a char (`'x'`) and a byte (`b'x'`) share no
            // comparable boundary in this report, so only literals of a kind
            // the changed boundary uses count. The rest are kept to name the
            // mismatch when nothing comparable is left.
            let (test_literals, other_kind_literals): (Vec<_>, Vec<_>) = related_tests
                .iter()
                .flat_map(|test| test.literals.iter().map(|literal| literal.value.as_str()))
                .partition(|literal| {
                    probe_literals
                        .iter()
                        .any(|boundary| literal_kind(boundary) == literal_kind(literal))
                });
            // Only a literal that flows into the changed owner's inputs can
            // activate the boundary. The activation authority separates
            // owner-call arguments (and table/builder inputs) from assertion
            // arguments; an expected value such as the `2000` in
            // `assert_eq!(tax_bps("EU"), 2000)` is an oracle, not an input.
            let mut input_literals = owner_input_values(activation)
                .into_iter()
                .flat_map(extract_literals)
                .collect::<Vec<_>>();
            input_literals.sort();
            input_literals.dedup();
            let boundary_input_literals = probe_literals
                .iter()
                .filter(|literal| input_literals.contains(literal))
                .cloned()
                .collect::<Vec<_>>();
            let boundary_oracle_only_literals = probe_literals
                .iter()
                .filter(|literal| {
                    !input_literals.contains(literal) && test_literals.contains(&literal.as_str())
                })
                .cloned()
                .collect::<Vec<_>>();
            let changed_operands = probe
                .before
                .as_deref()
                .filter(|before| !same_connectives(&probe.expression, before));
            if related_tests.is_empty() {
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    "No tests were found, so activation/infection cannot be estimated",
                )
            } else if let Some((owner, change)) = owner.zip(literal_condition) {
                literal_condition_infection(
                    owner,
                    change,
                    related_tests,
                    owner_has_other_callers,
                    assertion_macros_rebound,
                )
            } else if activation
                .missing_discriminators
                .iter()
                .any(|fact| fact.shape == DiscriminatorShape::Boundary)
            {
                StageEvidence::new(
                    StageState::Weak,
                    Confidence::Medium,
                    "Related tests contain input values, but the equality-boundary check is missing",
                )
            } else if let Some(guard) = activation
                .missing_discriminators
                .iter()
                .find(|fact| fact.shape == DiscriminatorShape::GuardDomain)
            {
                // #12414: the inserted guard's domain is the missing input;
                // the SPEC-0263 wording the guard-entry route already emits
                // names the values that stay outside it.
                StageEvidence::new(StageState::Weak, Confidence::Medium, guard.reason.clone())
            } else if !owner_has_other_callers
                && owner.is_some_and(|owner| {
                    changed_operands_are_never_evaluated(probe, activation, related_tests, owner)
                })
            {
                // #7970: the edit keeps the `&&`/`||` operand count and
                // changes some operands, but every test input short-circuits
                // before reaching them (`x == 0 || y == 1` called only with
                // `x = 0`). An input at the *unchanged* operand's boundary
                // is not an input at the changed one.
                StageEvidence::new(
                    StageState::Weak,
                    Confidence::Medium,
                    format!(
                        "Every related test input short-circuits `{}` before its changed operand is evaluated, so no test input reaches the changed boundary",
                        probe.expression.trim()
                    ),
                )
            } else if changed_operands.is_none() && has_observed_boundary_equality(activation) {
                StageEvidence::new(
                    StageState::Yes,
                    Confidence::Medium,
                    crate::domain::BOUNDARY_INPUT_OBSERVED_SUMMARY,
                )
            } else if probe_literals.is_empty()
                && changed_operands.is_none()
                && unresolved_boundary.is_none()
                && !owner_has_other_callers
                && !assertion_macros_rebound
                && owner.is_some_and(|owner| {
                    boolean_truth_table_is_exercised(probe, activation, related_tests, owner)
                })
            {
                // #6488: a pure bool chain has no literal boundary; its
                // boundary is the truth table. The related tests call the
                // owner with literal rows that separate the chain from its
                // connective swap, both constants and any one negated operand.
                StageEvidence::new(
                    StageState::Yes,
                    Confidence::Medium,
                    format!(
                        "Related test calls give literal truth-table rows that separate `{}` from its connective swap, constant `true`/`false` and any single negated operand",
                        probe.expression.trim()
                    ),
                )
            } else if let Some(literal) = unresolved_boundary
                .is_none()
                .then_some(())
                // A rebound assertion macro may discard the owner call whose
                // argument is read below (#10386), as the truth-table branch
                // above also declines.
                .filter(|()| !owner_has_other_callers && !assertion_macros_rebound)
                .and(owner)
                .and_then(|owner| added_equality_arm_input(probe, activation, owner))
            {
                // #14388: the edit appends `|| param == "lit"` to a chain of
                // string equalities on the same parameter, and a related
                // test calls the owner with exactly `"lit"`. The original
                // arms compare other string literals, so they are all false
                // on that input while the new arm is true: the input reaches
                // the change.
                StageEvidence::new(
                    StageState::Yes,
                    Confidence::Medium,
                    format!(
                        "A related test passes {}, which makes the added `||` arm true while every original {}-equality arm is false, so the input reaches the change",
                        literal.0,
                        literal.1.label()
                    ),
                )
            } else if probe_literals.is_empty() {
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    unresolved_boundary.map_or_else(
                        || no_literal_boundary_summary(&probe.expression),
                        unresolved_boundary_summary,
                    ),
                )
            } else if let Some(reason) = unresolved_boundary {
                // An operand ripr cannot read is not credited by a test
                // literal that merely equals the boundary literal: `x.len()
                // > 3` is not reached by `f(3)` (CodeRabbit review on
                // #6796). Only the exact observed equality above is.
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    unresolved_boundary_summary(reason),
                )
            } else if let Some(before) = changed_operands.filter(|_| {
                has_observed_boundary_equality(activation) || !boundary_input_literals.is_empty()
            }) {
                // #7898: the edit adds or removes a `&&`/`||` operand
                // (`other > 0 && 100 <= points` to `100 < points`). An input
                // at the surviving comparison's boundary says nothing about
                // the operand that came or went, so the boundary is not
                // credited. Without such an input the weak readings below
                // still name the missing boundary test.
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    format!(
                        "The edit changes the predicate's `&&`/`||` operands (before: `{}`), so a test input at the changed comparison's boundary does not establish that the whole predicate's change is reached",
                        before.trim()
                    ),
                )
            } else if !boundary_input_literals.is_empty() {
                StageEvidence::new(
                    StageState::Yes,
                    Confidence::Medium,
                    format!(
                        "Detected test input literal matching changed boundary: {}",
                        list_or_unknown(&boundary_input_literals)
                    ),
                )
            } else if let Some(opaque) = owner.and_then(|owner| {
                opaque_boundary_input(related_tests, &owner.name, &probe_literals)
            }) {
                // #15343, #15344: the boundary may reach the owner through an
                // input ripr does not read, so calling it absent (or an
                // expected value) would be a wrong actionable gap.
                StageEvidence::new(StageState::Unknown, Confidence::Low, opaque)
            } else if !boundary_oracle_only_literals.is_empty() {
                StageEvidence::new(
                    StageState::Weak,
                    Confidence::Medium,
                    format!(
                        "Related tests contain the changed boundary literal [{}] only outside the changed owner's inputs (for example as an expected value); no test input at the changed boundary was detected",
                        list_or_unknown(&boundary_oracle_only_literals)
                    ),
                )
            } else if !test_literals.is_empty() {
                StageEvidence::new(
                    StageState::Weak,
                    Confidence::Medium,
                    format!(
                        "Tests have literals [{}], but no detected value matches changed boundary [{}]",
                        list_or_unknown(&test_literals),
                        list_or_unknown(&probe_literals)
                    ),
                )
            } else if !other_kind_literals.is_empty() {
                // #6902: the tests do pass literals, just not of the
                // boundary's kind, so the strings/fixtures reading below
                // would send the reader after the wrong cause.
                let mut other_kind_literals = other_kind_literals;
                other_kind_literals.sort();
                other_kind_literals.dedup();
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    format!(
                        "The only literals ripr reads in related tests are {} literals [{}], which ripr does not compare with the changed {} boundary [{}]; activation/infection is unknown",
                        literal_kinds_label(&other_kind_literals),
                        list_or_unknown(&other_kind_literals),
                        literal_kinds_label(&probe_literals),
                        list_or_unknown(&probe_literals)
                    ),
                )
            } else {
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    format!(
                        "Related tests pass no literal ripr can compare with the changed boundary [{}] (inputs are strings, computed values or fixtures); activation/infection is unknown, and if a test input drives the compared value, one that sits exactly on the boundary would settle it",
                        list_or_unknown(&probe_literals)
                    ),
                )
            }
        }
        ProbeFamily::StaticUnknown => StageEvidence::new(
            StageState::Unknown,
            Confidence::Unknown,
            "Changed syntax is not mapped to a high-confidence probe family",
        ),
        _ => {
            if related_tests.is_empty() {
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    "No reachable tests were found, so infection cannot be established",
                )
            } else if let Some(unselected) = activation
                .missing_discriminators
                .iter()
                .find(|fact| is_unselected_input_reason(&fact.reason))
            {
                // RIPR-SPEC-0229: every related test's owner input selects a
                // different arm, so none activates this one. RIPR-SPEC-0263:
                // every input leaves the enclosing guard false, so none runs
                // the changed side effect.
                StageEvidence::new(
                    StageState::Weak,
                    Confidence::Medium,
                    unselected.reason.clone(),
                )
            } else if is_wildcard_discard(&probe.expression) {
                StageEvidence::new(
                    StageState::Unknown,
                    Confidence::Low,
                    "Changed value is bound to a discard pattern; it cannot infect a sink",
                )
            } else if let Some(reason) = owner
                .zip(probe.before.as_deref())
                .filter(|_| {
                    matches!(probe.family, ProbeFamily::ReturnValue)
                        && !owner_has_other_callers
                        && !assertion_macros_rebound
                })
                .and_then(|(owner, before)| {
                    crate::analysis::test_grip_evidence::change_is_invisible_to_every_assertion(
                        owner,
                        before,
                        &probe.expression,
                        related_tests,
                    )
                })
            {
                // #8659: every related test fails to tell the changed
                // expression from its baseline. `assertion_macros_rebound`
                // withholds the reading (#10920 review): both reasons read
                // assertion macros and comparison tokens by name, and a
                // rebound `assert!` may compare nothing — or check an exact
                // value its written tokens never show. The summary names why
                // the tests cannot see the change; a relational assertion's
                // inputs may evaluate differently, so the value-equality
                // wording would misstate that gap (#10916).
                match reason {
                    crate::analysis::test_grip_evidence::InvisibilityReason::SameValueAtEveryAssertedInput => {
                        StageEvidence::new(
                            StageState::Weak,
                            Confidence::Medium,
                            "Every related test calls the owner with literal inputs at which the changed expression and its baseline evaluate to the same value, so no test input reaches a difference",
                        )
                    }
                    crate::analysis::test_grip_evidence::InvisibilityReason::RelationalTruthUnchanged => {
                        StageEvidence::new(
                            StageState::Weak,
                            Confidence::Medium,
                            "Every related test's assertions hold with the same truth value on the baseline and the change, so no assertion tells the versions apart",
                        )
                    }
                }
            } else if let Some((owner, change)) = owner
                .zip(unary_tail)
                .filter(|_| matches!(probe.family, ProbeFamily::ReturnValue))
            {
                unary_tail_infection(
                    owner,
                    change,
                    related_tests,
                    owner_has_other_callers,
                    assertion_macros_rebound,
                )
            } else if let Some((before, after, kind)) = probe.before.as_deref().and_then(|before| {
                // A match-arm probe carries only the arm head (`_ =>`) even
                // when the edit sits in the arm body; read the whole arm line.
                let after = full_arm_line(owner, &probe.expression, before, probe.location.line)
                    .unwrap_or_else(|| probe.expression.clone());
                range_bound_change(before, &after).map(|kind| (before, after, kind))
            }) {
                range_bound_infection(
                    owner.filter(|_| !assertion_macros_rebound),
                    &aligned_range_texts(before, &after),
                    kind,
                    related_tests,
                    owner_has_other_callers,
                )
            } else {
                StageEvidence::new(
                    StageState::Yes,
                    Confidence::Medium,
                    "Reachable tests can plausibly activate this changed behavior",
                )
            }
        }
    }
}

/// #14815: infection for a prefix `-`/`!` added to or removed from the
/// owner's whole tail. `!` on `bool` or an integer changes the result at
/// every input, so any reaching input infects. `-` leaves the result
/// unchanged at `0`, so credit needs an asserted literal call at which the
/// owner folds to different values on the base-side owner and the current
/// one; a computed, bound, looped or constant argument stays unknown, and a
/// rebound assertion macro reads nothing.
fn unary_tail_infection(
    owner: &FunctionSummary,
    change: &super::UnaryTailChange,
    related_tests: &[&TestSummary],
    owner_has_other_callers: bool,
    assertion_macros_rebound: bool,
) -> StageEvidence {
    use crate::analysis::test_grip_evidence::{
        RangeChangeVisibility, every_owner_call_separates, owner_change_visibility,
    };
    let operand = &change.operand;
    if change.operator == super::UnaryOperator::Not {
        return StageEvidence::new(
            StageState::Yes,
            Confidence::Medium,
            format!(
                "The returned `{operand}` and `!{operand}` differ at every input, so any reaching call infects the result"
            ),
        );
    }
    if assertion_macros_rebound {
        return StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            "The workspace rebinds an assertion macro, so ripr does not read which asserted inputs tell the versions apart",
        );
    }
    match owner_change_visibility(
        owner,
        &change.baseline,
        related_tests,
        owner_has_other_callers,
    ) {
        RangeChangeVisibility::ExactInputSeparates(call) => StageEvidence::new(
            StageState::Yes,
            Confidence::Medium,
            format!(
                "A related test asserts an exact value for `{call}`, an input at which the returned `{operand}` and `-{operand}` differ, so the input reaches the change"
            ),
        ),
        RangeChangeVisibility::InexactInputSeparates(call) => StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            format!(
                "A related test calls `{call}`, where the returned `{operand}` and `-{operand}` differ, but only a relational or inequality assertion checks it; no exact assertion pins an input that tells them apart"
            ),
        ),
        RangeChangeVisibility::EveryAssertedInputAgrees(call) => StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            format!(
                "Every related test asserts the owner only at inputs where `{operand}` and `-{operand}` are equal (for example `{call}`), so no test input reaches the change"
            ),
        ),
        RangeChangeVisibility::NotEstablished => match every_owner_call_separates(
            owner,
            &change.baseline,
            related_tests,
            owner_has_other_callers,
        ) {
            Some(call) => StageEvidence::new(
                StageState::Yes,
                Confidence::Medium,
                format!(
                    "Every call the related tests make, for example `{call}`, passes a literal input at which the returned `{operand}` and `-{operand}` differ, so the inputs reach the change"
                ),
            ),
            None => StageEvidence::new(
                StageState::Unknown,
                Confidence::Low,
                format!(
                    "The returned `{operand}` and `-{operand}` agree at `{operand} = 0`; ripr could not show that any related test asserts an exact value at a literal input where they differ, so infection is not credited"
                ),
            ),
        },
    }
}

/// #14816: infection for an `if` condition forced to a literal. The literal
/// spells no boundary, so credit comes only from folding the base-side owner
/// and the current one at the related tests' literal calls: an asserted
/// exact value where they differ, or every call separating them. An input
/// that already took the forced branch on the base side agrees on both
/// versions and reaches nothing; a condition, argument or body the fold
/// cannot read stays unknown, and a rebound assertion macro reads nothing.
fn literal_condition_infection(
    owner: &FunctionSummary,
    change: &super::LiteralConditionChange,
    related_tests: &[&TestSummary],
    owner_has_other_callers: bool,
    assertion_macros_rebound: bool,
) -> StageEvidence {
    use crate::analysis::test_grip_evidence::{
        RangeChangeVisibility, every_owner_call_separates, owner_change_visibility,
    };
    let before = change.before.trim();
    let forced = change.forced;
    if assertion_macros_rebound {
        return StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            "The workspace rebinds an assertion macro, so ripr does not read which asserted inputs tell the versions apart",
        );
    }
    match owner_change_visibility(
        owner,
        &change.baseline,
        related_tests,
        owner_has_other_callers,
    ) {
        RangeChangeVisibility::ExactInputSeparates(call) => StageEvidence::new(
            StageState::Yes,
            Confidence::Medium,
            format!(
                "A related test asserts an exact value for `{call}`, an input at which the old condition `{before}` and the forced `{forced}` give different results, so the input reaches the change"
            ),
        ),
        RangeChangeVisibility::InexactInputSeparates(call) => StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            format!(
                "A related test calls `{call}`, where the old condition `{before}` and the forced `{forced}` give different results, but only a relational or inequality assertion checks it; no exact assertion pins an input that tells them apart"
            ),
        ),
        RangeChangeVisibility::EveryAssertedInputAgrees(call) => StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            format!(
                "Every related test asserts the owner only at inputs where the old condition `{before}` and the forced `{forced}` give the same result (for example `{call}`), so no test input reaches the change"
            ),
        ),
        RangeChangeVisibility::NotEstablished => match every_owner_call_separates(
            owner,
            &change.baseline,
            related_tests,
            owner_has_other_callers,
        ) {
            Some(call) => StageEvidence::new(
                StageState::Yes,
                Confidence::Medium,
                format!(
                    "Every call the related tests make, for example `{call}`, passes a literal input at which the old condition `{before}` and the forced `{forced}` give different results, so the inputs reach the change"
                ),
            ),
            None => StageEvidence::new(
                StageState::Unknown,
                Confidence::Low,
                format!(
                    "The condition `{before}` became the literal `{forced}`; ripr could not fold the owner at a related test's literal input to show that input took the other branch on the base side, so infection is not credited"
                ),
            ),
        },
    }
}

/// #14419: a changed range bound (`0..n` to `0..=n`, `1..n` to `0..n`)
/// changes the owner's result only at inputs where the two ranges yield
/// different values, and `0..n` and `0..=n` both sum to 0 at `n = 0`. A
/// reachable call is therefore not infection: credit needs an asserted
/// literal call at which the owner folds to different values on the
/// baseline and the change. A rebound assertion macro (`owner` withheld by
/// the caller) or an unread shape stays unknown, never `yes`.
///
/// A moved match range pattern routes an input to a different arm rather
/// than absorbing an element; when the fold cannot read the arms (a `&str`
/// result) it keeps the ordinary reading, so the arm-routing evidence
/// (#5992) still decides between a moved input and one inside both patterns.
fn range_bound_infection(
    owner: Option<&FunctionSummary>,
    (before, after): &(String, String),
    kind: RangeConsumer,
    related_tests: &[&TestSummary],
    owner_has_other_callers: bool,
) -> StageEvidence {
    use crate::analysis::test_grip_evidence::{RangeChangeVisibility, range_change_visibility};
    match owner.map(|owner| {
        range_change_visibility(owner, before, after, related_tests, owner_has_other_callers)
    }) {
        Some(RangeChangeVisibility::ExactInputSeparates(call)) => StageEvidence::new(
            StageState::Yes,
            Confidence::Medium,
            format!(
                "A related test asserts an exact value for `{call}`, an input at which the old and new range bounds give different results, so the input reaches the change"
            ),
        ),
        Some(RangeChangeVisibility::InexactInputSeparates(call)) => StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            format!(
                "A related test calls `{call}`, where the old and new range bounds give different results, but only a relational or inequality assertion checks it; no exact assertion pins an input that tells the two ranges apart"
            ),
        ),
        Some(RangeChangeVisibility::EveryAssertedInputAgrees(call)) => StageEvidence::new(
            StageState::Weak,
            Confidence::Medium,
            format!(
                "Every related test asserts the owner only at inputs where the old and new range bounds give the same result (for example `{call}`), so no test input reaches the change"
            ),
        ),
        Some(RangeChangeVisibility::NotEstablished) | None if kind == RangeConsumer::Pattern => {
            StageEvidence::new(
                StageState::Yes,
                Confidence::Medium,
                "Reachable tests can plausibly activate this changed behavior",
            )
        }
        Some(RangeChangeVisibility::NotEstablished) | None => StageEvidence::new(
            StageState::Unknown,
            Confidence::Low,
            "The change moves a range bound, which alters the result only at inputs where the old and new ranges yield different values (both `0..n` and `0..=n` are empty-sum at `n = 0`); ripr could not show that any related test's asserted input separates them, so infection is not credited",
        ),
    }
}

/// Whether `before` and `after` differ only inside one range expression (the
/// operator, `..` to `..=`, or an endpoint, `1..n` to `0..n`) whose values
/// feed a consumer that can absorb the changed element. The changed span is
/// the text between the longest common prefix and suffix; it must sit inside
/// a range expression in both versions.
///
/// Absorbing consumers are where a reaching input can leave the result
/// unchanged: `.sum()` adds the extra element 0, `.contains(&x)` and
/// `.any(..)` ignore it away from the endpoint, `.take(k)` drops it, and a
/// `for` loop body may do anything with it. A chain of length-preserving
/// adaptors (`.iter()`, `.map(..)`, `.collect()`, `.len()`, slicing) does
/// not absorb it: the result has one element more or fewer at every input
/// where either range is non-empty, which is the ordinary reading's premise,
/// so such ranges keep it. A version that does not parse as a statement
/// still counts when the whole difference is one `=` right after a `..`.
/// Match arm lines and block headers are parsed in a `match` or with their
/// block closed, so endpoint moves there (`1..5 =>` to `0..5 =>`,
/// `for i in 1..n {`) are read too.
///
/// The preserving list is a length claim, not an injectivity claim:
/// `.collect()` into a set or `Option<Vec<_>>` can still absorb an element,
/// and both ranges being empty at the test input (`(1..n)` and `(0..n)` at
/// `n = 0`) leaves any consumer unchanged. Those shapes keep the ordinary
/// reading and are a known limit.
///
/// Returns the consumer kind: [`RangeConsumer::Pattern`] when the range is a
/// match pattern on both sides, otherwise [`RangeConsumer::Absorbing`].
fn range_bound_change(before: &str, after: &str) -> Option<RangeConsumer> {
    use crate::analysis::language::mask_rust_comments_and_strings;
    let (before, after) = aligned_range_texts(before, after);
    let (raw_before, raw_after) = (before, after);
    let before = mask_rust_comments_and_strings(&raw_before);
    let after = mask_rust_comments_and_strings(&raw_after);
    if before == after {
        // A char endpoint (`'a'..='z'`) is masked like a string literal. An
        // edit confined to strings or comments is left alone, since their
        // `..` may be text; a changed char literal is read as an endpoint
        // whatever else the line holds.
        if char_literal_contents(&raw_before) == char_literal_contents(&raw_after) {
            return None;
        }
        return range_endpoints_moved(&raw_before, &raw_after).then(|| {
            if raw_after.contains("=>") {
                RangeConsumer::Pattern
            } else {
                RangeConsumer::Absorbing
            }
        });
    }
    let prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    let limit = before.len().min(after.len()) - prefix;
    let suffix = before
        .bytes()
        .rev()
        .zip(after.bytes().rev())
        .take(limit)
        .take_while(|(left, right)| left == right)
        .count();
    let span = |text: &str| prefix..text.len() - suffix;
    match (
        range_consumer(&before, span(&before)),
        range_consumer(&after, span(&after)),
    ) {
        (Some(RangeConsumer::Pattern), Some(RangeConsumer::Pattern)) => {
            Some(RangeConsumer::Pattern)
        }
        (Some(RangeConsumer::Preserving), _) | (_, Some(RangeConsumer::Preserving)) => None,
        (Some(_), Some(_)) => Some(RangeConsumer::Absorbing),
        _ => {
            if let Some(kind) = several_range_change(&before, &after) {
                return kind;
            }
            let (short, long) = if before.len() < after.len() {
                (&before, &after)
            } else {
                (&after, &before)
            };
            let one_equals = long.len() == short.len() + 1
                && long.get(span(long)) == Some("=")
                && long
                    .get(..prefix)
                    .is_some_and(|head| head.ends_with("..") && !head.ends_with("..."));
            // #14538: an edit inside a macro's input (`matches!(n, 1..=5)`
            // to `0..=6`) or on a line that does not parse: no range node is
            // read, so the line is gated when a range's written endpoints
            // or operator differ between the versions.
            (one_equals || range_endpoints_moved(&before, &after))
                .then_some(RangeConsumer::Absorbing)
        }
    }
}

/// #14538: each `..` / `..=` operator of `text` (not `...`) with its
/// written start and end operand texts, read lexically up to the nearest
/// delimiter. A macro input or a line that does not parse has no range node,
/// so its ranges are compared this way.
fn range_signatures(text: &str) -> Vec<(String, bool, String)> {
    let mut signatures = Vec::new();
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|rest| rest.find("..")) {
        let at = from + found;
        let rest = &text[at + 2..];
        if rest.starts_with('.') {
            from = at + 2 + rest.len() - rest.trim_start_matches('.').len();
            continue;
        }
        let inclusive = rest.starts_with('=');
        let after = at + 2 + usize::from(inclusive);
        let head = &text[..at];
        let tail = &text[after..];
        let start = &head[operand_start(head)..];
        let end = &tail[..operand_end(tail)];
        signatures.push((start.trim().to_string(), inclusive, end.trim().to_string()));
        from = after;
    }
    signatures
}

/// The byte offset in `head` where a range's start operand begins: after
/// the nearest unbalanced opener or a top-level `,`, `|`, `;` or `=`, so a
/// call or index inside the operand (`f(a, b)..`) stays whole.
fn operand_start(head: &str) -> usize {
    let mut depth = 0usize;
    for (at, ch) in head.char_indices().rev() {
        match ch {
            ')' | ']' | '}' => depth += 1,
            '(' | '[' | '{' if depth == 0 => return at + 1,
            '(' | '[' | '{' => depth -= 1,
            ',' | '|' | ';' | '=' if depth == 0 => return at + 1,
            _ => {}
        }
    }
    0
}

/// The byte offset in `tail` where a range's end operand stops: at the
/// nearest unbalanced closer, a block `{`, or a top-level `,`, `|`, `;` or
/// `=`, so `..=v.len() - 1` and `..=f(a, b)` stay whole.
fn operand_end(tail: &str) -> usize {
    let mut depth = 0usize;
    for (at, ch) in tail.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return at,
            ')' | ']' | '}' => depth -= 1,
            '{' if depth == 0 => return at,
            '{' => depth += 1,
            ',' | '|' | ';' | '=' if depth == 0 => return at,
            _ => {}
        }
    }
    tail.len()
}

/// The text of each char literal in code in `text`, in order, as the
/// comment-and-string masker reads them: a space char (`' '`) counts like
/// any other, and a quote inside a string or comment is not a char.
fn char_literal_contents(text: &str) -> Vec<&str> {
    let (_, spans) = crate::analysis::extract::mask::mask_with_char_literals(text);
    spans
        .into_iter()
        .map(|(start, end)| text.get(start..=end).unwrap_or_default())
        .collect()
}

/// Whether `before` and `after` hold the same number of ranges and at least
/// one range's endpoints or operator differ: a moved bound (`0..=5` to
/// `0..=6`, `'a'..='z'` to `'b'..='y'`), not an edit beside the ranges
/// (`[1, len, ..] => len + 2` to `2 + len`) or a range added or removed.
fn range_endpoints_moved(before: &str, after: &str) -> bool {
    let (old, new) = (range_signatures(before), range_signatures(after));
    !old.is_empty() && old.len() == new.len() && old != new
}

/// #14538: a line where several range expressions changed at once
/// (`(0..=a).sum() + (0..=b).sum()` to `(0..a).sum() + (0..b).sum()`). The
/// two versions must have the same outermost ranges in the same places with
/// the text between them unchanged. `Some(None)` when every changed range
/// keeps a length-preserving reading, `Some(Some(Absorbing))` when any
/// changed range feeds an absorbing consumer or pattern, `None` when the
/// line is not that shape.
fn several_range_change(before: &str, after: &str) -> Option<Option<RangeConsumer>> {
    let ranges = |text: &str| -> Option<Vec<std::ops::Range<usize>>> {
        const PREFIX: &str = "fn __ripr_range_probe() {\n";
        let closing = if text.trim_end().ends_with('{') {
            "}"
        } else {
            ""
        };
        let source = format!("{PREFIX}{text}{closing}\n}}\n");
        let parse = crate::analysis::syntax::parse_clean_source_file(&source)?;
        let spans: Vec<_> = parse
            .tree()
            .syntax()
            .descendants()
            .filter(|node| ast::RangeExpr::can_cast(node.kind()))
            .filter(|node| {
                !node
                    .ancestors()
                    .skip(1)
                    .any(|ancestor| ast::RangeExpr::can_cast(ancestor.kind()))
            })
            .map(|node| {
                let range = node.text_range();
                usize::from(range.start()) - PREFIX.len()..usize::from(range.end()) - PREFIX.len()
            })
            .collect();
        spans
            .iter()
            .all(|span| span.end <= text.len())
            .then_some(spans)
    };
    let (old, new) = (ranges(before)?, ranges(after)?);
    if old.len() < 2 || old.len() != new.len() {
        return None;
    }
    let skeleton = |text: &str, spans: &[std::ops::Range<usize>]| -> Option<String> {
        let mut out = String::new();
        let mut at = 0;
        for span in spans {
            out.push_str(text.get(at..span.start)?);
            out.push('\u{0}');
            at = span.end;
        }
        out.push_str(text.get(at..)?);
        Some(out)
    };
    if skeleton(before, &old)? != skeleton(after, &new)? {
        return None;
    }
    let mut changed = false;
    for (old_span, new_span) in old.iter().zip(&new) {
        if before.get(old_span.clone()) == after.get(new_span.clone()) {
            continue;
        }
        changed = true;
        // A range nested in a changed range (`(0..(0..=a).sum())`) is not
        // read on its own here.
        if before
            .get(old_span.clone())
            .is_none_or(|text| text.matches("..").count() > 1)
            || after
                .get(new_span.clone())
                .is_none_or(|text| text.matches("..").count() > 1)
        {
            return Some(Some(RangeConsumer::Absorbing));
        }
        match (
            range_consumer(before, old_span.clone()),
            range_consumer(after, new_span.clone()),
        ) {
            (Some(RangeConsumer::Preserving), Some(RangeConsumer::Preserving)) => {}
            _ => return Some(Some(RangeConsumer::Absorbing)),
        }
    }
    changed.then_some(None)
}

/// `before` and `after` trimmed, and cut to their arm heads when one is a
/// bare match-arm head: a match-arm probe may carry only the head
/// (`0..5 =>`) while its baseline carries the whole line (`1..5 => 1,`).
/// The other side is cut at its first `=>` outside strings and comments.
/// Anything else (a one-line `match k { 1 => 0, _ => (1..n).sum() }`, an
/// arrow inside a string) is compared whole.
pub(in crate::analysis) fn aligned_range_texts(before: &str, after: &str) -> (String, String) {
    use crate::analysis::language::mask_rust_comments_and_strings;
    let (before, after) = (before.trim(), after.trim());
    let is_head = |text: &str| {
        let masked = mask_rust_comments_and_strings(text);
        masked.ends_with("=>") && masked.matches("=>").count() == 1
    };
    let head_of = |text: &str| -> Option<String> {
        let masked = mask_rust_comments_and_strings(text);
        let at = masked.find("=>")?;
        text.get(..at + 2).map(str::to_string)
    };
    match (is_head(before), is_head(after)) {
        (true, false) => match head_of(after) {
            Some(head) => (before.to_string(), head),
            None => (before.to_string(), after.to_string()),
        },
        (false, true) => match head_of(before) {
            Some(head) => (head, after.to_string()),
            None => (before.to_string(), after.to_string()),
        },
        _ => (before.to_string(), after.to_string()),
    }
}

/// The owner's whole current arm line for a head-only match-arm probe
/// (`_ =>`) whose baseline is a whole arm line with the same head
/// (`_ => (0..n).sum(),`): the one other owner line starting with that head.
/// `None` when the probe is not a bare head, the head itself changed, or the
/// line is not unique.
fn full_arm_line(
    owner: Option<&FunctionSummary>,
    head: &str,
    before: &str,
    probe_line: usize,
) -> Option<String> {
    let owner = owner?;
    let (head, before) = (head.trim(), before.trim());
    if !head.ends_with("=>") || before == head || !before.starts_with(head) {
        return None;
    }
    let lines: Vec<(usize, &str)> = owner
        .body
        .lines()
        .enumerate()
        .map(|(offset, line)| (owner.start_line + offset, line.trim()))
        .filter(|(_, line)| line.starts_with(head) && *line != before)
        .collect();
    // Two arms share the head (`_ =>` in a nested match): the probe's own
    // line names the edited one.
    match lines.as_slice() {
        [(_, line)] => Some((*line).to_string()),
        _ => lines
            .iter()
            .find(|(absolute, _)| *absolute == probe_line)
            .map(|(_, line)| (*line).to_string()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RangeConsumer {
    /// A consumer that can leave the result unchanged when the range gains or
    /// loses an element.
    Absorbing,
    /// A match range pattern: it routes an input to an arm.
    Pattern,
    /// Only length-preserving adaptors, or the range value itself.
    Preserving,
}

/// Methods that take one range and slice by it, as `v[a..b]` does.
const SLICING_METHODS: [&str; 4] = ["get", "get_mut", "get_unchecked", "get_unchecked_mut"];

/// Adaptors after which one element more or fewer still shows up in the
/// result. Any other method on the range's chain is read as absorbing.
const LENGTH_PRESERVING_METHODS: [&str; 25] = [
    "iter",
    "iter_mut",
    "into_iter",
    "copied",
    "cloned",
    "map",
    "rev",
    "enumerate",
    "collect",
    "count",
    "len",
    "to_vec",
    "to_owned",
    "as_slice",
    "as_bytes",
    "bytes",
    "copy_from_slice",
    "clone_from_slice",
    "into",
    "try_into",
    "to_string",
    "chars",
    "char_indices",
    "par_iter",
    "into_par_iter",
];

/// How the innermost range expression of `text` (parsed as one block
/// statement) whose text covers `span` is consumed: walk up through
/// parentheses, references, a slice index and the method chain whose
/// receiver it is.
fn range_consumer(text: &str, span: std::ops::Range<usize>) -> Option<RangeConsumer> {
    const PREFIX: &str = "fn __ripr_range_probe() {\n";
    const ARM_PREFIX: &str = "fn __ripr_range_probe() {\nmatch __ripr_subject {\n";
    let covers = |node: &ra_ap_syntax::SyntaxNode, offset: usize| {
        let covered = node.text_range();
        usize::from(covered.start()) <= offset + span.start
            && offset + span.end <= usize::from(covered.end())
    };
    // A match arm line (`1..5 => 1,`, `_ => (0..n).sum(),`) parses inside a
    // `match`; a range pattern there is a membership test, which absorbs
    // every element away from the moved bound. A block header
    // (`for i in 1..n {`, `if (0..n).contains(&x) {`) parses once its block
    // is closed.
    let trimmed = text.trim_end();
    let closing = if trimmed.ends_with("=>") {
        " {},"
    } else if trimmed.ends_with('{') {
        "}"
    } else {
        ""
    };
    let arm = text
        .contains("=>")
        .then(|| {
            let source = format!("{ARM_PREFIX}{text}{closing}\n_ => {{}}\n}}\n}}\n");
            crate::analysis::syntax::parse_clean_source_file(&source)
        })
        .flatten();
    if let Some(parse) = &arm
        && parse
            .tree()
            .syntax()
            .descendants()
            .filter(|node| ast::RangePat::can_cast(node.kind()))
            .any(|node| covers(&node, ARM_PREFIX.len()))
    {
        return Some(RangeConsumer::Pattern);
    }
    let (parse, offset) = match arm {
        Some(parse) => (parse, ARM_PREFIX.len()),
        None => {
            let closing = if trimmed.ends_with('{') { "}" } else { "" };
            let source = format!("{PREFIX}{text}{closing}\n}}\n");
            (
                crate::analysis::syntax::parse_clean_source_file(&source)?,
                PREFIX.len(),
            )
        }
    };
    let range = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::RangeExpr::cast)
        .filter(|range| covers(range.syntax(), offset))
        .last()?;
    let mut node = range.syntax().clone();
    while let Some(parent) = node.parent() {
        if ast::RefExpr::can_cast(parent.kind()) || ast::ParenExpr::can_cast(parent.kind()) {
            node = parent;
            continue;
        }
        if let Some(index) = ast::IndexExpr::cast(parent.clone())
            && index.index().is_some_and(|inner| inner.syntax() == &node)
        {
            node = parent;
            continue;
        }
        if let Some(call) = ast::MethodCallExpr::cast(parent.clone())
            && call
                .receiver()
                .is_some_and(|receiver| receiver.syntax() == &node)
        {
            let name = call.name_ref().map(|name| name.text().to_string());
            if !name.is_some_and(|name| LENGTH_PRESERVING_METHODS.contains(&name.as_str())) {
                return Some(RangeConsumer::Absorbing);
            }
            node = parent;
            continue;
        }
        // `v.get(a..b)` and its unchecked or mutable forms slice like an
        // index expression.
        if ast::ArgList::can_cast(parent.kind())
            && let Some(call) = parent.parent().and_then(ast::MethodCallExpr::cast)
            && ast::ArgList::cast(parent.clone()).is_some_and(|args| args.args().count() == 1)
            && call
                .name_ref()
                .is_some_and(|name| SLICING_METHODS.contains(&name.text().to_string().as_str()))
        {
            node = call.syntax().clone();
            continue;
        }
        // A `for` header, or a range handed to a call (`Iterator::sum(0..n)`,
        // `helper(0..n)`) whose consumer is not read here.
        if ast::ForExpr::can_cast(parent.kind()) || ast::ArgList::can_cast(parent.kind()) {
            return Some(RangeConsumer::Absorbing);
        }
        break;
    }
    Some(RangeConsumer::Preserving)
}

/// #14388: the string literal a related test passes that reaches an appended
/// `||` arm. The probe must turn `p == "a" || ...` into the same chain plus
/// one more `p == "b"` arm: only `||`, every arm one `==` between the same
/// owner parameter `p` and a plain string literal with no escapes, `p` never
/// rebound in the owner body, and an owner-call argument fact binding `p` to
/// exactly `"b"`. Distinct literals make every original arm false on that
/// input and the new arm true, so the old and new predicates differ. Anything
/// else returns `None` and the ordinary reading stays.
///
/// #15027: integer and char arms read the same way (`n == 1 || n == 2`,
/// `c == ',' || c == ';'`). Every arm must use one literal kind, and the
/// parameter must be declared as a primitive integer for integer literals or
/// `char` for char literals, whose `==` compares the value. Integer literals
/// are plain decimal (no sign, suffix, underscore, radix prefix or leading
/// zero), so distinct spellings are distinct values; char literals are one
/// unescaped character that cannot read as an operator.
fn added_equality_arm_input(
    probe: &Probe,
    activation: &ActivationEvidence,
    owner: &FunctionSummary,
) -> Option<(String, ArmLiteralKind)> {
    let (parameter, literal, kind) = appended_equality_arm(probe, owner)?;
    activation
        .observed_values
        .iter()
        .filter(|fact| fact.context == ValueContext::FunctionArgument)
        .filter_map(|fact| fact.value.split_once(" = "))
        .any(|(name, value)| name.trim() == parameter && value.trim() == literal)
        .then_some((literal, kind))
}

/// #15027: the structural half of [`added_equality_arm_input`]: `probe`
/// appends one `|| p == lit` arm to a chain of equalities on the owner
/// parameter `p`, under every guard listed there except the test input. Any
/// input equal to `lit` makes the whole predicate flip, which is also what
/// lets the owner-tail return sink carry such a chain (flow.rs).
pub(in crate::analysis) fn appended_equality_arm(
    probe: &Probe,
    owner: &FunctionSummary,
) -> Option<(String, String, ArmLiteralKind)> {
    let before = probe.before.as_deref()?;
    let only_or = |text: &str| {
        let masked = crate::analysis::language::mask_rust_comments_and_strings(text);
        !masked.contains("&&") && !masked.contains('!') && !masked.contains('(')
    };
    if !only_or(&probe.expression) || !only_or(before) {
        return None;
    }
    let arms = |text: &str| -> Option<Vec<(String, String, ArmLiteralKind)>> {
        boolean_conditions(text)
            .into_iter()
            .map(equality_arm)
            .collect()
    };
    let after_arms = arms(&probe.expression)?;
    let before_arms = arms(before)?;
    let (added, kept) = after_arms.split_last()?;
    if kept != before_arms.as_slice() {
        return None;
    }
    let parameter = &added.0;
    let kind = added.2;
    let mut literals = after_arms
        .iter()
        .map(|(_, literal, _)| literal)
        .collect::<Vec<_>>();
    literals.sort();
    literals.dedup();
    if after_arms
        .iter()
        .any(|(name, _, arm_kind)| name != parameter || *arm_kind != kind)
        || literals.len() != after_arms.len()
    {
        return None;
    }
    // A parameter type with its own `PartialEq<&str>` could make an original
    // arm true on the new literal, so only plain string types qualify, and
    // likewise only primitive integers and `char` for those literals.
    if !super::activation::call_parameter_names(owner).contains(parameter)
        || !super::activation::parameter_type(owner, parameter)
            .is_some_and(|ty| kind.admits_parameter_type(&ty))
        || parameter_is_rebound(&owner.body, parameter)
        || !chain_is_first_decision(&owner.body, &probe.expression)
    {
        return None;
    }
    Some((parameter.clone(), added.1.clone(), kind))
}

/// The literal kind of one appended-arm equality (#14388, #15027).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum ArmLiteralKind {
    Str,
    Int,
    Char,
}

impl ArmLiteralKind {
    fn label(self) -> &'static str {
        match self {
            Self::Str => "string",
            Self::Int => "integer",
            Self::Char => "char",
        }
    }

    /// Whether `==` between a parameter of type `ty` and a literal of this
    /// kind compares the value itself.
    fn admits_parameter_type(self, ty: &str) -> bool {
        match self {
            Self::Str => is_plain_string_type(ty),
            Self::Int => matches!(
                ty.trim(),
                "u8" | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
            ),
            Self::Char => ty.trim() == "char",
        }
    }

    /// The kind of `text` when it is one literal this reading accepts.
    fn of(text: &str) -> Option<Self> {
        if text.len() >= 2
            && text.starts_with('"')
            && text.ends_with('"')
            && !text[1..text.len() - 1].contains(['"', '\\'])
        {
            return (!text[1..text.len() - 1].contains("==")).then_some(Self::Str);
        }
        // Plain decimal only: `5` and `05` or `5_u8` would otherwise be two
        // spellings of one value.
        if !text.is_empty()
            && text.len() <= 18
            && text.bytes().all(|byte| byte.is_ascii_digit())
            && (text == "0" || !text.starts_with('0'))
        {
            return Some(Self::Int);
        }
        let mut inner = text.strip_prefix('\'')?.strip_suffix('\'')?.chars();
        let (Some(only), None) = (inner.next(), inner.next()) else {
            return None;
        };
        // An escape could spell another char's value, and an operator
        // character could be read as part of the chain.
        (only.is_ascii_alphanumeric() || ",;:.-_+*/#@$%^~?<> ".contains(only)).then_some(Self::Char)
    }
}

/// Whether nothing in the owner before `chain` can take a call away from it
/// (#10856 for this reading): the chain occurs once in the owner's text, and
/// the body before it holds no `if`, `match`, loop, `return`, `break`,
/// `continue`, `?`, match arm, closure, other short-circuit, macro, call,
/// index, division or remainder, and the chain ends its expression.
/// An early return on another parameter (`if strict { return false; }`)
/// would keep a test that passes the new literal from ever evaluating the
/// chain. Strings and
/// comments are masked; a chain ripr cannot find exactly once declines.
fn chain_is_first_decision(function: &str, chain: &str) -> bool {
    let masked = crate::analysis::extract::mask_comments_and_strings(function);
    let masked_chain = crate::analysis::extract::mask_comments_and_strings(chain);
    let masked_chain = masked_chain.trim();
    if masked.len() != function.len() || masked_chain.is_empty() {
        return false;
    }
    let mut found = masked.match_indices(masked_chain).map(|(at, _)| at);
    let (Some(at), None) = (found.next(), found.next()) else {
        return false;
    };
    // The signature (up to the body's opening brace) is not control flow.
    let Some((_, head)) = masked[..at].split_once('{') else {
        return false;
    };
    // `!` (a macro such as `assert!` or `panic!`) and `(` (a call such as
    // `std::process::exit(1)`) can diverge without a keyword, so they
    // decline too; wrapping the chain in a call (`Some(...)`) declines with
    // them.
    // `[`, `/` and `%` (indexing, division, remainder) can panic first.
    if head.contains(['?', '|', '!', '(', '[', '/', '%'])
        || head.contains("&&")
        || head.contains("=>")
    {
        return false;
    }
    // The chain must also end its expression: a continuation such as a
    // following `&& strict` line binds tighter than the appended `||` and
    // could keep the new arm from deciding the result.
    let tail = masked[at + masked_chain.len()..].trim_start();
    if !tail.starts_with([',', ';', '}']) {
        return false;
    }
    let is_word = |ch: char| ch.is_ascii_alphanumeric() || ch == '_';
    !head.split(|ch: char| !is_word(ch)).any(|word| {
        matches!(
            word,
            "if" | "match" | "while" | "for" | "loop" | "return" | "break" | "continue"
        )
    })
}

/// Whether `ty` is `&str`, `&'a str`, `String` or `&String`: types whose
/// `==` against a string literal compares the text itself.
pub(in crate::analysis::classify) fn is_plain_string_type(ty: &str) -> bool {
    let compact = ty.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some(borrowed) = compact.strip_prefix('&') else {
        return compact == "String";
    };
    let borrowed = borrowed.trim_start();
    let inner = match borrowed.strip_prefix('\'') {
        Some(lifetime) => match lifetime.split_once(' ') {
            Some((name, rest)) if is_plain_identifier(name) => rest,
            _ => return false,
        },
        None => borrowed,
    };
    matches!(inner, "str" | "String")
}

/// `name == literal` or `literal == name` as `(name, literal, kind)`, for a
/// plain identifier and a literal [`ArmLiteralKind::of`] accepts; anything
/// else is `None`.
fn equality_arm(arm: &str) -> Option<(String, String, ArmLiteralKind)> {
    let (left, right) = arm.split_once("==")?;
    let (left, right) = (left.trim(), right.trim());
    let (name, literal, kind) = if let Some(kind) = ArmLiteralKind::of(right) {
        (left, right, kind)
    } else if let Some(kind) = ArmLiteralKind::of(left) {
        (right, left, kind)
    } else {
        return None;
    };
    is_plain_identifier(name).then(|| (name.to_string(), literal.to_string(), kind))
}

/// Whether `body` might give `name` a value other than the caller's
/// argument before a comparison reads it. Strict on purpose: every
/// whole-word use of `name`, strings and comments masked, must be its one
/// `name:` parameter declaration or an operand of `==`. Any other use (a
/// `let` or pattern that shadows it, an assignment, a borrow, a call) counts.
fn parameter_is_rebound(body: &str, name: &str) -> bool {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(body);
    let bytes = masked.as_bytes();
    let mut declarations = 0usize;
    for (at, _) in masked.match_indices(name) {
        let end = at + name.len();
        let whole_word = (at == 0 || !is_ident_byte(bytes[at - 1]))
            && (end >= bytes.len() || !is_ident_byte(bytes[end]));
        if !whole_word {
            continue;
        }
        let prefix = masked[..at].trim_end();
        let suffix = masked[end..].trim_start();
        if suffix.starts_with(':') && !suffix.starts_with("::") {
            declarations += 1;
        } else if !(suffix.starts_with("==") || prefix.ends_with("==")) {
            return true;
        }
    }
    declarations != 1
}

/// Resolve literal boundaries from live, non-computed local initializers.
fn local_initializer_boundary_literals(
    probe: &Probe,
    owner: Option<&FunctionSummary>,
) -> Option<Vec<String>> {
    let owner = owner?;
    let (left, right) = comparison_operands(&probe.expression)?;
    let mut literals = Vec::new();
    for operand in [left, right] {
        if let Some(initializer) =
            super::activation::live_local_initializer(owner, operand.trim(), probe.location.line)
        {
            if is_conditional_initializer(&initializer)
                || is_computed_value_expression(&initializer)
            {
                literals.extend(conditional_literal_results(&initializer));
            } else {
                literals.extend(boundary_literals(&initializer));
            }
        }
    }
    literals.sort();
    literals.dedup();
    (!literals.is_empty()).then_some(literals)
}

// Retain literal result branches when arithmetic appears in a condition or a
// different branch. The condition's literals and computed tails are not values
// returned by the conditional. Unreadable or statement-bearing branches abstain.
fn is_conditional_initializer(initializer: &str) -> bool {
    let Some(after_if) = initializer.trim_start().strip_prefix("if") else {
        return false;
    };
    !after_if.starts_with(|c: char| c.is_alphanumeric() || c == '_')
}

fn conditional_literal_results(initializer: &str) -> Vec<String> {
    if !is_conditional_initializer(initializer) {
        return Vec::new();
    }
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(&format!(
        "fn __ripr_boundary() {{ let _ = {initializer}; }}"
    )) else {
        return Vec::new();
    };
    let Some(ast::Expr::IfExpr(branch)) = parse
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)
        .and_then(|binding| binding.initializer())
    else {
        return Vec::new();
    };
    fn add_block(block: ast::BlockExpr, literals: &mut Vec<String>, depth: usize) {
        let Some(list) = block.stmt_list() else {
            return;
        };
        if depth == 0 || list.statements().next().is_some() {
            return;
        }
        if let Some(expr) = list.tail_expr() {
            add_expr(expr, literals, depth - 1);
        }
    }
    fn add_if(branch: ast::IfExpr, literals: &mut Vec<String>, depth: usize) {
        if depth == 0 {
            return;
        }
        if let Some(block) = branch.then_branch() {
            add_block(block, literals, depth - 1);
        }
        match branch.else_branch() {
            Some(ast::ElseBranch::Block(block)) => add_block(block, literals, depth - 1),
            Some(ast::ElseBranch::IfExpr(next)) => add_if(next, literals, depth - 1),
            None => {}
        }
    }
    fn add_expr(expr: ast::Expr, literals: &mut Vec<String>, depth: usize) {
        if depth == 0 {
            return;
        }
        match expr {
            ast::Expr::Literal(literal) => {
                literals.extend(extract_literals(&literal.syntax().text().to_string()))
            }
            ast::Expr::ParenExpr(paren) => {
                if let Some(inner) = paren.expr() {
                    add_expr(inner, literals, depth - 1);
                }
            }
            ast::Expr::PrefixExpr(prefix) if prefix.op_kind() == Some(ast::UnaryOp::Neg) => {
                let mut inner = prefix.expr();
                for _ in 0..depth {
                    match inner {
                        Some(ast::Expr::ParenExpr(paren)) => inner = paren.expr(),
                        Some(ast::Expr::Literal(literal))
                            if matches!(
                                literal.kind(),
                                ast::LiteralKind::IntNumber(_) | ast::LiteralKind::FloatNumber(_)
                            ) =>
                        {
                            literals
                                .extend(extract_literals(&format!("-{}", literal.syntax().text())));
                            break;
                        }
                        _ => break,
                    }
                }
            }
            ast::Expr::BlockExpr(block) => add_block(block, literals, depth - 1),
            ast::Expr::IfExpr(branch) => add_if(branch, literals, depth - 1),
            _ => {}
        }
    }
    let mut literals = Vec::new();
    add_if(branch, &mut literals, 32);
    literals
}

/// The literals that can be the changed predicate's boundary value. A
/// comparison operand that computes its value (`CURRENT - 2`, `2 + 2`)
/// does not compare against the literal it contains, so its literals are
/// not the boundary: `CURRENT - 2 > version` never compares against `2`
/// (#6671). Such an operand contributes nothing and the boundary stays
/// unknown unless another operand spells it. A compound predicate
/// (`&&`/`||`) is cut into its top-level conditions and each comparison
/// is read the same way; a condition that is not a comparison keeps its
/// literals. A predicate with no computed comparison operand anywhere
/// keeps every literal it contains.
fn boundary_literals(expression: &str) -> Vec<String> {
    let mut any_computed = false;
    let mut literals = Vec::new();
    for condition in boolean_conditions(expression) {
        // A condition that is not a comparison (`flag`, a `let` binding,
        // a closure call) keeps its literals, as before #6671.
        let Some((left, right)) = comparison_operands(condition) else {
            literals.extend(extract_literals(condition));
            continue;
        };
        for operand in [left, right] {
            if is_computed_value_expression(&operand) {
                any_computed = true;
            } else {
                literals.extend(extract_literals(&operand));
            }
        }
    }
    if !any_computed {
        return extract_literals(expression);
    }
    literals.sort();
    literals.dedup();
    literals
}

/// The top-level conditions of a boolean predicate: `a > 1 && (b < 2 ||
/// c)` -> `a > 1`, `b < 2`, `c`. Enclosing parentheses and a leading `!`
/// are peeled before cutting; `&&`/`||` inside strings, comments, nested
/// parentheses, brackets, or braces are not cuts.
fn boolean_conditions(expression: &str) -> Vec<&str> {
    let mut condition = expression.trim();
    loop {
        let unwrapped = condition.trim_start_matches('!').trim();
        match unwrapped
            .strip_prefix('(')
            .and_then(|inner| inner.strip_suffix(')'))
            .filter(|inner| balanced(inner))
        {
            Some(inner) => condition = inner.trim(),
            None => break,
        }
    }
    let masked = crate::analysis::language::mask_rust_comments_and_strings(condition);
    let bytes = masked.as_bytes();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut cuts = Vec::new();
    let mut idx = 0usize;
    while idx < bytes.len() {
        match bytes[idx] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'&' | b'|' if depth == 0 && bytes.get(idx + 1) == Some(&bytes[idx]) => {
                cuts.push(&condition[start..idx]);
                idx += 2;
                start = idx;
                continue;
            }
            _ => {}
        }
        idx += 1;
    }
    if cuts.is_empty() {
        return vec![condition];
    }
    cuts.push(&condition[start..]);
    cuts.into_iter().flat_map(boolean_conditions).collect()
}

/// Whether every parenthesis in `text` closes inside it, so `(a) && (b)`
/// is not mistaken for one parenthesized condition.
fn balanced(text: &str) -> bool {
    let masked = crate::analysis::language::mask_rust_comments_and_strings(text);
    let mut depth = 0i32;
    for ch in masked.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

fn unresolved_boundary_summary(reason: &str) -> String {
    // An operand that is a local or computed value comes from somewhere ripr
    // cannot trace (a test input it cannot fold, the runtime, a random draw).
    // Name the inspection without promising that literals supply a missing mapping.
    let next = if reason.starts_with("boundary operand") {
        "; inspect how existing test inputs reach this operand; literal inputs do not guarantee that ripr can trace the required value mapping"
    } else {
        ""
    };
    format!("Changed boundary input is unresolved: {reason}; activation/infection is unknown{next}")
}

/// Numeric, char or byte: (quoted, byte-prefixed).
fn literal_kind(literal: &str) -> (bool, bool) {
    let quoted = literal.ends_with('\'');
    (quoted, quoted && literal.starts_with("b'"))
}

/// The kinds present in `literals`, in a fixed order: `numeric`, `char`,
/// `byte`, joined with `/`.
fn literal_kinds_label<T: AsRef<str>>(literals: &[T]) -> String {
    [
        ((false, false), "numeric"),
        ((true, false), "char"),
        ((true, true), "byte"),
    ]
    .iter()
    .filter(|(kind, _)| {
        literals
            .iter()
            .any(|literal| literal_kind(literal.as_ref()) == *kind)
    })
    .map(|(_, label)| *label)
    .collect::<Vec<_>>()
    .join("/")
}

/// Why a changed predicate with no literal boundary stays unknown. A
/// boundary that names a constant says so: ripr could not see that
/// constant's value in the owner's file (or it is declared more than once),
/// and no related test passes the constant itself, so it cannot tell
/// whether any test input sits on the boundary.
fn no_literal_boundary_summary(expression: &str) -> String {
    match boundary_constant_operand_name(expression) {
        Some(name) => format!(
            "Predicate compares against constant {name}; ripr cannot see its value statically or match it to a related test input (a same-file integer const, a matching literal input, or an argument naming {name} would resolve it), so activation/infection is unknown"
        ),
        None => match comparison_operands(expression) {
            // Neither side spells a value (`val > surrogate_start`): say
            // what ripr compared and what would settle it, instead of
            // sending the reader after a literal that is not there.
            Some((left, right))
                if is_plain_path(&left)
                    && is_plain_path(&right)
                    && super::activation::literal_operand_value(&left).is_none()
                    && super::activation::literal_operand_value(&right).is_none() =>
            {
                format!(
                    "Predicate compares `{left}` with `{right}`, and ripr can read no literal or same-file constant for either, so it cannot tell which side of the boundary a test reaches (a value taken from a test input, the runtime or a random draw is not decidable statically; a test input that makes the two equal would settle it)"
                )
            }
            _ => "Predicate changed, but no literal boundary was visible in the changed expression"
                .to_string(),
        },
    }
}

/// An operand that is one identifier, field or module path
/// (`val`, `self.count`, `limits::MAX`): the only shape the splitter cuts
/// cleanly out of a compound predicate such as `ready && total < base * 3`.
fn is_plain_path(operand: &str) -> bool {
    !operand.is_empty()
        && operand
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':'))
        && !operand.starts_with(|ch: char| ch.is_ascii_digit())
}

/// Returns true iff the expression is an exact wildcard discard that provably
/// cannot infect any sink.  Matches the `let` + `_` + `:`/`=` token grammar
/// across any legal whitespace (#3233) but NOT `let _name` — those bindings
/// are still used.
fn is_wildcard_discard(expression: &str) -> bool {
    // Shared whitespace-stable predicate (#3233): the flow stage's
    // `value_is_swallowed` consumes the same authority, so the two stages
    // cannot drift apart on `let _ =` vs `let _=` vs `let _ :` tokenizations.
    super::text::is_wildcard_discard_binding(expression)
}

#[cfg(test)]
mod tests {
    use super::{RangeConsumer, range_bound_change, range_endpoints_moved, several_range_change};

    #[test]
    fn an_edit_on_a_range_line_inside_a_macro_input_is_gated() {
        // #14538: `matches!` input is not parsed, so any edit on a line
        // holding a range operator is gated.
        let absorbing = |before: &str, after: &str| {
            range_bound_change(before, after).is_some_and(|kind| kind == RangeConsumer::Absorbing)
        };
        assert!(absorbing("matches!(n, 1..=5)", "matches!(n, 0..=5)"));
        assert!(absorbing("matches!(n, 0..=5)", "matches!(n, 0..=6)"));
        assert!(absorbing("matches!(n, 0 ..= 5)", "matches!(n, 0 ..= 6)"));
        assert!(absorbing(
            "matches!(n, 0..=MAX - 1)",
            "matches!(n, 0..=MAX - 2)"
        ));
        assert!(absorbing("matches!(n, -1..=5)", "matches!(n, 0..=5)"));
        assert!(absorbing("matches!(n, 0..=5)", "matches!(n, 1..5)"));
        assert!(absorbing(
            "matches!(a, 1..=5) || matches!(b, 1..=5)",
            "matches!(a, 0..=5) || matches!(b, 0..=5)"
        ));
        // Char endpoints are masked like strings.
        assert!(absorbing(
            "matches!(c, 'a'..='z')",
            "matches!(c, 'b'..='z')"
        ));
        assert!(absorbing(
            "matches!(c, 'a'..='z')",
            "matches!(c, 'b'..='y')"
        ));
        assert!(absorbing(
            "matches!(c, '\u{e9}'..='z')",
            "matches!(c, '\u{e8}'..='z')"
        ));
        assert!(absorbing(
            "matches!(c, 'é'..='z')",
            "matches!(c, 'è'..='z')"
        ));
        // An edit beside the ranges, a range added or removed, or a line
        // with no range operator is not gated here.
        assert!(range_bound_change("matches!(n, Some(5))", "matches!(n, Some(6))").is_none());
        assert!(!range_endpoints_moved(
            "[1, len, ..] => Status::Complete(*len as usize + 2),",
            "[1, len, ..] => Status::Complete(2 + *len as usize),"
        ));
        assert!(!range_endpoints_moved(
            "crc: crc(body),",
            "crc: crc(&body[..]),"
        ));
        assert!(!range_endpoints_moved(
            "b'0'..=b'9' | b'a'..=b'z' | b'_' => true,",
            "b'0'..=b'9' | b'a'..=b'z' => true,"
        ));
        assert!(!range_endpoints_moved("f(a...b)", "f(a...c)"));
        // A call or index inside an operand stays whole.
        assert!(absorbing(
            "println!(\"{}\", (0..=v.len() - 1).sum::<usize>())",
            "println!(\"{}\", (0..=v.len() - 2).sum::<usize>())"
        ));
        assert!(range_endpoints_moved(
            "m!((f(a, b)..=n))",
            "m!((f(c, b)..=n))"
        ));
        assert!(range_endpoints_moved("m!(0..=f(a, b))", "m!(0..=f(a, c))"));
        // A string or comment holding `..` is not a char endpoint.
        assert!(range_bound_change("say(\"Done..\")", "say(\"Finished..\")").is_none());
        assert!(range_bound_change("x // 0..5", "x // 0..6").is_none());
        // A string or comment elsewhere on the line does not hide a moved
        // char endpoint.
        assert!(
            range_bound_change("'a'..='z' => 1, // letters", "'b'..='z' => 1, // letters")
                .is_some_and(|kind| kind == RangeConsumer::Pattern)
        );
        assert!(absorbing(
            "matches!(c, 'a'..='z') && s == \"x\"",
            "matches!(c, 'b'..='z') && s == \"x\""
        ));
        assert!(range_bound_change("' '..='a' => 1,", "'a'..=' ' => 1,").is_some());
        // A char-like quote inside a string is string text.
        assert!(range_bound_change("say(\"'a'..='z'\")", "say(\"'b'..='z'\")").is_none());
        assert!(absorbing(
            "matches!(c, 'a'..='z') // 0..5",
            "matches!(c, 'b'..='z') // 0..5"
        ));
    }

    #[test]
    fn several_changed_ranges_are_gated_unless_every_one_preserves_length() {
        // #14538: two absorbing sums changed on one line.
        assert!(matches!(
            several_range_change(
                "(0..=a).sum::<u32>() + (0..=b).sum::<u32>()",
                "(0..a).sum::<u32>() + (0..b).sum::<u32>()",
            ),
            Some(Some(RangeConsumer::Absorbing))
        ));
        // Two collected ranges keep the ordinary reading.
        assert!(matches!(
            several_range_change(
                "((0..=a).collect(), (0..=b).collect())",
                "((0..a).collect(), (0..b).collect())",
            ),
            Some(None)
        ));
        // A change outside the ranges is not this shape.
        assert!(
            several_range_change(
                "(0..=a).sum() + (0..=b).sum()",
                "(0..=a).sum() * (0..=b).sum()"
            )
            .is_none()
        );
        // A changed range nested inside a length-preserving one.
        assert!(matches!(
            several_range_change(
                "((0..(0..=a).sum()).collect(), (0..b).collect())",
                "((0..(0..a).sum()).collect(), (0..=b).collect())",
            ),
            Some(Some(RangeConsumer::Absorbing))
        ));
        // One range is the single-range path's job.
        assert!(several_range_change("(0..=a).sum()", "(0..a).sum()").is_none());
    }

    #[test]
    fn an_unmapped_local_operand_does_not_promise_literal_resolution() {
        let local = unresolved_boundary_summary(
            "boundary operand `unit` is a local or computed value ripr cannot map to the related tests' inputs, so it cannot tell which side of the boundary a test reaches",
        );
        assert!(local.ends_with(
            "activation/infection is unknown; inspect how existing test inputs reach this operand; literal inputs do not guarantee that ripr can trace the required value mapping"
        ));
        // A reason that is not about an operand keeps its own ending.
        let other = unresolved_boundary_summary("a compared argument is computed");
        assert!(other.ends_with("activation/infection is unknown"));
    }

    use super::super::super::rust_index::LiteralFact;
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn predicate_infection_uses_matching_test_literal() {
        let probe = probe(ProbeFamily::Predicate, "value > 10");
        let test = test_with_literals(&["10"]);
        let activation = activation_with(&[("value = 10", ValueContext::FunctionArgument)]);
        let evidence = infection_evidence(&probe, &[&test], &activation);

        assert_eq!(evidence.state, StageState::Yes);
        assert_eq!(
            evidence.summary,
            "Detected test input literal matching changed boundary: 10"
        );
    }

    #[test]
    fn predicate_literal_summary_elides_but_matching_uses_all_values() {
        let values: Vec<String> = (100..140).map(|i| i.to_string()).collect();
        let refs: Vec<&str> = values.iter().map(String::as_str).collect();
        let test = test_with_literals(&refs);
        let missing = probe(ProbeFamily::Predicate, "value > 999");
        let evidence = infection_evidence(&missing, &[&test], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Weak);
        assert!(
            evidence.summary.contains("24 values elided"),
            "{}",
            evidence.summary
        );
        assert!(
            evidence.summary.contains("100, 101"),
            "{}",
            evidence.summary
        );
        assert!(evidence.summary.len() < 1400, "{}", evidence.summary.len());

        // The boundary literal appears after the displayed window. An oracle
        // occurrence must still be recognized and must not become an input.
        let mut with_boundary = refs;
        with_boundary.push("42");
        let late = test_with_literals(&with_boundary);
        let boundary = probe(ProbeFamily::Predicate, "value > 42");
        let oracle_only = infection_evidence(&boundary, &[&late], &ActivationEvidence::default());
        assert_eq!(oracle_only.state, StageState::Weak);
        assert!(
            oracle_only.summary.contains("only outside"),
            "{}",
            oracle_only.summary
        );
        let activation = activation_with(&[("value = 42", ValueContext::FunctionArgument)]);
        let input = infection_evidence(&boundary, &[&late], &activation);
        assert_eq!(input.state, StageState::Yes);
        assert!(input.summary.contains("42"), "{}", input.summary);
    }

    #[test]
    fn predicate_literal_summary_bounds_unicode_and_counts_unique_other_kinds() {
        let chars: Vec<String> = ('a'..='t').map(|c| format!("'{c}'")).collect();
        let mut refs: Vec<&str> = chars.iter().map(String::as_str).collect();
        refs.push("'a'"); // Other-kind summaries sort and deduplicate first.
        let test = test_with_literals(&refs);
        let numeric = probe(ProbeFamily::Predicate, "value > 1");
        let evidence = infection_evidence(&numeric, &[&test], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        assert!(
            evidence.summary.contains("4 values elided"),
            "{}",
            evidence.summary
        );

        // Directly construct an oversized fact to test the byte boundary and
        // UTF-8 handling independently of the source parser's literal grammar.
        let huge = format!("'{}'", "é".repeat(5_000));
        let test = test_with_literals(&[huge.as_str()]);
        let evidence = infection_evidence(&numeric, &[&test], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        assert!(
            evidence.summary.contains("1 value elided"),
            "{}",
            evidence.summary
        );
        assert!(evidence.summary.len() < 1400, "{}", evidence.summary.len());
    }

    #[test]
    fn predicate_infection_matches_decimal_exponent_case() {
        let probe = probe(ProbeFamily::Predicate, "ratio < 4E-2");
        let test = test_with_literals(&["4e-2"]);
        let activation = activation_with(&[("ratio = 4e-2", ValueContext::FunctionArgument)]);
        let evidence = infection_evidence(&probe, &[&test], &activation);

        assert_eq!(evidence.state, StageState::Yes);
        assert_eq!(
            evidence.summary,
            "Detected test input literal matching changed boundary: 4e-2"
        );
    }

    #[test]
    fn predicate_infection_names_an_unmatched_constant_boundary() {
        let constant = probe(ProbeFamily::Predicate, "amount >= DISCOUNT_THRESHOLD");
        let test = test_with_literals(&["10000"]);
        let evidence = infection_evidence(&constant, &[&test], &ActivationEvidence::default());

        assert_eq!(evidence.state, StageState::Unknown);
        assert!(
            evidence
                .summary
                .starts_with("Predicate compares against constant DISCOUNT_THRESHOLD; ripr cannot see its value statically"),
            "{}",
            evidence.summary
        );

        let parameter = probe(ProbeFamily::Predicate, "amount >= threshold");
        assert_eq!(
            infection_evidence(&parameter, &[&test], &ActivationEvidence::default()).summary,
            "Predicate compares `amount` with `threshold`, and ripr can read no literal or same-file constant for either, so it cannot tell which side of the boundary a test reaches (a value taken from a test input, the runtime or a random draw is not decidable statically; a test input that makes the two equal would settle it)"
        );
    }

    #[test]
    fn predicate_infection_ignores_boundary_literal_used_only_as_expected_value() {
        // `assert_eq!(tax_bps("EU"), 2000)` in a related test that never
        // passes 2000 into the changed owner: the literal is the oracle's
        // expected value, not an input, so it cannot activate the boundary.
        let probe = probe(ProbeFamily::Predicate, "weight_grams > 2_000");
        let test = test_with_literals(&["2000"]);
        let activation = activation_with(&[("2000", ValueContext::AssertionArgument)]);
        let evidence = infection_evidence(&probe, &[&test], &activation);

        assert_eq!(evidence.state, StageState::Weak);
        assert_eq!(
            evidence.summary,
            "Related tests contain the changed boundary literal [2000] only outside the changed owner's inputs (for example as an expected value); no test input at the changed boundary was detected"
        );
    }

    /// #15343, #15344: a boundary that may reach the owner through an input
    /// ripr does not read is unknown, never an absent input.
    #[test]
    fn an_unread_owner_input_holding_the_boundary_is_opaque() {
        let boundary = ["100".to_string()];
        let test_with = |body: &str, attrs: &[&str], calls: &[&str]| {
            let mut test = test_with_literals(&["100", "0"]);
            test.body = body.into();
            test.attrs = attrs
                .iter()
                .map(|attr| (*attr).to_string())
                .collect::<Vec<_>>()
                .into();
            test.calls = calls
                .iter()
                .map(|text| crate::analysis::facts::CallFact {
                    name: "discount".to_string(),
                    line: 2,
                    text: (*text).to_string().into(),
                })
                .collect::<Vec<_>>()
                .into();
            test
        };
        let opaque = |test: &TestSummary| opaque_boundary_input(&[test], "discount", &boundary);
        // A same-file wrapper macro, unspliced and spliced (#9122).
        let wrapped = opaque(&test_with(
            "fn t() { macro_rules! check { ($i:expr, $o:expr) => { assert_eq!(discount($i), $o) }; } check!(100, 0); }",
            &[],
            &[],
        ));
        assert!(
            wrapped
                .as_deref()
                .is_some_and(|why| why.contains("`check!`")),
            "{wrapped:?}"
        );
        let spliced = opaque(&test_with(
            "fn t() { assert_eq!(discount(100), 0, \"input {}\", 100); }",
            &[],
            &[],
        ));
        assert!(spliced.is_some());
        // A local macro nested in a standard assertion.
        assert!(
            opaque(&test_with(
                "fn t() { macro_rules! d { ($x:expr) => { discount($x) }; } assert_eq!(d!(100), 0); }",
                &[],
                &[]
            ))
            .is_some()
        );
        // An rstest fixture parameter fed to the owner.
        let fixture = opaque(&test_with(
            "fn at_threshold(amount: u32) { assert_eq!(discount(amount), 0); }",
            &["#[rstest]"],
            &["assert_eq!(discount(amount), 0);"],
        ));
        assert!(
            fixture
                .as_deref()
                .is_some_and(|why| why.contains("fixture `amount`")),
            "{fixture:?}"
        );

        // Alternates that stay readable, so the absent-input reading stands.
        for (body, attrs, calls) in [
            // No unread macro holds the boundary; it feeds another function.
            (
                "fn t() { check!(50, 0); assert_eq!(other(100), 0); }",
                &["#[test]"][..],
                &[][..],
            ),
            (
                "fn t() { assert_eq!(discount_all(100), 0); }",
                &[][..],
                &[][..],
            ),
            // A macro the test does not define is no evidence; nor a negation.
            (
                "fn t() { info!(\"limit {}\", 100); assert_that!(discount(50)); }",
                &[][..],
                &[][..],
            ),
            ("fn t() { if !(x == 100) { return; } }", &[][..], &[][..]),
            // An owner call with the boundary that activation saw and declined.
            (
                "fn t() { assert_eq!(discount(100), 0); }",
                &[][..],
                &["assert_eq!(discount(100), 0);"][..],
            ),
            // Positional case rows bind bare parameters: no fixture.
            (
                "fn t(amount: u32, expected: u32) { assert_eq!(discount(amount), expected); }",
                &["#[rstest]", "#[case(50, 0)]"][..],
                &["assert_eq!(discount(amount), expected);"][..],
            ),
            (
                "fn t(amount: u32) { assert_eq!(discount(amount), 0); }",
                &["#[rstest(amount, case(50))]"][..],
                &["assert_eq!(discount(amount), 0);"][..],
            ),
            // `vec!` and the standard assertions are read.
            (
                "fn t() { assert_eq!(total(vec![100]), 0); }",
                &[][..],
                &[][..],
            ),
            // A `#[case]` or `#[values]` parameter is not a fixture.
            (
                "fn t(#[case] amount: u32) { assert_eq!(discount(amount), 0); }",
                &["#[rstest]", "#[case(50)]"][..],
                &["assert_eq!(discount(amount), 0);"][..],
            ),
            (
                "fn t(#[values(1, 2)] amount: u32) { assert_eq!(discount(amount), 0); }",
                &["#[rstest]"][..],
                &["assert_eq!(discount(amount), 0);"][..],
            ),
            // Without `#[rstest]` the parameter is no fixture.
            (
                "fn t(amount: u32) { assert_eq!(discount(amount), 0); }",
                &[][..],
                &["assert_eq!(discount(amount), 0);"][..],
            ),
            // A fixture beside a visible non-boundary literal: the literal
            // is a readable input, so its gap stands.
            (
                "fn t(ctx: Ctx) { assert_eq!(discount(&ctx, 50), 0); }",
                &["#[rstest]"][..],
                &["assert_eq!(discount(&ctx, 50), 0);"][..],
            ),
            // An owner call inside a macro that may discard it.
            (
                "fn t() { macro_rules! discard { ($($t:tt)*) => {}; } discard!(discount(100)); }",
                &[][..],
                &[][..],
            ),
            ("fn t() { skip!(discount(100)); }", &[][..], &[][..]),
            // A local macro that never calls the owner: 100 is an expected
            // value, not an input.
            (
                "fn t() { macro_rules! approx { ($a:expr, $b:expr) => { assert!(($a - $b) < 1) }; } approx!(discount(50), 100); }",
                &[][..],
                &[][..],
            ),
        ] {
            let test = test_with(body, attrs, calls);
            assert_eq!(opaque(&test), None, "{body}");
        }
    }

    /// #15343: through `infection_evidence_for_owner`, a fixture-fed owner
    /// call reads unknown, a readable boundary input still reads reached,
    /// and an expected-value-only boundary without a fixture stays weak.
    #[test]
    fn an_rstest_fixture_input_reads_unknown_through_the_owner_reading() {
        let body = "pub fn pick(amount: u32) -> u32 {\n    if amount >= 100 { 10 } else { 0 }\n}\n";
        let owner = owner_with_body(body);
        let mut changed = probe(ProbeFamily::Predicate, "amount >= 100");
        changed.location = SourceLocation::new("src/lib.rs", 2, 1);
        let evidence = |test: &TestSummary, activation: &ActivationEvidence| {
            infection_evidence_for_owner(
                &changed,
                &[test],
                activation,
                None,
                OwnerInputs {
                    owner: Some(&owner),
                    literal_length_boundary: None,
                    slice_counter_boundary: None,
                    unary_tail: None,
                    literal_condition: None,
                },
                false,
                false,
            )
        };
        let rstest = |body: &str, attrs: &[&str], call: &str, literals: &[&str]| {
            let mut test = test_with_literals(literals);
            test.body = body.into();
            test.attrs = attrs
                .iter()
                .map(|attr| (*attr).to_string())
                .collect::<Vec<_>>()
                .into();
            test.calls = vec![crate::analysis::facts::CallFact {
                name: "pick".to_string(),
                line: 2,
                text: call.to_string().into(),
            }]
            .into();
            test
        };
        let fed = rstest(
            "fn at_threshold(amount: u32) { assert_eq!(pick(amount), 10); }",
            &["#[rstest]"],
            "assert_eq!(pick(amount), 10);",
            &["10"],
        );
        let unknown = evidence(&fed, &activation_with(&[]));
        assert_eq!(unknown.state, StageState::Unknown, "{}", unknown.summary);
        assert!(
            unknown.summary.contains("fixture `amount`"),
            "{}",
            unknown.summary
        );
        // A readable boundary input still wins over the opaque reading.
        let reached = evidence(
            &fed,
            &activation_with(&[("amount = 100", ValueContext::FunctionArgument)]),
        );
        assert_eq!(reached.state, StageState::Yes, "{}", reached.summary);
        // The boundary only as an expected value, with no fixture: weak.
        let oracle_only = rstest(
            "fn t() { assert_eq!(pick(50), 100); }",
            &["#[test]"],
            "assert_eq!(pick(50), 100);",
            &["50", "100"],
        );
        let weak = evidence(
            &oracle_only,
            &activation_with(&[("amount = 50", ValueContext::FunctionArgument)]),
        );
        assert_eq!(weak.state, StageState::Weak, "{}", weak.summary);
    }

    #[test]
    fn predicate_infection_credits_the_same_literal_when_it_is_an_owner_input() {
        // Alternate of the expected-value case: the same boundary literal
        // passed as the owner's argument activates the boundary.
        let probe = probe(ProbeFamily::Predicate, "weight_grams > 2_000");
        let test = test_with_literals(&["2000", "400"]);
        let activation = activation_with(&[
            ("400", ValueContext::AssertionArgument),
            ("weight_grams = 2_000", ValueContext::FunctionArgument),
        ]);
        let evidence = infection_evidence(&probe, &[&test], &activation);

        assert_eq!(evidence.state, StageState::Yes);
        assert_eq!(
            evidence.summary,
            "Detected test input literal matching changed boundary: 2000"
        );
    }

    #[test]
    fn predicate_infection_credits_table_row_inputs_but_not_enum_variants() {
        let probe = probe(ProbeFamily::Predicate, "amount > 10");
        let test = test_with_literals(&["10", "11"]);
        let table = activation_with(&[("10", ValueContext::TableRow)]);
        assert_eq!(
            infection_evidence(&probe, &[&test], &table).state,
            StageState::Yes
        );

        // A non-input context (an enum variant) never counts as an input.
        let enum_only = activation_with(&[("10", ValueContext::EnumVariant)]);
        assert_eq!(
            infection_evidence(&probe, &[&test], &enum_only).state,
            StageState::Weak
        );
    }

    #[test]
    fn predicate_infection_names_the_boundary_when_tests_pass_no_literals() {
        let probe = probe(ProbeFamily::Predicate, "value > 10");
        let test = test_with_literals(&[]);
        let evidence = infection_evidence(&probe, &[&test], &ActivationEvidence::default());

        assert_eq!(evidence.state, StageState::Unknown);
        assert_eq!(
            evidence.summary,
            "Related tests pass no literal ripr can compare with the changed boundary [10] (inputs are strings, computed values or fixtures); activation/infection is unknown, and if a test input drives the compared value, one that sits exactly on the boundary would settle it"
        );
    }

    #[test]
    fn predicate_infection_does_not_compare_char_literals_with_numeric_boundaries() {
        let numeric = probe(ProbeFamily::Predicate, "value > 1");
        let chars = test_with_literals(&["'x'", "b','"]);
        let evidence = infection_evidence(&numeric, &[&chars], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        assert_eq!(
            evidence.summary,
            "The only literals ripr reads in related tests are char/byte literals ['x', b','], which ripr does not compare with the changed numeric boundary [1]; activation/infection is unknown"
        );

        // A byte boundary counts byte literals only, not the char `'x'`.
        let byte = probe(ProbeFamily::Predicate, "digit > b'9'");
        let evidence = infection_evidence(&byte, &[&chars], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Weak);
        assert!(evidence.summary.contains("[b',']"), "{}", evidence.summary);
    }

    #[test]
    fn non_predicate_infection_without_related_tests_is_unknown() {
        let probe = probe(ProbeFamily::ReturnValue, "value + 1");
        let evidence = infection_evidence(&probe, &[], &ActivationEvidence::default());

        assert_eq!(evidence.state, StageState::Unknown);
        assert_eq!(
            evidence.summary,
            "No reachable tests were found, so infection cannot be established"
        );
    }

    #[test]
    fn wildcard_discard_is_infection_unknown_even_with_related_tests() {
        let probe = probe(ProbeFamily::SideEffect, "let _ = compute_fee(amount)");
        let test = test_with_literals(&["42"]);
        let evidence = infection_evidence(&probe, &[&test], &ActivationEvidence::default());

        assert_eq!(evidence.state, StageState::Unknown);
        assert_eq!(
            evidence.summary,
            "Changed value is bound to a discard pattern; it cannot infect a sink"
        );
    }

    #[test]
    fn typed_wildcard_discard_is_infection_unknown() {
        let probe = probe(ProbeFamily::ReturnValue, "let _: u32 = helper(x)");
        let test = test_with_literals(&["1"]);
        let evidence = infection_evidence(&probe, &[&test], &ActivationEvidence::default());

        assert_eq!(evidence.state, StageState::Unknown);
        assert_eq!(
            evidence.summary,
            "Changed value is bound to a discard pattern; it cannot infect a sink"
        );
    }

    #[test]
    fn whitespace_padded_wildcard_discards_are_infection_unknown() {
        // #3233: the infection stage consumes the shared predicate directly,
        // so non-canonical whitespace shapes must classify as discards here
        // too — a regression to the old exact-prefix match in this stage
        // alone would fail this test even if the flow stage stayed correct.
        for expression in [
            "let _ : u32 = helper(x);",
            "let _=helper(x);",
            "let   _   =   helper(x);",
        ] {
            let probe = probe(ProbeFamily::ReturnValue, expression);
            let test = test_with_literals(&["1"]);
            let evidence = infection_evidence(&probe, &[&test], &ActivationEvidence::default());
            assert_eq!(
                evidence.state,
                StageState::Unknown,
                "`{expression}` must be a discard in the infection stage"
            );
        }
    }

    #[test]
    fn named_binding_is_not_a_discard_stays_yes() {
        // `let _name = ...` is a named binding that could be used — must stay Yes
        let probe = probe(ProbeFamily::ReturnValue, "let _result = helper(a)");
        let test = test_with_literals(&["1"]);
        let evidence = infection_evidence(&probe, &[&test], &ActivationEvidence::default());

        assert_eq!(evidence.state, StageState::Yes);
    }

    #[test]
    fn return_value_read_into_tail_stays_exposed() {
        // Control: `let result = helper(a); result + 1` (value read into return) must stay Yes
        let probe = probe(ProbeFamily::ReturnValue, "result + 1");
        let test = test_with_literals(&["1"]);
        let evidence = infection_evidence(&probe, &[&test], &ActivationEvidence::default());

        assert_eq!(evidence.state, StageState::Yes);
    }

    fn activation_with(facts: &[(&str, ValueContext)]) -> ActivationEvidence {
        ActivationEvidence {
            observed_values: facts
                .iter()
                .map(|(value, context)| ValueFact {
                    file: None,
                    line: 1,
                    text: String::new(),
                    value: (*value).to_string(),
                    context: context.clone(),
                })
                .collect(),
            missing_discriminators: Vec::new(),
        }
    }

    fn probe(family: ProbeFamily, expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:test".to_string()),
            location: SourceLocation::new("src/lib.rs", 1, 1),
            owner: None,
            family,
            delta: DeltaKind::Value,
            before: None,
            after: None,
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    fn test_with_literals(values: &[&str]) -> TestSummary {
        TestSummary {
            name: "value_boundary".to_string(),
            file: PathBuf::from("tests/value.rs"),
            start_line: 1,
            end_line: 3,
            body: "assert_eq!(score(10), 11);".into(),
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            assertions: Vec::new(),
            literals: values
                .iter()
                .map(|value| LiteralFact {
                    line: 1,
                    value: (*value).to_string(),
                })
                .collect(),
            attrs: crate::analysis::facts::SharedFactSlice::empty(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
            generated: None,
        }
    }

    fn owner_with_body(body: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::pick".to_string()),
            name: "pick".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: body.lines().count(),
            body: body.into(),
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            returns: Vec::new(),
            literals: crate::analysis::facts::SharedFactSlice::empty(),
            source_role: crate::analysis::facts::FunctionSourceRole::Production,
            attrs: crate::analysis::facts::SharedFactSlice::empty(),
            impl_attrs: Vec::new(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
            item: Default::default(),
            impl_context: Default::default(),
        }
    }

    fn local_boundary_evidence(body: &str, test_literal: &str) -> StageEvidence {
        // The predicate sits on line 3 of the owner body.
        local_predicate_evidence(body, "val > surrogate_start", 3, test_literal)
    }

    fn local_predicate_evidence(
        body: &str,
        expression: &str,
        line: usize,
        test_literal: &str,
    ) -> StageEvidence {
        let mut changed = probe(ProbeFamily::Predicate, expression);
        changed.location = SourceLocation::new("src/lib.rs", line, 1);
        let owner = owner_with_body(body);
        let test = test_with_literals(&[test_literal]);
        let activation = activation_with(&[(
            &format!("val = {test_literal}"),
            ValueContext::FunctionArgument,
        )]);
        infection_evidence_for_owner(
            &changed,
            &[&test],
            &activation,
            None,
            OwnerInputs {
                owner: Some(&owner),
                literal_length_boundary: None,
                slice_counter_boundary: None,
                unary_tail: None,
                literal_condition: None,
            },
            true,
            false,
        )
    }

    fn added_string_arm_evidence(
        body: &str,
        before: &str,
        after: &str,
        inputs: &[&str],
        other_callers: bool,
    ) -> StageEvidence {
        added_string_arm_evidence_with(body, before, after, inputs, other_callers, false)
    }

    fn added_string_arm_evidence_with(
        body: &str,
        before: &str,
        after: &str,
        inputs: &[&str],
        other_callers: bool,
        assertion_macros_rebound: bool,
    ) -> StageEvidence {
        let mut changed = probe(ProbeFamily::Predicate, after);
        changed.before = Some(before.to_string());
        let owner = owner_with_body(body);
        let literals = inputs
            .iter()
            .filter_map(|input| input.split_once(" = ").map(|(_, value)| value))
            .collect::<Vec<_>>();
        let test = test_with_literals(&literals);
        let facts = inputs
            .iter()
            .map(|input| (*input, ValueContext::FunctionArgument))
            .collect::<Vec<_>>();
        let activation = activation_with(&facts);
        infection_evidence_for_owner(
            &changed,
            &[&test],
            &activation,
            None,
            OwnerInputs {
                owner: Some(&owner),
                literal_length_boundary: None,
                slice_counter_boundary: None,
                unary_tail: None,
                literal_condition: None,
            },
            other_callers,
            assertion_macros_rebound,
        )
    }

    /// #15027: integer and char arms read like string arms; every near miss,
    /// including spellings that may share a value, stays unknown.
    #[test]
    fn an_input_equal_to_an_appended_integer_or_char_arm_reaches_the_change() {
        let int_body = "pub fn small(n: u32) -> bool {\n    n == 1 || n == 2\n}\n";
        let reached =
            added_string_arm_evidence(int_body, "n == 1", "n == 1 || n == 2", &["n = 2"], false);
        assert_eq!(reached.state, StageState::Yes, "{}", reached.summary);
        assert!(
            reached.summary.contains("integer-equality"),
            "{}",
            reached.summary
        );
        let char_body = "pub fn sep(c: char) -> bool {\n    c == ',' || c == ';'\n}\n";
        let reached = added_string_arm_evidence(
            char_body,
            "c == ','",
            "c == ',' || c == ';'",
            &["c = ';'"],
            false,
        );
        assert_eq!(reached.state, StageState::Yes, "{}", reached.summary);
        assert!(
            reached.summary.contains("char-equality"),
            "{}",
            reached.summary
        );
        // The string message is unchanged.
        let str_body =
            "pub fn pick(flag: &str) -> bool {\n    flag == \"-v\" || flag == \"--verbose\"\n}\n";
        let reached = added_string_arm_evidence(
            str_body,
            "flag == \"-v\"",
            "flag == \"-v\" || flag == \"--verbose\"",
            &["flag = \"--verbose\""],
            false,
        );
        assert!(
            reached
                .summary
                .contains("every original string-equality arm is false"),
            "{}",
            reached.summary
        );

        // Inputs that do not hit the added arm, or another caller.
        for (input, others) in [("n = 1", false), ("n = 3", false), ("n = 2", true)] {
            let evidence =
                added_string_arm_evidence(int_body, "n == 1", "n == 1 || n == 2", &[input], others);
            assert_ne!(
                evidence.state,
                StageState::Yes,
                "{input}: {}",
                evidence.summary
            );
        }
        // Integer types only: a newtype or float may compare otherwise.
        for body in [
            "pub fn small(n: Level) -> bool {\n    n == 1 || n == 2\n}\n",
            "pub fn small(n: f64) -> bool {\n    n == 1 || n == 2\n}\n",
            "pub fn small(n: &u32) -> bool {\n    n == 1 || n == 2\n}\n",
            "pub fn small(n: char) -> bool {\n    n == 1 || n == 2\n}\n",
        ] {
            let evidence =
                added_string_arm_evidence(body, "n == 1", "n == 1 || n == 2", &["n = 2"], false);
            assert_ne!(
                evidence.state,
                StageState::Yes,
                "{body}: {}",
                evidence.summary
            );
        }
        // `char` only for char literals.
        let wide = "pub fn sep(c: u8) -> bool {\n    c == ',' || c == ';'\n}\n";
        let evidence = added_string_arm_evidence(
            wide,
            "c == ','",
            "c == ',' || c == ';'",
            &["c = ';'"],
            false,
        );
        assert_ne!(evidence.state, StageState::Yes, "{}", evidence.summary);
        // Spellings that may share one value, mixed kinds and operator or
        // escaped chars decline.
        for (before, after, input) in [
            ("n == 5", "n == 5 || n == 05", "n = 05"),
            ("n == 5", "n == 5 || n == 5_u32", "n = 5_u32"),
            ("n == 5", "n == 5 || n == 0x5", "n = 0x5"),
            ("n == 10", "n == 10 || n == 1_0", "n = 1_0"),
            ("n == 1", "n == 1 || n == -2", "n = -2"),
            ("n == 1", "n == 1 || n == 1", "n = 1"),
            ("n == 1", "n == 1 || n == 'a'", "n = 'a'"),
        ] {
            let body = format!("pub fn small(n: u32) -> bool {{\n    {after}\n}}\n");
            let evidence = added_string_arm_evidence(&body, before, after, &[input], false);
            assert_ne!(
                evidence.state,
                StageState::Yes,
                "{after}: {}",
                evidence.summary
            );
        }
        for (before, after, input) in [
            ("c == 'A'", "c == 'A' || c == '\\x41'", "c = '\\x41'"),
            ("c == ','", "c == ',' || c == '|'", "c = '|'"),
            ("c == ','", "c == ',' || c == '!'", "c = '!'"),
            ("c == ','", "c == ',' || c == '\\''", "c = '\\''"),
        ] {
            let body = format!("pub fn sep(c: char) -> bool {{\n    {after}\n}}\n");
            let evidence = added_string_arm_evidence(&body, before, after, &[input], false);
            assert_ne!(
                evidence.state,
                StageState::Yes,
                "{after}: {}",
                evidence.summary
            );
        }
    }

    /// #14388 review: only types whose `==` against a literal compares the
    /// text qualify for the appended string-arm reading.
    #[test]
    fn only_plain_string_types_compare_a_literal_by_text() {
        for accepted in [
            "&str",
            "& str",
            "&'a str",
            "&'static str",
            "String",
            "&String",
        ] {
            assert!(is_plain_string_type(accepted), "{accepted}");
        }
        for rejected in [
            "&mut str",
            "&&str",
            "Cow<str>",
            "impl AsRef<str>",
            "Box<str>",
            "T",
            "Option<&str>",
            "&'a",
            "&'a mut str",
            "Flag",
        ] {
            assert!(!is_plain_string_type(rejected), "{rejected}");
        }
    }

    /// #14388 (agent-eval `struct_field`): `flag == "-v"` gains
    /// `|| flag == "--verbose"`; a related test calling the owner with
    /// `"--verbose"` reaches the added arm. Every near miss stays unknown.
    #[test]
    fn an_input_equal_to_an_appended_string_arm_reaches_the_change() {
        let body =
            "pub fn pick(flag: &str) -> bool {\n    flag == \"-v\" || flag == \"--verbose\"\n}\n";
        let before = "flag == \"-v\"";
        let after = "flag == \"-v\" || flag == \"--verbose\"";
        let reached =
            added_string_arm_evidence(body, before, after, &["flag = \"--verbose\""], false);
        assert_eq!(reached.state, StageState::Yes, "{}", reached.summary);
        assert!(
            reached.summary.contains("\"--verbose\""),
            "{}",
            reached.summary
        );

        // The original arm's literal does not reach the added arm.
        let original = added_string_arm_evidence(body, before, after, &["flag = \"-v\""], false);
        assert_ne!(original.state, StageState::Yes, "{}", original.summary);
        // A non-matching input does not either.
        let other = added_string_arm_evidence(body, before, after, &["flag = \"-q\""], false);
        assert_ne!(other.state, StageState::Yes, "{}", other.summary);
        // Another caller may feed the owner other inputs.
        let wrapped =
            added_string_arm_evidence(body, before, after, &["flag = \"--verbose\""], true);
        assert_ne!(wrapped.state, StageState::Yes, "{}", wrapped.summary);
        // A rebound assertion macro may discard the owner call.
        let discarded = added_string_arm_evidence_with(
            body,
            before,
            after,
            &["flag = \"--verbose\""],
            false,
            true,
        );
        assert_ne!(discarded.state, StageState::Yes, "{}", discarded.summary);
        // A parameter type with its own `PartialEq<&str>` may make an
        // original arm true on the new literal; plain string types qualify.
        let custom =
            "pub fn pick(flag: Flag) -> bool {\n    flag == \"-v\" || flag == \"--verbose\"\n}\n";
        let opaque =
            added_string_arm_evidence(custom, before, after, &["flag = \"--verbose\""], false);
        assert_ne!(opaque.state, StageState::Yes, "{}", opaque.summary);
        for plain in [
            "pub fn pick(flag: String) -> bool {\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick<'a>(flag: &'a str) -> bool {\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
        ] {
            let evidence =
                added_string_arm_evidence(plain, before, after, &["flag = \"--verbose\""], false);
            assert_eq!(
                evidence.state,
                StageState::Yes,
                "{plain}: {}",
                evidence.summary
            );
        }
        // The argument binds a different parameter.
        let two = "pub fn pick(mode: &str, flag: &str) -> bool {\n    flag == \"-v\" || flag == \"--verbose\"\n}\n";
        let elsewhere =
            added_string_arm_evidence(two, before, after, &["mode = \"--verbose\""], false);
        assert_ne!(elsewhere.state, StageState::Yes, "{}", elsewhere.summary);
        // A rebinding before the comparison changes what is compared.
        // The rebinding is paren-free so only the rebinding guard declines it.
        let shadowed = "pub fn pick(flag: &str, other: &str) -> bool {\n    let flag = other;\n    flag == \"-v\" || flag == \"--verbose\"\n}\n";
        let rebound =
            added_string_arm_evidence(shadowed, before, after, &["flag = \"--verbose\""], false);
        assert_ne!(rebound.state, StageState::Yes, "{}", rebound.summary);
        // A continuation after the changed line (`&& strict`) binds tighter
        // than the appended arm, so the chain does not end the expression.
        let continued = "pub fn pick(flag: &str, strict: bool) -> bool {\n    flag == \"-v\" || flag == \"--verbose\"\n        && strict\n}\n";
        let tail =
            added_string_arm_evidence(continued, before, after, &["flag = \"--verbose\""], false);
        assert_ne!(tail.state, StageState::Yes, "{}", tail.summary);
        // An early return on another parameter can keep the input away
        // from the chain, and so can an earlier `?`, `match` arm, panicking
        // macro, diverging call, index or division.
        for diverted in [
            "pub fn pick(flag: &str, strict: bool) -> bool {\n    if strict { return false; }\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick(flag: &str, gate: Option<()>) -> Option<bool> {\n    gate?;\n    Some(flag == \"-v\" || flag == \"--verbose\")\n}\n",
            "pub fn pick(flag: &str, mode: u8) -> bool {\n    match mode { 0 => false, _ => flag == \"-v\" || flag == \"--verbose\" }\n}\n",
            "pub fn pick(flag: &str, strict: bool) -> bool {\n    assert!(!strict);\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick(flag: &str, strict: bool) -> bool {\n    std::process::exit(i32::from(strict));\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick(flag: &str, at: usize) -> bool {\n    let _ = [true][at];\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick(flag: &str, by: u8) -> bool {\n    let _ = 1 / by;\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick(flag: &str, by: u8) -> bool {\n    let _ = 7 % by;\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick(flag: &str) -> bool {\n    let _g = move |x: u8| x;\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
            "pub fn pick(flag: &str) -> bool {\n    unreachable!{};\n    flag == \"-v\" || flag == \"--verbose\"\n}\n",
        ] {
            let evidence = added_string_arm_evidence(
                diverted,
                before,
                after,
                &["flag = \"--verbose\""],
                false,
            );
            assert_ne!(
                evidence.state,
                StageState::Yes,
                "{diverted}: {}",
                evidence.summary
            );
        }
        // A struct-literal field (the agent-eval shape) is not diverted, and
        // the literal-first spelling reads the same.
        let field = "pub fn pick(flag: &str) -> Options {\n    Options {\n        retries: 3,\n        verbose: \"-v\" == flag || \"--verbose\" == flag,\n    }\n}\n";
        let flipped = added_string_arm_evidence(
            field,
            "\"-v\" == flag",
            "\"-v\" == flag || \"--verbose\" == flag",
            &["flag = \"--verbose\""],
            false,
        );
        assert_eq!(flipped.state, StageState::Yes, "{}", flipped.summary);
        // An `&&` chain, a duplicated literal and a non-literal arm decline.
        for (before, after) in [
            ("flag == \"-v\"", "flag == \"-v\" && flag == \"--verbose\""),
            (
                "flag == \"--verbose\"",
                "flag == \"--verbose\" || flag == \"--verbose\"",
            ),
            ("flag == \"-v\"", "flag == \"-v\" || flag == other"),
            ("flag == \"-v\"", "flag == \"-v\" || flag != \"--verbose\""),
        ] {
            let evidence =
                added_string_arm_evidence(body, before, after, &["flag = \"--verbose\""], false);
            assert_ne!(
                evidence.state,
                StageState::Yes,
                "{after}: {}",
                evidence.summary
            );
        }
    }

    #[test]
    fn a_predicate_against_a_local_with_a_literal_initializer_reads_that_boundary() {
        // #11519 (fastrand): `val > surrogate_start` spells no literal, but
        // the local's live initializer is the boundary a test input hits.
        let owner = "fn pick(val: u32) -> bool {\n    let surrogate_start = 0xd800u32;\n    val > surrogate_start\n}\n";
        let evidence = local_boundary_evidence(owner, "0xd800u32");
        assert_eq!(evidence.state, StageState::Yes, "{}", evidence.summary);
        assert!(evidence.summary.contains("0xd800"), "{}", evidence.summary);
    }

    #[test]
    fn a_local_initializer_is_read_from_either_operand_position() {
        let owner = "fn pick(val: u32) -> bool {\n    let surrogate_start = 0xd800u32;\n    surrogate_start < val\n}\n";
        let evidence = local_predicate_evidence(owner, "surrogate_start < val", 3, "0xd800u32");
        assert_eq!(evidence.state, StageState::Yes, "{}", evidence.summary);
    }

    #[test]
    fn computed_local_does_not_contribute_its_operand_literals_as_boundaries() {
        let owner = "fn pick(low: u32, range: u32) -> bool {\n    let val = draw(0..=range) + low;\n    let surrogate_start = 0xd800u32;\n    val > surrogate_start\n}\n";
        let evidence = local_predicate_evidence(owner, "val > surrogate_start", 4, "0");
        assert_ne!(evidence.state, StageState::Yes, "{}", evidence.summary);
        assert!(evidence.summary.contains("0xd800"), "{}", evidence.summary);
        assert!(
            !evidence.summary.contains("boundary [0]"),
            "{}",
            evidence.summary
        );
        let reversed = local_predicate_evidence(owner, "surrogate_start < val", 4, "0");
        assert_ne!(reversed.state, StageState::Yes, "{}", reversed.summary);
        assert!(reversed.summary.contains("0xd800"), "{}", reversed.summary);
        let positive = local_predicate_evidence(owner, "val > surrogate_start", 4, "0xd800u32");
        assert_eq!(positive.state, StageState::Yes, "{}", positive.summary);
        let only_computed =
            "fn pick(low: u32) -> bool {\n    let val = low + 1;\n    val > other\n}\n";
        let unknown = local_predicate_evidence(only_computed, "val > other", 3, "1");
        assert_eq!(unknown.state, StageState::Unknown, "{}", unknown.summary);
    }

    #[test]
    fn conditional_initializer_retains_literal_result_not_computed_parts() {
        let owner = "fn pick(val: u32, si: bool, qty: u32) -> bool {\n    let unit = if si && qty + 1 > 3 { 1000 + 1 } else { 1024 };\n    val < unit\n}\n";
        let literal = local_predicate_evidence(owner, "val < unit", 3, "1024");
        assert_eq!(literal.state, StageState::Yes, "{}", literal.summary);
        for operand in ["1", "3", "1000"] {
            let evidence = local_predicate_evidence(owner, "val < unit", 3, operand);
            assert_ne!(evidence.state, StageState::Yes, "{}", evidence.summary);
            assert!(evidence.summary.contains("1024"), "{}", evidence.summary);
        }
        let signed = "fn pick(val: i32, qty: i32) -> bool {\n    let unit = if qty + 1 > 3 { (-5) } else { ((1024)) };\n    val < unit\n}\n";
        for literal in ["-5", "1024"] {
            let evidence = local_predicate_evidence(signed, "val < unit", 3, literal);
            assert_eq!(evidence.state, StageState::Yes, "{}", evidence.summary);
        }
        let values = super::conditional_literal_results(
            "if qty + 1 > 3 { -(5) } else if flag { { 7 } } else { 9 }",
        );
        assert_eq!(values, vec!["-5", "7", "9"]);
        let values =
            super::conditional_literal_results("if qty + 1 > 3 { let x = 7; x } else { 9 + 1 }");
        assert!(values.is_empty(), "{values:?}");
        let guard = "fn pick(val: u32, qty: u32) -> bool {\n    let unit = if qty > 3 { 1 } else { 2 };\n    val < unit\n}\n";
        let not_boundary = local_predicate_evidence(guard, "val < unit", 3, "3");
        assert_ne!(
            not_boundary.state,
            StageState::Yes,
            "{}",
            not_boundary.summary
        );
        let boundary = local_predicate_evidence(guard, "val < unit", 3, "2");
        assert_eq!(boundary.state, StageState::Yes, "{}", boundary.summary);
    }

    #[test]
    fn two_local_operands_and_multi_literal_initializers_contribute_their_literals() {
        let both = "fn pick(val: u32) -> bool {\n    let low = 5u32;\n    let high = 10u32;\n    low < high\n}\n";
        let evidence = local_predicate_evidence(both, "low < high", 4, "10u32");
        assert_eq!(evidence.state, StageState::Yes, "{}", evidence.summary);
        let multi = "fn pick(val: u32, si: bool) -> bool {\n    let unit = if si { 1000 } else { 1024 };\n    val < unit\n}\n";
        let evidence = local_predicate_evidence(multi, "val < unit", 3, "1024");
        assert_eq!(evidence.state, StageState::Yes, "{}", evidence.summary);
    }

    #[test]
    fn a_shadowed_or_later_local_does_not_supply_the_boundary() {
        // The live declaration at the predicate is the computed one.
        let shadowed = "fn pick(val: u32) -> bool {\n    let surrogate_start = 0xd800u32;\n    let surrogate_start = compute();\n    val > surrogate_start\n}\n";
        let evidence = local_predicate_evidence(shadowed, "val > surrogate_start", 4, "0xd800u32");
        assert_eq!(evidence.state, StageState::Unknown, "{}", evidence.summary);
        // A declaration after the predicate line does not feed it.
        let later = "fn pick(val: u32) -> bool {\n    let hit = val > surrogate_start;\n    let surrogate_start = 0xd800u32;\n    hit\n}\n";
        let evidence = local_predicate_evidence(later, "val > surrogate_start", 2, "0xd800u32");
        assert_eq!(evidence.state, StageState::Unknown, "{}", evidence.summary);
    }

    #[test]
    fn a_predicate_against_a_local_without_a_literal_initializer_stays_unknown() {
        // Discriminating negative: the same predicate shape, but the local is
        // computed, so no boundary is resolvable and infection stays unknown.
        let owner = "fn pick(val: u32) -> bool {\n    let surrogate_start = compute();\n    val > surrogate_start\n}\n";
        let evidence = local_boundary_evidence(owner, "0xd800u32");
        assert_eq!(evidence.state, StageState::Unknown, "{}", evidence.summary);
        // And with no declaration of the compared name at all.
        let bare =
            "fn pick(val: u32) -> bool {\n    let other = 1u32;\n    val > surrogate_start\n}\n";
        let evidence = local_boundary_evidence(bare, "0xd800u32");
        assert_eq!(evidence.state, StageState::Unknown, "{}", evidence.summary);
    }

    #[test]
    fn unresolved_boundary_input_is_unknown_not_weak() {
        // #6674/#6693: the boundary literal 16 appears in the test only as
        // an array length; with the activation authority's unresolved
        // reason the stage abstains instead of asking for input 16.
        let probe = probe(ProbeFamily::Predicate, "16 < count");
        let test = test_with_literals(&["16", "17"]);
        let unresolved = infection_evidence_with_boundary_input(
            &probe,
            &[&test],
            &ActivationEvidence::default(),
            Some("boundary operand `count` is a local or computed value"),
        );
        assert_eq!(unresolved.state, StageState::Unknown);
        assert!(unresolved.summary.contains("boundary operand `count`"));

        // Negative control: without the reason the literal path still
        // reads weak.
        let weak = infection_evidence(&probe, &[&test], &ActivationEvidence::default());
        assert_eq!(weak.state, StageState::Weak);
    }

    #[test]
    fn unresolved_boundary_input_is_not_credited_by_a_matching_input_literal() {
        // CodeRabbit review on #6796: `name.len() > 3` with an owner input
        // `3` would read `yes` from the literal match, though `3` is not
        // the length. The unresolved reason wins over that match.
        let probe = probe(ProbeFamily::Predicate, "name.len() > 3");
        let test = test_with_literals(&["3"]);
        let activation = activation_with(&[("3", ValueContext::FunctionArgument)]);
        let unresolved = infection_evidence_with_boundary_input(
            &probe,
            &[&test],
            &activation,
            Some("boundary operand `name.len()` is a local or computed value"),
        );
        assert_eq!(unresolved.state, StageState::Unknown);

        // Negative control: without the reason the literal match credits.
        let matched = infection_evidence(&probe, &[&test], &activation);
        assert_eq!(matched.state, StageState::Yes);

        // An exact observed equality still credits with the reason present.
        let observed = activation_with(&[("name.len() == 3", ValueContext::FunctionArgument)]);
        let exact = infection_evidence_with_boundary_input(
            &probe,
            &[&test],
            &observed,
            Some("boundary operand `name.len()` is a local or computed value"),
        );
        assert_eq!(exact.state, StageState::Yes);
    }

    #[test]
    fn literal_kind_filter_reads_only_the_spelled_boundary_literals() {
        // Merge of #6796 with the literal-kind filter: test literals are
        // kept by the kind of the boundary literals left after computed
        // operands are dropped. With no boundary literal left the stage is
        // unknown before any test literal is consulted, so dropping every
        // test literal never turns into a weak "no match" claim.
        let computed = probe(ProbeFamily::Predicate, "ready && total < base * 3");
        let numeric = test_with_literals(&["3", "4"]);
        let evidence = infection_evidence(&computed, &[&numeric], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        assert!(
            evidence.summary.contains("no literal boundary"),
            "{}",
            evidence.summary
        );

        // A spelled boundary beside a computed one compares only literals
        // of its own kind: a char literal is not a numeric input.
        let mixed = probe(ProbeFamily::Predicate, "amount > 10 && total < base * 3");
        let chars = test_with_literals(&["'x'", "3"]);
        let evidence = infection_evidence(&mixed, &[&chars], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Weak);
        assert!(
            evidence.summary.contains("Tests have literals [3]"),
            "{}",
            evidence.summary
        );
        let only_chars = test_with_literals(&["'x'"]);
        let evidence = infection_evidence(&mixed, &[&only_chars], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        // #6902: the char literal is named as a kind mismatch.
        assert_eq!(
            evidence.summary,
            "The only literals ripr reads in related tests are char literals ['x'], which ripr does not compare with the changed numeric boundary [10]; activation/infection is unknown"
        );
    }

    #[test]
    fn all_dropped_boundary_literals_read_the_no_literal_boundary_reason() {
        // ub-review on #6796: when every boundary literal is dropped (all
        // sit inside computed operands) and the literal-kind filter then
        // keeps no test literal, the summary is the no-literal-boundary
        // reason, never a no-static-path claim.
        let computed = probe(ProbeFamily::Predicate, "total < base * 3 || n == m + 1");
        let mixed_kinds = test_with_literals(&["3", "'x'", "1"]);
        let evidence =
            infection_evidence(&computed, &[&mixed_kinds], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        assert_eq!(
            evidence.summary,
            "Predicate changed, but no literal boundary was visible in the changed expression"
        );
        assert!(!evidence.summary.contains("static path"));

        // A computed operand over a named constant (`LIMIT * 3`) is not the
        // constant itself, so it reads the same no-literal reason.
        let constant = probe(ProbeFamily::Predicate, "total < LIMIT * 3");
        let evidence =
            infection_evidence(&constant, &[&mixed_kinds], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        assert_eq!(
            evidence.summary,
            "Predicate changed, but no literal boundary was visible in the changed expression"
        );
    }

    #[test]
    fn computed_comparison_operand_is_not_its_contained_literal() {
        // #6671: `CURRENT - 2 > version` never compares against `2`.
        assert!(boundary_literals("CURRENT - 2 > manifest.version").is_empty());
        assert!(boundary_literals("s.len() < 2 + 2").is_empty());
        assert_eq!(boundary_literals("amount * 2 > 100"), vec!["100"]);
        assert_eq!(boundary_literals("amount > 10"), vec!["10"]);
        // Compound predicates (CodeRabbit review on #6796): each
        // comparison is read alone, so a literal inside a computed
        // operand is never the boundary of the whole condition.
        assert_eq!(
            boundary_literals("amount > 10 && total < base * 3"),
            vec!["10"]
        );
        assert!(boundary_literals("(count < 2 + 2) || flag").is_empty());
        // A closure default is not a comparison operand: unchanged.
        assert_eq!(
            boundary_literals("let end = input.rfind(delim).map_or_else(|| 1, |idx| idx);"),
            vec!["1"]
        );
        assert_eq!(
            boundary_literals("!(limit - 1 >= used) && used != 0"),
            vec!["0"]
        );
        // No computed operand anywhere keeps every literal, as before.
        assert_eq!(
            boundary_literals("amount > 10 && total < 20"),
            vec!["10", "20"]
        );
        let compound = probe(ProbeFamily::Predicate, "ready && total < base * 3");
        let test = test_with_literals(&["3"]);
        let evidence = infection_evidence(&compound, &[&test], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
        let probe = probe(ProbeFamily::Predicate, "CURRENT - 2 > manifest.version");
        let test = test_with_literals(&["2", "4", "5"]);
        let evidence = infection_evidence(&probe, &[&test], &ActivationEvidence::default());
        assert_eq!(evidence.state, StageState::Unknown);
    }
}
