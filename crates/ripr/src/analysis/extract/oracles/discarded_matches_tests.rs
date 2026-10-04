use super::{extract_assertions, extract_line_scanned_oracles};
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::{OracleKind, OracleStrength};
use std::path::Path;

const DISCARDED: &[&str] = &[
    "matches!(value, _);",
    "matches!(value, 2);",
    "matches!(value, _ if value == 2);",
    "std::matches!(value, 2);",
    "let matched = matches!(value, 2);",
    "let _ = matches!(value, 2);",
    "matches!(expected_value, 2);",
    "let expected_match = matches!(value, 2);",
    "let expected_match: bool = (core::matches! { value, 2 });",
    "let expected_match = matches!(\nvalue,\n2\n);",
    "matches!(expected_metric_value, 2);",
    "expected_match = matches!(value, 2);",
    "let /* binder */ mut expected_match: bool = matches!(value, 2);",
    "matches!(matches!(expected_value, 2), true);",
    "matches!(value, 2); // assert_eq!(unrelated, 2)",
    "let expected_match = { matches!(value, 2) };",
    "expected_match = { core::matches!(value, 2) };",
    "matches!(expect_value, 2);",
    "let expect_match = matches!(value, 2);",
];

#[test]
fn discarded_matches_are_not_lexical_oracles() -> Result<(), String> {
    for statement in DISCARDED {
        let facts = extract_assertions(statement, 10);
        if !facts.is_empty() {
            return Err(format!(
                "discarded computation became an oracle: {statement}: {facts:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn deeply_nested_discarded_matchers_fail_closed() -> Result<(), String> {
    let nested = format!("{}value{}", "matches!(".repeat(1000), ", 2)".repeat(1000));
    let body = format!("let expected_match = {nested};");
    let facts = extract_assertions(&body, 1);
    if !facts.is_empty() {
        return Err(format!(
            "deep discarded computation became an oracle: {facts:?}"
        ));
    }
    Ok(())
}

#[test]
fn discarded_matchers_cannot_supply_an_unrelated_observers_pattern() -> Result<(), String> {
    for (statement, kind, strength) in [
        (
            "matches!(value, 2); unrelated.unwrap();",
            OracleKind::SmokeOnly,
            OracleStrength::Smoke,
        ),
        (
            "let expected_match = matches!(value, 2); unrelated.expect(\"present\");",
            OracleKind::SmokeOnly,
            OracleStrength::Smoke,
        ),
        (
            "matches!(value, 2); assert!(true);",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        ),
        (
            "matches!(value, 2); insta::assert_snapshot!(unrelated);",
            OracleKind::Snapshot,
            OracleStrength::Medium,
        ),
        (
            "matches!(result.unwrap(), 2);",
            OracleKind::SmokeOnly,
            OracleStrength::Smoke,
        ),
        (
            "matches!({ matches!(value, 2); unrelated.unwrap() }, 3);",
            OracleKind::SmokeOnly,
            OracleStrength::Smoke,
        ),
    ] {
        let facts = extract_assertions(statement, 10);
        let [fact] = facts.as_slice() else {
            return Err(format!(
                "expected the actual observer: {statement}: {facts:?}"
            ));
        };
        if fact.kind != kind
            || fact.strength != strength
            || fact.text.contains("matches!")
            || fact.observed_tokens.contains(&"value".to_string())
            || fact.observed_tokens.contains(&"expected_match".to_string())
        {
            return Err(format!(
                "discarded pattern contaminated observer: {statement}: {fact:?}"
            ));
        }
    }
    let body = "let expected_match = matches!(\nvalue,\n2); unrelated.unwrap();";
    for multiline in [
        extract_assertions(body, 10),
        extract_line_scanned_oracles(body, 10),
    ] {
        let [fact] = multiline.as_slice() else {
            return Err(format!(
                "expected one surviving multiline observer: {multiline:?}"
            ));
        };
        if fact.line != 12
            || fact.text != "unrelated.unwrap();"
            || fact.kind != OracleKind::SmokeOnly
            || fact.strength != OracleStrength::Smoke
            || fact.observed_tokens != ["unrelated"]
        {
            return Err(format!(
                "surviving observer lost its actual line or grip: {fact:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn discarded_matches_in_a_parsed_owner_test_are_not_oracles() -> Result<(), String> {
    for statement in DISCARDED {
        let source = format!(
            "pub fn score(value: i32) -> i32 {{ value + 2 }}\n\
             #[test]\nfn observes_score() {{\n\
             let value = score(1);\n{statement}\n}}\n"
        );
        let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let [test] = facts.tests.as_slice() else {
            return Err(format!(
                "expected one actual parsed test: {:?}",
                facts.tests
            ));
        };
        if test.name != "observes_score" || !test.assertions.is_empty() {
            return Err(format!(
                "discarded matcher received parsed credit: {statement}: {test:?}"
            ));
        }
        if !facts
            .functions
            .iter()
            .any(|function| function.name == "score")
        {
            return Err("fixture did not parse the changed owner".to_string());
        }
    }
    Ok(())
}

#[test]
fn asserting_wrappers_keep_consumed_pattern_oracles() -> Result<(), String> {
    for (statement, kind, strength) in [
        (
            "assert!(matches!(value, 2));",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        ),
        (
            "assert!(matches!(value, _ if value == 2));",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        ),
        (
            "assert_matches!(value, 2);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        ),
        (
            "ensure!(matches!(value, 2));",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        ),
        (
            "assert!(matches!(value, _));",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        ),
    ] {
        let lexical = extract_assertions(statement, 4);
        let source =
            format!("#[test]\nfn observes_score() {{\nlet value = score(1);\n{statement}\n}}\n");
        let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let [test] = parsed.tests.as_slice() else {
            return Err("retained control must contain one parsed test".to_string());
        };
        for facts in [&lexical, &test.assertions] {
            let [fact] = facts.as_slice() else {
                return Err(format!(
                    "expected one actual consumed oracle: {statement}: {facts:?}"
                ));
            };
            if fact.kind != kind || fact.strength != strength {
                return Err(format!("consumed assertion changed: {statement}: {fact:?}"));
            }
        }
    }
    Ok(())
}

#[test]
fn consumed_matcher_err_return_guard_keeps_its_exact_oracle() -> Result<(), String> {
    let source = r#"
#[test]
fn observes_score() -> Result<(), ()> {
    let expected_value = score(1);
    if !matches!(expected_value, 2) {
        return Err(());
    }
    Ok(())
}
"#;
    let lexical = extract_assertions(source, 1);
    let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), source)?;
    let [test] = parsed.tests.as_slice() else {
        return Err("terminal Err guard must contain one parsed test".to_string());
    };
    for facts in [&lexical, &test.assertions] {
        let [fact] = facts.as_slice() else {
            return Err(format!("expected one terminal Err guard: {facts:?}"));
        };
        if fact.kind != OracleKind::ExactValue || fact.strength != OracleStrength::Strong {
            return Err(format!("consumed matcher Err guard changed: {fact:?}"));
        }
    }
    Ok(())
}

#[test]
fn consumed_matcher_failure_guards_keep_their_result_oracle() -> Result<(), String> {
    let source = r#"
#[test]
fn pin_conditioned_panic() {
    match parse(input) {
        Ok(value) => assert_eq!(value, 1),
        Err(error) => {
            if !matches!(
                error.downcast_ref::<ParseError>(),
                Some(ParseError::InvalidData)
            ) {
                panic!("unexpected error: {error}");
            }
        }
    }
}
"#;
    let lexical = extract_assertions(source, 1);
    let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), source)?;
    let [test] = parsed.tests.as_slice() else {
        return Err("retained guard must contain one parsed test".to_string());
    };
    for facts in [&lexical, &test.assertions] {
        let guarded: Vec<_> = facts
            .iter()
            .filter(|fact| fact.kind == OracleKind::GuardedResultMatch)
            .collect();
        let [fact] = guarded.as_slice() else {
            return Err(format!("expected one consumed Result guard: {facts:?}"));
        };
        if fact.strength != OracleStrength::Strong || !fact.text.contains("ParseError::InvalidData")
        {
            return Err(format!("consumed Result guard changed: {fact:?}"));
        }
    }
    Ok(())
}

#[test]
fn asserted_bound_matcher_keeps_the_actual_assertion() -> Result<(), String> {
    let body = "let matched = matches!(value, 2);\nassert!(matched);";
    let lexical = extract_assertions(body, 4);
    let source = format!("#[test]\nfn observes_score() {{\nlet value = score(1);\n{body}\n}}\n");
    let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
    let [test] = parsed.tests.as_slice() else {
        return Err("bound control must contain one parsed test".to_string());
    };
    for facts in [&lexical, &test.assertions] {
        let [fact] = facts.as_slice() else {
            return Err(format!("expected only the actual assertion: {facts:?}"));
        };
        if !fact.text.starts_with("assert!(matched)") {
            return Err(format!("credited the bound computation instead: {fact:?}"));
        }
    }
    Ok(())
}

#[test]
fn matcher_computation_and_asserted_result_have_different_runtime_grip() {
    fn score(value: i32) -> i32 {
        value + 1
    }
    fn discard(value: i32) {
        let _matched = matches!(value, 2);
    }
    let original_value = score(1);
    let wrong_value = score(2);
    discard(original_value);
    discard(wrong_value);
    assert!(matches!(original_value, 2));
    assert!(!matches!(wrong_value, 2));
    let original = std::panic::catch_unwind(|| assert!(matches!(original_value, 2)));
    let wrong = std::panic::catch_unwind(|| assert!(matches!(wrong_value, 2)));
    assert!(matches!(original, Ok(())));
    assert!(matches!(wrong, Err(_)));
}
