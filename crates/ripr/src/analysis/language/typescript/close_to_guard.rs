//! RIPR-SPEC-0247 rule 6: `toBeCloseTo` on a call whose result the changed
//! `return` expression decides.
//!
//! `toBeCloseTo(expected)` passes when `|expected - received| < 0.005`, so
//! the extractor reads it `unknown`: nothing says how far a changed value
//! has to move before the assertion notices. When the call's arguments are
//! all literals and the owner's old and new `return` expressions evaluate
//! (`literal_eval`), the distance is known. The assertion pins the change
//! when the old value passes and the new value fails by a margin that
//! floating-point rounding cannot close. It is the same fact a mutant run
//! would observe for that one call, computed from the expression instead of
//! executed.

use super::*;

/// The default `toBeCloseTo` tolerance, `10 ** -2 / 2`. A call that passes a
/// `numDigits` argument has `argument_count` 2 and earns nothing.
const DEFAULT_TOLERANCE: f64 = 0.005;

/// Guard band around the tolerance. A value this near the boundary could
/// land on either side once the evaluator and the engine round differently.
const BOUNDARY_MARGIN: f64 = 1e-9;

/// Non-negated `toBeCloseTo(<number literal>)` assertions on the whole result
/// of an owner called with scalar literals, where the changed one-line
/// `return` moves the result across the tolerance.
#[allow(
    clippy::too_many_arguments,
    reason = "structurally distinct finding inputs; bundling them would allocate"
)]
pub(crate) fn ts_close_to_pin_facts(
    probe_family: &ProbeFamily,
    line: usize,
    line_text: &str,
    old_line_text: Option<&str>,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> PinFacts {
    let mut facts = PinFacts::default();
    let pins = |assertion: &TypeScriptAssertion| {
        assertion.matcher == "toBeCloseTo"
            && assertion.argument_count == 1
            && matches!(assertion.oracle_kind, OracleKind::Unknown)
            && assertion.error_payload.is_none()
            && assertion.expected_value_or_variant.is_some()
    };
    if !matches!(probe_family, ProbeFamily::ReturnValue)
        || !matches!(
            owner.owner_kind,
            OwnerKind::Function | OwnerKind::ArrowFunction
        )
        || !candidates
            .iter()
            .any(|candidate| candidate.test.assertions.iter().any(pins))
    {
        return facts;
    }
    let Some((old_expression, new_expression)) =
        plain_changed_return(line, line_text, old_line_text, owner, workspace_root)
    else {
        return facts;
    };
    if !changed_return_runs_unconditionally(line, owner, workspace_root) {
        return facts;
    }
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let test = candidate.test;
        if local_identifier_declared_in_test_body(&test.body_text, &owner.name) {
            continue;
        }
        // The extracted assertion keeps no promise modifier, and a
        // synchronous number cannot satisfy `.resolves` or `.rejects`.
        let code = ts_mask_literals_and_comments(&test.body_text);
        if code.contains(".resolves") || code.contains(".rejects") {
            continue;
        }
        let receivers =
            unshadowed_owner_namespace_receivers(test, owner, alias_map, workspace_root);
        // A bare `f(...)` is the owner only when this file imports `f` by
        // name from the owner's module.
        let bare_is_owner = test.imports_in_file.iter().any(|import| {
            !import.namespace
                && import.local == owner.name
                && import.imported.as_deref() == Some(owner.name.as_str())
                && import_source_matches_owner(import, &test.file, owner, alias_map, workspace_root)
        });
        let aliases = import_alias_owner_locals(test, owner, alias_map, workspace_root);
        let names = ts_owner_call_names(candidate, owner, alias_map, workspace_root);
        for assertion in test.assertions.iter().filter(|a| pins(a)) {
            let Some(expected) = assertion
                .expected_value_or_variant
                .as_deref()
                .and_then(|text| text.trim().parse::<f64>().ok())
                .filter(|value| value.is_finite())
            else {
                continue;
            };
            let Some(arguments) =
                ts_observed_owner_call_arguments(test, assertion, &names, owner, &receivers)
            else {
                continue;
            };
            if !arguments
                .iter()
                .all(|argument| is_scalar_literal(argument.trim()))
                || !observes_imported_owner_result(
                    test,
                    assertion,
                    owner,
                    &receivers,
                    bare_is_owner,
                    &aliases,
                )
            {
                continue;
            }
            let Some((old, new)) = literal_eval::return_expression_numbers(
                &owner.file,
                old_expression,
                new_expression,
                &owner.params,
                &arguments,
            ) else {
                continue;
            };
            let passes_before = (expected - old).abs() < DEFAULT_TOLERANCE - BOUNDARY_MARGIN;
            let fails_after = (expected - new).abs() > DEFAULT_TOLERANCE + BOUNDARY_MARGIN;
            if passes_before && fails_after {
                facts.credit(&test.file, assertion);
            }
        }
    }
    facts
}
