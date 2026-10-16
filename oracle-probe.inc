
// Reviewer-owned diagnostic only; never part of the submitted candidate.
#[test]
fn reviewer_exact8262_three_oracle_cases() {
    let gate = "pub fn gate(value: u32) -> bool {\n    10 <= value\n}\n";
    let tests = "use demo::gate;\n\n#[test]\nfn pins() {\n    assert_eq!(gate(10), true);\n}\n";
    let scoped_tests = "mod unrelated {\n    macro_rules! assert_eq {\n        ($($tokens:tt)*) => {};\n    }\n}\n\nuse demo::gate;\n\n#[test]\nfn pins() {\n    assert_eq!(gate(10), true);\n}\n";
    let glob = "use std::io::prelude::*;\npub fn drain(input: &mut Vec<u8>) { input.clear(); }\n";
    let cases = [
        ("positive-global-only", vec![(CONVERGENCE_GLOB, glob), (LIB, gate), (TESTS, tests)]),
        ("scoped-rest-definition", vec![(CONVERGENCE_GLOB, glob), (LIB, gate), (TESTS, scoped_tests)]),
        ("trusted-subset", vec![("src/a.rs", "pub fn noise() { let _ = stringify!(assert_eq); }\n"), ("src/z.rs", "pub use std::fmt::*;\n"), (LIB, gate), (TESTS, tests)]),
        ("same-path-rank", vec![(LIB, "use std::io::prelude::*;\npub use std::fmt::*;\npub fn gate(value: u32) -> bool {\n    10 <= value\n}\n"), (TESTS, tests)]),
    ];
    let mut failures = Vec::new();
    let mut compared = 0;
    for (label, files) in cases {
        let index = index(&files);
        let syntax = OwnerPinSyntax::default();
        let _ = syntax.trusted_macro_ambiguities(&index);
        let initially_complete = syntax.workspace_site_scan.borrow().as_ref().is_some_and(|scan| scan.complete);
        let roots = syntax.target_roots(&index);
        let resolved_modules = syntax.resolved_module_declarations(&index);
        let resolved = |path: &Path, line: usize, declaration: &str| {
            resolved_modules.contains(&(path.to_path_buf(), line, declaration.to_string()))
        };
        for root in [None, Some(Path::new(LIB)), Some(Path::new(TESTS))] {
            let old = workspace_macro_binding_site("assert_eq", root, &index, roots, &resolved);
            let retained = syntax.workspace_macro_site("assert_eq", root, &index);
            println!("ORACLE_CASE={label} ROOT={root:?} INITIAL_COMPLETE={initially_complete} OLD={old:?} RETAINED={retained:?}");
            compared += 1;
            if old != retained { failures.push(format!("{label} at {root:?}: old={old:?}; retained={retained:?}")); }
            if label == "positive-global-only" { assert_eq!(old, retained, "positive setup control"); }
        }
        if label == "scoped-rest-definition" {
            let owner = index.functions().iter().find(|function| function.name == "gate" && function.file == Path::new(LIB));
            assert!(owner.is_some(), "owner setup");
            let Some(owner) = owner else { continue };
            let test = index.tests().iter().find(|test| test.file == Path::new(TESTS));
            assert!(test.is_some(), "test setup");
            let Some(test) = test else { continue };
            let assertion = test.assertions.first();
            assert!(assertion.is_some(), "assertion setup");
            let Some(assertion) = assertion else { continue };
            let probe = predicate_probe(owner, "10 <= value");
            let actual = syntax.equality_assertion_refusal(&probe, test, assertion, &index);
            let old_site = workspace_macro_binding_site("assert_eq", Some(Path::new(TESTS)), &index, roots, &resolved);
            let old_refusal = AssertionRefusal::MacroBinding { name: "assert_eq".to_string(), site: old_site };
            println!("PRODUCTION_REFUSAL={actual:?} ACTUAL_IS_ANALYZER_LIMIT={:?} OLD_ORACLE_IS_ANALYZER_LIMIT={}",actual.as_ref().map(AssertionRefusal::is_analyzer_limit),old_refusal.is_analyzer_limit());
        }
    }
    assert_eq!(compared, 12, "nonempty complete matrix");
    assert!(failures.is_empty(), "retained-site oracle parity failures: {failures:#?}");
}
