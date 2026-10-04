use super::*;
use crate::analysis::new_test_target::NewTestProposalProvenance;
use crate::analysis::seams::ExpectedSink;

fn seam_at(
    file: &str,
    source: &str,
    needle: &str,
    kind: SeamKind,
    required: RequiredDiscriminator,
) -> Result<RepoSeam, String> {
    let offset = source
        .find(needle)
        .ok_or_else(|| format!("fixture has `{needle}`"))?;
    let line = source[..offset].matches('\n').count() + 1;
    Ok(RepoSeam::new(
        file,
        "owner",
        kind,
        offset,
        line,
        needle,
        required,
        ExpectedSink::ReturnValue,
    ))
}

fn boundary(description: &str) -> RequiredDiscriminator {
    RequiredDiscriminator::BoundaryValue {
        description: description.to_string(),
    }
}

const PRICE: &str = "pub fn price(amount: u32, threshold: u32) -> u32 {
    if amount >= threshold { amount - 10 } else { amount }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn price_works() {
        assert!(price(100, 50) > 0);
    }
}
";

#[test]
fn boundary_stub_inserts_into_the_existing_inline_module_and_stops_at_the_expected_value()
-> Result<(), String> {
    let seam = seam_at(
        "src/lib.rs",
        PRICE,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let stub = rust_test_stub(&seam, None, PRICE).map_err(|r| r.reason().to_string())?;
    let close = PRICE.rfind('}').ok_or("module close brace")?;
    assert_eq!(
        stub.placement,
        TestStubPlacement::ExistingInlineModule {
            file: PathBuf::from("src/lib.rs"),
            module_name: "tests".to_string(),
            offset: close,
        }
    );
    assert_eq!(stub.test_name, "price_boundary_discriminator");
    assert_eq!(
        stub.derived_inputs,
        vec!["amount = 100".to_string(), "threshold = 100".to_string()]
    );
    let expected = "
    // ripr: discriminate `amount >= threshold` in `price` (src/lib.rs:2).
    #[test]
    #[allow(unreachable_code)] // delete with the last todo!()
    fn price_boundary_discriminator() {
        use super::*;
        let amount: u32 = 100;
        let threshold: u32 = 100;
        let actual = price(amount, threshold);
        let expected: u32 = todo!(\"ripr: write the value `price` should return for amount = 100, threshold = 100\");
        assert_eq!(actual, expected);
    }
";
    assert_eq!(stub.text, expected);
    Ok(())
}

#[test]
fn method_owner_uses_receiver_syntax_not_a_free_call() -> Result<(), String> {
    // #5357: a method owner was suggested as `as_whole_units(...)`.
    let source = "pub struct ByteSize(u64);
impl ByteSize {
    pub fn as_whole_units(&self, unit: u64) -> u64 {
        if unit == 0 { 0 } else { self.0 / unit }
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "unit == 0",
        SeamKind::PredicateBoundary,
        boundary("unit == 0"),
    )?;
    let stub = rust_test_stub(&seam, None, source).map_err(|r| r.reason().to_string())?;
    assert!(matches!(
        stub.placement,
        TestStubPlacement::NewInlineModule { offset, .. } if offset == source.len()
    ));
    assert!(stub.text.starts_with("\n#[cfg(test)]\nmod ripr_tests {\n"));
    assert!(stub.text.contains(
        "let subject: ByteSize = todo!(\"ripr: build the `ByteSize` that `as_whole_units` runs on\");"
    ));
    assert!(stub.text.contains("let unit: u64 = 0;"));
    assert!(
        stub.text
            .contains("let actual = subject.as_whole_units(unit);")
    );
    assert!(!stub.text.contains("= as_whole_units("));
    Ok(())
}

#[test]
fn literal_and_const_comparisons_fill_one_side_and_leave_the_rest_as_todo() -> Result<(), String> {
    let source = "const LIMIT: i32 = 7;
fn grade(score: i32, bonus: i32) -> i32 { if score > LIMIT { bonus } else { 0 } }
fn floor(x: i64, y: i64) -> i64 { if x <= -3 { y } else { x } }
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "score > LIMIT",
        SeamKind::PredicateBoundary,
        boundary("score == LIMIT"),
    )?;
    let stub = rust_test_stub(&seam, None, source).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.derived_inputs, vec!["score = LIMIT".to_string()]);
    assert!(
        stub.text
            .contains("let bonus: i32 = todo!(\"ripr: choose `bonus`\");")
    );

    let seam = seam_at(
        "src/lib.rs",
        source,
        "x <= -3",
        SeamKind::PredicateBoundary,
        boundary("x == -3"),
    )?;
    let stub = rust_test_stub(&seam, None, source).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.derived_inputs, vec!["x = -3".to_string()]);
    Ok(())
}

