use super::{BindingReturnFlow, evaluate_binding_return_flow};
use crate::analysis::facts::{FunctionSourceRole, FunctionSummary};
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::{DeltaKind, Probe, ProbeFamily, ProbeId, SourceLocation, SymbolId};
use std::path::Path;

const HELPER: &str = "pub fn helper(x: i64) -> i64 {\n    x / 9\n}\n";
const OLD: &str = "    let result = helper(amount * 9);";
const NEW: &str = "    let result = helper(amount * 8);";
const PIN: &str = "#[test]\nfn score_of_ten_is_ten() {\n    assert_eq!(score(10), 10);\n}";

fn owner_source(body: &str) -> String {
    format!("{HELPER}\npub fn score(amount: i64) -> i64 {{\n{body}\n}}\n")
}

fn decide(
    body: &str,
    tests: &str,
    old: &str,
    new: &str,
) -> Result<Option<BindingReturnFlow>, String> {
    let source = format!("{}\n{tests}\n", owner_source(body));
    let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
    let owner = facts
        .functions
        .iter()
        .find(|function| function.name == "score")
        .ok_or("no score owner")?;
    let helpers: Vec<&FunctionSummary> = facts
        .functions
        .iter()
        .filter(|function| {
            function.name != "score" && function.source_role == FunctionSourceRole::Production
        })
        .collect();
    let related: Vec<_> = facts.tests.iter().collect();
    let line = source
        .lines()
        .position(|line| line.contains("helper(amount * 8)"))
        .ok_or("no changed line")?
        + 1;
    let probe = Probe {
        id: ProbeId("probe:src_lib_rs:let".to_string()),
        location: SourceLocation::new("src/lib.rs", line, 1),
        owner: Some(SymbolId("src/lib.rs::score".to_string())),
        family: ProbeFamily::StaticUnknown,
        delta: DeltaKind::Unknown,
        before: Some(old.trim().to_string()),
        after: Some(new.trim().to_string()),
        expression: new.trim().to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    };
    Ok(evaluate_binding_return_flow(
        &probe,
        owner,
        &helpers,
        &related,
        &|name| {
            facts
                .functions
                .iter()
                .filter(|function| function.name == name)
                .count()
        },
        &[source.as_str()],
        &std::collections::BTreeSet::new(),
    ))
}

#[test]
fn a_reassigned_binding_is_not_evaluated() -> Result<(), String> {
    let body = "    let mut result = helper(amount * 8);\n    result = 0;\n    result";
    assert_eq!(decide(body, PIN, OLD, NEW)?, None);
    Ok(())
}

#[test]
fn a_shadowed_binding_is_not_evaluated() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    let result = 0;\n    result";
    assert_eq!(decide(body, PIN, OLD, NEW)?, None);
    Ok(())
}

#[test]
fn a_partially_used_binding_is_not_evaluated() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    let _keep = result;\n    result";
    assert_eq!(decide(body, PIN, OLD, NEW)?, None);
    Ok(())
}

#[test]
fn a_transformed_return_is_not_evaluated() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    result + 1";
    let pin = "#[test]\nfn score_of_ten_is_eleven() {\n    assert_eq!(score(10), 11);\n}";
    assert_eq!(decide(body, pin, OLD, NEW)?, None);
    Ok(())
}

#[test]
fn an_early_return_before_the_binding_is_not_evaluated() -> Result<(), String> {
    let body = "    if amount == 0 { return 0; }\n    let result = helper(amount * 8);\n    result";
    let pin = "#[test]\nfn score_of_zero_is_zero() {\n    assert_eq!(score(0), 0);\n}";
    assert_eq!(decide(body, pin, OLD, NEW)?, None);
    Ok(())
}

#[test]
fn a_foreign_same_name_import_is_not_evaluated() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    result";
    let pin =
        "use other_crate::score;\n#[test]\nfn pins_foreign() {\n    assert_eq!(score(10), 10);\n}";
    assert_eq!(decide(body, pin, OLD, NEW)?, None);
    Ok(())
}

#[test]
fn a_renamed_binding_is_not_evaluated() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    result";
    assert_eq!(
        decide(body, PIN, "    let output = helper(amount * 9);", NEW)?,
        None
    );
    Ok(())
}

#[test]
fn a_named_binding_returned_bare_is_discriminated_by_the_pin() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    result";
    let flow = decide(body, PIN, OLD, NEW)?;
    let Some(BindingReturnFlow::Discriminated(detail)) = flow else {
        return Err(format!("score(10) is 10 before and 8 after: {flow:?}"));
    };
    assert_eq!(detail.binding, "result");
    assert_eq!(detail.owner, "score");
    assert_eq!(detail.old_result, "10");
    assert_eq!(detail.new_result, "8");
    Ok(())
}

