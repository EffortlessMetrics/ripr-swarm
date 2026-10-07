//! Same-test pairing for boundary-class (predicate) probes.
//!
//! `exposed` requires one test that both feeds a boundary input to the owner
//! and holds a discriminating oracle on that call's result. Crediting a
//! boundary call from one test and an exact oracle from another is a false
//! `exposed` (#4828): `>=` → `>` still passes both tests.

use super::super::facts::CallFact;
use super::super::rust_index::{FunctionSummary, OracleFact, TestSummary, extract_literals};
use super::activation::{
    call_arguments, comparison_operands, function_parameters, owner_argument_values,
};
use super::reveal::assertion_comparison_operands;
use super::text::delimited_contents_at;
use crate::analysis::syntax::HelperLoan;
use crate::domain::*;

/// Token carried in the discriminate summary when a predicate would otherwise
/// read `exposed` without an admitted oracle on the boundary call.
pub(in crate::analysis) const SAME_TEST_PAIRING_MISSING: &str = "same_test_pairing_missing";

pub(in crate::analysis) fn same_test_pairing_missing_summary() -> String {
    format!(
        "Discriminator unconfirmed: no admitted discriminating oracle is paired with the owner's boundary call ({SAME_TEST_PAIRING_MISSING}); a boundary input and a separate exact oracle do not establish that discriminator"
    )
}

/// True when some related test both feeds a boundary input to the owner and
/// holds an admitted discriminating oracle on that call's result.
///
/// Boundary credit is the activation authority's `==` facts (named constants,
/// helper hops, local bindings), not a second matcher. Call-argument matching
/// remains only for the literal/parameter shape those facts do not cover: the
/// argument must *be* the boundary literal or a name bound to it. An expression
/// that merely mentions the literal (`if false { 10 } else { 50 }`,
/// `std::cmp::max(10, 50)`) does not pair (#6668).
///
/// Non-predicate probes are not this gate; the caller must not use a `true`
/// result to promote a family this function does not judge.
///
/// `owner_pinned` is reveal's owner-return pin (RIPR-SPEC-0197): an
/// `assert!(owner(x))` on a bool owner discriminates its whole result even
/// though the classifier reads a bare `assert!` as a weak relational check.
///
/// `helper_loan` is the owner-pin admission's check helper an admitted
/// assertion is borrowed from (#6482). Such an assertion pairs through the
/// helper's calls in the test: see [`loan_pairs_boundary_call`].
pub(in crate::analysis) fn has_same_test_boundary_oracle_pairing(
    probe: &Probe,
    owner_fn: Option<&FunctionSummary>,
    related_tests: &[&TestSummary],
    activation: &ActivationEvidence,
    assertion_admitted: &dyn Fn(&TestSummary, &OracleFact) -> bool,
    owner_pinned: &dyn Fn(&TestSummary, &OracleFact) -> bool,
    helper_loan: &dyn Fn(&TestSummary, &OracleFact) -> Option<HelperLoan>,
) -> bool {
    if !matches!(probe.family, ProbeFamily::Predicate) {
        return false;
    }
    let Some(owner) = owner_fn else {
        return false;
    };
    related_tests.iter().any(|test| {
        test_pairs_boundary_input_with_oracle(
            probe,
            owner,
            test,
            activation,
            assertion_admitted,
            owner_pinned,
            helper_loan,
        )
    })
}

fn test_pairs_boundary_input_with_oracle(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    activation: &ActivationEvidence,
    assertion_admitted: &dyn Fn(&TestSummary, &OracleFact) -> bool,
    owner_pinned: &dyn Fn(&TestSummary, &OracleFact) -> bool,
    helper_loan: &dyn Fn(&TestSummary, &OracleFact) -> Option<HelperLoan>,
) -> bool {
    let bound_names = boundary_bound_locals(probe, owner, test, activation);
    test.assertions.iter().any(|assertion| {
        if !assertion_admitted(test, assertion)
            || !(assertion_is_discriminating(assertion) || owner_pinned(test, assertion))
        {
            return false;
        }
        // Only the asserted operands observe anything: a call or binding
        // named in a message argument (`assert!(gate(50), "{got}")`) is
        // formatted, not checked, so it cannot pair with the boundary.
        // An assertion credited from a check helper (outside the test's own
        // lines) names the helper's parameters, which may share a name with
        // a test local (`let input = 10; check(50, true)`): it pairs only
        // through the owner-pin-approved loan's exact mapping (#6482).
        if !(test.start_line..=test.end_line).contains(&assertion.line) {
            return helper_loan(test, assertion).is_some_and(|loan| {
                loan_pairs_boundary_call(owner, test, &loan, &assertion.text, activation)
            });
        }
        let operands = crate::analysis::extract::assertion_oracle_text(&assertion.text)
            .unwrap_or_else(|| assertion.text.clone());
        assertion_observes_boundary_owner_call(probe, owner, test, assertion, &operands, activation)
            || assertion_observes_bound_name(&operands, &bound_names)
    })
}

/// #6482: a borrowed check-helper assertion pairs with a boundary input the
/// test passes through that helper. In
///
/// ```text
/// fn check_pass(score: u32, want: bool) { assert_eq!(passes(score), want); }
/// #[test]
/// fn pass_mark_is_fifty() { check_pass(50, true); check_pass(49, false); }
/// ```
///
/// activation's `==` fact on the `check_pass(50, true)` line says that call
/// carries the boundary into the owner, and the assertion compares exactly
/// the owner call that receives it. Every condition fails closed:
///
/// - the helper calls the owner once, and that call is the asserted operand,
///   so the boundary call is the compared one;
/// - each of that call's arguments is one of the helper's own plain
///   parameters, passed unchanged (`passes(score + 1)` maps nothing);
/// - the activation fact sits on the line of an eager call to the helper
///   that is the only call expression on its line, so it describes that
///   call and that call runs;
/// - that call passes only scalar literals, with no comment or string
///   before it, and the fact's `parameter == value` matches the literal
///   that call itself feeds that owner parameter through the helper, so a
///   pooled fact from another file's same-line call through another helper
///   cannot stand in for it.
fn loan_pairs_boundary_call(
    owner: &FunctionSummary,
    test: &TestSummary,
    loan: &HelperLoan,
    assertion_text: &str,
    activation: &ActivationEvidence,
) -> bool {
    if loan.parameters.is_empty() || owner_call_count(&loan.helper, &owner.name) != 1 {
        return false;
    }
    // The owner call must be a whole compared operand: inside a block or
    // closure operand the parameter may be rebound before the call.
    let Some(compared) = assertion_comparison_operands(assertion_text) else {
        return false;
    };
    let Some(call) = compared
        .iter()
        .map(|operand| operand.trim())
        .find(|operand| operand.starts_with(&format!("{}(", owner.name)) && operand.ends_with(')'))
    else {
        return false;
    };
    let lists = owner_call_argument_lists(call, &owner.name);
    let [arguments] = lists.as_slice() else {
        return false;
    };
    if arguments.is_empty() {
        return false;
    }
    // `passes(score)` and not `passes(score) || other(..)`: the parentheses
    // opened after the name close at the operand's end.
    if delimited_contents_at(call, owner.name.len())
        .is_none_or(|contents| contents.len() + owner.name.len() + 2 != call.len())
    {
        return false;
    }
    if !arguments
        .iter()
        .all(|argument| loan.parameters.iter().any(|name| name == argument.trim()))
    {
        return false;
    }
    loan.call_lines.iter().any(|line| {
        // Every call fact on the line is the helper call: a call buried in a
        // macro (`assert!(passes(50) || true)`) or chained after it is
        // another source of the line's `==` fact.
        let mut on_line = test.calls.iter().filter(|call| call.line == *line);
        let Some(helper_call) = on_line.next() else {
            return false;
        };
        if helper_call.name != loan.name
            || on_line.any(|call| call.name != loan.name || call.text != helper_call.text)
        {
            return false;
        }
        // Activation's facts are pooled across related tests and carry no
        // file, so a fact on this line may describe another file's call
        // through another helper. The call must therefore fix its own
        // values (only scalar literals: `check_pass(n, true)` may bind `n`
        // differently per test), and the fact must agree with the literal
        // this call itself feeds the owner's parameter. A comment or string
        // before the call could feed the argument parser foreign text.
        let Some(literals) = loan_call_literals(&helper_call.text, &loan.name) else {
            return false;
        };
        let fed = owner_parameter_literals(owner, arguments, loan, &literals);
        if fed.is_empty() {
            return false;
        }
        activation.observed_values.iter().any(|fact| {
            fact.line == *line
                && matches!(fact.context, ValueContext::FunctionArgument)
                && !fact.text.is_empty()
                && fact.text.contains(&helper_call.text)
                && fact_matches_fed_literals(test, &fact.value, &fed)
        })
    })
}