#[test]
fn error_variant_stub_matches_the_variant_without_an_expected_todo() -> Result<(), String> {
    let source = "pub enum ParseError { Empty, TooLong(usize) }
pub fn parse(input: &str) -> Result<u8, ParseError> {
    if input.len() > 3 { return Err(ParseError::TooLong(input.len())); }
    Ok(0)
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "Err(ParseError::TooLong(input.len()))",
        SeamKind::ErrorVariant,
        RequiredDiscriminator::ErrorVariant {
            variant: "ParseError::TooLong".to_string(),
        },
    )?;
    let stub = rust_test_stub(&seam, None, source).map_err(|r| r.reason().to_string())?;
    assert!(stub.text.contains("let input: &str = todo!("));
    assert!(stub.text.contains(
        "assert!(matches!(actual, Err(ParseError::TooLong { .. })), \"expected Err(ParseError::TooLong {{ .. }})\");"
    ));
    assert!(!stub.text.contains("let expected"));
    Ok(())
}

#[test]
fn error_seam_without_a_nameable_variant_asserts_the_whole_value() -> Result<(), String> {
    let source = "pub fn parse_summary(raw: &str) -> Result<u8, String> {
    try_parse(raw).map_err(|error| error.to_string())
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "try_parse(raw).map_err(|error| error.to_string())",
        SeamKind::ErrorVariant,
        RequiredDiscriminator::ErrorVariant {
            variant: "try_parse(raw).map_err(|error| error.to_string())".to_string(),
        },
    )?;
    let stub = rust_test_stub(&seam, None, source).map_err(|r| r.reason().to_string())?;
    assert!(!stub.text.contains("matches!"));
    assert!(
        stub.text
            .contains("let expected: Result<u8, String> = todo!(")
    );
    assert_eq!(
        variant_pattern("Err(Error::TooLong(n))"),
        Some("Error::TooLong { .. }".to_string())
    );
    assert_eq!(
        variant_pattern("ErrorKind::Empty"),
        Some("ErrorKind::Empty { .. }".to_string())
    );
    assert_eq!(variant_pattern("try_parse(raw)"), None);
    Ok(())
}

#[test]
fn integration_proposal_writes_a_new_file_through_the_crate_path() -> Result<(), String> {
    let seam = seam_at(
        "src/lib.rs",
        PRICE,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let proposal = NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/price_tests.rs"),
        owner: "demo::price".to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
    };
    let stub = rust_test_stub(&seam, Some(&proposal), PRICE).map_err(|r| r.reason().to_string())?;
    assert_eq!(
        stub.placement,
        TestStubPlacement::NewIntegrationFile {
            file: PathBuf::from("tests/price_tests.rs")
        }
    );
    assert!(stub.text.starts_with("// ripr: discriminate"));
    assert!(stub.text.contains("    use demo::*;\n"));
    Ok(())
}

#[test]
fn non_std_return_types_compare_debug_text_instead_of_requiring_partial_eq() -> Result<(), String> {
    let source = "pub struct Out(u8);
pub fn build(n: u8) -> Result<Out, String> { if n > 2 { Ok(Out(n)) } else { Err(String::new()) } }
pub fn count(n: u8) -> Option<Vec<u8>> { if n > 2 { Some(vec![n]) } else { None } }
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "n > 2 { Ok",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let stub = rust_test_stub(&seam, None, source).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text
            .contains("assert_eq!(format!(\"{actual:?}\"), format!(\"{expected:?}\"));")
    );
    let seam = seam_at(
        "src/lib.rs",
        source,
        "n > 2 { Some",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let stub = rust_test_stub(&seam, None, source).map_err(|r| r.reason().to_string())?;
    assert!(stub.text.contains("assert_eq!(actual, expected);"));
    Ok(())
}

