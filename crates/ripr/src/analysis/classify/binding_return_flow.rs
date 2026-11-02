//! RIPR-SPEC-0096 example 2: decide a changed `let` returned unmodified.
//!
//! `binding_return` already names the whole-binding flow and refuses a
//! reassignment, shadow, intervening use or computed tail. This module takes
//! the next step only for that whole-binding shape: the owner's return *is*
//! the initializer, so a related test that compares the owner call to a
//! literal can be evaluated under the old initializer and the new one. A
//! same-file unique helper whose body is one arithmetic expression is inlined
//! once; anything else stays unread.
//!
//! Nothing here approximates. A transform tail, a rebound binding, a
//! preceding early exit, a foreign same-name import, a renamed binding, a
//! non-literal expectation or an unreadable helper ends in `None`, and the
//! caller keeps `static_unknown`.

use super::const_flow::expr::{
    Machine, Reader, Ty, Value, inline_helpers, parse_assertion, parse_expression, parse_fn_header,
    parse_reader,
};
use super::const_flow::{mentions_call, shares_module_scope};
use crate::analysis::facts::{FunctionImplContext, FunctionSourceRole, FunctionSummary};
use crate::analysis::language::{changed_let_binding, mask_rust_comments_and_strings};
use crate::analysis::probes::{ReturnFlowKind, resolve_changed_binding_return};
use crate::analysis::rust_index::{RustIndex, TestSummary, find_file_facts};
use crate::domain::Probe;
use std::collections::{BTreeMap, BTreeSet};

/// Related tests evaluated per owner. Past it the flow is unread rather
/// than sampled.
const TEST_LIMIT: usize = 8;

/// What evaluating the related tests under the old and new initializer showed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum BindingReturnFlow {
    /// A related assertion holds on the old initializer and fails on the new.
    Discriminated(BindingReturnFlowDetail),
    /// Every related assertion holds on both initializers.
    Indistinguishable(BindingReturnFlowDetail),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct BindingReturnFlowDetail {
    pub binding: String,
    pub owner: String,
    pub old_init: String,
    pub new_init: String,
    pub test: String,
    pub assertion: String,
    pub old_result: String,
    pub new_result: String,
    pub tests_evaluated: usize,
}

/// Evaluate the whole-binding return, or `None` when any part of it is
/// unreadable. `functions_named` counts same-file functions so a helper or
/// owner whose name is shared is not the function the test's call reaches.
pub(in crate::analysis) fn evaluate_binding_return_flow(
    probe: &Probe,
    owner: &FunctionSummary,
    helpers: &[&FunctionSummary],
    tests: &[&TestSummary],
    functions_named: &dyn Fn(&str) -> usize,
    import_sources: &[&str],
    crate_names: &BTreeSet<String>,
) -> Option<BindingReturnFlow> {
    if !matches!(owner.impl_context, FunctionImplContext::Free)
        || functions_named(&owner.name) != 1
        || tests.is_empty()
        || tests.len() > TEST_LIMIT
    {
        return None;
    }
    let (header_name, params) = parse_fn_header(&owner.body)?;
    if header_name != owner.name {
        return None;
    }
    let flow = whole_binding_return(probe, owner)?;
    let offset = probe.location.line.checked_sub(owner.start_line)?;
    if preceding_early_exit(&owner.body, offset) {
        return None;
    }
    let old_init = initializer_text(probe.before.as_deref()?, &flow.binding)?;
    let new_init = flow.initializer.as_str();
    if old_init == new_init {
        return None;
    }
    let helper_readers = helper_readers(owner, helpers, functions_named)?;
    let old_body = inline_helpers(&parse_expression(old_init)?, &helper_readers)?;
    let new_body = inline_helpers(&parse_expression(new_init)?, &helper_readers)?;
    let before = Reader::from_parts(owner.name.clone(), params.clone(), old_body)?;
    let after = Reader::from_parts(owner.name.clone(), params, new_body)?;
    let dummy = Value::Int(0);
    let dummy_ty = Ty::Int {
        min: i64::MIN,
        max: i64::MAX,
    };
    let before_readers = [before];
    let after_readers = [after];
    let before_machine = Machine {
        readers: &before_readers,
        constant: "",
        constant_ty: dummy_ty,
        constant_value: &dummy,
    };
    let after_machine = Machine {
        readers: &after_readers,
        constant: "",
        constant_ty: dummy_ty,
        constant_value: &dummy,
    };
    let owner_name = owner.name.as_str();
    if import_sources
        .iter()
        .any(|src| super::reveal::file_imports_foreign_callee_name(src, owner_name, crate_names))
    {
        return None;
    }

    let mut first_seen: Option<BindingReturnFlowDetail> = None;
    let mut failing: Option<BindingReturnFlowDetail> = None;
    for test in tests {
        if super::reveal::file_imports_foreign_callee_name(&test.body, owner_name, crate_names) {
            return None;
        }
        let mut evaluated = 0usize;
        let mut calls_evaluated = 0usize;
        for oracle in &test.assertions {
            if !mentions_call(&oracle.text, owner_name) {
                continue;
            }
            let assertion = parse_assertion(&oracle.text)?;
            let names = assertion.call_names();
            if !assertion.has_literal_expectation()
                || names.is_empty()
                || !names.iter().all(|name| name == owner_name)
            {
                return None;
            }
            if !before_machine.holds(&assertion)? {
                return None;
            }
            evaluated += 1;
            calls_evaluated += names.len();
            let detail = BindingReturnFlowDetail {
                binding: flow.binding.clone(),
                owner: owner.name.clone(),
                old_init: old_init.to_string(),
                new_init: new_init.to_string(),
                test: test.name.clone(),
                assertion: oracle.text.split_whitespace().collect::<Vec<_>>().join(" "),
                old_result: before_machine
                    .call_result(&assertion)
                    .map(|value| value.render())
                    .unwrap_or_default(),
                new_result: after_machine
                    .call_result(&assertion)
                    .map(|value| value.render())
                    .unwrap_or_default(),
                tests_evaluated: 0,
            };
            if first_seen.is_none() {
                first_seen = Some(detail.clone());
            }
            if !after_machine.holds(&assertion)? && failing.is_none() {
                failing = Some(detail);
            }
        }
        let body_calls = test
            .body_calls()
            .filter(|call| call.name == owner.name)
            .count();
        if evaluated == 0 || body_calls != calls_evaluated {
            return None;
        }
    }
    let detail = |detail: BindingReturnFlowDetail| BindingReturnFlowDetail {
        tests_evaluated: tests.len(),
        ..detail
    };
    match (failing, first_seen) {
        (Some(failing), _) => Some(BindingReturnFlow::Discriminated(detail(failing))),
        (None, Some(first)) => Some(BindingReturnFlow::Indistinguishable(detail(first))),
        (None, None) => None,
    }
}