/// Whether an activation value (`input == 10`, or a conjunction such as
/// `y == 0 && x == 0`) consists only of `parameter == literal` terms, each
/// naming an owner parameter and the very literal this call feeds it.
fn fact_matches_fed_literals(test: &TestSummary, value: &str, fed: &[(String, String)]) -> bool {
    !value.contains("||")
        && value.split(" && ").all(|term| {
            term.split_once(" == ").is_some_and(|(parameter, literal)| {
                fed.iter().any(|(name, fed)| {
                    name == parameter.trim() && same_scalar_literal(test, fed, literal.trim())
                })
            })
        })
}

/// Two whole scalar literals that spell one value (`10u32` and `10`), read
/// the way the direct path reads an owner argument. Anything that is not a
/// whole literal on both sides must match exactly.
fn same_scalar_literal(test: &TestSummary, fed: &str, literal: &str) -> bool {
    if fed == literal {
        return true;
    }
    if !argument_is_whole_scalar_literal(fed) || !argument_is_whole_scalar_literal(literal) {
        return false;
    }
    match (
        owner_argument_values(test, fed).as_slice(),
        owner_argument_values(test, literal).as_slice(),
    ) {
        ([fed], [literal]) => fed == literal,
        _ => false,
    }
}

/// The helper call's arguments when each is a whole scalar literal and no
/// comment or string precedes the call on its line; `None` otherwise.
fn loan_call_literals(text: &str, helper: &str) -> Option<Vec<String>> {
    let prefix = &text[..text.find(&format!("{helper}("))?];
    if text.contains("//") || text.contains("/*") || prefix.contains(['"', '\'']) {
        return None;
    }
    let arguments = call_arguments(text, helper)?;
    (!arguments.is_empty()
        && arguments
            .iter()
            .all(|argument| argument_is_whole_scalar_literal(argument.trim())))
    .then(|| {
        arguments
            .iter()
            .map(|argument| argument.trim().to_string())
            .collect()
    })
}

/// `(owner parameter, literal)` for each owner parameter whose argument is a
/// helper parameter the call fills with a literal. Empty when the owner's
/// parameter list and its call's arguments do not line up one to one.
fn owner_parameter_literals(
    owner: &FunctionSummary,
    owner_arguments: &[String],
    loan: &HelperLoan,
    literals: &[String],
) -> Vec<(String, String)> {
    let owner_parameters = function_parameters(owner);
    if owner_parameters.len() != owner_arguments.len() || loan.parameters.len() != literals.len() {
        return Vec::new();
    }
    owner_parameters
        .into_iter()
        .zip(owner_arguments)
        .filter_map(|(parameter, argument)| {
            let slot = loan
                .parameters
                .iter()
                .position(|name| name == argument.trim())?;
            Some((parameter, literals[slot].clone()))
        })
        .collect()
}

fn assertion_is_discriminating(assertion: &OracleFact) -> bool {
    matches!(assertion.strength, OracleStrength::Strong)
        || matches!(
            assertion.kind,
            OracleKind::ExactValue
                | OracleKind::WholeObjectEquality
                | OracleKind::ExactErrorVariant
        )
}

fn assertion_observes_boundary_owner_call(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    assertion: &OracleFact,
    operands: &str,
    activation: &ActivationEvidence,
) -> bool {
    let subject = assertion_subject(operands);
    let lists = owner_call_argument_lists(&subject, &owner.name);
    if lists
        .iter()
        .any(|arguments| argument_list_activates_boundary(probe, owner, test, arguments))
    {
        return true;
    }
    // Line-level activation cannot tell two same-name calls apart. Use it
    // only when the assertion text names the owner once, inside an operand,
    // no other owner call shares the line (`let got = gate(10);
    // assert!(gate(50));`), and each compared argument is a literal,
    // identifier, or path. A compound compared argument can mint a false
    // activation `==` fact from a buried scalar (#6668); named constants
    // (`LIMIT`, `parcels::BULK_ITEMS`) and helper hops keep identifier /
    // path / literal compared arguments. An extra unrelated compound
    // argument (`make_context()`) does not block that path when every
    // identifier operand maps to a parameter. An unresolved operand (`let
    // amount = raw; amount >= threshold`) fail-closes to the whole argument
    // list. The call fact keeps the original text so it matches extracted
    // calls.
    lists.len() == 1
        && owner_call_count(&assertion.text, &owner.name) == 1
        && line_owner_call_count(test, assertion.line, &owner.name) <= 1
        && owner_call_arguments_admit_activation_fallback(probe, owner, &lists[0])
        && activation_marks_boundary_call(
            activation,
            &CallFact {
                line: assertion.line,
                name: owner.name.clone(),
                text: assertion.text.clone(),
            },
        )
}

/// Owner calls the test's call facts record on one line, counting each
/// distinct fact text once so per-line and per-call extraction agree.
fn line_owner_call_count(test: &TestSummary, line: usize, name: &str) -> usize {
    let mut texts = test
        .calls
        .iter()
        .filter(|call| call.line == line && call.name == name)
        .map(|call| call.text.as_str())
        .collect::<Vec<_>>();
    texts.sort_unstable();
    texts.dedup();
    texts
        .into_iter()
        .map(|text| owner_call_count(text, name).max(1))
        .sum()
}

fn assertion_observes_bound_name(operands: &str, bound_names: &[String]) -> bool {
    // A binding named only in a comment or string is not observed.
    let masked = crate::analysis::extract::mask_comments_and_strings(operands);
    bound_names.iter().any(|name| contains_ident(&masked, name))
}

fn boundary_bound_locals(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    activation: &ActivationEvidence,
) -> Vec<String> {
    // Last binding of a name wins so `let got = gate(10); let got = gate(100)`
    // does not keep the shadowed boundary result.
    let mut last: Vec<(String, bool)> = Vec::new();
    for (offset, line) in test.body.lines().enumerate() {
        let Some(name) = let_binding_name(line) else {
            continue;
        };
        let line_number = test.start_line + offset;
        let call = CallFact {
            line: line_number,
            name: owner.name.clone(),
            text: line.trim().to_string(),
        };
        let is_boundary = owner_call_activates_boundary(probe, owner, test, &call, activation);
        if let Some(existing) = last.iter_mut().find(|(bound, _)| bound == &name) {
            existing.1 = is_boundary;
        } else {
            last.push((name, is_boundary));
        }
    }
    last.into_iter()
        .filter_map(|(name, is_boundary)| is_boundary.then_some(name))
        .collect()
}

