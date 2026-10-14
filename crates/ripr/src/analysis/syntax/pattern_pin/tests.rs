use super::*;

const MACRO: &str = r#"
macro_rules! assert_matches {
    ($value:expr, $pattern:pat) => {
        match $value {
            $pattern => {}
            other => panic!("{other:?} does not match {}", stringify!($pattern)),
        }
    };
}
"#;

fn file(macro_text: &str, tests: &str) -> String {
    format!(
        "pub fn parse_unit(raw: &str) -> Result<u32, ()> {{ Ok(raw.len() as u32) }}\n{macro_text}\n{tests}"
    )
}

/// Every admitted pin as (line, invocation text, value operand).
fn admitted(source: &str) -> Vec<(usize, String, String)> {
    let pins = pattern_pins(source);
    let mut found: Vec<_> = pins
        .tests
        .values()
        .flat_map(|test| {
            test.pins
                .iter()
                .map(|((line, text), pin)| (*line, text.clone(), pin.value.clone()))
        })
        .collect();
    found.sort();
    found
}

fn admitted_values(source: &str) -> Vec<String> {
    admitted(source)
        .into_iter()
        .map(|(_, _, value)| value)
        .collect()
}

#[test]
fn exact_pin_through_a_panicking_local_macro_is_admitted() -> Result<(), String> {
    let source = file(
        MACRO,
        "#[test]\nfn unknown_unit_is_rejected() {\n    assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));\n    assert_matches!(parse_unit(\"k\"), Ok(1_000));\n}\n",
    );
    let found = admitted(&source);
    assert_eq!(
        found
            .iter()
            .map(|(_, text, value)| (text.as_str(), value.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                "assert_matches!(parse_unit(\"q\"), Err(UnitError::Unknown));",
                "parse_unit(\"q\")"
            ),
            (
                "assert_matches!(parse_unit(\"k\"), Ok(1_000));",
                "parse_unit(\"k\")"
            ),
        ]
    );
    let pins = pattern_pins(&source);
    let test = pins.tests.values().next().ok_or("premise: one test")?;
    let pin = test.pins.values().next().ok_or("premise: one pin")?;
    assert_eq!(
        pin.macros,
        BTreeSet::from(["panic".to_string(), "stringify".to_string()])
    );
    // The lookup checks the body the index holds against the parsed one.
    let ((test_start, test_end, name), facts) =
        pins.tests.iter().next().ok_or("premise: one test")?;
    let ((line, text), _) = facts.pins.iter().next().ok_or("premise: one pin")?;
    assert!(
        pins.pin((*test_start, *test_end, name), &facts.body, (*line, text))
            .is_some()
    );
    assert!(
        pins.pin(
            (*test_start, *test_end, name),
            "fn stale() {}",
            (*line, text)
        )
        .is_none()
    );
    Ok(())
}

#[test]
fn any_delimiter_and_a_declared_trailing_comma_are_the_same_rule() {
    let trailing = MACRO.replace("$pattern:pat)", "$pattern:pat $(,)?)");
    let source = file(
        &trailing,
        "#[test]\nfn t() {\n    assert_matches![parse_unit(\"q\"), Err(E::A),];\n    assert_matches!{parse_unit(\"r\"), Err(E::A)}\n}\n",
    );
    assert_eq!(
        admitted_values(&source),
        vec!["parse_unit(\"q\")", "parse_unit(\"r\")"]
    );
    // Without `$(,)?` a trailing comma does not take the first rule.
    let source = file(
        MACRO,
        "#[test]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A),);\n}\n",
    );
    assert!(admitted(&source).is_empty());
}

#[test]
fn broad_patterns_are_not_exact_pins() {
    for pattern in [
        "Err(_)",
        "Err(..)",
        "Err(e)",
        "Ok(1..=3)",
        "Ok(1) | Ok(2)",
        "None",
        "Err(E { code: 1 })",
        "&Ok(1)",
    ] {
        let source = file(
            MACRO,
            &format!(
                "#[test]\nfn t() {{\n    assert_matches!(parse_unit(\"q\"), {pattern});\n}}\n"
            ),
        );
        assert!(admitted(&source).is_empty(), "{pattern} must not pin");
    }
}

#[test]
fn a_rule_that_does_not_panic_on_mismatch_is_not_trusted() {
    let tests = "#[test]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n";
    for macro_text in [
        // The first rule discards the value; an unused later rule panics.
        "macro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { let _ = $value; };\n    ($value:expr, $pattern:pat, $msg:expr) => { match $value { $pattern => {} _ => panic!($msg) } };\n}\n",
        // Mismatch arm does nothing.
        "macro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { match $value { $pattern => {} _ => {} } };\n}\n",
        // Mismatch arm only prints.
        "macro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { match $value { $pattern => {} _ => println!(\"no\") } };\n}\n",
        // The text names panic, but only in the matched arm.
        "macro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { match $value { $pattern => panic!(\"hit\"), _ => {} } };\n}\n",
        // The panic message can leave the test first.
        "macro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { match $value { $pattern => {} _ => panic!(\"{}\", { return; }) } };\n}\n",
        // A guard on the mismatch arm.
        "macro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { match $value { $pattern => {} other if true => panic!(\"no\"), _ => {} } };\n}\n",
        // The first rule takes something else.
        "macro_rules! assert_matches {\n    ($value:tt, $pattern:pat) => { match $value { $pattern => {} _ => panic!(\"no\") } };\n}\n",
        // An attribute may drop the definition.
        "#[cfg(feature = \"x\")]\nmacro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { match $value { $pattern => {} _ => panic!(\"no\") } };\n}\n",
    ] {
        let source = file(macro_text, tests);
        assert!(admitted(&source).is_empty(), "trusted: {macro_text}");
    }
    // The same text with the panicking rule first is trusted.
    let source = file(
        "macro_rules! assert_matches {\n    ($value:expr, $pattern:pat) => { match $value { $pattern => {} _ => panic!(\"no\") } };\n    ($value:expr, $pattern:pat, $msg:expr) => { let _ = $value; };\n}\n",
        tests,
    );
    assert_eq!(admitted_values(&source), vec!["parse_unit(\"q\")"]);
}