#[test]
fn an_unpinned_literal_is_indistinguishable() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    result";
    let far = "#[test]\nfn score_of_zero_is_zero() {\n    assert_eq!(score(0), 0);\n}";
    let flow = decide(body, far, OLD, NEW)?;
    assert!(
        matches!(flow, Some(BindingReturnFlow::Indistinguishable(_))),
        "{flow:?}"
    );
    Ok(())
}

#[test]
fn a_self_computed_expected_value_is_unreadable() -> Result<(), String> {
    let body = "    let result = helper(amount * 8);\n    result";
    let own = "#[test]\nfn computed() {\n    assert_eq!(score(10), helper(10 * 9));\n}";
    assert_eq!(decide(body, own, OLD, NEW)?, None);
    Ok(())
}

#[test]
fn a_duplicate_owner_name_is_unreadable() -> Result<(), String> {
    let source = format!(
        "{HELPER}\npub fn score(amount: i64) -> i64 {{\n    let result = helper(amount * 8);\n    result\n}}\npub fn other() {{\n    fn score(amount: i64) -> i64 {{ amount }}\n}}\n{PIN}\n"
    );
    let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
    let owner = facts
        .functions
        .iter()
        .find(|function| function.name == "score" && function.body.contains("helper"))
        .ok_or("no score owner")?;
    let helpers: Vec<&FunctionSummary> = facts
        .functions
        .iter()
        .filter(|function| function.name == "helper")
        .collect();
    let related: Vec<_> = facts.tests.iter().collect();
    let line = source
        .lines()
        .position(|line| line.contains("helper(amount * 8)"))
        .ok_or("no changed line")?
        + 1;
    let probe = Probe {
        id: ProbeId("probe:src_lib_rs:let".to_string()),
        location: SourceLocation::new("src/lib.rs", line, 1),
        owner: Some(SymbolId("src/lib.rs::score".to_string())),
        family: ProbeFamily::StaticUnknown,
        delta: DeltaKind::Unknown,
        before: Some(OLD.trim().to_string()),
        after: Some(NEW.trim().to_string()),
        expression: NEW.trim().to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    };
    assert_eq!(
        evaluate_binding_return_flow(
            &probe,
            owner,
            &helpers,
            &related,
            &|name| facts
                .functions
                .iter()
                .filter(|function| function.name == name)
                .count(),
            &[source.as_str()],
            &std::collections::BTreeSet::new(),
        ),
        None
    );
    Ok(())
}

/// #13577: the helper scan reads one module-scope table per file per run.
#[test]
fn same_file_helper_scan_builds_one_module_scope_table_per_run() -> Result<(), String> {
    use super::same_file_binding_helpers;
    use crate::analysis::classify::const_flow::{MODULE_SCOPE_BUILDS, shares_module_scope};
    use crate::analysis::facts::RustIndex;
    use crate::analysis::syntax::run_memo;
    let mut source = String::from("pub fn score(x: i64) -> i64 { x }\n");
    for number in 0..30 {
        source.push_str(&format!(
            "pub fn helper_{number}(x: i64) -> i64 {{ x + {number} }}\n"
        ));
    }
    source.push_str("mod inner {\n    pub fn nested(x: i64) -> i64 { x }\n}\n");
    let path = Path::new("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(path, &source)?;
    let functions = facts.functions.clone();
    let mut index = RustIndex::default();
    index.insert_file_only(path.to_path_buf(), facts);
    index.extend_functions(functions);
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == "score")
        .ok_or("no score owner")?;
    // Independent reference: the original per-candidate question, asked
    // outside any run scope so every ask builds its own table.
    let facts = index.files().get(path).ok_or("file indexed")?;
    MODULE_SCOPE_BUILDS.with(|builds| builds.set(0));
    let reference: Vec<String> = facts
        .functions
        .iter()
        .filter(|function| {
            function.name != "score"
                && function.source_role == FunctionSourceRole::Production
                && matches!(
                    function.impl_context,
                    crate::analysis::facts::FunctionImplContext::Free
                )
                && shares_module_scope(&facts.source, owner.start_line, &[function.start_line])
        })
        .map(|function| function.name.clone())
        .collect();
    let candidates = facts.functions.len() - 1;
    assert_eq!(
        MODULE_SCOPE_BUILDS.with(std::cell::Cell::get),
        candidates,
        "outside a scope each candidate builds its own table"
    );
    assert_eq!(
        reference.len(),
        30,
        "the nested module's function is not a sibling"
    );
    assert!(!reference.iter().any(|name| name == "nested"));

    let probes = 5;
    let _scope = run_memo::enter();
    MODULE_SCOPE_BUILDS.with(|builds| builds.set(0));
    for _ in 0..probes {
        let helpers = same_file_binding_helpers(owner, &index).ok_or("helpers decided")?;
        let names: Vec<String> = helpers
            .iter()
            .map(|function| function.name.clone())
            .collect();
        assert_eq!(names, reference);
    }
    assert_eq!(
        MODULE_SCOPE_BUILDS.with(std::cell::Cell::get),
        1,
        "{probes} probes over {candidates} candidates read one table"
    );
    Ok(())
}