fn let_binding_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("let")?;
    if rest
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return None;
    }
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("mut ").unwrap_or(rest).trim_start();
    let name: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() || name == "_" || name.starts_with(|ch: char| ch.is_ascii_digit()) {
        return None;
    }
    let after = rest.get(name.len()..)?.trim_start();
    let (before_eq, _) = after.split_once('=')?;
    let before_eq = before_eq.trim();
    (before_eq.is_empty() || before_eq.starts_with(':')).then_some(name)
}

fn owner_call_activates_boundary(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    call: &CallFact,
    activation: &ActivationEvidence,
) -> bool {
    if call.name != owner.name {
        return false;
    }
    if let Some(arguments) = call_arguments(&call.text, &call.name) {
        if argument_list_activates_boundary(probe, owner, test, &arguments) {
            return true;
        }
        if !owner_call_arguments_admit_activation_fallback(probe, owner, &arguments) {
            return false;
        }
    }
    owner_call_count(&call.text, &owner.name) == 1
        && activation_marks_boundary_call(activation, call)
}

fn argument_list_activates_boundary(
    probe: &Probe,
    owner: &FunctionSummary,
    test: &TestSummary,
    arguments: &[String],
) -> bool {
    let arg_values: Vec<Vec<String>> = arguments
        .iter()
        .map(|argument| pairing_argument_values(test, argument))
        .collect();
    let Some((left, right)) = comparison_operands(&probe.expression) else {
        return false;
    };
    let parameters = function_parameters(owner);
    let left_index = parameter_index(&parameters, &left);
    let right_index = parameter_index(&parameters, &right);
    if let (Some(left_index), Some(right_index)) = (left_index, right_index) {
        return values_overlap(
            arg_values.get(left_index).map(Vec::as_slice).unwrap_or(&[]),
            arg_values
                .get(right_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
    }
    if let Some(left_index) = left_index {
        let right_literals = extract_literals(&right);
        return values_overlap(
            arg_values.get(left_index).map(Vec::as_slice).unwrap_or(&[]),
            &right_literals,
        );
    }
    if let Some(right_index) = right_index {
        let left_literals = extract_literals(&left);
        return values_overlap(
            arg_values
                .get(right_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            &left_literals,
        );
    }
    false
}

fn assertion_subject(text: &str) -> String {
    for marker in ["assert_eq!(", "assert_ne!(", "assert_matches!(", "assert!("] {
        if let Some(index) = find_marker(text, marker)
            && let Some(open) = index.checked_add(marker.len().saturating_sub(1))
            && let Some(inner) = delimited_contents_at(text, open)
        {
            return inner;
        }
    }
    text.to_string()
}

/// Owner calls in code only: a spelling inside a comment or string literal
/// (`/* gate(10) */`, `"gate(10)"`) is not a call. Positions come from the
/// length-preserving mask; arguments are read from the original text so
/// string arguments (`classify("word")`) keep their values.
fn owner_call_argument_lists(text: &str, name: &str) -> Vec<Vec<String>> {
    let needle = format!("{name}(");
    let masked = crate::analysis::extract::mask_comments_and_strings(text);
    let mut lists = Vec::new();
    let mut from = 0usize;
    while from < masked.len() {
        let Some(rel) = masked.get(from..).and_then(|rest| rest.find(&needle)) else {
            break;
        };
        let abs = from + rel;
        if abs > 0 {
            let before = masked.as_bytes()[abs - 1];
            if before.is_ascii_alphanumeric() || before == b'_' {
                from = abs + 1;
                continue;
            }
        }
        if let Some(arguments) = call_arguments(text.get(abs..).unwrap_or(""), name) {
            lists.push(arguments);
        }
        from = abs + needle.len();
    }
    lists
}

fn owner_call_count(text: &str, name: &str) -> usize {
    owner_call_argument_lists(text, name).len()
}

fn contains_ident(text: &str, name: &str) -> bool {
    find_ident_at(text, name).is_some()
}

fn find_ident_at(text: &str, name: &str) -> Option<usize> {
    if name.is_empty() {
        return None;
    }
    let mut from = 0usize;
    while from < text.len() {
        let rel = text.get(from..).and_then(|rest| rest.find(name))?;
        let abs = from + rel;
        let before_ok = abs == 0 || {
            let before = text.as_bytes()[abs - 1];
            !(before.is_ascii_alphanumeric() || before == b'_')
        };
        let end = abs + name.len();
        let after_ok = end >= text.len() || {
            let after = text.as_bytes()[end];
            !(after.is_ascii_alphanumeric() || after == b'_')
        };
        if before_ok && after_ok {
            return Some(abs);
        }
        from = abs + 1;
    }
    None
}

fn find_marker(text: &str, marker: &str) -> Option<usize> {
    let mut from = 0usize;
    while from < text.len() {
        let rel = text.get(from..).and_then(|rest| rest.find(marker))?;
        let abs = from + rel;
        let before_ok = abs == 0 || {
            let before = text.as_bytes()[abs - 1];
            !(before.is_ascii_alphanumeric() || before == b'_')
        };
        if before_ok {
            return Some(abs);
        }
        from = abs + 1;
    }
    None
}

/// Values pairing may treat as this argument's input. Unlike
/// [`owner_argument_values`], which collects every scalar token inside the
/// expression, this admits only the argument as a whole: a scalar literal
/// (including a type suffix), or a plain identifier resolved to a local /
/// rstest binding. Compound expressions that merely contain a boundary
/// token are empty; infection `==` facts remain the call-level path for
/// named constants whose arguments are themselves identifiers.
fn pairing_argument_values(test: &TestSummary, argument: &str) -> Vec<String> {
    let trimmed = argument.trim();
    if argument_is_direct_pairing_shape(trimmed) {
        return owner_argument_values(test, trimmed);
    }
    Vec::new()
}

fn argument_is_direct_pairing_shape(argument: &str) -> bool {
    !argument.is_empty()
        && (argument_is_plain_identifier(argument) || argument_is_whole_scalar_literal(argument))
}

/// Identifier or path (`LIMIT`, `parcels::BULK_ITEMS`) that may carry a
/// named-constant infection `==` fact. Calls such as `std::cmp::max(10, 50)`
/// are not paths.
fn argument_is_activation_fallback_shape(argument: &str) -> bool {
    argument_is_direct_pairing_shape(argument) || argument_is_path_identifier(argument)
}

fn argument_is_path_identifier(text: &str) -> bool {
    text.contains("::") && text.split("::").all(argument_is_plain_identifier)
}

fn owner_call_arguments_admit_activation_fallback(
    probe: &Probe,
    owner: &FunctionSummary,
    arguments: &[String],
) -> bool {
    if arguments.is_empty() {
        return false;
    }
    let Some((left, right)) = comparison_operands(&probe.expression) else {
        return false;
    };
    let parameters = function_parameters(owner);
    let compared: Vec<usize> = [left.as_str(), right.as_str()]
        .into_iter()
        .filter_map(|operand| parameter_index(&parameters, operand))
        .collect();
    // Compared parameters mint the false first-scalar `==` fact. Extra
    // arguments are not that producer, so `gate(LIMIT, make_context())`
    // still admits a named-constant equality. When an identifier operand
    // is not a parameter (`let amount = raw; amount >= threshold`), this
    // layer cannot see the alias without forking activation.rs, so the
    // whole list stays fail-closed. Helper hops also take that path.
    let unresolved_operand = [left.as_str(), right.as_str()].into_iter().any(|operand| {
        comparison_operand_needs_argument_slot(operand)
            && parameter_index(&parameters, operand).is_none()
    });
    let indices: Vec<usize> = if compared.is_empty() || unresolved_operand {
        (0..arguments.len()).collect()
    } else {
        compared
    };
    indices.iter().all(|&idx| {
        arguments
            .get(idx)
            .is_some_and(|argument| argument_is_activation_fallback_shape(argument.trim()))
    })
}

fn comparison_operand_needs_argument_slot(operand: &str) -> bool {
    let trimmed = operand.trim();
    !trimmed.is_empty() && !argument_is_whole_scalar_literal(trimmed)
}

fn argument_is_plain_identifier(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn argument_is_whole_scalar_literal(argument: &str) -> bool {
    if argument == "true" || argument == "false" {
        return true;
    }
    if argument_is_whole_quoted_literal(argument) {
        return true;
    }
    argument_is_whole_numeric_literal(argument)
}

/// True when `argument` is one numeric token (`10`, `-10`, `10u32`, `1.5e-3`),
/// not an expression that contains a number (`10-offset`, `10+1`).
fn argument_is_whole_numeric_literal(argument: &str) -> bool {
    if argument.is_empty() || argument.contains(char::is_whitespace) {
        return false;
    }
    let bytes = argument.as_bytes();
    let mut idx = usize::from(bytes.first() == Some(&b'-'));
    if idx >= bytes.len() || !bytes[idx].is_ascii_digit() {
        return false;
    }
    if bytes[idx] == b'0'
        && let Some(radix) = bytes.get(idx + 1).and_then(|marker| match marker {
            b'x' | b'X' => Some(16u32),
            b'o' | b'O' => Some(8),
            b'b' | b'B' => Some(2),
            _ => None,
        })
    {
        idx += 2;
        let digits_start = idx;
        while idx < bytes.len() && (bytes[idx] == b'_' || (bytes[idx] as char).is_digit(radix)) {
            idx += 1;
        }
        if idx == digits_start {
            return false;
        }
    } else {
        while idx < bytes.len() && (bytes[idx].is_ascii_digit() || bytes[idx] == b'_') {
            idx += 1;
        }
        if idx < bytes.len()
            && bytes[idx] == b'.'
            && bytes.get(idx + 1).is_some_and(|next| next.is_ascii_digit())
        {
            idx += 1;
            while idx < bytes.len() && (bytes[idx].is_ascii_digit() || bytes[idx] == b'_') {
                idx += 1;
            }
        }
        if idx < bytes.len() && (bytes[idx] == b'e' || bytes[idx] == b'E') {
            let mut exponent = idx + 1;
            if bytes
                .get(exponent)
                .is_some_and(|sign| *sign == b'+' || *sign == b'-')
            {
                exponent += 1;
            }
            let digits_start = exponent;
            while exponent < bytes.len()
                && (bytes[exponent].is_ascii_digit() || bytes[exponent] == b'_')
            {
                exponent += 1;
            }
            if exponent > digits_start {
                idx = exponent;
            }
        }
    }
    if idx < bytes.len() {
        if !bytes[idx].is_ascii_alphabetic() {
            return false;
        }
        while idx < bytes.len() {
            if !bytes[idx].is_ascii_alphanumeric() && bytes[idx] != b'_' {
                return false;
            }
            idx += 1;
        }
    }
    idx == bytes.len()
}

fn argument_is_whole_quoted_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let Some((&quote, rest)) = bytes.split_first() else {
        return false;
    };
    if quote != b'"' && quote != b'\'' {
        return false;
    }
    let mut escaped = false;
    for (offset, &byte) in rest.iter().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' {
            escaped = true;
            continue;
        }
        if byte == quote {
            return offset + 1 == rest.len();
        }
    }
    false
}

fn parameter_index(parameters: &[String], operand: &str) -> Option<usize> {
    parameters.iter().position(|parameter| parameter == operand)
}

fn values_overlap(left: &[String], right: &[String]) -> bool {
    !left.is_empty() && !right.is_empty() && left.iter().any(|value| right.contains(value))
}

/// Infection already recorded that this owner-call line sits on the boundary
/// (`left == right` from named constants, helper hops, or local bindings).
fn activation_marks_boundary_call(activation: &ActivationEvidence, call: &CallFact) -> bool {
    activation.observed_values.iter().any(|fact| {
        fact.line == call.line
            && fact.value.contains(" == ")
            && (fact.text.is_empty() || fact.text.contains(&call.text))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::analysis::rust_index::{
        CallFact, LiteralFact, OracleFact, extract_identifier_tokens,
    };
    use std::path::PathBuf;

    // These units isolate semantic pairing of already-admitted oracle facts.
    // Public API/runtime controls exercise the real parser-backed admission.
    fn pairing_with_admitted_oracles(
        probe: &Probe,
        owner: Option<&FunctionSummary>,
        tests: &[&TestSummary],
        activation: &ActivationEvidence,
    ) -> bool {
        has_same_test_boundary_oracle_pairing(
            probe,
            owner,
            tests,
            activation,
            &|_, _| true,
            &|_, _| false,
            &|_, _| None,
        )
    }

    #[test]
    fn split_tests_do_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let boundary = test_summary(
            "boundary",
            "let _ = gate(10); let _ = gate(9);",
            vec![
                call("gate", "let _ = gate(10);"),
                call("gate", "let _ = gate(9);"),
            ],
            vec![],
            &["10", "9"],
        );
        let far = test_summary(
            "far",
            "assert_eq!(gate(100), true);",
            vec![call("gate", "assert_eq!(gate(100), true);")],
            vec![exact("assert_eq!(gate(100), true);")],
            &["100"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&boundary, &far],
                &ActivationEvidence::default(),
            ),
            "a no-oracle boundary call plus a far exact oracle must not pair"
        );
    }

    #[test]
    fn same_call_assert_eq_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let paired = test_summary(
            "paired_boundary",
            "assert_eq!(gate(10), true);",
            vec![call("gate", "assert_eq!(gate(10), true);")],
            vec![exact("assert_eq!(gate(10), true);")],
            &["10"],
        );
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "assert_eq!(gate(10), true) must pair"
        );
    }

    #[test]
    fn buried_if_expression_argument_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let buried = test_summary(
            "buried_if",
            "assert_eq!(gate(if false { 10 } else { 50 }), true);",
            vec![call(
                "gate",
                "assert_eq!(gate(if false { 10 } else { 50 }), true);",
            )],
            vec![exact(
                "assert_eq!(gate(if false { 10 } else { 50 }), true);",
            )],
            &["10", "50"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&buried],
                &ActivationEvidence::default(),
            ),
            "gate(if false {{ 10 }} else {{ 50 }}) evaluates to 50, so mentioning 10 must not pair"
        );
    }

    #[test]
    fn buried_max_call_argument_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let buried = test_summary(
            "buried_max",
            "assert_eq!(gate(std::cmp::max(10, 50)), true);",
            vec![call(
                "gate",
                "assert_eq!(gate(std::cmp::max(10, 50)), true);",
            )],
            vec![exact("assert_eq!(gate(std::cmp::max(10, 50)), true);")],
            &["10", "50"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&buried],
                &ActivationEvidence::default(),
            ),
            "gate(std::cmp::max(10, 50)) evaluates to 50, so mentioning 10 must not pair"
        );
    }

    #[test]
    fn buried_if_expression_in_assert_bang_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let buried = test_summary(
            "buried_assert_bang",
            "assert!(gate(if false { 10 } else { 50 }));",
            vec![call("gate", "assert!(gate(if false { 10 } else { 50 }));")],
            vec![exact("assert!(gate(if false { 10 } else { 50 }));")],
            &["10", "50"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&buried],
                &ActivationEvidence::default(),
            ),
            "a bool-owner assert!(gate(if false {{ 10 }} else {{ 50 }})) pin must not pair from a buried literal"
        );
    }

    #[test]
    fn buried_if_expression_does_not_pair_via_activation_equality() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let buried = test_summary(
            "buried_if_activation",
            "assert_eq!(gate(if false { 10 } else { 50 }), true);",
            vec![call(
                "gate",
                "assert_eq!(gate(if false { 10 } else { 50 }), true);",
            )],
            vec![exact(
                "assert_eq!(gate(if false { 10 } else { 50 }), true);",
            )],
            &["10", "50"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(gate(if false { 10 } else { 50 }), true); | first scalar"
                    .to_string(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            !pairing_with_admitted_oracles(&probe, Some(&owner), &[&buried], &activation),
            "a false activation == fact from a buried scalar must not restore pairing"
        );
    }

    #[test]
    fn subtraction_from_boundary_literal_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let subtracted = test_summary(
            "subtracted",
            "let offset = 1;\nassert_eq!(gate(10-offset), false);",
            vec![call("gate", "assert_eq!(gate(10-offset), false);")],
            vec![exact("assert_eq!(gate(10-offset), false);")],
            &["10", "1"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(gate(10-offset), false); | first scalar".to_string(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&subtracted],
                &ActivationEvidence::default(),
            ),
            "gate(10-offset) evaluates to 9 when offset is 1, so mentioning 10 must not pair"
        );
        assert!(
            !pairing_with_admitted_oracles(&probe, Some(&owner), &[&subtracted], &activation),
            "a false activation == fact from 10-offset must not restore pairing"
        );
    }

    #[test]
    fn local_bound_to_boundary_literal_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let paired = test_summary(
            "local_boundary",
            "let threshold = 10;\nassert_eq!(gate(threshold), true);",
            vec![call("gate", "assert_eq!(gate(threshold), true);")],
            vec![exact("assert_eq!(gate(threshold), true);")],
            &["10"],
        );
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "let threshold = 10; assert_eq!(gate(threshold), true) must pair"
        );
    }

    #[test]
    fn named_constant_activation_equality_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let paired = test_summary(
            "const_boundary",
            "assert_eq!(gate(LIMIT), true);",
            vec![call("gate", "assert_eq!(gate(LIMIT), true);")],
            vec![exact("assert_eq!(gate(LIMIT), true);")],
            &["10"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(gate(LIMIT), true); | named constant".to_string(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&paired], &activation),
            "gate(LIMIT) must pair when infection already recorded input == 10"
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "gate(LIMIT) must not pair from the identifier spelling alone"
        );
    }

    #[test]
    fn path_qualified_named_constant_activation_equality_pairs() {
        let probe = predicate_probe("items >= BULK_ITEMS");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::bulk_rate".to_string()),
            name: "bulk_rate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 6,
            body:
                "pub fn bulk_rate(items: u32) -> u32 { if items >= BULK_ITEMS { 90 } else { 100 } }"
                    .into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let paired = test_summary(
            "bulk_rate_boundary",
            "assert_eq!(bulk_rate(parcels::BULK_ITEMS), 90);",
            vec![call(
                "bulk_rate",
                "assert_eq!(bulk_rate(parcels::BULK_ITEMS), 90);",
            )],
            vec![exact("assert_eq!(bulk_rate(parcels::BULK_ITEMS), 90);")],
            &["90"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(bulk_rate(parcels::BULK_ITEMS), 90); | argument names constant BULK_ITEMS"
                    .to_string(),
                value: "items == BULK_ITEMS".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&paired], &activation),
            "bulk_rate(parcels::BULK_ITEMS) must pair when infection recorded items == BULK_ITEMS"
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "parcels::BULK_ITEMS must not pair from the path spelling alone"
        );
    }

    #[test]
    fn named_constant_pairs_despite_compound_unrelated_argument() {
        let probe = predicate_probe("input >= 10");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: "pub fn gate(input: u32, context: u32) -> bool { input >= 10 }".into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let paired = test_summary(
            "const_with_context",
            "assert_eq!(gate(LIMIT, make_context()), true);",
            vec![call(
                "gate",
                "assert_eq!(gate(LIMIT, make_context()), true);",
            )],
            vec![exact("assert_eq!(gate(LIMIT, make_context()), true);")],
            &["10"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(gate(LIMIT, make_context()), true); | named constant".to_string(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&paired], &activation),
            "gate(LIMIT, make_context()) must pair when infection recorded input == 10 on the compared parameter"
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "without the activation == fact, LIMIT plus a compound extra argument must not pair"
        );
    }

    #[test]
    fn compound_compared_argument_does_not_pair_via_unrelated_identifier() {
        let probe = predicate_probe("input >= 10");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: "pub fn gate(input: u32, marker: u32) -> bool { input >= 10 }".into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let buried = test_summary(
            "buried_with_marker",
            "assert_eq!(gate(if false { 10 } else { 50 }, LIMIT), true);",
            vec![call(
                "gate",
                "assert_eq!(gate(if false { 10 } else { 50 }, LIMIT), true);",
            )],
            vec![exact(
                "assert_eq!(gate(if false { 10 } else { 50 }, LIMIT), true);",
            )],
            &["10", "50"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(gate(if false { 10 } else { 50 }, LIMIT), true); | first scalar"
                    .to_string(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            !pairing_with_admitted_oracles(&probe, Some(&owner), &[&buried], &activation),
            "a compound compared argument must not pair just because an extra argument is an identifier"
        );
    }

    #[test]
    fn aliased_operand_buried_literal_does_not_pair_via_activation() {
        let probe = predicate_probe("amount >= threshold");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 5,
            body: "pub fn gate(raw: u32, threshold: u32) -> bool {\n    let amount = raw;\n    amount >= threshold\n}"
                .into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let buried = test_summary(
            "aliased_buried",
            "assert_eq!(gate(if false { 10 } else { 50 }, 10), true);",
            vec![call(
                "gate",
                "assert_eq!(gate(if false { 10 } else { 50 }, 10), true);",
            )],
            vec![exact(
                "assert_eq!(gate(if false { 10 } else { 50 }, 10), true);",
            )],
            &["10", "50"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(gate(if false { 10 } else { 50 }, 10), true); | first scalar"
                    .to_string(),
                value: "amount == threshold".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            !pairing_with_admitted_oracles(&probe, Some(&owner), &[&buried], &activation),
            "a buried literal on an aliased input must not pair just because the named parameter is a literal"
        );
    }

    #[test]
    fn aliased_operand_named_constant_activation_still_pairs() {
        let probe = predicate_probe("amount >= threshold");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 5,
            body: "pub fn gate(raw: u32, threshold: u32) -> bool {\n    let amount = raw;\n    amount >= threshold\n}"
                .into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let paired = test_summary(
            "aliased_const",
            "assert_eq!(gate(LIMIT, 10), true);",
            vec![call("gate", "assert_eq!(gate(LIMIT, 10), true);")],
            vec![exact("assert_eq!(gate(LIMIT, 10), true);")],
            &["10"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(gate(LIMIT, 10), true); | named constant".to_string(),
                value: "amount == threshold".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&paired], &activation),
            "gate(LIMIT, 10) must still pair on an aliased operand when every argument is a name or literal"
        );
    }

    #[test]
    fn typed_literal_argument_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let paired = test_summary(
            "typed_literal",
            "assert_eq!(gate(10u32), true);",
            vec![call("gate", "assert_eq!(gate(10u32), true);")],
            vec![exact("assert_eq!(gate(10u32), true);")],
            &["10"],
        );
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "assert_eq!(gate(10u32), true) is the boundary literal itself and must pair"
        );
    }

    #[test]
    fn same_test_split_calls_do_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mixed = test_summary(
            "mixed",
            "let _ = gate(10);\nassert_eq!(gate(100), true);",
            vec![
                call("gate", "let _ = gate(10);"),
                call("gate", "assert_eq!(gate(100), true);"),
            ],
            vec![exact("assert_eq!(gate(100), true);")],
            &["10", "100"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&mixed],
                &ActivationEvidence::default(),
            ),
            "boundary call and far oracle in the same test still need the oracle on the boundary call"
        );
    }

    #[test]
    fn same_line_split_calls_do_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mixed = test_summary(
            "mixed",
            "let _ = gate(10); assert_eq!(gate(100), true);",
            vec![
                call("gate", "let _ = gate(10); assert_eq!(gate(100), true);"),
                call("gate", "let _ = gate(10); assert_eq!(gate(100), true);"),
            ],
            vec![exact("let _ = gate(10); assert_eq!(gate(100), true);")],
            &["10", "100"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&mixed],
                &ActivationEvidence::default(),
            ),
            "a same-line unasserted boundary call plus a far exact oracle must not pair"
        );
    }

    #[test]
    fn irrelevant_argument_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: "pub fn gate(input: u32, marker: u32) -> bool { input >= 10 }".into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let far = test_summary(
            "far",
            "assert_eq!(gate(100, 10), true);",
            vec![call("gate", "assert_eq!(gate(100, 10), true);")],
            vec![exact("assert_eq!(gate(100, 10), true);")],
            &["100", "10"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&far],
                &ActivationEvidence::default(),
            ),
            "a boundary literal in an unused argument must not pair"
        );
    }

    #[test]
    fn shadowed_boundary_binding_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut shadowed = test_summary(
            "shadowed",
            "let got = gate(10);\nlet got = gate(100);\nassert_eq!(got, true);",
            vec![
                call("gate", "let got = gate(10);"),
                call("gate", "let got = gate(100);"),
            ],
            vec![exact("assert_eq!(got, true);")],
            &["10", "100"],
        );
        shadowed.calls[0].line = 1;
        shadowed.calls[1].line = 2;
        shadowed.assertions[0].line = 3;
        shadowed.end_line = 4;
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&shadowed],
                &ActivationEvidence::default(),
            ),
            "asserting a binding that later shadows the boundary result must not pair"
        );
    }

    #[test]
    fn short_let_bound_name_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut bound = test_summary(
            "bound",
            "let x = gate(10);\nassert_eq!(x, true);",
            vec![call("gate", "let x = gate(10);")],
            vec![exact("assert_eq!(x, true);")],
            &["10"],
        );
        bound.calls[0].line = 1;
        bound.assertions[0].line = 2;
        bound.end_line = 3;
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&bound],
                &ActivationEvidence::default(),
            ),
            "let x = gate(10); assert_eq!(x, true) must pair"
        );
    }

    #[test]
    fn let_bound_boundary_call_asserted_later_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut bound = test_summary(
            "bound",
            "let got = gate(10);\nassert_eq!(got, true);",
            vec![call("gate", "let got = gate(10);")],
            vec![exact("assert_eq!(got, true);")],
            &["10"],
        );
        bound.calls[0].line = 1;
        bound.assertions[0].line = 2;
        bound.end_line = 3;
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&bound],
                &ActivationEvidence::default(),
            ),
            "let got = gate(10); assert_eq!(got, true) must pair"
        );
    }

    #[test]
    fn typed_let_bound_boundary_call_asserted_later_pairs() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut bound = test_summary(
            "bound",
            "let got: bool = gate(10);\nassert_eq!(got, true);",
            vec![call("gate", "let got: bool = gate(10);")],
            vec![exact("assert_eq!(got, true);")],
            &["10"],
        );
        bound.calls[0].line = 1;
        bound.assertions[0].line = 2;
        bound.end_line = 3;
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&bound],
                &ActivationEvidence::default(),
            ),
            "let got: bool = gate(10); assert_eq!(got, true) must pair"
        );
    }

    #[test]
    fn let_binding_name_requires_keyword_boundary_and_allows_type_ascription() {
        assert_eq!(
            let_binding_name("let got = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(
            let_binding_name("let mut got = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(
            let_binding_name("let got: bool = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(
            let_binding_name("let mut got: bool = gate(10);").as_deref(),
            Some("got")
        );
        assert_eq!(let_binding_name("letter = gate(10);"), None);
        assert_eq!(let_binding_name("let_got = gate(10);"), None);
        assert_eq!(let_binding_name("let _ = gate(10);"), None);
    }

    #[test]
    fn pairing_argument_values_keep_direct_inputs_and_drop_buried_literals() {
        let local = test_summary(
            "local",
            "let threshold = 10;\nassert_eq!(gate(threshold), true);",
            vec![],
            vec![],
            &["10"],
        );
        assert_eq!(
            pairing_argument_values(&local, "10"),
            vec!["10".to_string()]
        );
        assert_eq!(
            pairing_argument_values(&local, "10u32"),
            vec!["10".to_string()]
        );
        assert_eq!(
            pairing_argument_values(&local, "-10"),
            vec!["-10".to_string()]
        );
        assert!(
            pairing_argument_values(&local, "10-offset").is_empty(),
            "subtraction of a local is not the boundary literal"
        );
        assert!(argument_is_whole_numeric_literal("1.5e-3"));
        assert!(argument_is_whole_numeric_literal("-10"));
        assert!(argument_is_whole_numeric_literal("10u32"));
        assert!(!argument_is_whole_numeric_literal("10-offset"));
        assert!(!argument_is_whole_numeric_literal("10+1"));
        assert_eq!(
            pairing_argument_values(&local, "threshold"),
            vec!["10".to_string()]
        );
        assert!(
            pairing_argument_values(&local, "if false { 10 } else { 50 }").is_empty(),
            "an if-expression that mentions 10 is not the boundary input"
        );
        assert!(
            pairing_argument_values(&local, "std::cmp::max(10, 50)").is_empty(),
            "a call that mentions 10 is not the boundary input"
        );
        assert!(
            pairing_argument_values(&local, "LIMIT").is_empty(),
            "an unresolved name is not a pairing argument; infection == facts cover named constants"
        );
        assert!(
            pairing_argument_values(&local, "parcels::BULK_ITEMS").is_empty(),
            "a path-qualified constant is not a pairing argument value; infection == facts cover it"
        );
        assert!(argument_is_path_identifier("parcels::BULK_ITEMS"));
        assert!(argument_is_activation_fallback_shape("parcels::BULK_ITEMS"));
        assert!(!argument_is_path_identifier("std::cmp::max(10, 50)"));
        assert!(!argument_is_activation_fallback_shape(
            "std::cmp::max(10, 50)"
        ));
    }

    #[test]
    fn two_parameter_equal_args_in_assert_eq_pair() {
        let probe = predicate_probe("amount >= discount_threshold");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::discounted_total".to_string()),
            name: "discounted_total".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 6,
            body: "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 { amount }"
                .into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let paired = test_summary(
            "equality_threshold_discounts",
            "assert_eq!(discounted_total(100, 100), 90);",
            vec![call(
                "discounted_total",
                "assert_eq!(discounted_total(100, 100), 90);",
            )],
            vec![exact("assert_eq!(discounted_total(100, 100), 90);")],
            &["100", "90"],
        );
        assert!(
            pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "equal parameter arguments in the asserted owner call must pair"
        );
    }

    #[test]
    fn weak_assertion_on_boundary_call_does_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let weak = test_summary(
            "smoke",
            "assert!(gate(10));",
            vec![call("gate", "assert!(gate(10));")],
            vec![OracleFact {
                line: 1,
                text: "assert!(gate(10));".to_string(),
                kind: OracleKind::RelationalCheck,
                strength: OracleStrength::Weak,
                observed_tokens: extract_identifier_tokens("assert!(gate(10));"),
                ok_value_observed: None,
            }],
            &["10"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&weak],
                &ActivationEvidence::default(),
            ),
            "a weak oracle on the boundary call is not a discriminating pairing"
        );
    }

    #[test]
    fn activation_equality_fact_pairs_when_call_args_are_not_probe_literals() {
        let probe = predicate_probe("final_label == \"alpha\"");
        let owner = FunctionSummary {
            id: SymbolId("src/lib.rs::classify".to_string()),
            name: "classify".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 9,
            end_line: 16,
            body: "pub fn classify(input: &str) -> &'static str { input }".into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        };
        let paired = test_summary(
            "word_label_is_word",
            "assert_eq!(classify(\"word\"), \"word\");",
            vec![call(
                "classify",
                "assert_eq!(classify(\"word\"), \"word\");",
            )],
            vec![exact("assert_eq!(classify(\"word\"), \"word\");")],
            &["word"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "assert_eq!(classify(\"word\"), \"word\"); | helper hop".to_string(),
                value: "final_label == \"alpha\"".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&paired], &activation),
            "an activation == fact on the asserted owner call must pair even when the call args are not the probe literals"
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&paired],
                &ActivationEvidence::default(),
            ),
            "without the activation == fact, classify(\"word\") must not pair against final_label == \"alpha\""
        );
    }

    #[test]
    fn comments_and_strings_in_operands_do_not_pair() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let mut commented = test_summary(
            "commented",
            "let got = gate(10);\nassert_eq!(gate(50), true /* got */);",
            vec![
                call("gate", "let got = gate(10);"),
                call("gate", "assert_eq!(gate(50), true /* got */);"),
            ],
            vec![exact("assert_eq!(gate(50), true /* got */);")],
            &["10", "50"],
        );
        commented.calls[1].line = 2;
        commented.assertions[0].line = 2;
        commented.end_line = 3;
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&commented],
                &ActivationEvidence::default(),
            ),
            "a boundary binding named only in an operand comment must not pair"
        );
        let quoted = test_summary(
            "quoted",
            "assert_eq!(gate(50), true, \"{}\", \"gate(10)\"); assert_eq!(gate(50) /* gate(10) */, true);",
            vec![call("gate", "assert_eq!(gate(50) /* gate(10) */, true);")],
            vec![exact("assert_eq!(gate(50) /* gate(10) */, true);")],
            &["10", "50"],
        );
        assert!(
            !pairing_with_admitted_oracles(
                &probe,
                Some(&owner),
                &[&quoted],
                &ActivationEvidence::default(),
            ),
            "an owner call spelled inside a comment is not a boundary call"
        );
    }

    #[test]
    fn quoted_owner_text_on_the_line_keeps_the_activation_fallback() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let line = "let note = \"gate(50)\"; assert!(gate(n));";
        let quoted = test_summary(
            "quoted",
            line,
            vec![call("gate", line)],
            vec![exact("assert!(gate(n));")],
            &[],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: String::new(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&quoted], &activation),
            "a quoted owner spelling is not a second call on the line"
        );
    }

    #[test]
    fn line_activation_does_not_pair_through_another_owner_call_on_the_line() {
        let probe = predicate_probe("input >= 10");
        let owner = gate_owner();
        let line = "let got = gate(10); assert!(gate(50));";
        let mixed = test_summary(
            "mixed",
            line,
            vec![call("gate", line), call("gate", line)],
            vec![exact("assert!(gate(50));")],
            &["10", "50"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: String::new(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        assert!(
            !pairing_with_admitted_oracles(&probe, Some(&owner), &[&mixed], &activation),
            "a boundary activation from gate(10) must not pair through the far assert!(gate(50)) on its line"
        );
        let alone = test_summary(
            "alone",
            "assert!(gate(50));",
            vec![call("gate", "assert!(gate(50));")],
            vec![exact("assert!(gate(50));")],
            &["50"],
        );
        assert!(
            pairing_with_admitted_oracles(&probe, Some(&owner), &[&alone], &activation),
            "a line holding only the asserted owner call keeps the activation fallback"
        );
    }

    fn predicate_probe(expression: &str) -> Probe {
        Probe {
            id: ProbeId("probe:test".to_string()),
            location: SourceLocation::new("src/lib.rs", 2, 1),
            owner: Some(SymbolId("src/lib.rs::gate".to_string())),
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: None,
            after: None,
            expression: expression.to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        }
    }

    /// #6482: `fn check(input: u32, want: bool) { assert_eq!(gate(input),
    /// want); }` called as `check(10, true)` on line 2 of the test.
    fn loan_case(
        helper: &str,
        call_text: &str,
        activation_line: usize,
    ) -> (TestSummary, OracleFact, HelperLoan, ActivationEvidence) {
        let assertion = OracleFact {
            line: 9,
            ..exact("assert_eq!(gate(input), want);")
        };
        let mut test = test_summary(
            "through_helper",
            call_text,
            vec![CallFact {
                line: 2,
                ..call("check", call_text)
            }],
            vec![assertion.clone()],
            &["10"],
        );
        test.calls.push(CallFact {
            line: 9,
            ..call("gate", "assert_eq!(gate(input), want);")
        });
        let loan = HelperLoan {
            name: "check".to_string(),
            helper: helper.to_string(),
            parameters: vec!["input".to_string(), "want".to_string()],
            call_lines: vec![2],
        };
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: activation_line,
                text: format!("{call_text} | input"),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        (test, assertion, loan, activation)
    }

    const CHECK: &str = "fn check(input: u32, want: bool) { assert_eq!(gate(input), want); }";

    fn loan_pairs(
        test: &TestSummary,
        assertion: &OracleFact,
        loan: &HelperLoan,
        activation: &ActivationEvidence,
    ) -> bool {
        has_same_test_boundary_oracle_pairing(
            &predicate_probe("input >= 10"),
            Some(&gate_owner()),
            &[test],
            activation,
            &|_, _| true,
            &|_, _| false,
            &|_, candidate| (candidate == assertion).then(|| loan.clone()),
        )
    }

    /// A borrowed assertion names the helper's parameter `got`; the test's
    /// own `let got = gate(10);` is another binding. Neither the bound-name
    /// nor the direct-call path may pair it, only an approved loan (#6482).
    #[test]
    fn a_borrowed_assertion_pairs_only_through_its_loan() {
        let assertion = OracleFact {
            line: 9,
            ..exact("assert_eq!(got, true);")
        };
        let body = "let got = gate(10);\ncheck(gate(50));";
        let test = test_summary(
            "shared_name",
            body,
            vec![
                CallFact {
                    line: 1,
                    ..call("gate", "let got = gate(10);")
                },
                CallFact {
                    line: 2,
                    ..call("check", "check(gate(50));")
                },
            ],
            vec![assertion.clone()],
            &["10", "50"],
        );
        let activation = ActivationEvidence {
            observed_values: vec![ValueFact {
                line: 1,
                text: "let got = gate(10); | input".to_string(),
                value: "input == 10".to_string(),
                context: ValueContext::FunctionArgument,
            }],
            missing_discriminators: Vec::new(),
        };
        let pairs = |test: &TestSummary| {
            has_same_test_boundary_oracle_pairing(
                &predicate_probe("input >= 10"),
                Some(&gate_owner()),
                &[test],
                &activation,
                &|_, _| true,
                &|_, _| false,
                &|_, _| None,
            )
        };
        assert!(!pairs(&test));
        // Control: the same assertion written in the test's own body pairs
        // through the bound name.
        let mut inline = test.clone();
        inline.assertions[0].line = 3;
        assert!(pairs(&inline), "control");
    }

    #[test]
    fn a_borrowed_helper_assertion_pairs_through_the_boundary_call() {
        let (test, assertion, loan, activation) = loan_case(CHECK, "check(10, true);", 2);
        assert!(loan_pairs(&test, &assertion, &loan, &activation));
        // Control: without the loan the helper's assertion names only the
        // helper's parameter, so nothing pairs.
        assert!(!has_same_test_boundary_oracle_pairing(
            &predicate_probe("input >= 10"),
            Some(&gate_owner()),
            &[&test],
            &activation,
            &|_, _| true,
            &|_, _| false,
            &|_, _| None,
        ));
    }

    #[test]
    fn a_borrowed_helper_assertion_pairs_only_on_an_exact_mapping() {
        let (test, assertion, loan, activation) = loan_case(CHECK, "check(10, true);", 2);
        // The boundary fact sits on another line than the helper's call.
        let (_, _, _, elsewhere) = loan_case(CHECK, "check(10, true);", 3);
        assert!(!loan_pairs(&test, &assertion, &loan, &elsewhere));
        // The call is not one the admission saw as eager and alone on its
        // line.
        let unlisted = HelperLoan {
            call_lines: Vec::new(),
            ..loan.clone()
        };
        assert!(!loan_pairs(&test, &assertion, &unlisted, &activation));
        // Parameters that cannot be mapped (a pattern or rebound name).
        let unmapped = HelperLoan {
            parameters: Vec::new(),
            ..loan.clone()
        };
        assert!(!loan_pairs(&test, &assertion, &unmapped, &activation));
        // A second owner call in the helper: the boundary may reach that one.
        let twice = HelperLoan {
            helper: "fn check(input: u32, want: bool) { let _ = gate(1); assert_eq!(gate(input), want); }"
                .to_string(),
            ..loan.clone()
        };
        assert!(!loan_pairs(&test, &assertion, &twice, &activation));
        // A computed or partial operand is not the parameter passed on.
        for text in [
            "assert_eq!(gate(input + 1), want);",
            "assert_eq!(gate(input) || other(), want);",
            "assert_eq!({ let input = 1; gate(input) }, want);",
        ] {
            let computed = OracleFact {
                line: 9,
                ..exact(text)
            };
            let mut computed_test = test.clone();
            computed_test.assertions = vec![computed.clone()];
            assert!(
                !loan_pairs(&computed_test, &computed, &loan, &activation),
                "{text}"
            );
        }
        // The test's call on that line names another function.
        let mut other_call = test.clone();
        other_call.calls[0].name = "other".to_string();
        assert!(!loan_pairs(&other_call, &assertion, &loan, &activation));
        // Another call shares the line (inside a macro the syntax scan
        // cannot see into): the line's fact may describe that call.
        let mut crowded = test.clone();
        crowded.calls.push(CallFact {
            line: 2,
            ..call("gate", "assert!(gate(10) || true);")
        });
        assert!(!loan_pairs(&crowded, &assertion, &loan, &activation));
        // A fact on the same line number that names another call (another
        // file's test, pooled into one activation) is not this call's.
        let mut foreign = activation.clone();
        foreign.observed_values[0].text = "let _ = gate(10); | input".to_string();
        assert!(!loan_pairs(&test, &assertion, &loan, &foreign));
        let mut untexted = activation.clone();
        untexted.observed_values[0].text = String::new();
        assert!(!loan_pairs(&test, &assertion, &loan, &untexted));
        // A zero-argument owner call maps no parameter.
        let empty = OracleFact {
            line: 9,
            ..exact("assert_eq!(gate(), want);")
        };
        let mut empty_test = test.clone();
        empty_test.assertions = vec![empty.clone()];
        assert!(!loan_pairs(&empty_test, &empty, &loan, &activation));
        // A helper call passing a local or computed input: another test's
        // identical call on the same line may bind it to a different value,
        // and its pooled fact would then pair this test's far input.
        for call_text in [
            "check(n, true);",
            "check(LIMIT + 1, true);",
            "check(n(), true);",
        ] {
            let (test, assertion, loan, activation) = loan_case(CHECK, call_text, 2);
            assert!(
                !loan_pairs(&test, &assertion, &loan, &activation),
                "{call_text}"
            );
        }
        // A comment or string before the call could feed the argument
        // parser text that is not this call's.
        for call_text in [
            "/* check(10, true) */ check(n, true);",
            "let _ = \"check(10, true)\"; check(n, true);",
        ] {
            let (test, assertion, loan, activation) = loan_case(CHECK, call_text, 2);
            assert!(
                !loan_pairs(&test, &assertion, &loan, &activation),
                "{call_text}"
            );
        }
        // A pooled fact for the same line and text whose value is not the
        // literal this call feeds the owner (another file's `check` with its
        // parameters swapped): `check(50, true)` feeds `input` 50, not 10.
        let (fifty, fifty_assertion, fifty_loan, mut swapped) =
            loan_case(CHECK, "check(50, true);", 2);
        swapped.observed_values[0].value = "input == 10".to_string();
        assert!(!loan_pairs(&fifty, &fifty_assertion, &fifty_loan, &swapped));
        // A typed literal feeds the same value the direct path reads:
        // `check(10u32, true)` pairs with `input == 10`, and `check(11u32,
        // true)` does not.
        let (typed, typed_assertion, typed_loan, typed_activation) =
            loan_case(CHECK, "check(10u32, true);", 2);
        assert!(loan_pairs(
            &typed,
            &typed_assertion,
            &typed_loan,
            &typed_activation
        ));
        let (eleven, eleven_assertion, eleven_loan, eleven_activation) =
            loan_case(CHECK, "check(11u32, true);", 2);
        assert!(!loan_pairs(
            &eleven,
            &eleven_assertion,
            &eleven_loan,
            &eleven_activation
        ));
        assert!(!same_scalar_literal(&test, "10", "ten"));
        // A conjunction pairs only when every term is a literal the call
        // feeds (`gate(x, y)` reached with `check(0, 0)`).
        let fed = [
            ("x".to_string(), "0".to_string()),
            ("y".to_string(), "0".to_string()),
        ];
        assert!(fact_matches_fed_literals(&test, "y == 0 && x == 0", &fed));
        assert!(!fact_matches_fed_literals(&test, "y == 0 && x == 1", &fed));
        assert!(!fact_matches_fed_literals(&test, "y == 0 || x == 0", &fed));
        assert!(!fact_matches_fed_literals(&test, "y == 0 && x > 0", &fed));
        assert!(!fact_matches_fed_literals(&test, "y == 0 && z == 0", &fed));
        // The fact must name the owner parameter that literal reaches.
        let mut renamed = activation.clone();
        renamed.observed_values[0].value = "other == 10".to_string();
        assert!(!loan_pairs(&test, &assertion, &loan, &renamed));
        assert!(loan_pairs(&test, &assertion, &loan, &activation), "control");
    }

    fn gate_owner() -> FunctionSummary {
        FunctionSummary {
            id: SymbolId("src/lib.rs::gate".to_string()),
            name: "gate".to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: "pub fn gate(input: u32) -> bool { input >= 10 }".into(),
            calls: vec![],
            returns: vec![],
            literals: vec![],
            source_role: FunctionSourceRole::Production,
            attrs: vec![],
            impl_attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
            impl_context: Default::default(),
            item: Default::default(),
        }
    }

    fn test_summary(
        name: &str,
        body: &str,
        calls: Vec<CallFact>,
        assertions: Vec<OracleFact>,
        literals: &[&str],
    ) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/gate.rs"),
            start_line: 1,
            end_line: 4,
            body: body.into(),
            calls,
            assertions,
            literals: literals
                .iter()
                .map(|value| LiteralFact {
                    line: 1,
                    value: (*value).to_string(),
                })
                .collect(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn call(name: &str, text: &str) -> CallFact {
        CallFact {
            line: 1,
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    fn exact(text: &str) -> OracleFact {
        OracleFact {
            line: 1,
            text: text.to_string(),
            kind: OracleKind::ExactValue,
            strength: OracleStrength::Strong,
            observed_tokens: extract_identifier_tokens(text),
            ok_value_observed: None,
        }
    }
}