#[test]
fn an_invocation_another_rule_may_take_is_refused() {
    let source = file(
        MACRO,
        "#[test]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A), \"message\");\n}\n",
    );
    assert!(admitted(&source).is_empty());
}

#[test]
fn ambiguous_macro_bindings_refuse_every_pin() {
    let tests = "#[test]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n";
    for extra in [
        "use other::assert_matches;\n",
        "use other::thing as assert_matches;\n",
        "#[macro_use]\nextern crate other;\n",
        "generate!();\n",
        MACRO,
    ] {
        let source = file(&format!("{MACRO}{extra}"), tests);
        assert!(admitted(&source).is_empty(), "trusted beside: {extra}");
    }
}

#[test]
fn only_a_plain_test_with_an_unskipped_top_level_invocation_pins() -> Result<(), String> {
    let refused = [
        // After a `return`.
        "#[test]\nfn t() {\n    return;\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        // Inside a closure.
        "#[test]\nfn t() {\n    let check = || assert_matches!(parse_unit(\"q\"), Err(E::A));\n    check();\n}\n",
        // Inside a branch.
        "#[test]\nfn t() {\n    if flag() {\n        assert_matches!(parse_unit(\"q\"), Err(E::A));\n    }\n}\n",
        // After a loop.
        "#[test]\nfn t() {\n    for _ in 0..0 {}\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        // After a diverging macro.
        "#[test]\nfn t() {\n    panic!(\"early\");\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        // After an unknown macro.
        "#[test]\nfn t() {\n    setup!();\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        // After a `?`.
        "#[test]\nfn t() {\n    let value = parse_unit(\"x\").ok();\n    let _ = value?;\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        // A test that may not run, or whose outcome is settled.
        "#[test]\n#[should_panic]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        "#[test]\n#[ignore]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        // Not a test.
        "fn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
        // A value operand that may leave the test.
        "#[test]\nfn t() {\n    assert_matches!({ return; }, Err(E::A));\n}\n",
    ];
    for tests in refused {
        let source = file(MACRO, tests);
        assert!(admitted(&source).is_empty(), "admitted: {tests}");
    }
    // Allowlisted statements before the invocation keep it.
    let source = file(
        MACRO,
        "#[test]\nfn t() {\n    let input = \"q\";\n    assert_eq!(input.len(), 1);\n    assert!(matches!(parse_unit(\"k\"), Ok(_)));\n    assert_matches!(parse_unit(\"m\"), Ok(_));\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
    );
    let pins = pattern_pins(&source);
    let pin = pins
        .tests
        .values()
        .flat_map(|test| test.pins.values())
        .next()
        .ok_or("the allowlisted prefix must keep the pin")?;
    assert_eq!(pin.value, "parse_unit(\"q\")");
    assert!(pin.macros.contains("assert_eq") && pin.macros.contains("matches"));
    Ok(())
}

#[test]
fn a_definition_after_the_test_or_outside_its_module_does_not_bind() {
    let tests = "#[test]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n";
    let after = format!("pub fn parse_unit(raw: &str) -> u32 {{ 0 }}\n{tests}{MACRO}");
    assert!(admitted(&after).is_empty());
    let sibling = format!(
        "pub fn parse_unit(raw: &str) -> u32 {{ 0 }}\nmod helpers {{\n{MACRO}\n}}\nmod tests {{\n{tests}\n}}\n"
    );
    assert!(admitted(&sibling).is_empty());
    let enclosing = format!(
        "pub fn parse_unit(raw: &str) -> u32 {{ 0 }}\nmod tests {{\n{MACRO}\n{tests}\n}}\n"
    );
    assert_eq!(admitted_values(&enclosing), vec!["parse_unit(\"q\")"]);
}

#[test]
fn string_and_comment_text_never_defines_or_invokes_the_macro() {
    // A definition only in a comment binds nothing.
    let commented = "pub fn parse_unit(raw: &str) -> u32 { 0 }\n// macro_rules! assert_matches { ($v:expr, $p:pat) => { match $v { $p => {} _ => panic!() } } }\n#[test]\nfn t() {\n    assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n";
    assert!(admitted(commented).is_empty());
    // An invocation only in a string or a comment is no assertion.
    let quoted = file(
        MACRO,
        "#[test]\nfn t() {\n    let _ = \"assert_matches!(parse_unit(\\\"q\\\"), Err(E::A));\";\n    // assert_matches!(parse_unit(\"q\"), Err(E::A));\n}\n",
    );
    assert!(admitted(&quoted).is_empty());
}
