// #9122: same-file `macro_rules!` assertion wrappers.
use super::{assertion_texts, index_for, index_for_files, test_named};
use crate::analysis::facts::{RustIndex, TestFact};
use crate::domain::OracleStrength;
use std::error::Error;

const GATE: &str = "pub fn gate(x: u32) -> bool {\n    x > 10\n}\n\n";

fn strengths(index: &RustIndex, name: &str) -> Result<Vec<OracleStrength>, Box<dyn Error>> {
    let test: &TestFact = test_named(index, name)?;
    Ok(test
        .assertions
        .iter()
        .map(|assertion| assertion.strength.clone())
        .collect())
}

fn module_with(definition: &str, test_body: &str) -> String {
    format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n    {definition}\n\n    #[test]\n    fn probe() {{\n        {test_body}\n    }}\n}}\n"
    )
}

#[test]
fn exactly_expandable_local_assertion_macro_earns_the_expanded_assertions_grade()
-> Result<(), Box<dyn Error>> {
    let index = index_for(&module_with(
        "macro_rules! assert_gate {\n        ($x:expr, $want:expr) => {{\n            assert_eq!(gate($x), $want);\n        }};\n    }",
        "assert_gate!(11, true);",
    ))?;

    let test = test_named(&index, "probe")?;

    assert_eq!(test.assertions.len(), 1, "{:?}", assertion_texts(test));
    assert_eq!(test.assertions[0].text, "assert_eq!(gate(11), true);");
    assert_eq!(strengths(&index, "probe")?, vec![OracleStrength::Strong]);
    assert!(
        test.body.contains("assert_eq!(gate(11), true);"),
        "the body now reads the expansion: {}",
        test.body
    );
    Ok(())
}

#[test]
fn local_assertion_macro_that_does_not_expand_exactly_stays_capped_at_unknown()
-> Result<(), Box<dyn Error>> {
    for (shape, definition, call) in [
        (
            "several arms",
            "($x:expr) => {{ assert_eq!(gate($x), true); }};\n        ($x:expr, $want:expr) => {{ assert_eq!(gate($x), $want); }};",
            "assert_gate!(11);",
        ),
        (
            "a non-expr fragment",
            "($x:literal) => {{ assert_eq!(gate($x), true); }};",
            "assert_gate!(11);",
        ),
        (
            "more than the assertion in the transcriber",
            "($x:expr) => {{ let y = 1; assert_eq!(gate($x), y); }};",
            "assert_gate!(11);",
        ),
        (
            "a transcriber that spans lines",
            "($x:expr) => {{ assert_eq!(\n            gate($x),\n            true\n        ); }};",
            "assert_gate!(11);",
        ),
        (
            "a tautology after substitution",
            "($x:expr, $y:expr) => {{ assert_eq!($x, $y); }};",
            "assert_gate!(gate(11), gate(11));",
        ),
    ] {
        let index = index_for(&module_with(
            &format!("macro_rules! assert_gate {{\n        {definition}\n    }}"),
            call,
        ))?;
        assert_eq!(
            strengths(&index, "probe")?,
            vec![OracleStrength::Unknown],
            "{shape}"
        );
    }
    Ok(())
}

#[test]
fn expansion_wraps_compound_arguments() -> Result<(), Box<dyn Error>> {
    let index = index_for(&module_with(
        "macro_rules! assert_gate {\n        ($x:expr, $want:expr) => { assert_eq!(gate($x), $want); };\n    }",
        "assert_gate!(a + b, true);",
    ))?;
    assert_eq!(
        assertion_texts(test_named(&index, "probe")?),
        vec!["assert_eq!(gate((a + b)), true);"]
    );
    Ok(())
}

#[test]
fn local_macros_without_assertion_evidence_grant_nothing() -> Result<(), Box<dyn Error>> {
    for (shape, definition, call) in [
        (
            "only logs",
            "($x:expr) => {{ println!(\"{}\", gate($x)); }};",
            "assert_gate!(3);",
        ),
        (
            "assertion only in a string and a comment",
            "($x:expr) => {{ // assert_eq!(gate($x), true);\n let _ = \"assert_eq!(1, 1)\"; }};",
            "assert_gate!(3);",
        ),
        (
            "one arm without evidence",
            "(1) => {{ assert_eq!(gate(1), true); }};\n        ($x:expr) => {{ let _ = gate($x); }};",
            "assert_gate!(3);",
        ),
        (
            "panic with no condition",
            "($x:expr) => {{ let _ = gate($x); panic!(\"always\"); }};",
            "assert_gate!(3);",
        ),
        (
            "hand-rolled panic check stays the opaque macro limit",
            "($x:expr) => {{ if gate($x) { panic!(\"open\"); } }};",
            "assert_gate!(3);",
        ),
        (
            "call inside a closure",
            "($x:expr) => {{ assert_eq!(gate($x), true); }};",
            "let run = || assert_gate!(11); run();",
        ),
        (
            "call under a branch",
            "($x:expr) => {{ assert_eq!(gate($x), true); }};",
            "if gate(1) { assert_gate!(11); }",
        ),
    ] {
        let index = index_for(&module_with(
            &format!("macro_rules! assert_gate {{\n        {definition}\n    }}"),
            call,
        ))?;
        let test = test_named(&index, "probe")?;
        assert!(
            test.assertions.is_empty(),
            "{shape}: {:?}",
            assertion_texts(test)
        );
    }
    Ok(())
}

