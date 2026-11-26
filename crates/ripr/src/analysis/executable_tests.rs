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
    Ok(executable_test_function_bodies(path, source)?
        .into_keys()
        .collect())
}

/// The same tests keyed by name, each with its body text, so two versions of
/// one file can be compared test by test. Ambiguous duplicate symbol
/// identities are left out, as they are from the names.
pub(crate) fn executable_test_function_bodies(
    path: &Path,
    source: &str,
) -> Result<BTreeMap<String, String>, String> {
    let facts = RaRustSyntaxAdapter.summarize_file(path, source)?;
    let prefix = format!("{}::", stable_path_text(path));
    Ok(facts
        .functions
        .iter()
        .filter(|function| {
            function.source_role.registers_executable_test()
                && matches!(function.item.container, facts::FunctionContainer::Free)
                && function.item.has_body
                && !function
                    .id
                    .0
                    .ends_with(&format!("{}#L{}", function.name, function.start_line))
        })
        .filter_map(|function| {
            function
                .id
                .0
                .strip_prefix(&prefix)
                .map(|name| (name.to_string(), function.body.as_str().to_string()))
        })
        .collect())
}