/// Same-file unique free helpers whose bodies are one value-closed expression.
pub(in crate::analysis) fn same_file_binding_helpers<'a>(
    owner: &'a FunctionSummary,
    index: &'a RustIndex,
) -> Option<Vec<&'a FunctionSummary>> {
    let facts = find_file_facts(index, &owner.file)?;
    let named = |name: &str| facts.functions.iter().filter(|f| f.name == name).count();
    if named(&owner.name) != 1 {
        return None;
    }
    let mut helpers = Vec::new();
    for function in facts.functions.iter() {
        if function.name == owner.name
            || function.source_role != FunctionSourceRole::Production
            || !matches!(function.impl_context, FunctionImplContext::Free)
            || !shares_module_scope(&facts.source, owner.start_line, &[function.start_line])
        {
            continue;
        }
        if named(&function.name) != 1 {
            return None;
        }
        helpers.push(function);
    }
    Some(helpers)
}

fn whole_binding_return(
    probe: &Probe,
    owner: &FunctionSummary,
) -> Option<crate::analysis::probes::ChangedBindingReturnFlow> {
    let offset = probe.location.line.checked_sub(owner.start_line)?;
    let line = owner.body.lines().nth(offset)?;
    let (binding, initializer) = changed_let_binding(line)?;
    let flow = resolve_changed_binding_return(
        binding,
        initializer,
        owner.body.as_str(),
        owner.start_line,
        probe.location.line,
    )?;
    (flow.kind == ReturnFlowKind::Whole).then_some(flow)
}

fn initializer_text<'a>(line: &'a str, expected_binding: &str) -> Option<&'a str> {
    let (binding, init) = changed_let_binding(line)?;
    (binding == expected_binding).then_some(init)
}

/// `return`, `?`, or `break` on a line before the changed `let`. The
/// whole-binding relation still holds for the tail path; credit does not,
/// because this evaluator treats every owner call as the initializer.
fn preceding_early_exit(body: &str, changed_offset: usize) -> bool {
    mask_rust_comments_and_strings(body)
        .lines()
        .take(changed_offset)
        .any(|line| {
            let trimmed = line.trim();
            contains_ident(trimmed, "return")
                || contains_ident(trimmed, "break")
                || trimmed.contains('?')
        })
}

fn contains_ident(text: &str, ident: &str) -> bool {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .any(|part| part == ident)
}

fn helper_readers(
    owner: &FunctionSummary,
    helpers: &[&FunctionSummary],
    functions_named: &dyn Fn(&str) -> usize,
) -> Option<BTreeMap<String, Reader>> {
    let mut readers = BTreeMap::new();
    for helper in helpers {
        if helper.name == owner.name || functions_named(&helper.name) != 1 {
            return None;
        }
        if !matches!(helper.impl_context, FunctionImplContext::Free) {
            return None;
        }
        let Some(reader) = parse_reader(&helper.body).filter(Reader::is_value_closed) else {
            continue;
        };
        if reader.name != helper.name {
            return None;
        }
        if readers.insert(helper.name.clone(), reader).is_some() {
            return None;
        }
    }
    Some(readers)
}

#[cfg(test)]
mod tests;
