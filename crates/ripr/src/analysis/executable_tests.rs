//! Executable test functions defined in one Rust file, from parser-backed
//! facts: their module-qualified names and their item text (#16311).

use super::facts;
use super::stable_path_text;
use super::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Module-qualified executable test functions defined in one Rust file, from
/// parser-backed facts. Ambiguous duplicate symbol identities fail closed.
pub(crate) fn executable_test_function_names(
    path: &Path,
    source: &str,
) -> Result<BTreeSet<String>, String> {
    Ok(executable_test_functions(path, source)?
        .0
        .into_keys()
        .collect())
}

/// The same tests keyed by name, each with its body text, so two versions of
/// one file can be compared test by test. `None` when the file defines an
/// ambiguous duplicate identity (for example `cfg` twins of one test): the
/// names leave such tests out, so a comparison could mistake a surviving twin
/// for an added test.
pub(crate) fn executable_test_function_bodies(
    path: &Path,
    source: &str,
) -> Result<Option<BTreeMap<String, String>>, String> {
    let (bodies, ambiguous) = executable_test_functions(path, source)?;
    Ok((!ambiguous).then_some(bodies))
}

/// The unambiguous tests by name with their body text, and whether any
/// executable test was left out as an ambiguous duplicate identity.
fn executable_test_functions(
    path: &Path,
    source: &str,
) -> Result<(BTreeMap<String, String>, bool), String> {
    let facts = RaRustSyntaxAdapter.summarize_file(path, source)?;
    let prefix = format!("{}::", stable_path_text(path));
    let mut ambiguous = false;
    let mut bodies = BTreeMap::new();
    for function in facts.functions.iter().filter(|function| {
        function.source_role.registers_executable_test()
            && matches!(function.item.container, facts::FunctionContainer::Free)
            && function.item.has_body
    }) {
        if function
            .id
            .0
            .ends_with(&format!("{}#L{}", function.name, function.start_line))
        {
            ambiguous = true;
            continue;
        }
        if let Some(name) = function.id.0.strip_prefix(&prefix) {
            bodies.insert(name.to_string(), function.body.as_str().to_string());
        }
    }
    Ok((bodies, ambiguous))
}