#[test]
fn existing_test_name_gets_a_numbered_suffix() -> Result<(), String> {
    let source = PRICE.replace("fn price_works()", "fn price_boundary_discriminator()");
    let seam = seam_at(
        "src/lib.rs",
        &source,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let stub = rust_test_stub(&seam, None, &source).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.test_name, "price_boundary_discriminator_2");
    Ok(())
}

#[test]
fn refusals_name_the_blocker() -> Result<(), String> {
    let cases: [(&str, &str, SeamKind, TestStubRefusal); 6] = [
        (
            "pub async fn f(x: u8) -> u8 { if x > 1 { 1 } else { 0 } }\n",
            "x > 1",
            SeamKind::PredicateBoundary,
            TestStubRefusal::OwnerAsync,
        ),
        (
            "pub fn f<T: Copy>(x: T, n: u8) -> u8 { if n > 1 { 1 } else { 0 } }\n",
            "n > 1",
            SeamKind::PredicateBoundary,
            TestStubRefusal::OwnerGeneric,
        ),
        (
            "pub fn f(x: u8) { if x > 1 { println!(\"hi\"); } }\n",
            "x > 1",
            SeamKind::PredicateBoundary,
            TestStubRefusal::NoReturnValue,
        ),
        (
            "pub fn f(x: u8) -> u8 { if x > 1 { 1 } else { 0 } }\n#[cfg(test)]\nmod tests;\n",
            "x > 1",
            SeamKind::PredicateBoundary,
            TestStubRefusal::OutOfLineTestModule,
        ),
        (
            "mod inner { pub fn f(x: u8) -> u8 { if x > 1 { 1 } else { 0 } } }\n",
            "x > 1",
            SeamKind::PredicateBoundary,
            TestStubRefusal::OwnerInNestedModule,
        ),
        (
            "pub fn f(x: u8) -> u8 { log(x); x }\n",
            "log(x)",
            SeamKind::SideEffect,
            TestStubRefusal::ObserverRequired,
        ),
    ];
    for (source, needle, kind, refusal) in cases {
        let seam = seam_at("src/lib.rs", source, needle, kind, boundary(""))?;
        assert_eq!(
            rust_test_stub(&seam, None, source),
            Err(refusal),
            "{source}"
        );
    }
    let seam = seam_at(
        "src/app.py",
        "def f(x):\n    return x > 1\n",
        "x > 1",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    assert_eq!(
        rust_test_stub(&seam, None, "def f(x):\n    return x > 1\n"),
        Err(TestStubRefusal::NotRustSource)
    );
    Ok(())
}

#[test]
fn stub_text_never_uses_overclaiming_vocabulary() -> Result<(), String> {
    let seam = seam_at(
        "src/lib.rs",
        PRICE,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let stub = rust_test_stub(&seam, None, PRICE).map_err(|r| r.reason().to_string())?;
    for word in [
        "killed",   // ripr-allow: static-language: test guard verifying this term stays absent
        "survived", // ripr-allow: static-language: test guard verifying this term stays absent
        "untested", // ripr-allow: static-language: test guard verifying this term stays absent
        "proven",   // ripr-allow: static-language: test guard verifying this term stays absent
        "adequate", // ripr-allow: static-language: test guard verifying this term stays absent
    ] {
        assert!(!stub.text.contains(word), "{word}");
    }
    Ok(())
}

#[test]
fn lifetime_annotated_mut_reference_binds_the_referent_mutably() -> Result<(), String> {
    const SOURCE: &str = "pub fn bump<'a>(counter: &'a mut u32, limit: u32) -> bool {
    *counter >= limit
}
";
    let seam = seam_at(
        "src/lib.rs",
        SOURCE,
        "*counter >= limit",
        SeamKind::PredicateBoundary,
        boundary("*counter == limit"),
    )?;
    let stub = rust_test_stub(&seam, None, SOURCE).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains("let mut counter: u32 ="),
        "{}",
        stub.text
    );
    assert!(
        stub.text.contains("bump(&mut counter, limit)"),
        "{}",
        stub.text
    );
    assert_eq!(strip_mut_reference("&mut Vec<u8>"), Some("Vec<u8>"));
    assert_eq!(strip_mut_reference("&'_ mut u32"), Some("u32"));
    assert_eq!(strip_mut_reference("&'static mut u32"), Some("u32"));
    assert_eq!(strip_mut_reference("&'_ u32"), None);
    assert_eq!(strip_mut_reference("&mutable"), None);
    Ok(())
}
