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
fn local_assertion_macro_call_earns_an_unknown_oracle_never_the_inner_strength()
-> Result<(), Box<dyn Error>> {
    let index = index_for(&module_with(
        "macro_rules! assert_gate {\n        ($x:expr, $want:expr) => {{\n            assert_eq!(gate($x), $want);\n        }};\n    }",
        "assert_gate!(11, true);",
    ))?;

    let test = test_named(&index, "probe")?;

    assert_eq!(test.assertions.len(), 1, "{:?}", assertion_texts(test));
    assert_eq!(test.assertions[0].text, "assert_gate!(11, true)");
    assert_eq!(
        strengths(&index, "probe")?,
        vec![OracleStrength::Unknown],
        "an in-body assert_eq! is strong; the wrapper is capped at unknown"
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
