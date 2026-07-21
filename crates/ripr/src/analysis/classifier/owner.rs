use crate::analysis::rust_index::{FunctionSummary, RustIndex};
use crate::domain::Probe;

pub(in crate::analysis) fn resolve_owner_function<'index>(
    probe: &Probe,
    index: &'index RustIndex,
) -> Option<&'index FunctionSummary> {
    let owner = probe.owner.as_ref()?;
    index
        .functions
        .iter()
        .find(|function| &function.id == owner)
}

/// Return true only for a changed owner that is itself an assertion/oracle
/// helper and has no non-test callers. This is a guidance-only signal: it does
/// not change the exposure classification.
pub(in crate::analysis) fn owner_is_assertion_helper(
    owner: Option<&FunctionSummary>,
    index: &RustIndex,
) -> bool {
    let Some(owner) = owner else {
        return false;
    };
    if owner.is_test || !has_assertion_shape(&owner.body) {
        return false;
    }

    let unique_owner_name = index
        .functions
        .iter()
        .filter(|function| function.name == owner.name)
        .count()
        == 1;
    if !unique_owner_name {
        return false;
    }

    !index.functions.iter().any(|caller| {
        !caller.is_test
            && caller.id != owner.id
            && caller.calls.iter().any(|call| call.name == owner.name)
    })
}

fn has_assertion_shape(body: &str) -> bool {
    [
        "assert!(",
        "assert_eq!(",
        "assert_ne!(",
        "debug_assert!(",
        "debug_assert_eq!(",
        "debug_assert_ne!(",
        ".expect(",
    ]
    .iter()
    .any(|marker| body.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::{CallFact, FunctionFact};
    use std::path::PathBuf;

    fn function(name: &str, is_test: bool, body: &str) -> FunctionFact {
        FunctionFact {
            id: crate::domain::SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 10,
            body: body.to_string(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            is_test,
            attrs: Vec::new(),
        }
    }

    #[test]
    fn assertion_helper_requires_no_production_caller() {
        let owner = function(
            "assert_workspace_source_paths_are_stable",
            false,
            "assert!(!span.file.contains('\\\\'));",
        );
        let index = RustIndex {
            functions: vec![owner.clone()],
            ..RustIndex::default()
        };

        assert!(owner_is_assertion_helper(Some(&owner), &index));
    }

    #[test]
    fn assertion_helper_signal_is_fail_closed_for_production_callers() {
        let owner = function("validate", false, "assert_eq!(value, expected);");
        let mut caller = function("run", false, "validate();");
        caller.calls.push(CallFact {
            line: 2,
            name: owner.name.clone(),
            text: "validate()".to_string(),
        });
        let index = RustIndex {
            functions: vec![owner.clone(), caller],
            ..RustIndex::default()
        };

        assert!(!owner_is_assertion_helper(Some(&owner), &index));
    }
}
