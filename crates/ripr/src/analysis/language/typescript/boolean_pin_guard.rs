//! RIPR-SPEC-0247 rule 5: a truthiness assertion on a call to an owner
//! declared to return `boolean` pins the call's value.
//!
//! `assert.ok(isAdult(18))` and `expect(isAdult(17)).toBeFalsy()` read
//! `smoke_only` because truthiness also accepts `1`, `"x"` and `{}`. A
//! `boolean` owner returns only `true` or `false`, so the same assertion
//! fails for exactly the other value: it is an exact pin. The pin is
//! per-call; whether it sits at the changed boundary stays with the row
//! activation witness.

use super::*;

/// Matchers that assert only the truthiness of the observed value.
const TRUTHINESS_MATCHERS: [&str; 4] = ["ok", "notOk", "toBeTruthy", "toBeFalsy"];

/// `true` when the owner source declares its return type as exactly
/// `boolean` (same shape rules as the `number` annotation reader).
pub(crate) fn ts_owner_declares_boolean_return(owner_source: &str) -> bool {
    ts_owner_declared_return_type(owner_source).is_some_and(|annotation| annotation == "boolean")
}

/// Truthiness assertions on the whole result of an owner declared to return
/// `boolean`, called with scalar literal arguments only.
pub(crate) fn ts_boolean_pin_facts(
    probe_family: &ProbeFamily,
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> PinFacts {
    let mut facts = PinFacts::default();
    if !matches!(
        probe_family,
        ProbeFamily::Predicate | ProbeFamily::ReturnValue
    ) || !matches!(
        owner.owner_kind,
        OwnerKind::Function | OwnerKind::ArrowFunction
    ) || !owner
        .source_text
        .as_deref()
        .is_some_and(ts_owner_declares_boolean_return)
    {
        return facts;
    }
    let pins = |assertion: &TypeScriptAssertion| {
        TRUTHINESS_MATCHERS.contains(&assertion.matcher.as_str())
            && matches!(assertion.oracle_kind, OracleKind::SmokeOnly)
            && assertion.error_payload.is_none()
    };
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        let test = candidate.test;
        if local_identifier_declared_in_test_body(&test.body_text, &owner.name) {
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
            let Some(observed) = assertion.observed_expression.as_deref() else {
                continue;
            };
            let mut call = assertion.clone();
            call.observed_expression = Some(strip_negations(observed).to_string());
            let literal_arguments =
                ts_observed_owner_call_arguments(test, &call, &names, owner, &receivers)
                    .is_some_and(|arguments| {
                        arguments
                            .iter()
                            .all(|argument| is_scalar_literal(argument.trim()))
                    });
            if literal_arguments
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

/// `text` without leading `!` operators (`!isAdult(17)`, `!!isAdult(17)`).
fn strip_negations(text: &str) -> &str {
    let mut text = text.trim();
    while let Some(rest) = text.strip_prefix('!') {
        text = rest.trim_start();
    }
    text
}
