//! RIPR-SPEC-0247 rule 2, typed amendment: loose `==` equality against `0`
//! or `1` pins an owner declared to return `number`.
//!
//! Rule 2 leaves `assert.equal(x, 1)` weak because `true == 1` and
//! `"1" == 1`. Those coercions need a non-number on the left. When the owner
//! declares its return type as exactly `number` and the assertion observes
//! the whole result of one owner call, `==` compares two numbers, which
//! answers as `===` does, so the assertion pins the value.
//!
//! Only function and arrow owners qualify. A method's result is reached
//! through a receiver, and the whole-result check reads only `f(...)`,
//! `ns.f(...)` or a local bound to one, so a same-named free function the
//! test imports could otherwise borrow the method's annotation.

use super::*;

/// `true` when the owner source declares its return type as exactly
/// `number`: `function f(x: T): number {` or `(x: T): number => ...`.
/// A generic parameter list, an `async` or
/// generator owner, and any other annotation (`number | string`,
/// `Promise<number>`, none at all) read `false`.
pub(crate) fn ts_owner_declares_number_return(owner_source: &str) -> bool {
    let masked = ts_mask_literals_and_comments(owner_source);
    let Some(open) = masked.find('(') else {
        return false;
    };
    let head = &masked[..open];
    // A generic list can hold parentheses of its own; `async` and `*`
    // change what a call returns.
    if head.contains(['<', '*'])
        || head
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .any(|word| word == "async")
    {
        return false;
    }
    let mut depth = 0_usize;
    let mut close = None;
    for (offset, c) in masked[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return false;
    };
    let Some(annotation) = masked[close + 1..].trim_start().strip_prefix(':') else {
        return false;
    };
    let end = [annotation.find('{'), annotation.find("=>")]
        .into_iter()
        .flatten()
        .min();
    end.is_some_and(|end| annotation[..end].trim() == "number")
}

/// Loose-equality assertions against a number literal on the whole result
/// of an owner declared to return `number`.
pub(crate) fn ts_typed_loose_number_facts(
    owner: &TypeScriptOwner,
    candidates: &[TypeScriptRelatedCandidate<'_>],
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> PinFacts {
    let mut facts = PinFacts::default();
    let pins = |assertion: &TypeScriptAssertion| {
        assertion.loose_number_expected
            && matches!(assertion.oracle_kind, OracleKind::RelationalCheck)
            && assertion.error_payload.is_none()
            && assertion.expected_value_or_variant.is_some()
    };
    if !candidates
        .iter()
        .any(|candidate| candidate.test.assertions.iter().any(pins))
    {
        return facts;
    }
    if !matches!(
        owner.owner_kind,
        OwnerKind::Function | OwnerKind::ArrowFunction
    ) || !owner
        .source_text
        .as_deref()
        .is_some_and(ts_owner_declares_number_return)
    {
        return facts;
    }
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
    {
        if local_identifier_declared_in_test_body(&candidate.test.body_text, &owner.name) {
            continue;
        }
        let receivers =
            unshadowed_owner_namespace_receivers(candidate.test, owner, alias_map, workspace_root);
        // A bare `f(...)` is the owner only when this file imports `f` by
        // name from the owner's module: a namespace import relates the test
        // without vouching for a bare `f` imported from elsewhere.
        let bare_is_owner = candidate.test.imports_in_file.iter().any(|import| {
            !import.namespace
                && import.local == owner.name
                && import.imported.as_deref() == Some(owner.name.as_str())
                && import_source_matches_owner(
                    import,
                    &candidate.test.file,
                    owner,
                    alias_map,
                    workspace_root,
                )
        });
        let aliases = import_alias_owner_locals(candidate.test, owner, alias_map, workspace_root);
        for assertion in candidate.test.assertions.iter().filter(|a| pins(a)) {
            if observes_imported_owner_result(
                candidate.test,
                assertion,
                owner,
                &receivers,
                bare_is_owner,
                &aliases,
            ) {
                facts.credit(&candidate.test.file, assertion);
            }
        }
    }
    facts
}

/// `true` when `assertion` reads the whole result of one owner call through
/// a binding this file imports from the owner's module: a namespace member
/// `ns.f(...)`, a renamed import `g(...)`, or, when `bare_is_owner`, a bare
/// `f(...)` or a local bound once to it.
fn observes_imported_owner_result(
    test: &TypeScriptTest,
    assertion: &TypeScriptAssertion,
    owner: &TypeScriptOwner,
    receivers: &[String],
    bare_is_owner: bool,
    aliases: &[&str],
) -> bool {
    let Some(observed) = assertion.observed_expression.as_deref() else {
        return false;
    };
    let observed = observed.trim();
    let observed = observed
        .strip_prefix("await ")
        .map_or(observed, str::trim_start);
    let call_on = |callee: &str| {
        observed
            .strip_prefix(callee)
            .is_some_and(ts_is_one_argument_list)
    };
    (bare_is_owner && ts_observes_whole_owner_result(test, assertion, owner, receivers))
        || receivers
            .iter()
            .any(|receiver| call_on(&format!("{receiver}.{}", owner.name)))
        || aliases.iter().any(|alias| call_on(alias))
}
