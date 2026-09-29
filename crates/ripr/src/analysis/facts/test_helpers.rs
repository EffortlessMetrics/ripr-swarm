//! Same-file test-side assertion helpers.
//!
//! Rust suites often move the call and the assertion into a helper and keep
//! each test to a list of cases:
//!
//! ```text
//! fn check(input: u32, expected: bool) { assert_eq!(gate(input), expected); }
//!
//! #[test]
//! fn boundary() { check(10, false); check(11, true); }
//! ```
//!
//! Without this pass the test body has no owner call and no assertion, so a
//! changed `gate` reads as reached only by name and never discriminated
//! (`reachable_unrevealed`), although mutating `gate` fails the test.
//!
//! This pass is the one authority that credits such helpers. It runs after
//! every role authority, so the helper's role is final, and it widens a
//! test's facts in one bounded direction:
//!
//! - the helper is a non-test, evidence-role (`CfgTestModule`) function in
//!   the test's own file, and its name is defined exactly once there;
//! - the test calls it as a direct free function (`check(..)`, not
//!   `self.check(..)` or `path::check(..)`), and neither a nested `fn` nor
//!   a `let` binding in the test shadows the name;
//! - one hop only: the helper's calls and assertions are added with their
//!   own line numbers; helpers the helper calls are not followed.
//!
//! Anything else (cross-file helpers, ambiguous names, production callees,
//! the lexical fallback) contributes nothing, so a test only ever gains
//! evidence the helper body really contains.

use super::{FunctionFact, FunctionSourceRole, RustIndex, TestFact};
use crate::analysis::extract::extract_assertions;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(super) fn credit_same_file_assertion_helpers(index: &mut RustIndex) {
    let mut helpers_by_file: BTreeMap<PathBuf, BTreeMap<String, Vec<&FunctionFact>>> =
        BTreeMap::new();
    for (file, facts) in &index.files {
        if facts.used_lexical_fallback {
            continue;
        }
        let names = helpers_by_file.entry(file.clone()).or_default();
        for function in &facts.functions {
            names
                .entry(function.name.clone())
                .or_default()
                .push(function);
        }
    }

    let mut widened: BTreeMap<(PathBuf, usize, String), TestFact> = BTreeMap::new();
    for test in &index.tests {
        let Some(functions_by_name) = helpers_by_file.get(&test.file) else {
            continue;
        };
        let mut credited = test.clone();
        let mut credited_helpers: Vec<&str> = Vec::new();
        for call in &test.calls {
            if credited_helpers.contains(&call.name.as_str()) {
                continue;
            }
            let Some(helper) = unique_assertion_helper(functions_by_name, &call.name) else {
                continue;
            };
            if !is_direct_call_site(&call.text, &call.name) || test_shadows(test, &call.name) {
                continue;
            }
            credited_helpers.push(&helper.name);
            credited.calls.extend(helper.calls.iter().cloned());
            credited
                .assertions
                .extend(extract_assertions(&helper.body, helper.start_line));
        }
        if !credited_helpers.is_empty() {
            widened.insert(
                (test.file.clone(), test.start_line, test.name.clone()),
                credited,
            );
        }
    }
    if widened.is_empty() {
        return;
    }

    for test in &mut index.tests {
        if let Some(credited) =
            widened.get(&(test.file.clone(), test.start_line, test.name.clone()))
        {
            *test = credited.clone();
        }
    }
    for facts in index.files.values_mut() {
        for test in &mut facts.tests {
            if let Some(credited) =
                widened.get(&(test.file.clone(), test.start_line, test.name.clone()))
            {
                *test = credited.clone();
            }
        }
    }
}

/// The one same-file definition of `name`, when it is an evidence-only
/// helper. A second definition (a same-named test, production function or
/// helper in another module of the file) makes the call ambiguous.
fn unique_assertion_helper<'facts>(
    functions_by_name: &BTreeMap<String, Vec<&'facts FunctionFact>>,
    name: &str,
) -> Option<&'facts FunctionFact> {
    match functions_by_name.get(name)?.as_slice() {
        [helper] if helper.source_role == FunctionSourceRole::CfgTestModule => Some(helper),
        _ => None,
    }
}

/// A nested `fn` item or a `let` binding with the helper's name means the
/// call may not reach the file-level helper.
fn test_shadows(test: &TestFact, name: &str) -> bool {
    test.nested_fn_names.iter().any(|nested| nested == name)
        || test.let_bindings.iter().any(|binding| binding.name == name)
}

/// Whether `text` calls `name` as a free function: the occurrence is not
/// preceded by an identifier character, a receiver `.` or a path `::`.
fn is_direct_call_site(text: &str, name: &str) -> bool {
    let needle = format!("{name}(");
    text.match_indices(&needle).any(|(at, _)| {
        text[..at].chars().next_back().is_none_or(|before| {
            !before.is_alphanumeric() && before != '_' && before != '.' && before != ':'
        })
    })
}

#[cfg(test)]
mod tests;