#[test]
fn local_assertion_macro_outside_the_tests_file_or_shadowed_grants_nothing()
-> Result<(), Box<dyn Error>> {
    let definition = "macro_rules! assert_gate {\n    ($x:expr, $want:expr) => { assert_eq!(gate($x), $want); };\n}\n";
    // Defined in another file: the call cannot be attributed.
    let index = index_for_files(&[
        ("src/lib.rs", &module_with("", "assert_gate!(11, true);")),
        ("src/macros.rs", definition),
    ])?;
    assert!(test_named(&index, "probe")?.assertions.is_empty());
    // A `use` in the test may import a different macro of that name.
    let index = index_for(&module_with(
        definition,
        "use other::*;\n        assert_gate!(11, true);",
    ))?;
    assert!(test_named(&index, "probe")?.assertions.is_empty());
    Ok(())
}

#[test]
fn local_macro_reusing_a_recognized_assertion_name_keeps_the_recognized_grade()
-> Result<(), Box<dyn Error>> {
    let index = index_for(&module_with(
        "macro_rules! assert_matches {\n        ($e:expr, $p:pat) => { assert!(matches!($e, $p)); };\n    }",
        "assert_matches!(gate(11), true);",
    ))?;

    let test = test_named(&index, "probe")?;

    assert_eq!(test.assertions.len(), 1, "{:?}", assertion_texts(test));
    assert_eq!(
        test.assertions[0].text, "assert_matches!(gate(11), true);",
        "the ordinary extractor's fact stands; no wrapper fact is added"
    );
    Ok(())
}

#[test]
fn unattributable_local_macros_grant_nothing() -> Result<(), Box<dyn Error>> {
    for (shape, definition, call) in [
        (
            "assert!(true) tautology",
            "($x:expr) => {{ assert!(true); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "assert_eq!(1, 1) tautology",
            "($x:expr) => {{ assert_eq!(1, 1); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "fragment compared with itself",
            "($x:expr) => {{ assert_eq!($x, $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "assertion over no fragment of the arm",
            "($x:expr) => {{ assert_eq!(gate(11), true); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "one arm attributes, another asserts a constant",
            "(1) => {{ assert!(true); }};\n        ($x:expr) => {{ assert_eq!(gate($x), true); }};",
            "assert_gate!(3);",
        ),
        (
            "message argument carries the fragment, condition is constant",
            "($x:expr) => {{ assert!(true, \"{}\", $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "constant disjunction with a message fragment",
            "($x:expr) => {{ assert!(false || true, \"{}\", $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "fragment or true",
            "($x:expr) => {{ assert!($x || true); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "true or fragment",
            "($x:expr) => {{ assert!(true || $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "fragment equals itself",
            "($x:expr) => {{ assert!($x == $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "fragment equals its own clone",
            "($x:expr) => {{ assert_eq!($x.clone(), $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "equal literals with a message fragment",
            "($x:expr) => {{ assert_eq!(1, 1, \"{}\", $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "fragment only in the diagnostic message",
            "($x:expr) => {{ assert_eq!(gate(11), true, \"{}\", $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "fragment only in the assert! message",
            "($x:expr) => {{ assert!(gate(11), \"{}\", $x); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "parenthesized constant disjunction",
            "($x:expr) => {{ assert!((true) || ($x)); }};",
            "assert_gate!(gate(3));",
        ),
        (
            "parenthesized self-comparisons",
            "($x:expr, $y:expr) => {{ assert!(($x == $x) && ($y == $y)); }};",
            "assert_gate!(1, 2);",
        ),
        (
            "turbofish commas are not split",
            "($x:expr) => {{ assert_eq!(gate($x), parse::<A, B>()); }};",
            "assert_gate!(3);",
        ),
        (
            "assertion under a never-true branch",
            "($x:expr) => {{ if false {{ assert_eq!(gate($x), true); }} }};",
            "assert_gate!(3);",
        ),
        (
            "assertion under a loop",
            "($x:expr) => {{ for _ in 0..0 {{ assert_eq!(gate($x), true); }} }};",
            "assert_gate!(3);",
        ),
        (
            "assertion under a match",
            "($x:expr) => {{ match 0 {{ 1 => assert_eq!(gate($x), true), _ => {{}} }} }};",
            "assert_gate!(3);",
        ),
        (
            "assertion inside a closure",
            "($x:expr) => {{ let _f = || assert_eq!(gate($x), true); }};",
            "assert_gate!(3);",
        ),
        (
            "recursive macro",
            "($x:expr) => {{ assert_eq!(gate($x), true); assert_gate!($x); }};",
            "assert_gate!(3);",
        ),
        (
            "no arm takes the call's argument count",
            "($x:expr, $y:expr) => {{ assert_eq!(gate($x), $y); }};",
            "assert_gate!(3);",
        ),
        (
            "matcher binds no fragment",
            "() => {{ assert_eq!(gate(3), true); }};",
            "assert_gate!();",
        ),
    ] {
        let index = index_for(&module_with(
            &format!("macro_rules! assert_gate {{\n        {definition}\n    }}"),
            call,
        ))?;
        let test = test_named(&index, "probe")?;
        assert!(
            test.assertions.is_empty(),
            "{shape}: {:?}",
            assertion_texts(test)
        );
    }
    Ok(())
}

