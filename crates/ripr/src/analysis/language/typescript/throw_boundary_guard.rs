//! A throw test at the exact boundary input of a changed throw guard.
//!
//! `if (n <= 0) throw new RangeError("bad")` changed to `n < 0` stops
//! throwing for `n == 0` and for nothing else. `expect(() => f(0)).toThrow()`
//! passes before the change and fails after it, so that one test is the
//! check the gap asks for, even though `toThrow` alone reads as a broad
//! error check.
//!
//! The credit is deliberately narrow. The changed line must be a single
//! `if (<one comparison>) throw ...` line, the owner must contain no other
//! `throw`, the test must call the owner with literal arguments only, and no
//! assertion in that test may be negated. The literal argument for the
//! compared parameter must equal the boundary literal, so a throw test at
//! another input credits nothing and is not shown as a strong related test.

use super::*;

/// Matchers that assert only that the call throws (or rejects).
const THROW_MATCHERS: [&str; 2] = ["toThrow", "toThrowError"];

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '$')
}

/// `Some(class)` when `line_text` is one `if (<comparison>) throw ...` line:
/// `class` is the identifier after `throw new`, when there is one. `None`
/// for any other shape, including a condition joined with `&&`, `||` or
/// `??` and a throw that sits on a later line.
fn guarded_throw_class(line_text: &str) -> Option<Option<String>> {
    let masked = ts_mask_literals_and_comments(line_text);
    // The masked text must line up byte for byte with the original so the
    // class name can be read from the original.
    if masked.len() != line_text.len() {
        return None;
    }
    let code = masked.trim();
    let code = code.strip_prefix("} else").map_or(code, str::trim_start);
    let code = code.strip_prefix("else").map_or(code, str::trim_start);
    let after_if = code.strip_prefix("if")?.trim_start();
    if !after_if.starts_with('(') {
        return None;
    }
    let mut depth = 0_usize;
    let mut close = None;
    for (offset, c) in after_if.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    close = Some(offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let condition = &after_if[1..close];
    if ["&&", "||", "??"]
        .iter()
        .any(|joiner| condition.contains(joiner))
    {
        return None;
    }
    let rest = after_if[close + 1..].trim_start();
    let after_throw = rest.strip_prefix("throw")?;
    if after_throw.starts_with(is_identifier_char) {
        return None;
    }
    // `masked` and `line_text` have equal length, so the same distance from
    // the end locates the text after the keyword in the original.
    let tail_len = after_throw.trim_end().len();
    let trailing = masked.len() - masked.trim_end().len();
    let tail_start = line_text.len() - trailing - tail_len;
    let class = line_text
        .get(tail_start..line_text.len() - trailing)
        .map(str::trim_start)
        .and_then(|text| text.strip_prefix("new "))
        .map(str::trim_start)
        .and_then(leading_identifier)
        .map(str::to_string);
    Some(class)
}

/// `true` when the only failure source the owner text shows is its one
/// `throw`: no second `throw`, and nothing that can reject or throw through
/// another route, so a call at the boundary cannot fail identically whatever
/// the guard says. Conservative syntactic rule (RIPR-SPEC-0097): any
/// `Promise.`, `new Promise`, `reject`, `await`, `yield`, `.then(` or `.catch(` in the masked
/// owner text declines. Failures inside a called helper are not visible
/// here and stay with the weak reading.
fn owner_has_single_throw(owner_source: &str) -> bool {
    let masked = ts_mask_literals_and_comments(owner_source);
    let keyword = |word: &str| {
        masked
            .match_indices(word)
            .filter(|(index, _)| {
                let before = masked[..*index].chars().next_back();
                let after = masked[index + word.len()..].chars().next();
                !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
            })
            .count()
    };
    let other_failure_route = masked.contains("Promise.")
        || masked.contains("new Promise")
        || masked.contains("reject")
        || masked.contains(".then(")
        || masked.contains(".catch(")
        || keyword("await") > 0
        || keyword("yield") > 0;
    keyword("throw") == 1 && !other_failure_route
}

/// The boundary the changed guard compares: the index of the owner
/// parameter and the numeric literal it is compared with, from a condition
/// `param OP 5` or `5 OP param`. `None` when the condition is anything else.
fn guard_boundary(line_text: &str, owner: &TypeScriptOwner) -> Option<(usize, String)> {
    let masked = ts_mask_literals_and_comments(line_text);
    let code = masked.trim();
    let code = code.strip_prefix("} else").map_or(code, str::trim_start);
    let code = code.strip_prefix("else").map_or(code, str::trim_start);
    let after_if = code.strip_prefix("if")?.trim_start();
    let mut depth = 0_usize;
    let mut close = None;
    for (offset, c) in after_if.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    close = Some(offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let condition = after_if.get(1..close?)?;
    let (lhs, rhs) = ["===", "!==", "<=", ">=", "==", "!=", "<", ">"]
        .iter()
        .find_map(|op| condition.split_once(op))?;
    let (lhs, rhs) = (lhs.trim(), rhs.trim());
    let param_index = |name: &str| owner.params.iter().position(|param| param == name);
    let literal = |text: &str| {
        (!text.ends_with('n'))
            .then(|| numeric_literal_value(text))
            .flatten()
    };
    match (
        param_index(lhs),
        literal(rhs),
        param_index(rhs),
        literal(lhs),
    ) {
        (Some(index), Some(value), _, _) | (_, _, Some(index), Some(value)) => Some((index, value)),
        _ => None,
    }
}

/// `true` when every `expect(<observed>)` in the test body carries a
/// `.rejects` modifier of its own, so a rejecting check on one call cannot
/// stand for a synchronous throw check on another.
fn observed_always_rejects(body_text: &str, observed: &str) -> bool {
    let compact = |text: &str| -> String {
        ts_mask_literals_and_comments(text)
            .split_whitespace()
            .collect()
    };
    let body = compact(body_text);
    let needle = format!("expect({})", compact(observed));
    let mut seen = 0_usize;
    for (index, _) in body.match_indices(&needle) {
        seen += 1;
        if !body[index + needle.len()..].starts_with(".rejects.") {
            return false;
        }
    }
    seen > 0
}

/// Throw assertions on a literal call to the owner, where the changed line is
/// a guarded `throw` and the owner has no other `throw`.
pub(crate) fn ts_throw_boundary_pin_facts(
    probe_family: &ProbeFamily,
    line_text: &str,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> PinFacts {
    let mut facts = PinFacts::default();
    if !matches!(probe_family, ProbeFamily::Predicate)
        || !matches!(
            owner.owner_kind,
            OwnerKind::Function | OwnerKind::ArrowFunction
        )
        || !owner
            .source_text
            .as_deref()
            .is_some_and(owner_has_single_throw)
    {
        return facts;
    }
    let Some(thrown_class) = guarded_throw_class(line_text) else {
        return facts;
    };
    let Some((boundary_index, boundary_value)) = guard_boundary(line_text, owner) else {
        return facts;
    };
    let pins = |assertion: &TypeScriptAssertion| {
        THROW_MATCHERS.contains(&assertion.matcher.as_str())
            && match (&assertion.error_payload, assertion.argument_count) {
                (None, 0) => true,
                // The class the guard throws, written as a bare identifier.
                (Some(payload), 1) => {
                    matches!(
                        payload.kind,
                        TypeScriptErrorPayloadKind::ThrowsClass
                            | TypeScriptErrorPayloadKind::RejectsThrowClass
                    ) && thrown_class.as_deref() == Some(payload.expected.as_str())
                }
                _ => false,
            }
    };
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let test = candidate.test;
        if local_identifier_declared_in_test_body(&test.body_text, &owner.name) {
            continue;
        }
        // A negated check anywhere in the test leaves the whole test
        // uncredited.
        let compact_body: String = ts_mask_literals_and_comments(&test.body_text)
            .split_whitespace()
            .collect();
        if compact_body.contains(".not.") {
            continue;
        }
        let receivers =
            unshadowed_owner_namespace_receivers(test, owner, alias_map, workspace_root);
        let bare_is_owner = test.imports_in_file.iter().any(|import| {
            !import.namespace
                && import.local == owner.name
                && import.imported.as_deref() == Some(owner.name.as_str())
                && import_source_matches_owner(import, &test.file, owner, alias_map, workspace_root)
        });
        let aliases = import_alias_owner_locals(test, owner, alias_map, workspace_root);
        let names = ts_owner_call_names(candidate, owner, alias_map, workspace_root);
        for assertion in test.assertions.iter().filter(|a| pins(a)) {
            let Some(observed) = assertion.observed_expression.as_deref() else {
                continue;
            };
            // `() => f(0)`; an async or block-bodied callback is left out.
            let observed = observed.trim();
            let call_text = match observed.strip_prefix("()") {
                Some(rest) => match rest.trim_start().strip_prefix("=>") {
                    Some(body) => body.trim(),
                    None => continue,
                },
                // `expect(f(0)).rejects.toThrow()`: no callback.
                None if observed_always_rejects(&test.body_text, observed) => observed,
                None => continue,
            };
            let mut call = assertion.clone();
            call.observed_expression = Some(call_text.to_string());
            // Every argument is a scalar literal and the one the guard
            // compares equals the boundary literal: a throw test at another
            // input is not the check for this change.
            let at_boundary =
                ts_observed_owner_call_arguments(test, &call, &names, owner, &receivers)
                    .is_some_and(|arguments| {
                        arguments
                            .iter()
                            .all(|argument| is_scalar_literal(argument.trim()))
                            && arguments.get(boundary_index).is_some_and(|argument| {
                                numeric_literal_value(argument.trim()).as_deref()
                                    == Some(boundary_value.as_str())
                            })
                    });
            if at_boundary
                && observes_imported_owner_result(
                    test,
                    &call,
                    owner,
                    &receivers,
                    bare_is_owner,
                    &aliases,
                )
            {
                facts.credit(&test.file, assertion);
            }
        }
    }
    facts
}
