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
fn local_assertion_macro_call_earns_a_weak_oracle_never_the_inner_strength()
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
        vec![OracleStrength::Weak],
        "an in-body assert_eq! is strong; the wrapper is capped at weak"
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
