use super::*;

fn check_owner_pin(support: &str, test: &str, expected: bool) -> Result<(), String> {
    let source = format!(
        "#[derive(Debug, PartialEq)] pub enum Fault {{ Odd, Bad }}\n\
         pub fn parse(text: &str) -> Result<u8, Fault> {{\n\
         if text.len() % 2 != 0 {{ return Err(Fault::Odd); }}\nOk(1)\n}}\n\
         pub fn parse_other(_: &str) -> Result<u8, Fault> {{ Err(Fault::Odd) }}\n\
         {support}\n#[cfg(test)] mod tests {{ use super::*; {test} }}\n"
    );
    check_source(&source, expected).map(|_| ())
}

fn check_source(source: &str, expected: bool) -> Result<TestGripEvidence, String> {
    check_files(&[(PathBuf::from("src/lib.rs"), source)], expected)
}

fn check_files(files: &[(PathBuf, &str)], expected: bool) -> Result<TestGripEvidence, String> {
    check_files_for_owner(files, expected, "src/lib.rs::parse")
}

fn check_files_for_owner(
    files: &[(PathBuf, &str)],
    expected: bool,
    owner: &str,
) -> Result<TestGripEvidence, String> {
    let index = index_from_files(files)?;
    let seams = inventory_seams_from_index(&[PathBuf::from("src/lib.rs")], &index);
    let seam = seams
        .iter()
        .find(|seam| {
            seam.owner() == owner
                && seam.kind() == SeamKind::ErrorVariant
                && seam.expression().contains("Odd")
        })
        .ok_or("missing selected parse error seam")?;
    let full = evidence_for_seam(seam, &index);
    let compact = compact_evidence_for_seam(seam, &CompactGripContext::new(&index));
    let readings = |e: &TestGripEvidence| {
        [
            e.reach.state.clone(),
            e.activate.state.clone(),
            e.propagate.state.clone(),
            e.observe.state.clone(),
            e.discriminate.state.clone(),
        ]
    };
    println!(
        "owner={} full={:?} compact={:?}",
        seam.owner(),
        readings(&full),
        readings(&compact)
    );
    assert_eq!(readings(&full), readings(&compact));
    let class = crate::analysis::seam_classification::classify_seam(seam, &full);
    println!("classified={class:?}");
    assert_eq!(
        full.discriminate.state == StageState::Yes,
        expected,
        "{files:?}"
    );
    if !expected {
        assert_ne!(
            crate::analysis::seam_classification::classify_seam(seam, &full),
            SeamGripClass::StronglyGripped
        );
    }
    Ok(full)
}

#[test]
fn error_variant_owner_original_other_owner_is_not_a_pin() -> Result<(), String> {
    check_owner_pin(
        "",
        "#[test] fn odd() { assert_eq!(parse_other(\"1\").unwrap_err(), Fault::Odd); }",
        false,
    )
}

#[test]
fn error_variant_owner_discarded_owner_does_not_lend_its_pin() -> Result<(), String> {
    check_owner_pin(
        "",
        "#[test] fn odd() { let _ = parse(\"1\"); assert_eq!(parse_other(\"1\").unwrap_err(), Fault::Odd); }",
        false,
    )
}

#[test]
fn error_variant_owner_direct_pin_retained() -> Result<(), String> {
    check_owner_pin(
        "",
        "#[test] fn odd() { assert_eq!(parse(\"1\").unwrap_err(), Fault::Odd); }",
        true,
    )
}

#[test]
fn error_variant_owner_production_entry_receipt_retained() -> Result<(), String> {
    check_owner_pin(
        "fn helper(s: &str) -> Result<u8, Fault> { parse(s) }\nfn entry(s: &str) -> Result<u8, Fault> { helper(s) }",
        "#[test] fn odd() { assert_eq!(entry(\"1\").unwrap_err(), Fault::Odd); }",
        true,
    )
}

#[test]
fn error_variant_owner_authenticated_test_helper_receipt_retained() -> Result<(), String> {
    check_owner_pin(
        "",
        "fn entry(s: &str) -> Result<u8, Fault> { parse(s) }\n#[test] fn odd() { assert_eq!(entry(\"1\").unwrap_err(), Fault::Odd); }",
        true,
    )
}

#[test]
fn error_variant_owner_discarded_helper_does_not_lend_its_pin() -> Result<(), String> {
    check_owner_pin(
        "",
        "fn entry(s: &str) -> Result<u8, Fault> { parse(s) }\n#[test] fn odd() { let _ = entry(\"1\"); assert_eq!(parse_other(\"1\").unwrap_err(), Fault::Odd); }",
        false,
    )
}

