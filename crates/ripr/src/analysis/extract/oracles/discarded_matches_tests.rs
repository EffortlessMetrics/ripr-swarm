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
    // The parser's supplemental line scanner admits fallible helpers, not
    // bare unwraps. Challenge each producer with an observer it actually owns.
    let helper_body = "let expected_match = matches!(\nvalue,\n2); ensure!(unrelated.is_ok());";
    for (multiline, observer_text) in [
        (extract_assertions(body, 10), "unrelated.unwrap();"),
        (
            extract_line_scanned_oracles(helper_body, 10),
            "ensure!(unrelated.is_ok());",
        ),
    ] {
        let [fact] = multiline.as_slice() else {
            return Err(format!(
                "expected one surviving multiline observer: {multiline:?}"
            ));
        };
        if fact.line != 12
            || fact.text != observer_text
            || fact.kind != OracleKind::SmokeOnly
            || fact.strength != OracleStrength::Smoke
            || !fact.observed_tokens.contains(&"unrelated".to_string())
            || fact.observed_tokens.contains(&"value".to_string())
            || fact.observed_tokens.contains(&"expected_match".to_string())
        {
            return Err(format!(
                "surviving observer lost its actual line or grip: {fact:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn repeated_matcher_text_in_type_trivia_cannot_move_the_observer() -> Result<(), String> {
    for body in [
        "let expect_match: (\n/* matches!(\nresult.unwrap(),\n2) */\nbool\n) = matches!(\nresult.unwrap(),\n2);",
        "let expect_match: (\n/* unrelated\ncomment\ntext */\nbool\n) = matches!(\nresult.unwrap(),\n2);",
    ] {
        let facts = extract_assertions(body, 10);
        let [fact] = facts.as_slice() else {
            return Err(format!("expected one actual scrutinee observer: {facts:?}"));
        };
        if fact.line != 16
            || fact.text != "result.unwrap();"
            || fact.kind != OracleKind::SmokeOnly
            || fact.strength != OracleStrength::Smoke
            || fact.observed_tokens != ["result"]
        {
            return Err(format!("repeated comment moved the observer: {fact:?}"));
        }
    }
    let diagnostic_only =
        "let expect_match: (\n/* matches!(\nresult.unwrap(),\n2) */\nbool\n) = matches!(value, 2);";
    let facts = extract_assertions(diagnostic_only, 10);
    if !facts.is_empty() {
        return Err(format!("comment-only observer received credit: {facts:?}"));
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

// SPEC0154 owns these assertion twins. The failure body's Err constructor
// cannot turn a scalar value pin into an error-variant pin.
struct TerminalMatcherTwin {
    guard: &'static str,
    assertion: &'static str,
    guard_sibling: usize,
    assertion_sibling: usize,
    kind: OracleKind,
    strength: OracleStrength,
}

const TERMINAL_MATCHER_TWINS: &[TerminalMatcherTwin] = &[
    TerminalMatcherTwin {
        guard: "if !matches!(value, 2) { return Err(()); }",
        assertion: "assert!(matches!(value, 2));",
        guard_sibling: 7,
        assertion_sibling: 7,
        kind: OracleKind::ExactValue,
        strength: OracleStrength::Strong,
    },
    TerminalMatcherTwin {
        guard: "if !matches!(\nvalue,\n2\n) {\nreturn Err(());\n}",
        assertion: "assert!(matches!(\nvalue,\n2\n));",
        guard_sibling: 12,
        assertion_sibling: 10,
        kind: OracleKind::ExactValue,
        strength: OracleStrength::Strong,
    },
    TerminalMatcherTwin {
        guard: "if !matches!(value, _) { return Err(()); }",
        assertion: "assert!(matches!(value, _));",
        guard_sibling: 7,
        assertion_sibling: 7,
        kind: OracleKind::RelationalCheck,
        strength: OracleStrength::Weak,
    },
    TerminalMatcherTwin {
        guard: "if !matches!(\nvalue,\n_\n) {\nreturn Err(());\n}",
        assertion: "assert!(matches!(\nvalue,\n_\n));",
        guard_sibling: 12,
        assertion_sibling: 10,
        kind: OracleKind::RelationalCheck,
        strength: OracleStrength::Weak,
    },
];

fn terminal_matcher_twin_source(statement: &str) -> String {
    format!(
        "fn score() -> i32 {{ 2 }}\nfn sibling() -> i32 {{ 7 }}\n\
         #[test]\nfn observes_score() -> Result<(), ()> {{\n\
         let value = score();\n{statement}\nassert_eq!(sibling(), 7);\nOk(())\n}}\n"
    )
}

fn check_terminal_matcher_twins(parsed_route: bool) -> Result<(), String> {
    for twin in TERMINAL_MATCHER_TWINS {
        for (statement, sibling_line) in [
            (twin.guard, twin.guard_sibling),
            (twin.assertion, twin.assertion_sibling),
        ] {
            let source = terminal_matcher_twin_source(statement);
            let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
            let [test] = parsed.tests.as_slice() else {
                return Err(format!("missing named control subject: {:?}", parsed.tests));
            };
            if test.name != "observes_score"
                || test.file != Path::new("src/lib.rs")
                || !test
                    .calls
                    .iter()
                    .any(|call| call.name == "score" && call.line == 5)
            {
                return Err(format!(
                    "control did not reach its real owner call: {test:?}"
                ));
            }
            let lexical = extract_assertions(&source, 1);
            let facts = if parsed_route {
                &test.assertions
            } else {
                &lexical
            };
            let [matcher, sibling] = facts.as_slice() else {
                return Err(format!(
                    "guard/assert twin lost or invented evidence (parsed={parsed_route}): {statement}: {facts:?}"
                ));
            };
            if matcher.line != 6
                || matcher.kind != twin.kind
                || matcher.strength != twin.strength
                || !matcher.observed_tokens.contains(&"value".to_string())
                || sibling.line != sibling_line
                || sibling.kind != OracleKind::ExactValue
                || sibling.strength != OracleStrength::Strong
                || !sibling.text.starts_with("assert_eq!(sibling()")
            {
                return Err(format!("guard/assert twin or sibling changed: {facts:?}"));
            }
        }
    }
    Ok(())
}

#[test]
fn lexical_terminal_matcher_guard_twins_preserve_multiline_and_sibling_coordinates()
-> Result<(), String> {
    check_terminal_matcher_twins(false)
}

#[test]
fn parsed_terminal_matcher_guard_twins_preserve_multiline_and_sibling_coordinates()
-> Result<(), String> {
    check_terminal_matcher_twins(true)
}

fn check_terminal_guard_boundary(parsed_route: bool, literal_brace: bool) -> Result<(), String> {
    let (declaration, setup, assertion, guard, guard_sibling, call_line) = if literal_brace {
        (
            "fn score(_: &str) -> i32 { 2 }",
            "let _setup = 0;",
            "assert!(matches!(score(\"{\"), _));",
            "if !matches!(score(\"{\"), _) { return Err(()); }",
            7,
            6,
        )
    } else {
        (
            "fn score() -> i32 { 2 }",
            "let value = score();",
            "assert!(matches!(\nvalue,\n_\n));",
            "if !matches!(\nvalue,\n_\n)\n{\nreturn Err(());\n}",
            13,
            5,
        )
    };
    // The accepted #5410 wildcard contract independently fixes this meaning.
    // Check the genuine assertion before the guard, then require the same grip.
    for (statement, sibling_line) in [
        (assertion, if literal_brace { 7 } else { 10 }),
        (guard, guard_sibling),
    ] {
        let source = format!(
            "{declaration}\nfn sibling() -> i32 {{ 7 }}\n#[test]\n\
             fn observes_score() -> Result<(), ()> {{\n{setup}\n{statement}\n\
             assert_eq!(sibling(), 7);\nOk(())\n}}\n"
        );
        let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let [test] = parsed.tests.as_slice() else {
            return Err("guard-boundary control lost its named subject".to_string());
        };
        if test.name != "observes_score"
            || test.file != Path::new("src/lib.rs")
            || !test
                .calls
                .iter()
                .any(|call| call.name == "score" && call.line == call_line)
        {
            return Err(format!("guard-boundary owner call not reached: {test:?}"));
        }
        let lexical = extract_assertions(&source, 1);
        let facts = if parsed_route {
            &test.assertions
        } else {
            &lexical
        };
        let [matcher, sibling] = facts.as_slice() else {
            return Err(format!(
                "guard-boundary evidence missing (parsed={parsed_route}, literal={literal_brace}): {statement}: {facts:?}"
            ));
        };
        if matcher.line != 6
            || matcher.kind != OracleKind::RelationalCheck
            || matcher.strength != OracleStrength::Weak
            || !matcher
                .observed_tokens
                .contains(&if literal_brace { "score" } else { "value" }.to_string())
            || (literal_brace && !matcher.text.contains("score(\"{\")"))
            || sibling.line != sibling_line
            || sibling.kind != OracleKind::ExactValue
            || sibling.strength != OracleStrength::Strong
            || sibling.text.trim_end_matches(';') != "assert_eq!(sibling(), 7)"
            || !sibling.observed_tokens.contains(&"sibling".to_string())
        {
            return Err(format!(
                "guard-boundary twin grip or sibling changed (parsed={parsed_route}, literal={literal_brace}): {facts:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn lexical_terminal_guard_literal_brace_keeps_wildcard_twin() -> Result<(), String> {
    check_terminal_guard_boundary(false, true)
}

#[test]
fn parsed_terminal_guard_literal_brace_keeps_wildcard_twin() -> Result<(), String> {
    check_terminal_guard_boundary(true, true)
}

#[test]
fn lexical_terminal_guard_later_body_brace_keeps_twin_and_sibling() -> Result<(), String> {
    check_terminal_guard_boundary(false, false)
}

#[test]
fn parsed_terminal_guard_later_body_brace_keeps_twin_and_sibling() -> Result<(), String> {
    check_terminal_guard_boundary(true, false)
}

#[test]
fn opaque_and_unnegated_terminal_matcher_guards_do_not_gain_credit() -> Result<(), String> {
    for statement in [
        "if matches!(value, 2) { return Err(()); }",
        "if matches!(\nvalue,\n2\n) {\nreturn Err(());\n}",
        "if opaque(value) { return Err(()); }",
    ] {
        let source = terminal_matcher_twin_source(statement);
        let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let [test] = parsed.tests.as_slice() else {
            return Err("opaque control subject missing".to_string());
        };
        let lexical = extract_assertions(&source, 1);
        for facts in [&lexical, &test.assertions] {
            let [sibling] = facts.as_slice() else {
                return Err(format!(
                    "opaque guard received credit: {statement}: {facts:?}"
                ));
            };
            if !sibling.text.starts_with("assert_eq!(sibling()") {
                return Err(format!(
                    "opaque guard displaced the live sibling: {sibling:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn lexical_terminal_matcher_owns_condition_continuations_without_owning_sibling_assertions()
-> Result<(), String> {
    // The binding belongs to the enclosing test. Its expect_ spelling must
    // not turn a continuation operand into a second assertion fact.
    for (body, sibling_line) in [
        (
            "if !matches!(\nexpect_value,\n2\n) {\nreturn Err(());\n}\nassert_eq!(sibling(), 7);",
            16,
        ),
        (
            "if !matches!(\nexpect_value,\n2\n) { return Err(()); } assert_eq!(sibling(), 7);",
            13,
        ),
        (
            "if !matches!(\nexpect_value,\n2\n) { return Err(()) } assert_eq!(sibling(), 7);",
            13,
        ),
        (
            "if !matches!(\nexpect_value,\n2\n) { return Err({ let _ = \" ); } assert!(phantom())\"; () }); } assert_eq!(sibling(), 7);",
            13,
        ),
    ] {
        let source = format!(
            "fn score() -> i32 {{ 2 }}\nfn sibling() -> i32 {{ 7 }}\n\
         #[test]\nfn observes_score() -> Result<(), ()> {{\n\
         let expect_value = score();\n{body}\nOk(())\n}}\n"
        );
        let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let [test] = parsed.tests.as_slice() else {
            return Err("condition-ownership control subject missing".to_string());
        };
        if test.name != "observes_score"
            || !test
                .calls
                .iter()
                .any(|call| call.name == "score" && call.line == 5)
        {
            return Err(format!(
                "condition-ownership control lost its owner call: {test:?}"
            ));
        }
        let facts = extract_assertions(body, 10);
        let [guard, sibling] = facts.as_slice() else {
            return Err(format!(
                "condition continuation received separate credit: {facts:?}"
            ));
        };
        if guard.line != 10
            || guard.kind != OracleKind::ExactValue
            || guard.strength != OracleStrength::Strong
            || !guard.observed_tokens.contains(&"expect_value".to_string())
            || sibling.line != sibling_line
            || !sibling.text.starts_with("assert_eq!(sibling()")
            || sibling.kind != OracleKind::ExactValue
            || sibling.strength != OracleStrength::Strong
            || !sibling.observed_tokens.contains(&"sibling".to_string())
            || sibling
                .observed_tokens
                .contains(&"expect_value".to_string())
            || sibling.observed_tokens.contains(&"phantom".to_string())
            || sibling.observed_tokens.contains(&"Err".to_string())
        {
            return Err(format!(
                "condition/sibling ownership or coordinates changed: {facts:?}"
            ));
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
    assert!(wrong.is_err());
}

#[test]
fn terminal_guard_balanced_scrutinee_keeps_its_same_row_sibling() -> Result<(), String> {
    // A block scrutinee cannot own the guard body or its following observer.
    for (statement, sibling_line) in [
        (
            "assert!(matches!({ score() }, 2));\nassert_eq!(sibling(), 7);",
            7,
        ),
        (
            "if !matches!({ score() }, 2) { return Err(()); } assert_eq!(sibling(), 7);",
            6,
        ),
    ] {
        let source = format!(
            "fn score() -> i32 {{ 2 }}\nfn sibling() -> i32 {{ 7 }}\n#[test]\n\
             fn observes_score() -> Result<(), ()> {{\nlet _setup = 0;\n{statement}\nOk(())\n}}\n"
        );
        let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let [test] = parsed.tests.as_slice() else {
            return Err("balanced guard lost its named subject".to_string());
        };
        if test.name != "observes_score"
            || test.file != Path::new("src/lib.rs")
            || !test
                .calls
                .iter()
                .any(|call| call.name == "score" && call.line == 6)
            || !test
                .calls
                .iter()
                .any(|call| call.name == "sibling" && call.line == sibling_line)
        {
            return Err(format!(
                "balanced guard owner/sibling calls missing: {test:?}"
            ));
        }
        let lexical = extract_assertions(&source, 1);
        for facts in [&lexical, &test.assertions] {
            // Same-row lexical sorting and parsed traversal can differ.
            if facts.len() != 2 {
                return Err(format!(
                    "balanced guard lost or invented evidence: {facts:?}"
                ));
            }
            let matcher = facts
                .iter()
                .find(|fact| fact.observed_tokens.contains(&"score".to_string()))
                .ok_or("balanced guard lost its actual score observer")?;
            let sibling = facts
                .iter()
                .find(|fact| fact.text.trim_end_matches(';') == "assert_eq!(sibling(), 7)")
                .ok_or("balanced guard swallowed its actual sibling")?;
            if matcher.line != 6
                || matcher.kind != OracleKind::ExactValue
                || matcher.strength != OracleStrength::Strong
                || !matcher.text.contains("matches!({ score() }, 2)")
                || sibling.line != sibling_line
                || sibling.kind != OracleKind::ExactValue
                || sibling.strength != OracleStrength::Strong
                || !sibling.observed_tokens.contains(&"sibling".to_string())
                || sibling.observed_tokens.contains(&"score".to_string())
                || sibling.observed_tokens.contains(&"Err".to_string())
            {
                return Err(format!(
                    "balanced guard grip or sibling ownership changed: {facts:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn wrapped_discarded_matchers_cannot_borrow_sibling_observers() -> Result<(), String> {
    for computation in [
        "let held = { matches!(value, 2) };",
        "held = ({ core::matches!(value, 2) });",
        "let held = {\n{ matches!(value, 2) }\n};",
        "let held = identity(matches!(value, 2));",
    ] {
        for helper_route in [false, true] {
            let observer = if helper_route {
                "ensure!(unrelated.is_ok());"
            } else {
                "assert_eq!(unrelated, 7);"
            };
            let body = format!("{computation} {observer}");
            let facts = if helper_route {
                extract_line_scanned_oracles(&body, 10)
            } else {
                extract_assertions(&body, 10)
            };
            let [fact] = facts.as_slice() else {
                return Err(format!(
                    "wrapped computation lost its sibling: {body}: {facts:?}"
                ));
            };
            let (kind, strength) = if helper_route {
                (OracleKind::SmokeOnly, OracleStrength::Smoke)
            } else {
                (OracleKind::ExactValue, OracleStrength::Strong)
            };
            if fact.line != 10 + computation.matches('\n').count()
                || fact.text != observer
                || fact.kind != kind
                || fact.strength != strength
                || (helper_route && fact.observed_tokens != ["ensure", "unrelated"])
                || (!helper_route && fact.observed_tokens != ["unrelated"])
            {
                return Err(format!(
                    "wrapped matcher contaminated sibling grip: {fact:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn wrapped_discarded_matchers_retain_actual_scrutinee_observers() -> Result<(), String> {
    for body in [
        "let held = { matches!({ assert_eq!(value, 2); value }, 2) };",
        "let held = ({\n{ matches!({\nassert_eq!(value, 2); value\n}, 2) }\n});",
    ] {
        let facts = extract_assertions(body, 10);
        let [fact] = facts.as_slice() else {
            return Err(format!(
                "pure block wrapper lost actual observer: {facts:?}"
            ));
        };
        let expected_line = 10
            + body[..body.find("assert_eq!").ok_or("missing stimulus")?]
                .matches('\n')
                .count();
        if fact.line != expected_line
            || !fact.text.starts_with("assert_eq!(value, 2)")
            || fact.text.contains("matches!")
            || fact.kind != OracleKind::ExactValue
            || fact.strength != OracleStrength::Strong
            || fact.observed_tokens != ["value"]
        {
            return Err(format!(
                "wrapped actual observer lost its own grip: {fact:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn terminal_block_matcher_guards_equal_their_assertion_twins() -> Result<(), String> {
    for (pattern, kind, strength) in [
        ("2", OracleKind::ExactValue, OracleStrength::Strong),
        ("_", OracleKind::RelationalCheck, OracleStrength::Weak),
    ] {
        for statement in [
            format!("assert!(matches!(value, {pattern}));"),
            format!("assert!({{ matches!(value, {pattern}) }});"),
            format!("if !{{ matches!(value, {pattern}) }} {{ return Err(()); }}"),
        ] {
            let source = terminal_matcher_twin_source(&statement);
            let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
            let [test] = parsed.tests.as_slice() else {
                return Err("terminal failure control lost its named subject".to_string());
            };
            if test.name != "observes_score"
                || !test
                    .calls
                    .iter()
                    .any(|call| call.name == "score" && call.line == 5)
            {
                return Err(format!(
                    "terminal control did not reach its owner: {test:?}"
                ));
            }
            let lexical = extract_assertions(&source, 1);
            for facts in [&lexical, &test.assertions] {
                let [matcher, sibling] = facts.as_slice() else {
                    return Err(format!(
                        "terminal failure/twin evidence missing: {statement}: {facts:?}"
                    ));
                };
                if matcher.line != 6
                    || matcher.kind != kind
                    || matcher.strength != strength
                    || matcher.observed_tokens != ["matches", "value"]
                    || sibling.line != 7
                    || sibling.kind != OracleKind::ExactValue
                    || sibling.strength != OracleStrength::Strong
                    || sibling.observed_tokens != ["sibling"]
                {
                    return Err(format!(
                        "terminal failure/twin or sibling grip changed: {facts:?}"
                    ));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn nonfirst_quoted_and_recovered_failure_macros_cannot_pin_a_matcher() -> Result<(), String> {
    for statement in [
        "if !matches!(value, 2) { let _message = \"panic!(bad)\"; }",
        "if !matches!(value, 2) { let _setup = 0; panic!(\"bad\"); }",
        "if !matches!(value, 2) { recover(bail!(\"bad\")); }",
        "if !matches!(value, 2) { /* panic!(bad); */ let _setup = 0; }",
        "if !matches!(value, 2) { bail!(\"unresolved custom macro\"); }",
        "if !matches!(value, 2) { panic!(\"unresolved macro authority\"); }",
        "macro_rules! panic { ($message:expr) => {{ let _ = $message; }}; } if !matches!(value, 2) { panic!(\"diagnostic only\"); }",
        "macro_rules! bail { ($message:expr) => {{ let _ = $message; }}; } if !matches!(value, 2) { bail!(\"diagnostic only\"); }",
    ] {
        let source = terminal_matcher_twin_source(statement);
        let parsed = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), &source)?;
        let [test] = parsed.tests.as_slice() else {
            return Err("terminal negative lost its named subject".to_string());
        };
        let lexical = extract_assertions(&source, 1);
        for facts in [&lexical, &test.assertions] {
            let [sibling] = facts.as_slice() else {
                return Err(format!(
                    "nonterminal failure received matcher credit: {statement}: {facts:?}"
                ));
            };
            if sibling.line != 7 || sibling.observed_tokens != ["sibling"] {
                return Err(format!(
                    "terminal negative lost its real sibling: {sibling:?}"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn wrapped_discarded_boolean_and_consumed_twin_have_independent_runtime_controls()
-> Result<(), String> {
    fn discard(value: i32) {
        let _held = { matches!(value, 2) };
        let unrelated = 7;
        assert_eq!(unrelated, 7);
    }
    macro_rules! bail {
        ($message:expr) => {
            return Err($message)
        };
    }
    fn consumed(value: i32) -> Result<(), &'static str> {
        if !matches!(value, 2) {
            bail!("bad");
        }
        Ok(())
    }
    fn nondiverging_custom_bail(value: i32) {
        macro_rules! bail {
            ($message:expr) => {{
                let _ = $message;
            }};
        }
        if !matches!(value, 2) {
            bail!("diagnostic only");
        }
    }
    for value in [2, 3] {
        if std::panic::catch_unwind(|| discard(value)).is_err() {
            return Err("discarded boolean unexpectedly failed".to_string());
        }
        if std::panic::catch_unwind(|| nondiverging_custom_bail(value)).is_err() {
            return Err("custom bail name incorrectly implied divergence".to_string());
        }
        let wildcard_guard = || -> Result<(), &'static str> {
            if !{ matches!(value, _) } {
                return Err("wildcard rejected a value");
            }
            Ok(())
        };
        if wildcard_guard().is_err() {
            return Err("a whole wildcard block unexpectedly discriminated".to_string());
        }
    }
    if consumed(2).is_err() || consumed(3).is_ok() {
        return Err("resolved bail control did not discriminate 2 from 3".to_string());
    }
    let original_value = 2;
    let original = std::panic::catch_unwind(|| assert!(matches!(original_value, 2)));
    let wrong_value = 3;
    let wrong = std::panic::catch_unwind(|| assert!(matches!(wrong_value, 2)));
    if original.is_err() || wrong.is_ok() {
        return Err("consumed assertion twin did not discriminate 2 from 3".to_string());
    }
    Ok(())
}

#[test]
fn nonmatcher_blocks_keep_their_actual_observer_coordinates() -> Result<(), String> {
    for helper_route in [false, true] {
        let observer = if helper_route {
            "ensure!(unrelated.is_ok());"
        } else {
            "assert_eq!(value, 2);"
        };
        let body = format!("let held = {{\n{observer}\nfalse\n}};");
        let facts = if helper_route {
            extract_line_scanned_oracles(&body, 10)
        } else {
            extract_assertions(&body, 10)
        };
        let [fact] = facts.as_slice() else {
            return Err(format!(
                "nonmatcher block lost its actual observer: {facts:?}"
            ));
        };
        let expected_tokens = if helper_route {
            vec!["ensure", "unrelated"]
        } else {
            vec!["value"]
        };
        if fact.line != 11 || fact.text != observer || fact.observed_tokens != expected_tokens {
            return Err(format!(
                "nonmatcher block shifted or contaminated its observer: {fact:?}"
            ));
        }
    }
    Ok(())
}
