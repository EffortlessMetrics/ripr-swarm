use super::super::{
    AssertionContextRefusal, builtin_name_holds_uncached, default_enabled_gate,
    item_context_refusal, owner_pin_assertions, parse_clean_source_file,
};
use super::*;
use crate::analysis::facts::cfg_predicates::attribute_test_build_availability;
use ra_ap_syntax::AstNode;
use std::collections::BTreeSet;

/// `item_context_refusal` as written before the memo, kept verbatim as the
/// reference.
fn reference_item_context_refusal(
    item: &SyntaxNode,
    default_features: Option<&BTreeSet<String>>,
) -> Option<AssertionContextRefusal> {
    let mut source_file = false;
    for (depth, node) in item.ancestors().enumerate() {
        if depth > 0
            && !ast::ItemList::can_cast(node.kind())
            && !ast::Module::can_cast(node.kind())
            && !ast::SourceFile::can_cast(node.kind())
        {
            return Some(AssertionContextRefusal::NestedItem);
        }
        if let Some(attr) = node.children().filter_map(ast::Attr::cast).find(|attr| {
            attribute_test_build_availability(&attr.syntax().text().to_string()) != Some(true)
                && !default_enabled_gate(attr, default_features)
        }) {
            return Some(AssertionContextRefusal::GatedItem(
                attr.syntax().text().to_string(),
            ));
        }
        source_file |= ast::SourceFile::can_cast(node.kind());
    }
    (!source_file).then_some(AssertionContextRefusal::NestedItem)
}

/// Sources covering rebinding imports and `extern crate` renames of `test`
/// and of a lint name, `use super::*` chains, nested modules, inner and outer
/// gates that are on, off, unknown or feature-settled, and nested items.
const SOURCES: [&str; 8] = [
    "#[test]\nfn plain() { assert_eq!(1, 1); }\n",
    "use skipper::serial as test;\n#[test]\nfn rebound() { assert_eq!(1, 1); }\nmod inner {\n    use super::*;\n    #[test]\n    fn through_glob() { assert_eq!(1, 1); }\n}\n",
    "extern crate skipper as test;\nmod a {\n    mod b {\n        #[test]\n        fn deep() {}\n    }\n}\n",
    "#![cfg(feature = \"fast\")]\n#[test]\nfn gated_file() {}\n#[cfg(test)]\nmod t {\n    #[test]\n    fn on() {}\n}\n",
    "#[cfg(feature = \"slow\")]\nmod slow {\n    #![doc = \"slow\"]\n    #[test]\n    fn under_feature() {}\n}\n#[cfg(not(test))]\nmod off {\n    #[test]\n    fn never() {}\n}\n",
    "#[cfg(windows)]\nmod platform {\n    #[test]\n    fn unknown_gate() {}\n}\nfn outer() {\n    fn nested() {}\n}\n",
    "use lints::warn as allow;\nmod m {\n    use super::*;\n    #[allow(unused)]\n    #[test]\n    fn lint_rebound() {}\n    mod n {\n        use super::*;\n        #[test]\n        fn two_levels() {}\n    }\n}\n",
    "mod clean {\n    use std::fmt;\n    #[test]\n    fn ok() {}\n}\nimpl S {\n    fn method() {}\n}\n",
];

fn fixtures() -> Result<Vec<ra_ap_syntax::Parse<ast::SourceFile>>, String> {
    SOURCES
        .iter()
        .map(|source| parse_clean_source_file(source).ok_or(format!("must parse: {source}")))
        .collect()
}

/// Inside a scan (cold and warm) and outside one, both memoized readings
/// answer exactly as the direct readings, for every function and every
/// module scope, under no default features and under `fast`. Both answers
/// of each reading occur.
#[test]
fn memoized_module_readings_answer_as_the_direct_readings() -> Result<(), String> {
    let fast = BTreeSet::from(["fast".to_string(), "slow".to_string()]);
    let mut refusals = BTreeSet::new();
    let mut holds = BTreeSet::new();
    for parse in fixtures()? {
        let root = parse.tree().syntax().clone();
        let functions: Vec<SyntaxNode> = root
            .descendants()
            .filter(|node| ast::Fn::can_cast(node.kind()))
            .collect();
        let scopes: Vec<SyntaxNode> = functions.iter().filter_map(SyntaxNode::parent).collect();
        assert!(!module_memo_is_live(), "no memo outside a scan");
        for pass in 0..3 {
            let scan = (pass > 0).then(ModuleScan::enter);
            for features in [None, Some(&fast)] {
                for function in &functions {
                    let expected = reference_item_context_refusal(function, features);
                    assert_eq!(
                        item_context_refusal(function, features),
                        expected,
                        "pass {pass}: {}",
                        function.text()
                    );
                    refusals.insert(format!("{expected:?}").chars().take(9).collect::<String>());
                }
            }
            for scope in &scopes {
                for name in ["test", "allow", "doc"] {
                    let expected = builtin_name_holds_uncached(name, scope);
                    assert_eq!(super::builtin_name_holds(name, scope), expected, "{name}");
                    holds.insert(expected);
                }
            }
            drop(scan);
            assert!(!module_memo_is_live());
        }
    }
    assert_eq!(
        holds,
        BTreeSet::from([false, true]),
        "premise: names held and rebound"
    );
    for kind in ["None", "Some(Gate", "Some(Nest"] {
        assert!(
            refusals.contains(kind),
            "premise: {kind} occurs in {refusals:?}"
        );
    }
    Ok(())
}

/// The module readings one `owner_pin_assertions` scan of a file holding
/// `tests` tests performs, with its admission results.
fn scan_walks(tests: usize) -> (usize, usize, bool) {
    let mut source = String::from("use helpers::check;\n");
    for case in 0..tests {
        source.push_str(&format!(
            "\n#[test]\n#[allow(unused)]\nfn case_{case}() {{\n    assert_eq!(check({case}), {case});\n}}\n"
        ));
    }
    let before = MODULE_WALKS.with(std::cell::Cell::get);
    let assertions = owner_pin_assertions(&source, &["assert_eq"]);
    let walks = MODULE_WALKS.with(std::cell::Cell::get) - before;
    let admitted = assertions
        .functions
        .values()
        .all(|function| function.refusal.is_none());
    (walks, assertions.functions.len(), admitted)
}

/// Production path: admitting every test of a file reads each function's
/// own attributes once and the shared module answers a fixed number of
/// times, not once per test: doubling the tests adds exactly one reading
/// per added test. Before the memo, every test re-read the file's items for
/// `#[test]` and for the file's inner attributes. No memo is left behind.
#[test]
fn admitting_many_tests_in_one_module_reads_the_module_once() {
    let (walks_300, functions_300, admitted_300) = scan_walks(300);
    let (walks_600, functions_600, admitted_600) = scan_walks(600);
    assert_eq!(
        (functions_300, functions_600),
        (300, 600),
        "every test is identified"
    );
    assert!(admitted_300 && admitted_600, "premise: no test is refused");
    assert_eq!(
        walks_600 - walks_300,
        300,
        "one reading per added test (its own attributes): {walks_300} then {walks_600}"
    );
    assert!(
        walks_300 - 300 <= 8,
        "shared module readings: {} for 300 tests",
        walks_300 - 300
    );
    assert!(!module_memo_is_live(), "the scan's memo ends with it");
}