#[test]
fn macro_defined_inside_the_test_or_exported_twice_grants_nothing() -> Result<(), Box<dyn Error>> {
    let definition =
        "macro_rules! assert_gate {\n        ($x:expr) => { assert_eq!(gate($x), true); };\n    }";
    // Defined inside the test function: not at item position.
    let index = index_for(&module_with(
        "",
        &format!("{definition}\n        assert_gate!(11);"),
    ))?;
    assert!(test_named(&index, "probe")?.assertions.is_empty());
    // A second `#[macro_export]` definition of the name makes it ambiguous.
    let index = index_for(&module_with(
        &format!(
            "#[macro_export]\n    {definition}\n    mod other {{\n        #[macro_export]\n        macro_rules! assert_gate {{ ($x:expr) => {{}}; }}\n    }}"
        ),
        "assert_gate!(11);",
    ))?;
    assert!(test_named(&index, "probe")?.assertions.is_empty());
    Ok(())
}

#[test]
fn non_ascii_identifiers_in_an_attributed_macro_are_credited_once() -> Result<(), Box<dyn Error>> {
    for (shape, definition) in [
        (
            "non-ascii identifier in the condition",
            "($x:expr) => {{ assert!($x == résultat); }};",
        ),
        (
            "non-ascii identifier beside a constant disjunction operand",
            "($x:expr) => {{ assert!(gate($x) && résultat == résultat2); }};",
        ),
    ] {
        let index = index_for(&module_with(
            &format!("macro_rules! assert_gate {{\n        {definition}\n    }}"),
            "assert_gate!(11);",
        ))?;
        let test = test_named(&index, "probe")?;
        assert_eq!(test.assertions.len(), 1, "{shape}");
    }
    Ok(())
}

#[test]
fn macro_credit_survives_an_unrelated_assertion_on_the_same_line() -> Result<(), Box<dyn Error>> {
    let index = index_for(&module_with(
        "macro_rules! assert_gate {\n        ($x:expr) => { assert_eq!(gate($x), true); };\n    }",
        "assert_gate!(11); assert!(true);",
    ))?;
    let test = test_named(&index, "probe")?;
    assert!(
        test.assertions
            .iter()
            .any(|assertion| assertion.text.starts_with("assert_eq!(gate(11)")),
        "{:?}",
        assertion_texts(test)
    );
    Ok(())
}

#[test]
fn multi_attribute_cfg_gated_macro_grants_nothing() -> Result<(), Box<dyn Error>> {
    let index = index_for(&module_with(
        "#[allow(unused_macros)]\n    #[cfg(feature = \"never\")]\n    macro_rules! assert_gate {\n        ($x:expr) => { assert_eq!(gate($x), true); };\n    }",
        "assert_gate!(11);",
    ))?;
    assert!(test_named(&index, "probe")?.assertions.is_empty());
    Ok(())
}

#[test]
fn macro_call_nested_in_a_multiline_helper_call_is_not_credited() -> Result<(), Box<dyn Error>> {
    let index = index_for(&module_with(
        "macro_rules! assert_gate {\n        ($x:expr) => { assert_eq!(gate($x), true); };\n    }",
        "assert_x(\n            assert_gate!(11),\n        );",
    ))?;
    let test = test_named(&index, "probe")?;
    assert!(
        test.assertions
            .iter()
            .all(|assertion| !assertion.text.starts_with("assert_gate!")),
        "{:?}",
        assertion_texts(test)
    );
    Ok(())
}

#[test]
fn view_expands_several_calls_on_one_line_beside_non_ascii_text() {
    use super::super::expanded_assertion_view;
    let source = module_with(
        "macro_rules! assert_gate {\n        ($x:expr, $want:expr) => {{ assert_eq!(gate($x), $want); }};\n    }",
        "assert_gate!(11, true); assert_gate!(2, false); // héllo ✓",
    );
    let view = expanded_assertion_view(&source);
    assert!(
        view.contains("assert_eq!(gate(11), true); assert_eq!(gate(2), false); // héllo ✓"),
        "{view}"
    );
    assert_eq!(view.matches('\n').count(), source.matches('\n').count());
}
