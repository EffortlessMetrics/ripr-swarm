//! RIPR-SPEC-0320: a truthiness assertion on the whole result of an owner
//! declared to return `boolean` pins that result.
//!
//! `assert.ok(isAdult(18))` and `assert.ok(!isAdult(17))` read as smoke
//! because `ok` passes for any truthy value. For an owner whose declared
//! return type is exactly `boolean` the only truthy result is `true`, so the
//! assertion is the exact pin `=== true` (or `=== false` under one `!`).
//!
//! The same restrictions as the typed loose-number amendment apply: only
//! function and arrow owners qualify, the declared annotation must be exactly
//! `boolean`, and the assertion must observe one whole owner call through a
//! binding the test imports from the owner's module. Whether the inputs reach
//! the changed boundary stays with the boundary witness, which reads the
//! credited pin like any other.

use super::*;

/// Matchers whose only effect is a truthiness check on their first argument.
fn is_truthiness_check(assertion: &TypeScriptAssertion) -> bool {
    matches!(assertion.oracle_kind, OracleKind::SmokeOnly)
        && matches!(
            assertion.matcher.as_str(),
            "ok" | "isTrue" | "isFalse" | "toBeTruthy" | "toBeFalsy"
        )
        && assertion.error_payload.is_none()
}

/// Truthiness assertions on the whole result of an owner declared to return
/// `boolean`.
pub(crate) fn ts_typed_boolean_assert_facts(
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> PinFacts {
    ts_typed_owner_pin_facts(
        owner,
        candidates,
        alias_map,
        workspace_root,
        &TypedOwnerPin {
            pins: &is_truthiness_check,
            declares_return: &|source| ts_owner_declares_return_type(source, "boolean"),
            negated: true,
            observed_ok: &observed_call_has_literal_arguments,
        },
    )
}

/// `true` when `observed` is one call whose every argument is a number,
/// string, boolean or `null` literal. A loop variable or an expression can
/// hold the pin to one side of a boundary the assertion never crosses (a
/// property check over `x, x` pins `true` only), so those stay weak.
fn observed_call_has_literal_arguments(observed: &str) -> bool {
    let observed = observed.trim();
    let observed = observed
        .strip_prefix("await ")
        .map_or(observed, str::trim_start);
    let Some(open) = observed.find('(') else {
        return false;
    };
    let Some(arguments) = observed[open + 1..].strip_suffix(')') else {
        return false;
    };
    let arguments = arguments.trim();
    arguments.is_empty()
        || arguments
            .split(',')
            .all(|argument| is_literal_argument(argument.trim()))
}

fn is_literal_argument(argument: &str) -> bool {
    if matches!(argument, "true" | "false" | "null") || argument.parse::<f64>().is_ok() {
        return true;
    }
    let mut chars = argument.chars();
    match (chars.next(), chars.next_back()) {
        (Some(quote @ ('"' | '\'')), Some(end)) if quote == end => {
            let inner = &argument[1..argument.len() - 1];
            !inner.contains(quote) && !inner.contains('\\')
        }
        _ => false,
    }
}