#[test]
fn error_variant_owner_bound_pin_with_expected_payload_helper_retained() -> Result<(), String> {
    check_source(
        r#"
#[derive(Debug, PartialEq)] pub enum Fault { Odd(u8) }
pub fn parse(_: &str) -> Result<u8, Fault> { Err(Fault::Odd(7)) }
fn expected_payload() -> u8 { 7 }
#[cfg(test)] mod tests { use super::*;
#[test] fn odd() { let err = parse("1").unwrap_err(); assert_eq!(err, Fault::Odd(expected_payload())); }
}
"#,
        true,
    ).map(|_| ())
}

#[test]
fn error_variant_owner_expected_side_and_message_calls_supply_no_custody() -> Result<(), String> {
    for assertion in [
        "assert_eq!(parse_other(\"1\").unwrap_err(), Fault::Odd(expected_payload()));",
        "assert_eq!(Fault::Odd(expected_payload()), parse_other(\"1\").unwrap_err());",
        "assert_eq!(parse_other(\"1\").unwrap_err(), Fault::Odd(7), \"{:?}\", parse(\"1\"));",
        "assert_eq!(Fault::Odd(7), parse_other(\"1\").unwrap_err(), \"{:?}\", parse(\"1\"));",
    ] {
        let source = format!(
            r#"
#[derive(Debug, PartialEq)] pub enum Fault {{ Odd(u8), Bad }}
pub fn parse(_: &str) -> Result<u8, Fault> {{ Err(Fault::Odd(7)) }}
pub fn parse_other(_: &str) -> Result<u8, Fault> {{ Err(Fault::Odd(7)) }}
fn expected_payload() -> u8 {{ let _ = parse("1"); 7 }}
#[cfg(test)] mod tests {{ use super::*; #[test] fn odd() {{ {assertion} }} }}
"#
        );
        check_source(&source, false)?;
    }
    Ok(())
}

#[test]
fn error_variant_owner_multiline_test_helper_receipt_retained() -> Result<(), String> {
    let source = r#"
#[derive(Debug, PartialEq)] pub enum Fault { Odd }
pub fn parse_input(_: &str) -> Result<u8, Fault> { Err(Fault::Odd) }
#[cfg(test)] mod tests { use super::*;
fn parse_input_entry(s: &str) -> Result<u8, Fault> {
    parse_input(s)
}
#[test] fn odd() {
    assert_eq!(
        Fault::Odd,
        parse_input_entry("1").unwrap_err()
    );
}
}
"#;
    let evidence = check_files_for_owner(
        &[(PathBuf::from("src/lib.rs"), source)],
        true,
        "src/lib.rs::parse_input",
    )?;
    assert!(
        evidence
            .related_tests
            .iter()
            .any(|test| test.relation_reason == RelationReason::HelperOwnerCall),
        "fixture must establish authenticated helper custody: {:?}",
        evidence.related_tests
    );
    Ok(())
}

#[test]
fn error_variant_owner_discarded_entry_in_argument_supplies_no_custody() -> Result<(), String> {
    let wrong = r#"#[derive(Debug, PartialEq)] pub enum Fault { Odd, Bad }
pub fn parse_input(_: &str) -> Result<u8, Fault> { Err(Fault::Odd) }
pub fn parse_other(_: &str) -> Result<u8, Fault> { Err(Fault::Odd) }
#[cfg(test)] mod tests {
    use super::*;
    fn entry(s: &str) -> Result<u8, Fault> {
        parse_input(s)
    }
    #[test] fn argument_block() {
        assert_eq!(parse_other({ let _ = entry("1"); "1" }).unwrap_err(), Fault::Odd);
    }
}
"#;
    let genuine = wrong.replace(
        "parse_other({ let _ = entry(\"1\"); \"1\" }).unwrap_err()",
        "entry(\"1\").unwrap_err()",
    );
    let positive = check_files_for_owner(
        &[(PathBuf::from("src/lib.rs"), &genuine)],
        true,
        "src/lib.rs::parse_input",
    )?;
    assert!(
        positive
            .related_tests
            .iter()
            .any(|test| test.relation_reason == RelationReason::HelperOwnerCall)
    );
    for expression in [
        "parse_other({ let _ = entry(\"1\"); \"1\" }).unwrap_err()",
        "parse_other(\"1\").map(|value| { let _ = entry(\"1\"); value }).unwrap_err()",
    ] {
        let source = wrong.replace(
            "parse_other({ let _ = entry(\"1\"); \"1\" }).unwrap_err()",
            expression,
        );
        check_files_for_owner(
            &[(PathBuf::from("src/lib.rs"), &source)],
            false,
            "src/lib.rs::parse_input",
        )?;
    }
    Ok(())
}
