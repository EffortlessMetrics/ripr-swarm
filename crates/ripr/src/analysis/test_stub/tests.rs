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
    let stub = rust_test_stub(&seam, None, PRICE, None).map_err(|r| r.reason().to_string())?;
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
        let expected: u32 = todo!(\"ripr: write the value `price` should return for amount = 100, threshold = 100\");
        assert_eq!(price(amount, threshold), expected);
    }
";
    assert_eq!(stub.text, expected);
    Ok(())
}

#[test]
fn unrelated_generic_does_not_remove_a_concrete_stub_comparison() -> Result<(), String> {
    let source = format!("fn helper<u32>() {{}}\n{PRICE}");
    let seam = seam_at(
        "src/lib.rs",
        &source,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let stub = rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains("let expected: u32 = todo!("),
        "{}",
        stub.text
    );
    assert!(
        stub.text
            .contains("assert_eq!(price(amount, threshold), expected);"),
        "{}",
        stub.text
    );
    Ok(())
}

/// #8891: a second run for the same seam recomputes byte-identical text. The
/// name collision is ripr's own earlier write, so the stub is flagged
/// already present instead of suffixing a `_2` duplicate.
#[test]
fn recomputed_stub_for_the_same_seam_is_flagged_already_present() -> Result<(), String> {
    let seam = seam_at(
        "src/lib.rs",
        PRICE,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let first = rust_test_stub(&seam, None, PRICE, None).map_err(|r| r.reason().to_string())?;
    assert!(!first.already_present);
    assert_eq!(first.test_name, "price_boundary_discriminator");
    // Simulate the first `--write`: the insertion is the stub text at the
    // placement offset.
    let close = PRICE.rfind('}').ok_or("module close brace")?;
    let (head, tail) = PRICE.split_at(close);
    let rewritten = format!("{head}{}{tail}", first.text.trim_start_matches('\n'));
    let seam = seam_at(
        "src/lib.rs",
        &rewritten,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let second =
        rust_test_stub(&seam, None, &rewritten, None).map_err(|r| r.reason().to_string())?;
    assert!(
        second.already_present,
        "the byte-identical recompute must be detected as already present"
    );
    assert_eq!(
        second.test_name, "price_boundary_discriminator",
        "no `_2` suffix for ripr's own earlier write"
    );
    Ok(())
}

/// A different test that happens to share the base name is not ripr's stub:
/// the `--write` semantics are unchanged and the name still suffixes.
/// #8990 review X1: applying the write twice must report ripr's own `_2`
/// write as already present on the second run — the file stays unchanged —
/// instead of suffixing a `_3` duplicate.
#[test]
fn same_named_but_different_test_still_suffixes() -> Result<(), String> {
    let with_other_test = "pub fn price(amount: u32, threshold: u32) -> u32 {
    if amount >= threshold { amount - 10 } else { amount }
}

#[cfg(test)]
mod tests {
    #[test]
    fn price_boundary_discriminator() {
        assert_eq!(price(100, 100), 90);
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        with_other_test,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let stub =
        rust_test_stub(&seam, None, with_other_test, None).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.test_name, "price_boundary_discriminator_2");
    assert!(
        !stub.already_present,
        "a genuinely different test with the same name is not an already-written stub"
    );

    // Simulate the first `--write`: the `_2` stub text lands inside the
    // inline module, after the user's same-named test.
    let close = with_other_test.rfind('}').ok_or("module close brace")?;
    let (head, tail) = with_other_test.split_at(close);
    let rewritten = format!("{head}{}{tail}", stub.text.trim_start_matches('\n'));
    let seam = seam_at(
        "src/lib.rs",
        &rewritten,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let second =
        rust_test_stub(&seam, None, &rewritten, None).map_err(|r| r.reason().to_string())?;
    assert!(
        second.already_present,
        "the second run must find ripr's own `_2` write under the suffixed name, not suffix a `_3`"
    );
    assert_eq!(
        second.test_name, "price_boundary_discriminator_2",
        "the already-written report names the test on disk"
    );
    // The library still renders the stub text; the no-edit guarantee is the
    // CLI's skip on `already_present` (second_write_for_the_same_seam_reports_
    // already_written and the end-to-end byte pins cover the file staying put).
    Ok(())
}

/// #8990 review X2: a copy of the rendered stub inside a block comment or a
/// raw string is not a test Cargo discovers. The name-collision check and the
/// already-written detection parse the module, so the commented stub neither
/// occupies the base name nor flags the stub present: `--write` inserts the
/// real test under the base name.
#[test]
fn commented_stub_does_not_count_as_written_or_named() -> Result<(), String> {
    let seam = seam_at(
        "src/lib.rs",
        PRICE,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let rendered = rust_test_stub(&seam, None, PRICE, None).map_err(|r| r.reason().to_string())?;
    let anchor = "    #[test]\n    fn price_works() {";
    for embedded in [
        // Block comment holding the exact proposed function.
        format!("    /*\n{}    */", rendered.text.trim_start_matches('\n')),
        // Raw string holding the exact proposed function.
        format!(
            "    const COMMENTED: &str = r##\"\n{}\"##;",
            rendered.text.trim_start_matches('\n')
        ),
    ] {
        let source = PRICE.replacen(anchor, &format!("{embedded}\n{anchor}"), 1);
        let seam = seam_at(
            "src/lib.rs",
            &source,
            "amount >= threshold",
            SeamKind::PredicateBoundary,
            boundary("amount == threshold"),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        assert_eq!(
            stub.test_name, "price_boundary_discriminator",
            "a commented-out stub does not occupy the base name: {embedded}"
        );
        assert!(
            !stub.already_present,
            "a commented-out stub is not an already-written test: {embedded}"
        );
    }
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
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
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
            .contains("assert_eq!(subject.as_whole_units(unit), expected);")
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
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
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
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
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
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
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
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(!stub.text.contains("matches!"));
    assert!(
        stub.text
            .contains("let expected: Result<u8, String> = todo!(")
    );
    assert_eq!(
        variant_pattern("Err(Error::TooLong(n))", None, PathScope::ChildModule),
        Some("Error::TooLong { .. }".to_string())
    );
    assert_eq!(
        variant_pattern("ErrorKind::Empty", None, PathScope::ChildModule),
        Some("ErrorKind::Empty { .. }".to_string())
    );
    assert_eq!(
        variant_pattern("try_parse(raw)", None, PathScope::ChildModule),
        None
    );
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
        workspace_identity: String::new(),
    };
    let stub =
        rust_test_stub(&seam, Some(&proposal), PRICE, None).map_err(|r| r.reason().to_string())?;
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
fn return_types_without_visible_partial_eq_leave_the_assertion_to_fill_in() -> Result<(), String> {
    let source = "pub struct Out(u8);
#[derive(Debug, Clone, PartialEq)]
pub struct Seen(u8);
pub fn seen(n: u8) -> Seen { if n > 4 { Seen(n) } else { Seen(0) } }
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
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(stub.text.contains("let _ = &actual;"), "{}", stub.text);
    assert!(
        stub.text
            .contains("todo!(\"ripr: assert that `actual` is the value `build` should return"),
        "{}",
        stub.text
    );
    assert!(!stub.text.contains("assert_eq!"), "{}", stub.text);
    assert!(!stub.text.contains("format!"), "{}", stub.text);
    // A local type whose derives name PartialEq and Debug compares directly.
    let seam = seam_at(
        "src/lib.rs",
        source,
        "n > 4",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains("let expected: Seen = todo!("),
        "{}",
        stub.text
    );
    assert!(
        stub.text.contains("assert_eq!(seen(n), expected);"),
        "{}",
        stub.text
    );
    let seam = seam_at(
        "src/lib.rs",
        source,
        "n > 2 { Some",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains("assert_eq!(count(n), expected);"),
        "{}",
        stub.text
    );
    assert!(!stub.text.contains("let actual"), "{}", stub.text);
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
    let stub = rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
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
            rust_test_stub(&seam, None, source, None),
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
        rust_test_stub(&seam, None, "def f(x):\n    return x > 1\n", None),
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
    let stub = rust_test_stub(&seam, None, PRICE, None).map_err(|r| r.reason().to_string())?;
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
    let stub = rust_test_stub(&seam, None, SOURCE, None).map_err(|r| r.reason().to_string())?;
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

#[test]
fn integration_stub_leaves_a_named_constant_as_a_fill_in() -> Result<(), String> {
    let proposal = NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/gate.rs"),
        owner: "demo::gate".to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        workspace_identity: String::new(),
    };
    for source in [
        "const LIMIT: u8 = 3;
pub fn gate(n: u8) -> u8 {
    if n > LIMIT { n } else { 0 }
}
",
        "pub(crate) const LIMIT: u8 = 3;
pub fn gate(n: u8) -> u8 {
    if n > LIMIT { n } else { 0 }
}
",
    ] {
        let seam = seam_at(
            "src/lib.rs",
            source,
            "n > LIMIT",
            SeamKind::PredicateBoundary,
            boundary("n == LIMIT"),
        )?;
        // Inline, the private constant is in scope through `use super::*`.
        let inline =
            rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
        assert!(
            inline.text.contains("let n: u8 = LIMIT;"),
            "{}",
            inline.text
        );
        // A `tests/` file cannot see private or `pub(crate)` constants.
        let stub = rust_test_stub(&seam, Some(&proposal), source, None)
            .map_err(|r| r.reason().to_string())?;
        assert!(!stub.text.contains("= LIMIT;"), "{}", stub.text);
        assert!(stub.text.contains("let n: u8 = todo!("), "{}", stub.text);
        assert!(stub.derived_inputs.is_empty());
    }
    Ok(())
}

#[test]
fn integration_stub_leaves_a_nested_module_public_constant_as_a_fill_in() -> Result<(), String> {
    const SOURCE: &str = "mod limits {
    pub const LIMIT: u8 = 3;
}
use limits::LIMIT;
pub fn gate(n: u8) -> u8 {
    if n > LIMIT { n } else { 0 }
}
";
    let seam = seam_at(
        "src/lib.rs",
        SOURCE,
        "n > LIMIT",
        SeamKind::PredicateBoundary,
        boundary("n == LIMIT"),
    )?;
    let proposal = NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/gate.rs"),
        owner: "demo::gate".to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        workspace_identity: String::new(),
    };
    let inline = rust_test_stub(&seam, None, SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert!(
        inline.text.contains("let n: u8 = LIMIT;"),
        "{}",
        inline.text
    );
    let stub =
        rust_test_stub(&seam, Some(&proposal), SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert!(!stub.text.contains("= LIMIT;"), "{}", stub.text);
    assert!(stub.text.contains("let n: u8 = todo!("), "{}", stub.text);
    assert!(stub.derived_inputs.is_empty());
    Ok(())
}

#[test]
fn integration_stub_ignores_nested_const_after_a_brace_in_a_string() -> Result<(), String> {
    const SOURCE: &str = "mod limits {
    const PRELUDE: &str = \"}\";
    pub const LIMIT: u8 = 3;
}
use limits::LIMIT;
pub fn gate(n: u8) -> u8 {
    if n > LIMIT { n } else { 0 }
}
";
    let seam = seam_at(
        "src/lib.rs",
        SOURCE,
        "n > LIMIT",
        SeamKind::PredicateBoundary,
        boundary("n == LIMIT"),
    )?;
    let proposal = NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/gate.rs"),
        owner: "demo::gate".to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        workspace_identity: String::new(),
    };
    let stub =
        rust_test_stub(&seam, Some(&proposal), SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert!(!stub.text.contains("= LIMIT;"), "{}", stub.text);
    assert!(stub.text.contains("let n: u8 = todo!("), "{}", stub.text);
    assert!(stub.derived_inputs.is_empty());
    Ok(())
}

#[test]
fn integration_stub_leaves_a_cfg_test_public_constant_as_a_fill_in() -> Result<(), String> {
    const SOURCE: &str = "#[cfg(test)]
pub const LIMIT: u8 = 3;
pub fn gate(n: u8) -> u8 {
    #[cfg(test)]
    if n > LIMIT {
        return n;
    }
    n
}
";
    let seam = seam_at(
        "src/lib.rs",
        SOURCE,
        "n > LIMIT",
        SeamKind::PredicateBoundary,
        boundary("n == LIMIT"),
    )?;
    let proposal = NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/gate.rs"),
        owner: "demo::gate".to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        workspace_identity: String::new(),
    };
    let inline = rust_test_stub(&seam, None, SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert!(
        inline.text.contains("let n: u8 = LIMIT;"),
        "{}",
        inline.text
    );
    let stub =
        rust_test_stub(&seam, Some(&proposal), SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert!(!stub.text.contains("= LIMIT;"), "{}", stub.text);
    assert!(stub.text.contains("let n: u8 = todo!("), "{}", stub.text);
    assert!(stub.derived_inputs.is_empty());
    Ok(())
}

#[test]
fn integration_stub_leaves_a_feature_gated_public_constant_as_a_fill_in() -> Result<(), String> {
    const SOURCE: &str = "#[cfg(feature = \"special\")]
pub const LIMIT: u8 = 3;
pub fn gate(n: u8) -> u8 {
    #[cfg(feature = \"special\")]
    if n > LIMIT {
        return n;
    }
    n
}
";
    let seam = seam_at(
        "src/lib.rs",
        SOURCE,
        "n > LIMIT",
        SeamKind::PredicateBoundary,
        boundary("n == LIMIT"),
    )?;
    let proposal = NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/gate.rs"),
        owner: "demo::gate".to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        workspace_identity: String::new(),
    };
    let stub =
        rust_test_stub(&seam, Some(&proposal), SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert!(!stub.text.contains("= LIMIT;"), "{}", stub.text);
    assert!(stub.text.contains("let n: u8 = todo!("), "{}", stub.text);
    assert!(stub.derived_inputs.is_empty());
    Ok(())
}

#[test]
fn integration_stub_rebases_crate_paths_and_keeps_a_public_constant() -> Result<(), String> {
    const SOURCE: &str = "pub const LIMIT: u8 = 3;
pub struct Tag;
pub fn gate(n: u8, tag: crate::Tag) -> u8 {
    let _ = tag;
    if n > LIMIT { n } else { 0 }
}
";
    let seam = seam_at(
        "src/lib.rs",
        SOURCE,
        "n > LIMIT",
        SeamKind::PredicateBoundary,
        boundary("n == LIMIT"),
    )?;
    let proposal = NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/gate.rs"),
        owner: "demo::gate".to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        workspace_identity: String::new(),
    };
    let stub =
        rust_test_stub(&seam, Some(&proposal), SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert_eq!(
        stub.placement,
        TestStubPlacement::NewIntegrationFile {
            file: PathBuf::from("tests/gate.rs")
        }
    );
    assert_eq!(stub.derived_inputs, vec!["n = LIMIT".to_string()]);
    assert!(stub.text.contains("use demo::*;\n"), "{}", stub.text);
    assert!(stub.text.contains("let n: u8 = LIMIT;"), "{}", stub.text);
    assert!(
        stub.text.contains("let tag: demo::Tag = todo!("),
        "{}",
        stub.text
    );
    assert!(
        !stub.text.contains("crate::"),
        "integration stubs must rebase crate:: to the crate name: {}",
        stub.text
    );
    Ok(())
}

#[test]
fn integration_stub_refuses_self_and_super_parameter_paths() -> Result<(), String> {
    let proposal = |owner: &str| NewTestTargetProposal {
        kind: NewTestKind::Integration,
        file: PathBuf::from("tests/flag.rs"),
        owner: owner.to_string(),
        provenance: NewTestProposalProvenance::ProducerOwned,
        workspace_identity: String::new(),
    };
    let self_source = "pub struct Cfg { pub on: bool }
pub fn flag(cfg: &self::Cfg, n: i32) -> bool {
    if n < 0 { cfg.on } else { !cfg.on }
}
";
    let super_source = "pub struct Cfg { pub on: bool }
pub fn flag(cfg: &super::Cfg, n: i32) -> bool {
    if n < 0 { cfg.on } else { !cfg.on }
}
";
    for (source, needle) in [(self_source, "n < 0"), (super_source, "n < 0")] {
        let seam = seam_at(
            "src/lib.rs",
            source,
            needle,
            SeamKind::PredicateBoundary,
            boundary(""),
        )?;
        assert_eq!(
            rust_test_stub(&seam, Some(&proposal("demo::flag")), source, None),
            Err(TestStubRefusal::ParameterUnsupported),
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn result_aliases_and_qualified_or_shadowed_types_are_not_assumed_comparable() {
    let plain = "pub fn f() {}";
    assert_eq!(
        value_traits("Result<u8, String>", plain),
        ValueTraits::PartialEqAndDebug
    );
    assert_eq!(
        value_traits("std::io::Result<u8>", plain),
        ValueTraits::Unknown
    );
    assert_eq!(value_traits("Result<u8>", plain), ValueTraits::Unknown);
    assert_eq!(
        value_traits("Result<Vec<(u8, u8)>, String>", plain),
        ValueTraits::PartialEqAndDebug
    );
    let alias = "pub type Result<T> = std::result::Result<T, Bad>;
#[derive(Debug)]
pub struct Bad;";
    assert_eq!(value_traits("Result<u8, Bad>", alias), ValueTraits::Unknown);
    let local_error = "#[derive(Debug, PartialEq)]
pub enum Error { A }";
    assert_eq!(
        value_traits("Error", local_error),
        ValueTraits::PartialEqAndDebug
    );
    assert_eq!(
        value_traits("Result<u8, std::io::Error>", local_error),
        ValueTraits::Unknown
    );
    let shadow = "pub struct Duration(pub u64);";
    assert_eq!(value_traits("Duration", shadow), ValueTraits::Unknown);
    let test_only = "pub fn f() {}
#[cfg(test)]
mod tests {
    #[derive(Debug, PartialEq)]
    pub struct Out;
}";
    assert_eq!(value_traits("Out", test_only), ValueTraits::Unknown);
}

/// The inline module a boundary stub for `needle` is inserted into.
fn stub_module_name(source: &str, needle: &str) -> Result<String, String> {
    let seam = seam_at(
        "src/lib.rs",
        source,
        needle,
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    match rust_test_stub(&seam, None, source, None).map_err(|r| r.as_str().to_string())? {
        RustTestStub {
            placement: TestStubPlacement::ExistingInlineModule { module_name, .. },
            ..
        } => Ok(module_name),
        other => Err(format!("unexpected placement {:?}", other.placement)),
    }
}

#[test]
fn several_inline_test_modules_pick_the_one_naming_the_owner_then_the_nearest() -> Result<(), String>
{
    // #5471: bytesize keeps two `#[cfg(test)]` modules in `lib.rs`.
    let source = "pub fn early(x: u8) -> u8 { if x > 1 { 1 } else { 0 } }

#[cfg(test)]
mod a {
    #[test]
    fn smoke() {}
}

pub fn late(x: u8) -> u8 { if x > 2 { 1 } else { 0 } }

#[cfg(test)]
mod b {
    use super::*;
    #[test]
    fn early_works() { assert_eq!(early(0), 0); }
}
";
    let module_name = stub_module_name;
    // `b` already names `early`, so it wins over the nearer `a`.
    assert_eq!(module_name(source, "x > 1")?, "b");
    // Nothing names `late`: the nearest module after it.
    assert_eq!(module_name(source, "x > 2")?, "b");
    // Nothing names `tail` and nothing follows it: the nearest before it.
    let with_tail =
        format!("{source}pub fn tail(x: u8) -> u8 {{ if x > 3 {{ 1 }} else {{ 0 }} }}\n");
    assert_eq!(module_name(&with_tail, "x > 3")?, "b");
    // Nothing names `head`: the nearest after it is `a`, not `b`.
    let with_head =
        format!("pub fn head(x: u8) -> u8 {{ if x > 4 {{ 1 }} else {{ 0 }} }}\n{source}");
    assert_eq!(module_name(&with_head, "x > 4")?, "a");
    Ok(())
}

#[test]
fn comments_and_strings_do_not_count_as_naming_the_owner() -> Result<(), String> {
    // Codex review of #5477: a nearer module mentioning `early` only in a
    // comment or string must not outrank the one that calls it.
    let source = "pub fn early(x: u8) -> u8 { if x > 1 { 1 } else { 0 } }

#[cfg(test)]
mod a {
    // early is covered in b
    #[test]
    fn s() { let _ = \"early\"; }
}

#[cfg(test)]
mod b {
    use super::*;
    #[test]
    fn early_works() { assert_eq!(early(0), 0); }
}
";
    assert_eq!(stub_module_name(source, "x > 1")?, "b");
    // `early_works` is a different identifier, not a mention of `early`.
    let renamed = source.replace("assert_eq!(early(0), 0)", "assert!(true)");
    assert_eq!(stub_module_name(&renamed, "x > 1")?, "a");
    Ok(())
}

#[test]
fn lifetime_only_impl_binds_the_subject_with_an_elided_lifetime() -> Result<(), String> {
    // #5471: humantime `impl<'a> Parser<'a> { fn parse_unit(&mut self, ..) }`.
    for header in ["impl<'a> Parser<'a> {", "impl Parser<'_> {"] {
        let source = format!(
            "pub struct Parser<'a> {{ src: &'a str }}
{header}
    pub fn parse_unit(&mut self, start: usize, end: usize) -> Result<u64, String> {{
        if end > start {{ Ok(1) }} else {{ Err(String::new()) }}
    }}
    pub fn open(src: &str, limit: usize) -> usize {{
        if src.len() > limit {{ limit }} else {{ 0 }}
    }}
}}
"
        );
        let seam = seam_at(
            "src/lib.rs",
            &source,
            "end > start",
            SeamKind::PredicateBoundary,
            boundary("end == start"),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        assert!(
            stub.text.contains(
                "let mut subject: Parser<'_> = todo!(\"ripr: build the `Parser<'_>` that `parse_unit` runs on\");"
            ),
            "{header}: {}",
            stub.text
        );
        // `&mut self`: the call stays bound before `expected` (#13861 review)
        // and is made exactly once.
        assert!(
            stub.text
                .contains("let actual = subject.parse_unit(start, end);"),
            "{header}: {}",
            stub.text
        );
        assert!(
            stub.text.contains("assert_eq!(actual, expected);"),
            "{header}: {}",
            stub.text
        );
        assert_eq!(
            stub.text.matches("parse_unit(start, end)").count(),
            1,
            "{header}: {}",
            stub.text
        );
        assert!(!stub.text.contains("'a"), "{header}: {}", stub.text);
        // An associated function is called through the bare type path.
        let seam = seam_at(
            "src/lib.rs",
            &source,
            "src.len() > limit",
            SeamKind::PredicateBoundary,
            boundary(""),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        assert!(
            stub.text
                .contains("assert_eq!(Parser::open(src, limit), expected);"),
            "{header}: {}",
            stub.text
        );
    }
    Ok(())
}

#[test]
fn type_or_const_generic_impls_are_refused_as_generic_impls() -> Result<(), String> {
    for source in [
        "pub struct W<T>(T);\nimpl<T> W<T> {\n    pub fn f(&self, n: u8) -> u8 { if n > 1 { 1 } else { 0 } }\n}\n",
        "pub struct W<T>(T);\nimpl W<u8> {\n    pub fn f(&self, n: u8) -> u8 { if n > 1 { 1 } else { 0 } }\n}\n",
        "pub struct A<const N: usize>;\nimpl<const N: usize> A<N> {\n    pub fn f(&self, n: u8) -> u8 { if n > 1 { 1 } else { 0 } }\n}\n",
    ] {
        let seam = seam_at(
            "src/lib.rs",
            source,
            "n > 1",
            SeamKind::PredicateBoundary,
            boundary(""),
        )?;
        assert_eq!(
            rust_test_stub(&seam, None, source, None),
            Err(TestStubRefusal::OwnerGenericImpl),
            "{source}"
        );
    }
    assert_eq!(
        TestStubRefusal::OwnerGenericImpl.as_str(),
        "owner_generic_impl"
    );
    assert!(
        TestStubRefusal::OwnerGenericImpl
            .reason()
            .contains("type or const generics")
    );
    Ok(())
}

const FROM_STR: &str = "use std::str::FromStr;
#[derive(Debug, PartialEq)]
pub enum Unit { Second, Minute }
#[derive(Debug, PartialEq)]
pub enum Error { Unknown }
impl FromStr for Unit {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            \"s\" => Ok(Unit::Second),
            \"m\" => Ok(Unit::Minute),
            _ => Err(Error::Unknown),
        }
    }
}
";

#[test]
fn trait_impl_method_is_called_through_its_trait_path() -> Result<(), String> {
    // #5471: humantime `impl FromStr for Unit { fn from_str(..) }`.
    let seam = seam_at(
        "src/lib.rs",
        FROM_STR,
        "\"m\" => Ok(Unit::Minute)",
        SeamKind::MatchArm,
        RequiredDiscriminator::MatchArmTaken {
            arm: "\"m\" => Ok(Unit::Minute)".to_string(),
        },
    )?;
    let stub = rust_test_stub(&seam, None, FROM_STR, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text
            .contains("let actual = <Unit as FromStr>::from_str(s);"),
        "{}",
        stub.text
    );
    // The return type names `Self::Err`, so no comparison is assumed: the
    // assertion stays the developer's fill-in.
    assert!(stub.text.contains("let _ = &actual;"), "{}", stub.text);
    assert!(!stub.text.contains("Unit::Err"), "{}", stub.text);
    assert!(!stub.text.contains("let expected"), "{}", stub.text);

    let seam = seam_at(
        "src/lib.rs",
        FROM_STR,
        "Err(Error::Unknown)",
        SeamKind::ErrorVariant,
        RequiredDiscriminator::ErrorVariant {
            variant: "Error::Unknown".to_string(),
        },
    )?;
    let stub = rust_test_stub(&seam, None, FROM_STR, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text
            .contains("assert!(matches!(actual, Err(Error::Unknown { .. }))"),
        "{}",
        stub.text
    );
    assert_eq!(
        concrete_type("Result<Self, Self::Err>", Some("Unit"), Some("FromStr")),
        "Result<Unit, <Unit as FromStr>::Err>"
    );
    assert_eq!(
        variant_pattern("Self::Err::Bad", Some("Unit"), PathScope::ChildModule),
        None
    );
    Ok(())
}

#[test]
fn trait_impl_method_with_a_receiver_passes_it_by_reference() -> Result<(), String> {
    let source = "pub struct Meter(u32);
impl std::fmt::Display for Meter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 > 3 { write!(f, \"big\") } else { write!(f, \"small\") }
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "self.0 > 3",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text
            .contains("let actual = <Meter as std::fmt::Display>::fmt(&subject, &mut f);"),
        "{}",
        stub.text
    );
    assert!(
        stub.text
            .contains("let mut f: std::fmt::Formatter<'_> = todo!("),
        "{}",
        stub.text
    );
    Ok(())
}

const VERSION: &str = "#[derive(Debug, Clone, PartialEq)]
pub struct Version { pub major: u64, pub minor: u64, pub patch: u64 }
impl Version {
    pub fn next_minor(&self) -> Version {
        Version { major: self.major, minor: self.minor + 1, patch: 0 }
    }
    pub fn staged(&self) -> u64 {
        let v = Version { major: 1, minor: self.minor * 2, patch: 3 };
        v.minor
    }
}
";

#[test]
fn field_of_the_returned_struct_literal_asserts_the_whole_return_value() -> Result<(), String> {
    // #5471: semver `Version::next_minor` builds and returns a `Version`.
    let field = |needle: &str| RequiredDiscriminator::FieldValue {
        field: needle.to_string(),
    };
    let seam = seam_at(
        "src/lib.rs",
        VERSION,
        "minor: self.minor + 1",
        SeamKind::FieldConstruction,
        field("minor: self.minor + 1"),
    )?;
    let stub = rust_test_stub(&seam, None, VERSION, None).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.test_name, "next_minor_field_discriminator");
    assert!(
        stub.text
            .contains("assert_eq!(subject.next_minor(), expected);"),
        "{}",
        stub.text
    );
    assert!(
        stub.text.contains(
            "let expected: Version = todo!(\"ripr: write the value `next_minor` should return, including its field `minor: self.minor + 1`\");"
        ),
        "{}",
        stub.text
    );
    // A literal bound to a local is not the returned value: refused, with a
    // reason naming that.
    let seam = seam_at(
        "src/lib.rs",
        VERSION,
        "minor: self.minor * 2",
        SeamKind::FieldConstruction,
        field("minor: self.minor * 2"),
    )?;
    assert_eq!(
        rust_test_stub(&seam, None, VERSION, None),
        Err(TestStubRefusal::FieldNotReturned)
    );
    assert!(
        TestStubRefusal::FieldNotReturned
            .reason()
            .contains("does not return directly")
    );
    Ok(())
}

const BUILDER: &str = "pub struct Req { pub url: String, pub timeout: u32 }
impl Req {
    pub fn timeout(mut self, t: u32) -> Self {
        self.timeout = t * 2;
        self
    }
    pub fn rebuilt(&self) -> Req {
        Req { url: String::new(), timeout: self.timeout * 2 }
    }
}
";

#[test]
fn field_write_in_a_method_names_the_receiver_not_a_struct_literal() -> Result<(), String> {
    // #11398: `self.timeout = t * 2` builds no returned literal; the refusal
    // says the field is set on `self`, not "a struct literal the owner does
    // not return".
    let field = |needle: &str| RequiredDiscriminator::FieldValue {
        field: needle.to_string(),
    };
    let write = seam_at(
        "src/lib.rs",
        BUILDER,
        "self.timeout = t * 2",
        SeamKind::FieldConstruction,
        field("self.timeout = t * 2"),
    )?;
    // #12061: `mut self` writes a moved copy, so the refusal is the by-value
    // one; only a borrowed receiver keeps the in-place wording.
    assert_eq!(
        rust_test_stub(&write, None, BUILDER, None),
        Err(TestStubRefusal::FieldWrittenByValue)
    );
    assert!(
        TestStubRefusal::FieldWrittenByValue
            .reason()
            .contains("by-value `self`")
    );
    let borrowed_source = "pub struct Req { pub timeout: u32 }
impl Req {
    pub fn set_timeout(&mut self, t: u32) {
        self.timeout = t * 2;
    }
}
";
    let borrowed = seam_at(
        "src/lib.rs",
        borrowed_source,
        "self.timeout = t * 2",
        SeamKind::FieldConstruction,
        field("self.timeout = t * 2"),
    )?;
    assert_eq!(
        rust_test_stub(&borrowed, None, borrowed_source, None),
        Err(TestStubRefusal::FieldWrittenInMethod)
    );
    assert!(
        TestStubRefusal::FieldWrittenInMethod
            .reason()
            .contains("sets a field of `self` inside a method")
    );
    // A returned struct-literal field in the same impl still gets its stub:
    // the new refusal is keyed on the assignment shape only.
    let literal = seam_at(
        "src/lib.rs",
        BUILDER,
        "timeout: self.timeout * 2",
        SeamKind::FieldConstruction,
        field("timeout: self.timeout * 2"),
    )?;
    let stub = rust_test_stub(&literal, None, BUILDER, None).map_err(|r| r.reason().to_string())?;
    assert!(stub.text.contains("subject.rebuilt()"), "{}", stub.text);
    Ok(())
}

#[test]
fn feature_gated_test_modules_are_never_chosen() -> Result<(), String> {
    // Review of #5477: `cargo test` does not build a feature-gated module, so
    // a stub there would never compile or run.
    let gated_first = "pub fn early(x: u32) -> u32 { if x > 40 { 1 } else { 0 } }

#[cfg(all(test, feature = \"slow\"))]
mod slow_tests {
    #[test]
    fn s() {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke() {}
}
";
    let seam = seam_at(
        "src/lib.rs",
        gated_first,
        "x > 40",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    match rust_test_stub(&seam, None, gated_first, None).map_err(|r| r.as_str().to_string())? {
        RustTestStub {
            placement: TestStubPlacement::ExistingInlineModule { module_name, .. },
            ..
        } => assert_eq!(module_name, "tests"),
        other => return Err(format!("unexpected placement {:?}", other.placement)),
    }
    for only_gated in [
        "pub fn early(x: u32) -> u32 { if x > 40 { 1 } else { 0 } }

#[cfg(all(test, feature = \"slow\"))]
mod slow_tests {
    #[test]
    fn s() {}
}
",
        "pub fn early(x: u32) -> u32 { if x > 40 { 1 } else { 0 } }

#[cfg(test)]
mod slow_tests {
    #![cfg(feature = \"slow\")]
    #[test]
    fn s() {}
}
",
    ] {
        let seam = seam_at(
            "src/lib.rs",
            only_gated,
            "x > 40",
            SeamKind::PredicateBoundary,
            boundary(""),
        )?;
        assert_eq!(
            rust_test_stub(&seam, None, only_gated, None).map(|stub| stub.test_name),
            Err(TestStubRefusal::AmbiguousTestModule),
            "{only_gated}"
        );
    }
    Ok(())
}

#[test]
fn impls_local_to_a_block_are_refused() -> Result<(), String> {
    // Review of #5477: names inside a fn body or `const _` block are out of
    // the test module's reach.
    for source in [
        "pub struct X(pub u8);
const _: () = {
    use std::fmt::Display;
    impl Display for X {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            if self.0 > 31 { write!(f, \"a\") } else { write!(f, \"b\") }
        }
    }
};
",
        "pub fn outer() {
    struct L(u8);
    impl<'a> L {
        fn m2(&self, n: u8) -> u8 { if n > 31 { 1 } else { 0 } }
    }
}
",
    ] {
        let needle = if source.contains("self.0 > 31") {
            "self.0 > 31"
        } else {
            "n > 31"
        };
        let seam = seam_at(
            "src/lib.rs",
            source,
            needle,
            SeamKind::PredicateBoundary,
            boundary(""),
        )?;
        assert_eq!(
            rust_test_stub(&seam, None, source, None).map(|stub| stub.test_name),
            Err(TestStubRefusal::OwnerUnsupported),
            "{source}"
        );
    }
    Ok(())
}

#[test]
fn a_proptest_sibling_module_does_not_outrank_one_naming_the_owner() -> Result<(), String> {
    // A macro-only `proptest!` module is nearer, but the module that already
    // calls the owner wins.
    let source = "pub fn early(x: u32) -> u32 { if x > 40 { 1 } else { 0 } }

#[cfg(test)]
mod props {
    proptest::proptest! {
        #[test]
        fn any(x in 0u32..10) { let _ = x; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn early_works() { assert_eq!(early(0), 0); }
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "x > 40",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    match rust_test_stub(&seam, None, source, None).map_err(|r| r.as_str().to_string())? {
        RustTestStub {
            placement: TestStubPlacement::ExistingInlineModule { module_name, .. },
            ..
        } => assert_eq!(module_name, "tests"),
        other => return Err(format!("unexpected placement {:?}", other.placement)),
    }
    Ok(())
}

#[test]
fn owners_behind_a_non_test_cfg_are_refused() -> Result<(), String> {
    // Codex review of #5477: a stub beside a feature-gated owner compiles
    // out of a plain `cargo test`, which then builds zero tests and passes.
    let refused = [
        "#![cfg(feature = \"x\")]\npub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n",
        "#[cfg(feature = \"x\")]\nmod inner {\n    pub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n    #[cfg(test)]\n    mod tests {}\n}\n",
        "mod inner {\n    #![cfg(unix)]\n    pub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n}\n",
        "#[cfg(feature = \"x\")]\npub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n",
        "pub struct S;\n#[cfg(not(test))]\nimpl S {\n    pub fn f(&self, n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n}\n",
    ];
    for source in refused {
        let seam = seam_at(
            "src/lib.rs",
            source,
            "n > 5",
            SeamKind::PredicateBoundary,
            boundary(""),
        )?;
        assert_eq!(
            rust_test_stub(&seam, None, source, None).map(|stub| stub.test_name),
            Err(TestStubRefusal::OwnerUnsupported),
            "{source}"
        );
    }
    // Attributes a plain test build keeps do not refuse.
    let kept =
        "/// Docs.\n#[inline]\n#[must_use]\npub fn f(n: u8) -> u8 { if n > 5 { 1 } else { 0 } }\n";
    let seam = seam_at(
        "src/lib.rs",
        kept,
        "n > 5",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    assert!(rust_test_stub(&seam, None, kept, None).is_ok(), "{kept}");
    Ok(())
}

#[test]
fn relational_boundary_over_a_field_names_the_equal_inputs() -> Result<(), String> {
    const SOURCE: &str = "pub struct Bucket {
    tokens: u32,
}

impl Bucket {
    pub fn allow(&mut self, n: u32) -> bool {
        self.tokens > n
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        SOURCE,
        "self.tokens > n",
        SeamKind::PredicateBoundary,
        boundary("self.tokens > n"),
    )?;
    let stub = rust_test_stub(&seam, None, SOURCE, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains("choose `n` so that self.tokens == n"),
        "{}",
        stub.text
    );
    assert!(
        stub.text
            .contains("when self.tokens == n (where `>` and its neighbor differ)"),
        "{}",
        stub.text
    );
    assert!(
        !stub.text.contains("so that self.tokens > n"),
        "inputs satisfying the changed comparison itself do not separate `>` from `>=`: {}",
        stub.text
    );
    Ok(())
}

#[test]
fn unsized_mut_reference_parameters_bind_an_owned_stand_in() {
    assert_eq!(
        owned_mut_binding("dyn Sink", "sink"),
        (
            "std::boxed::Box<dyn Sink>".to_string(),
            "&mut *sink".to_string()
        )
    );
    assert_eq!(
        owned_mut_binding("[u8]", "buf"),
        ("std::vec::Vec<u8>".to_string(), "&mut buf".to_string())
    );
    assert_eq!(
        owned_mut_binding("str", "name"),
        ("std::string::String".to_string(), "&mut name".to_string())
    );
    // A slice of arrays is still unsized; a parenthesized object type too.
    assert_eq!(
        owned_mut_binding("[[u8; 4]]", "rows"),
        (
            "std::vec::Vec<[u8; 4]>".to_string(),
            "&mut rows".to_string()
        )
    );
    assert_eq!(
        owned_mut_binding("(dyn Sink + Send)", "sink"),
        (
            "std::boxed::Box<dyn Sink + Send>".to_string(),
            "&mut *sink".to_string()
        )
    );
    // A fixed-size array is sized: it stays as written.
    assert_eq!(
        owned_mut_binding("[u8; 4]", "buf"),
        ("[u8; 4]".to_string(), "&mut buf".to_string())
    );
    assert_eq!(
        owned_mut_binding("Vec<u8>", "v"),
        ("Vec<u8>".to_string(), "&mut v".to_string())
    );
}

/// #6300: an owner that returns `()` or `Result<(), _>` hands the changed
/// value out through a `&mut` input, so the stub asserts that input after the
/// call. `Ok(())` alone cannot tell the old arm from the new one.
#[test]
fn unit_result_owner_asserts_the_mut_input_after_the_call() -> Result<(), String> {
    let source = "pub fn add_unit(unit: u8, out: &mut u64) -> Result<(), String> {
    match unit {
        1 => *out += 1,
        2 => *out += 14,
        _ => return Err(String::new()),
    }
    Ok(())
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "2 => *out += 14",
        SeamKind::MatchArm,
        RequiredDiscriminator::MatchArmTaken {
            arm: "2".to_string(),
        },
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|refusal| refusal.as_str())?;
    for expected in [
        "let mut out: u64 = todo!(",
        "let actual = add_unit(unit, &mut out);",
        "assert_eq!(actual, expected);",
        "let expected_out: u64 = todo!(",
        "assert_eq!(out, expected_out);",
    ] {
        assert!(stub.text.contains(expected), "{expected}\n{}", stub.text);
    }
    // A `&mut` argument keeps the call bound before `expected` and makes it
    // exactly once (#13861 review).
    assert_eq!(
        stub.text.matches("add_unit(unit, &mut out)").count(),
        1,
        "{}",
        stub.text
    );
    assert!(
        stub.fill_ins
            .iter()
            .any(|fill_in| fill_in.contains("`out` should hold after `add_unit` runs")),
        "{:?}",
        stub.fill_ins
    );
    Ok(())
}

#[test]
fn unit_owner_with_a_mut_input_is_stubbed_without_a_unit_binding() -> Result<(), String> {
    let source = "pub fn bump(x: u8, total: &mut u32) { if x > 3 { *total += 1; } }\n";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "x > 3",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|refusal| refusal.as_str())?;
    assert!(stub.text.contains("bump(x, &mut total);"), "{}", stub.text);
    assert!(
        !stub.text.contains("let actual"),
        "a unit call has no value to bind:\n{}",
        stub.text
    );
    assert!(
        stub.text.contains("assert_eq!(total, expected_total);"),
        "{}",
        stub.text
    );
    Ok(())
}

#[test]
fn mut_self_is_asserted_and_an_unknown_type_leaves_the_assertion_to_fill_in() -> Result<(), String>
{
    let source = "pub struct Counter(u32);
impl Counter {
    pub fn tick(&mut self, n: u8) { if n > 1 { self.0 += 1; } }
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "n > 1",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|refusal| refusal.as_str())?;
    assert!(stub.text.contains("subject.tick("), "{}", stub.text);
    assert!(stub.text.contains("let _ = &subject;"), "{}", stub.text);
    assert!(
        stub.fill_ins
            .iter()
            .any(|fill_in| fill_in.contains("`subject` should hold after `tick` runs")),
        "{:?}",
        stub.fill_ins
    );
    Ok(())
}

#[test]
fn match_arm_on_a_unit_result_owner_without_a_mut_input_needs_an_observer() -> Result<(), String> {
    let source = "pub fn check(unit: u8) -> Result<(), String> {
    let _secs = match unit {
        1 => 60,
        2 => 120,
        _ => return Err(String::new()),
    };
    Ok(())
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "2 => 120",
        SeamKind::MatchArm,
        RequiredDiscriminator::MatchArmTaken {
            arm: "2".to_string(),
        },
    )?;
    assert_eq!(
        rust_test_stub(&seam, None, source, None),
        Err(TestStubRefusal::ObserverRequired)
    );
    Ok(())
}

#[test]
fn returns_unit_recognizes_unit_and_unit_results_only() {
    for unit in [
        "()",
        " () ",
        "Result<(), Error>",
        "io::Result<()>",
        "Result<(), Vec<u8>>",
        "Result<(), HashMap<K, V>>",
        "Result<(), (A, B)>",
    ] {
        assert!(returns_unit(unit), "{unit}");
    }
    for value in [
        "u8",
        "Option<()>",
        "Result<u8, ()>",
        "Result<(u8, u8), E>",
        "Vec<()>",
        "Result<HashMap<K, V>, E>",
        "Result<Vec<()>, E>",
    ] {
        assert!(!returns_unit(value), "{value}");
    }
}

#[test]
fn impl_into_and_as_ref_parameters_get_a_concrete_stand_in() -> Result<(), String> {
    // #8361: `impl Into<String>` was refused as `parameter_unsupported`, but
    // the concrete target type is itself a valid argument.
    let source = "pub fn label(name: impl Into<String>, path: impl AsRef<str>, n: u8) -> u8 {
    let _ = (name.into(), path.as_ref());
    if n > 3 { 1 } else { 0 }
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "n > 3",
        SeamKind::PredicateBoundary,
        boundary("n > 3"),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains("let name: String = todo!("),
        "{}",
        stub.text
    );
    assert!(
        stub.text.contains("let path: &str = todo!("),
        "{}",
        stub.text
    );
    assert!(!stub.text.contains("impl "), "{}", stub.text);
    Ok(())
}

#[test]
fn other_impl_trait_parameters_stay_refused() -> Result<(), String> {
    for param in [
        "w: impl std::fmt::Display",
        "w: impl Into<Vec<u8>>",
        "w: impl AsRef<Vec<u8>>",
        "w: impl AsRef<Path>",
        "w: &impl Into<String>",
        "w: impl Into<impl Clone>",
    ] {
        let source = format!(
            "pub fn f({param}, n: u8) -> u8 {{
    if n > 3 {{ 1 }} else {{ 0 }}
}}
"
        );
        let seam = seam_at(
            "src/lib.rs",
            &source,
            "n > 3",
            SeamKind::PredicateBoundary,
            boundary("n > 3"),
        )?;
        assert_eq!(
            rust_test_stub(&seam, None, &source, None),
            Err(TestStubRefusal::ParameterUnsupported),
            "{param}"
        );
    }
    Ok(())
}

// #11551: the proof obligation is advisory, symbolic and derived from one
// written relational comparison; anything else yields no obligation.
#[test]
fn proof_obligation_describes_a_single_relational_boundary_and_nothing_else() -> Result<(), String>
{
    let seam = seam_at(
        "src/lib.rs",
        PRICE,
        "amount >= threshold",
        SeamKind::PredicateBoundary,
        boundary("amount == threshold"),
    )?;
    let obligation = proof_obligation_for(&seam).ok_or("relational boundary has an obligation")?;
    assert_eq!(obligation.kind, "boundary");
    assert_eq!(obligation.function, "owner");
    assert_eq!(obligation.changed_predicate, "amount >= threshold");
    assert_eq!(
        obligation.witness_inputs,
        vec![
            "amount just below threshold".to_string(),
            "amount == threshold".to_string(),
            "amount just above threshold".to_string(),
        ]
    );
    assert!(obligation.property_skeleton.contains("kani::proof"));
    assert!(!obligation.limits.is_empty());

    let spaced = seam_at(
        "src/lib.rs",
        "pub fn f(s: &str) -> bool { s < \"a  b\" }\n",
        "s < \"a  b\"",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    let literal = proof_obligation_for(&spaced).ok_or("string comparison has an obligation")?;
    assert_eq!(literal.changed_predicate, "s < \"a  b\"");
    assert_eq!(literal.witness_inputs[1], "s == \"a  b\"");

    // `*/` in an operand would close the skeleton's block comment early.
    let closer = seam_at(
        "src/lib.rs",
        "pub fn f(s: &str) -> bool { s < \"a*/b\" }\n",
        "s < \"a*/b\"",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    assert!(proof_obligation_for(&closer).is_none());

    // Block comments nest, so `/*` leaves the skeleton's comment unterminated.
    let opener = seam_at(
        "src/lib.rs",
        "pub fn f(s: &str) -> bool { s < \"a/*b\" }\n",
        "s < \"a/*b\"",
        SeamKind::PredicateBoundary,
        boundary(""),
    )?;
    assert!(proof_obligation_for(&opener).is_none());

    let compound = "pub fn f(a: u32, b: u32) -> bool { a > 1 && b < 2 }\n";
    for (needle, source, kind, file) in [
        (
            "a > 1 && b < 2",
            compound,
            SeamKind::PredicateBoundary,
            "src/lib.rs",
        ),
        (
            "a == b",
            "pub fn f(a: u32, b: u32) -> bool { a == b }\n",
            SeamKind::PredicateBoundary,
            "src/lib.rs",
        ),
        (
            "a > b",
            "pub fn f(a: u32, b: u32) -> bool { a > b }\n",
            SeamKind::ReturnValue,
            "src/lib.rs",
        ),
        (
            "a > b",
            "pub fn f(a: u32, b: u32) -> bool { a > b }\n",
            SeamKind::PredicateBoundary,
            "src/index.ts",
        ),
    ] {
        let seam = seam_at(file, source, needle, kind, boundary(""))?;
        assert_eq!(proof_obligation_for(&seam), None, "{needle} {file}");
    }
    Ok(())
}

/// #8361: the receiver is built from the constructor an existing test calls
/// with literal arguments; a non-literal call, a cfg-gated constructor and a
/// constructor that does not return the type are not used.
#[test]
fn receiver_reuses_a_literal_constructor_call_from_an_existing_test() -> Result<(), String> {
    let source = "pub struct Bucket { tokens: u32 }
impl Bucket {
    pub fn new(capacity: u32) -> Self { Bucket { tokens: capacity } }
    pub fn allow(&mut self, n: u32) -> bool {
        if self.tokens > n { self.tokens -= n; true } else { false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn variable_capacity() {
        let cap = 9;
        let mut b = Bucket::new(cap);
        assert!(b.allow(1));
    }
    #[test]
    fn literal_capacity() {
        let mut b = Bucket::new(5);
        assert!(b.allow(2));
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "self.tokens > n",
        SeamKind::PredicateBoundary,
        boundary("self.tokens == n"),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text
            .contains("let mut subject: Bucket = Bucket::new(5);"),
        "{}",
        stub.text
    );
    assert!(
        stub.text
            .contains("// ripr: built like the existing test `literal_capacity`")
    );
    assert!(!stub.fill_ins.iter().any(|fill| fill.contains("build the")));
    assert_eq!(
        stub.receiver_setup.as_deref(),
        Some("subject = Bucket::new(5), as in the existing test `literal_capacity`")
    );
    Ok(())
}

#[test]
fn receiver_stays_a_todo_without_an_established_constructor() -> Result<(), String> {
    // `new` needs an argument no test supplies as a literal, `fresh` is
    // cfg-gated, and `maybe` returns `Option<Self>`: none builds a receiver.
    let source = "pub struct Gauge { v: u32 }
impl Gauge {
    pub fn new(v: u32) -> Self { Gauge { v } }
    #[cfg(feature = \"extra\")]
    pub fn fresh() -> Self { Gauge { v: 0 } }
    pub fn maybe() -> Option<Self> { None }
    pub fn read(&self, n: u32) -> u32 {
        if n > 4 { self.v } else { 0 }
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        source,
        "n > 4",
        SeamKind::PredicateBoundary,
        boundary("n == 4"),
    )?;
    let stub = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains(
            "let subject: Gauge = todo!(\"ripr: build the `Gauge` that `read` runs on\");"
        )
    );
    assert_eq!(stub.receiver_setup, None);
    Ok(())
}

#[test]
fn receiver_prefers_a_no_argument_new_then_default() -> Result<(), String> {
    let with_new = "pub struct Pool { n: u8 }
impl Pool {
    pub fn blank() -> Pool { Pool { n: 1 } }
    pub fn new() -> Self { Pool { n: 0 } }
    pub fn size(&self, k: u8) -> u8 {
        if k > 2 { self.n } else { k }
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        with_new,
        "k > 2",
        SeamKind::PredicateBoundary,
        boundary("k == 2"),
    )?;
    let stub = rust_test_stub(&seam, None, with_new, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text.contains("let subject: Pool = Pool::new();"),
        "{}",
        stub.text
    );

    let with_default = "#[derive(Default)]
pub struct Pool { n: u8 }
impl Pool {
    pub fn size(&self, k: u8) -> u8 {
        if k > 2 { self.n } else { k }
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        with_default,
        "k > 2",
        SeamKind::PredicateBoundary,
        boundary("k == 2"),
    )?;
    let stub =
        rust_test_stub(&seam, None, with_default, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text
            .contains("let subject: Pool = <Pool as ::core::default::Default>::default();"),
        "{}",
        stub.text
    );
    assert_eq!(
        stub.receiver_setup.as_deref(),
        Some("subject = <Pool as ::core::default::Default>::default(), its derived `Default`")
    );
    Ok(())
}

/// #14825 review: a second run must not reuse the first stub's own
/// receiver call (which would change its comment and write a `_2` copy).
#[test]
fn receiver_stub_rerun_is_already_present() -> Result<(), String> {
    let source = "pub struct Ledger { entries: Vec<i64> }
impl Ledger {
    pub fn empty() -> Ledger { Ledger { entries: Vec::new() } }
}
impl Ledger {
    pub fn total(&self, cap: i64) -> i64 {
        let sum: i64 = self.entries.iter().sum();
        if sum > cap { cap } else { sum }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
";
    let needle = "sum > cap";
    let seam = seam_at(
        "src/lib.rs",
        source,
        needle,
        SeamKind::PredicateBoundary,
        boundary("sum == cap"),
    )?;
    let first = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(
        first
            .text
            .contains("let subject: Ledger = Ledger::empty();")
    );
    assert!(
        first
            .text
            .contains("built with its no-argument constructor")
    );
    let close = source.rfind('}').ok_or("module close brace")?;
    let (head, tail) = source.split_at(close);
    let rewritten = format!("{head}{}{tail}", first.text.trim_start_matches('\n'));
    let seam = seam_at(
        "src/lib.rs",
        &rewritten,
        needle,
        SeamKind::PredicateBoundary,
        boundary("sum == cap"),
    )?;
    let second =
        rust_test_stub(&seam, None, &rewritten, None).map_err(|r| r.reason().to_string())?;
    assert!(second.already_present, "{}", second.text);
    assert_eq!(second.test_name, first.test_name);
    Ok(())
}

#[test]
fn receiver_skips_generic_default_helpers_and_shadowed_names() -> Result<(), String> {
    // A derived `Default` on a generic type bounds its argument. Such an
    // owner is refused before any receiver is built; the receiver's own
    // no-arguments guard on `Default` stays as defence in depth.
    let generic = "#[derive(Default)]
pub struct W<T>(Option<T>);
pub struct NoDef;
impl W<NoDef> {
    pub fn f(&self, n: u8) -> u8 { if n > 3 { 1 } else { 0 } }
}
";
    let seam = seam_at(
        "src/lib.rs",
        generic,
        "n > 3",
        SeamKind::PredicateBoundary,
        boundary("n == 3"),
    )?;
    assert_eq!(
        rust_test_stub(&seam, None, generic, None).err(),
        Some(TestStubRefusal::OwnerGenericImpl)
    );

    // A literal call in a non-test helper, and one under a module that
    // imports its own `Bucket`, are not reused.
    for tests in [
        "    use super::*;\n    fn make() -> Bucket { Bucket::new(5) }\n",
        "    use super::*;\n    use crate::other::Bucket;\n    #[test]\n    fn t() { let _b = Bucket::new(5); }\n",
    ] {
        let source = format!(
            "pub struct Bucket {{ tokens: u32 }}
impl Bucket {{
    pub fn new(capacity: u32) -> Self {{ Bucket {{ tokens: capacity }} }}
    pub fn allow(&self, n: u32) -> bool {{ self.tokens > n }}
}}

#[cfg(test)]
mod tests {{
{tests}}}
"
        );
        let seam = seam_at(
            "src/lib.rs",
            &source,
            "self.tokens > n",
            SeamKind::PredicateBoundary,
            boundary("self.tokens == n"),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        assert_eq!(stub.receiver_setup, None, "{}", stub.text);
    }
    Ok(())
}

/// #14825 CodeRabbit review: a call under `cfg` is not copied, a scope that
/// imports another `Bucket` (by name or glob, or `super::` from a nested
/// module, a `mod Bucket`, an item-position macro or `include!`, a macro
/// definition naming it, or `extern crate Bucket`) blocks the no-argument fallback too, and
/// `use super::Bucket;` outside a nested module names the owner, so reuse still applies under it.
#[test]
fn receiver_skips_cfg_calls_and_shadowed_fallbacks() -> Result<(), String> {
    let build = |ctor: &str, tests: &str| {
        format!(
            "pub struct Bucket {{ tokens: u32 }}
impl Bucket {{
{ctor}    pub fn allow(&self, n: u32) -> bool {{ self.tokens > n }}
}}

#[cfg(test)]
mod tests {{
{tests}}}
"
        )
    };
    let with_capacity = "    pub fn new(capacity: u32) -> Self { Bucket { tokens: capacity } }\n";
    let no_argument = "    pub fn new() -> Self { Bucket { tokens: 0 } }\n";
    let cases = [
        (
            with_capacity,
            "    use super::*;\n    #[cfg(any())]\n    #[test]\n    fn off() { let _b = Bucket::new(\"wrong\"); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    #[test]\n    fn on() {\n        #[cfg(any())]\n        let _b = Bucket::new(\"wrong\");\n    }\n",
            None,
        ),
        (
            no_argument,
            "    use super::*;\n    use crate::other::Bucket;\n",
            None,
        ),
        (
            with_capacity,
            "    use crate::other::*;\n    #[test]\n    fn t() { let _b = Bucket::new(\"x\"); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    mod inner {\n        use super::Bucket;\n    }\n    #[test]\n    fn t() { let _b = Bucket::new(5); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    mod Bucket { pub fn new(_s: &str) -> u8 { 0 } }\n    #[test]\n    fn t() { let _x: u8 = Bucket::new(\"x\"); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    macro_rules! helpers { () => { mod Bucket { pub fn new(_s: &str) -> u8 { 0 } } } }\n    helpers!();\n    #[test]\n    fn t() { let _x: u8 = Bucket::new(\"x\"); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    include!(\"helpers.rs\");\n    #[test]\n    fn t() { let _b = Bucket::new(5); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    helpers!();\n    #[test]\n    fn t() { let _b = Bucket::new(5); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    #[test]\n    fn drains() {\n        let _b = Bucket::new(5);\n        assert!(Bucket::new(1).allow(0));\n        let _v = vec![Bucket::new(2)];\n    }\n",
            Some("subject = Bucket::new(5), as in the existing test `drains`"),
        ),
        (
            with_capacity,
            "    use super::*;\n    macro_rules! mk { () => { struct Bucket; impl Bucket { fn new(_: u8) -> u8 { 0 } } } }\n    #[test]\n    fn t() { mk!(); let _b = Bucket::new(5); }\n",
            None,
        ),
        (
            with_capacity,
            "    use super::*;\n    extern crate Bucket as other;\n    #[test]\n    fn drains() { let _b = Bucket::new(5); }\n",
            Some("subject = Bucket::new(5), as in the existing test `drains`"),
        ),
        (
            with_capacity,
            "    use super::*;\n    extern crate Bucket;\n    #[test]\n    fn t() { let _b = Bucket::new(5); }\n",
            None,
        ),
        (
            with_capacity,
            "    #[test]\n    fn drains() {\n        use super::*;\n        let _b = Bucket::new(5);\n    }\n",
            Some("subject = Bucket::new(5), as in the existing test `drains`"),
        ),
        (
            with_capacity,
            "    use super::Bucket;\n    #[test]\n    fn drains() { let _b = Bucket::new(5); }\n",
            Some("subject = Bucket::new(5), as in the existing test `drains`"),
        ),
    ];
    for (ctor, tests, expected) in cases {
        let source = build(ctor, tests);
        let seam = seam_at(
            "src/lib.rs",
            &source,
            "self.tokens > n",
            SeamKind::PredicateBoundary,
            boundary("self.tokens == n"),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        assert_eq!(stub.receiver_setup.as_deref(), expected, "{}", stub.text);
    }
    Ok(())
}

/// #14825 re-review: the reused-from-a-test origin is stable on rerun, and a
/// no-argument constructor with a non-neutral name (`connect`) is not used.
#[test]
fn receiver_reuse_rerun_is_already_present_and_names_stay_neutral() -> Result<(), String> {
    let source = "pub struct Bucket { tokens: u32 }
impl Bucket {
    pub fn new(capacity: u32) -> Self { Bucket { tokens: capacity } }
    pub fn allow(&self, n: u32) -> bool { self.tokens > n }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drains() {
        let b = Bucket::new(5);
        assert!(b.allow(2));
    }
}
";
    let needle = "self.tokens > n";
    let seam = seam_at(
        "src/lib.rs",
        source,
        needle,
        SeamKind::PredicateBoundary,
        boundary("self.tokens == n"),
    )?;
    let first = rust_test_stub(&seam, None, source, None).map_err(|r| r.reason().to_string())?;
    assert!(first.text.contains("Bucket::new(5);"), "{}", first.text);
    let close = source.rfind('}').ok_or("module close brace")?;
    let (head, tail) = source.split_at(close);
    let rewritten = format!("{head}{}{tail}", first.text.trim_start_matches('\n'));
    let seam = seam_at(
        "src/lib.rs",
        &rewritten,
        needle,
        SeamKind::PredicateBoundary,
        boundary("self.tokens == n"),
    )?;
    let second =
        rust_test_stub(&seam, None, &rewritten, None).map_err(|r| r.reason().to_string())?;
    assert!(second.already_present, "{}", second.text);

    let connect = "pub struct Link { up: bool }
impl Link {
    pub fn connect() -> Self { Link { up: true } }
    pub fn rate(&self, n: u8) -> u8 { if n > 2 && self.up { n } else { 0 } }
}
";
    let seam = seam_at(
        "src/lib.rs",
        connect,
        "n > 2",
        SeamKind::PredicateBoundary,
        boundary("n == 2"),
    )?;
    let stub = rust_test_stub(&seam, None, connect, None).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.receiver_setup, None, "{}", stub.text);
    Ok(())
}

/// #14825 Devin review: a namesake `Default` trait has no `default()`, and a
/// constructor call inside a closure is not known to have returned.
#[test]
fn receiver_skips_namesake_default_and_closure_calls() -> Result<(), String> {
    let namesake = "trait Default {}
#[derive(Default)]
pub struct Pool { n: u8 }
impl Pool {
    pub fn size(&self, k: u8) -> u8 {
        if k > 2 { self.n } else { k }
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        namesake,
        "k > 2",
        SeamKind::PredicateBoundary,
        boundary("k == 2"),
    )?;
    let stub = rust_test_stub(&seam, None, namesake, None).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.receiver_setup, None, "{}", stub.text);

    let closure = "pub struct Bucket { tokens: u32 }
impl Bucket {
    pub fn new(capacity: u32) -> Self { Bucket { tokens: capacity } }
    pub fn allow(&self, n: u32) -> bool { self.tokens > n }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caught() {
        let built = std::panic::catch_unwind(|| Bucket::new(0));
        assert!(built.is_err());
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        closure,
        "self.tokens > n",
        SeamKind::PredicateBoundary,
        boundary("self.tokens == n"),
    )?;
    let stub = rust_test_stub(&seam, None, closure, None).map_err(|r| r.reason().to_string())?;
    assert_eq!(stub.receiver_setup, None, "{}", stub.text);
    Ok(())
}

/// #14825 re-review: a nested `use std::{default::Default}` is the standard
/// trait, not a namesake; an `async` block and a literal holding a raw
/// control character are not copied.
#[test]
fn receiver_reads_nested_std_imports_and_skips_async_and_control_literals() -> Result<(), String> {
    let nested = "use std::{default::Default, fmt};
#[derive(Default)]
pub struct Pool { n: u8 }
impl Pool {
    pub fn size(&self, k: u8) -> u8 {
        if k > 2 { self.n } else { k }
    }
}
";
    let seam = seam_at(
        "src/lib.rs",
        nested,
        "k > 2",
        SeamKind::PredicateBoundary,
        boundary("k == 2"),
    )?;
    let stub = rust_test_stub(&seam, None, nested, None).map_err(|r| r.reason().to_string())?;
    assert!(
        stub.text
            .contains("<Pool as ::core::default::Default>::default()"),
        "{}",
        stub.text
    );

    for body in [
        "let _f = async { Label::new(\"a\") };",
        "let _l = Label::new(\"a\u{1b}[2J\");",
    ] {
        let source = format!(
            "pub struct Label {{ text: &'static str }}
impl Label {{
    pub fn new(text: &'static str) -> Self {{ Label {{ text }} }}
    pub fn long(&self, n: usize) -> bool {{ self.text.len() > n }}
}}

#[cfg(test)]
mod tests {{
    use super::*;
    #[test]
    fn t() {{
        {body}
    }}
}}
"
        );
        let seam = seam_at(
            "src/lib.rs",
            &source,
            "self.text.len() > n",
            SeamKind::PredicateBoundary,
            boundary("self.text.len() == n"),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        assert_eq!(stub.receiver_setup, None, "{}", stub.text);
    }
    Ok(())
}

/// #8361: a match-arm stub writes the value the changed arm's own pattern
/// names for each parameter it decides, and keeps the fill-in otherwise.
#[test]
fn match_arm_stub_writes_the_input_the_arm_pattern_names() -> Result<(), String> {
    let machine = "#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State { Idle, Running, Done, Failed }
#[derive(Debug, Clone, Copy)]
pub enum Event { Start, Finish, Fail, Reset }
pub struct Machine { state: State }
impl Machine {
    pub fn new() -> Self { Machine { state: State::Idle } }
    pub fn step(&mut self, ev: Event) -> Result<State, String> {
        let next = match (self.state, ev) {
            (State::Idle, Event::Start) => State::Running,
            (State::Running, Event::Finish) => State::Done,
            (State::Running, Event::Fail) => State::Done,
            (_, Event::Reset) => State::Idle,
            (s, e) => return Err(format!(\"invalid {:?} in {:?}\", e, s)),
        };
        self.state = next;
        Ok(next)
    }
}
";
    let numbers = |body: &str| format!("pub fn secs(n: u8) -> u32 {{\n{body}}}\n");
    let cases: Vec<(String, &str, Option<&str>)> = vec![
        (
            machine.to_string(),
            "(State::Running, Event::Fail) => State::Done",
            Some("ev = Event::Fail"),
        ),
        (
            numbers("    match n {\n        1 => 60,\n        2 => 120,\n        _ => 0,\n    }\n"),
            "2 => 120",
            Some("n = 2"),
        ),
        (
            numbers("    match n {\n        1 | 3 => 60,\n        2 | 4 => 120,\n        _ => 0,\n    }\n"),
            "2 | 4 => 120",
            Some("n = 2"),
        ),
        // A guard can still refuse the value.
        (
            numbers("    match n {\n        1 => 60,\n        2 if n > 0 => 120,\n        _ => 0,\n    }\n"),
            "2 if n > 0 => 120",
            None,
        ),
        // An earlier binding arm may take it first.
        (
            numbers("    match n {\n        x if x > 5 => 60,\n        2 => 120,\n        _ => 0,\n    }\n"),
            "2 => 120",
            None,
        ),
        // So may an earlier guarded arm on the same value.
        (
            numbers("    match n {\n        2 if n > 9 => 60,\n        2 => 120,\n        _ => 0,\n    }\n"),
            "2 => 120",
            None,
        ),
        // The parameter is rebound before the match.
        (
            numbers("    let n = n + 1;\n    match n {\n        1 => 60,\n        2 => 120,\n        _ => 0,\n    }\n"),
            "2 => 120",
            None,
        ),
        (
            "pub fn unit(s: &str) -> u32 {\n    match s {\n        \"m\" => 60,\n        \"h\" => 3600,\n        _ => 1,\n    }\n}\n".to_string(),
            "\"h\" => 3600",
            Some("s = \"h\""),
        ),
        (
            "pub enum Mode { A, B }\npub fn cost(m: Mode) -> u32 {\n    match m {\n        Mode::A => 1,\n        Mode::B => 2,\n    }\n}\n".to_string(),
            "Mode::B => 2",
            Some("m = Mode::B"),
        ),
        // A reference parameter would need `&Mode::B`.
        (
            "pub enum Mode { A, B }\npub fn cost(m: &Mode) -> u32 {\n    match m {\n        Mode::A => 1,\n        Mode::B => 2,\n    }\n}\n".to_string(),
            "Mode::B => 2",
            None,
        ),
        // A tuple element that is neither a parameter nor receiver state.
        (
            "pub fn pick(n: u8) -> u32 {\n    let k = n / 2;\n    match (k, n) {\n        (0, 1) => 1,\n        (1, 2) => 2,\n        _ => 0,\n    }\n}\n".to_string(),
            "(1, 2) => 2",
            None,
        ),
        // A literal is written as spelled, spaces included.
        (
            "pub fn unit(s: &str) -> u32 {\n    match s {\n        \"per hour\" => 3600,\n        _ => 1,\n    }\n}\n".to_string(),
            "\"per hour\" => 3600",
            Some("s = \"per hour\""),
        ),
        (
            "pub fn kind(c: char) -> u8 {\n    match c {\n        ' ' => 1,\n        _ => 0,\n    }\n}\n".to_string(),
            "' ' => 1",
            Some("c = ' '"),
        ),
        // A `mut` parameter may change before the match.
        (
            "fn bump(n: &mut u8) { *n += 1; }\npub fn secs(mut n: u8) -> u32 {\n    bump(&mut n);\n    match n {\n        1 => 60,\n        2 => 120,\n        _ => 0,\n    }\n}\n".to_string(),
            "2 => 120",
            None,
        ),
        // A macro call before the match may rebind it.
        (
            numbers("    double!(n);\n    match n {\n        1 => 60,\n        2 => 120,\n        _ => 0,\n    }\n"),
            "2 => 120",
            None,
        ),
        // Earlier arms are compared by value: `0x2` already takes 2.
        (
            numbers("    match n {\n        0x2 => 60,\n        2 | 4 => 120,\n        _ => 0,\n    }\n"),
            "2 | 4 => 120",
            None,
        ),
        // ...and `1_0` is not 2.
        (
            numbers("    match n {\n        1_0 => 60,\n        2u8 => 120,\n        _ => 0,\n    }\n"),
            "2u8 => 120",
            Some("n = 2u8"),
        ),
        // A constant may equal the value.
        (
            "pub struct Limits;\nimpl Limits { pub const TWO: u8 = 2; }\npub fn secs(n: u8) -> u32 {\n    match n {\n        Limits::TWO => 60,\n        2 => 120,\n        _ => 0,\n    }\n}\n".to_string(),
            "2 => 120",
            None,
        ),
        // A renamed import may name the same variant.
        (
            "pub enum Mode { A, B }\nuse Mode as M;\npub fn cost(m: Mode) -> u32 {\n    match m {\n        M::B => 1,\n        Mode::B => 2,\n        Mode::A => 3,\n    }\n}\n".to_string(),
            "Mode::B => 2",
            None,
        ),
        // A path that is not a unit variant of a file-local enum.
        (
            "pub enum Mode { A, B(u8) }\npub fn cost(m: Mode) -> u32 {\n    match m {\n        Mode::A => 1,\n        _ => 2,\n    }\n}\n".to_string(),
            "Mode::A => 1",
            Some("m = Mode::A"),
        ),
        // A raw-identifier rebinding names the same local.
        (
            numbers("    let r#n = n + 1;\n    match n {\n        1 => 60,\n        2 => 120,\n        _ => 0,\n    }\n"),
            "2 => 120",
            None,
        ),
        // A local constant of the name shadows the parameter in the block.
        (
            numbers("    match n {\n        1 => 60,\n        2 => 120,\n        _ => 0,\n    }\n    #[allow(non_upper_case_globals)]\n    const n: u8 = 1;\n"),
            "2 => 120",
            None,
        ),
        // A `cfg` on the arm may remove it from the test build.
        (
            numbers("    match n {\n        #[cfg(not(test))]\n        2 => 120,\n        _ => 0,\n    }\n"),
            "2 => 120",
            None,
        ),
        // A nested match header is not the enclosing arm.
        (
            "pub fn route(mode: u8, input: u8) -> u8 {\n    match mode {\n        1 => match input {\n            0 => 1,\n            _ => 2,\n        },\n        _ => 0,\n    }\n}\n".to_string(),
            "match input",
            None,
        ),
        // A bare uppercase state pattern may be a constant, not a binding.
        (
            "pub const RUNNING: u8 = 1;\npub struct M { state: u8 }\nimpl M {\n    pub fn new() -> Self { M { state: 0 } }\n    pub fn step(&self, n: u8) -> u8 {\n        match (self.state, n) {\n            (RUNNING, 2) => 4,\n            _ => 0,\n        }\n    }\n}\n".to_string(),
            "(RUNNING, 2) => 4",
            Some("n = 2"),
        ),
        // A one-element tuple scrutinee.
        (
            numbers("    match (n,) {\n        (1,) => 60,\n        (2,) => 120,\n        _ => 0,\n    }\n"),
            "(2,) => 120",
            Some("n = 2"),
        ),
    ];
    for (source, arm, expected) in cases {
        let seam = seam_at(
            "src/lib.rs",
            &source,
            arm,
            SeamKind::MatchArm,
            RequiredDiscriminator::MatchArmTaken {
                arm: arm.to_string(),
            },
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        let derived: Vec<&str> = stub.derived_inputs.iter().map(String::as_str).collect();
        match expected {
            Some(input) => {
                assert_eq!(derived, vec![input], "{arm}: {}", stub.text);
                if source.contains("RUNNING") {
                    assert!(
                        stub.text
                            .contains("set it so `self.state` matches `RUNNING`"),
                        "{arm}: a constant state pattern is named: {}",
                        stub.text
                    );
                } else if source.contains("self.state") {
                    assert!(
                        stub.text
                            .contains("set it so `self.state` matches `State::Running`"),
                        "{arm}: the receiver line names the arm's state: {}",
                        stub.text
                    );
                }
                let (binding, value) = input.split_once(" = ").ok_or("binding = value")?;
                assert!(
                    stub.text.contains(&format!("{binding}: "))
                        && stub.text.contains(&format!(" = {value};")),
                    "{arm}: {}",
                    stub.text
                );
                assert!(
                    !stub
                        .fill_ins
                        .iter()
                        .any(|f| f.contains(&format!("`{binding}`"))),
                    "{arm}: {:?}",
                    stub.fill_ins
                );
            }
            None => assert!(derived.is_empty(), "{arm}: {derived:?} {}", stub.text),
        }
    }
    Ok(())
}

/// #8361: when the written inputs decide the owner's path to a constant, the
/// `expected` fill-in names what the changed code returns (and, for a
/// relational boundary, what the neighbor operator returns); the value
/// itself stays a `todo!()`. Anything the evaluation cannot decide gets no
/// hint.
#[test]
fn expected_fill_in_names_what_the_changed_code_returns() -> Result<(), String> {
    let module = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n}\n";
    let arm = |text: &str| RequiredDiscriminator::MatchArmTaken {
        arm: text.to_string(),
    };
    let cases: Vec<(String, &str, SeamKind, RequiredDiscriminator, Option<&str>)> = vec![
        (
            format!(
                "pub fn tier(qty: u32) -> (u32, &'static str) {{\n    if qty > 10 {{ (20, \"bulk\") }} else {{ (0, \"retail\") }}\n}}\n{module}"
            ),
            "qty > 10",
            SeamKind::PredicateBoundary,
            boundary("qty == 10"),
            Some(
                "(the changed code returns (0, \"retail\"); with `>=` it returns (20, \"bulk\"); keep the intended one)",
            ),
        ),
        (
            format!(
                "pub fn secs(n: u8) -> u32 {{\n    match n {{\n        1 => 60,\n        2 => 120,\n        _ => 0,\n    }}\n}}\n{module}"
            ),
            "2 => 120",
            SeamKind::MatchArm,
            arm("2 => 120"),
            Some("(the changed code returns 120; keep the intended one)"),
        ),
        // Early returns and prelude wrappers.
        (
            format!(
                "#[derive(Debug, PartialEq)]\npub enum E {{ Big, Neg }}\npub fn check(n: i32) -> Result<i32, E> {{\n    if n < 0 {{\n        return Err(E::Neg);\n    }}\n    if n >= 100 {{\n        return Err(E::Big);\n    }}\n    Ok(n)\n}}\n{module}"
            ),
            "n >= 100",
            SeamKind::PredicateBoundary,
            boundary("n == 100"),
            Some(
                "(the changed code returns Err(E::Big); with `>` it returns Ok(100); keep the intended one)",
            ),
        ),
        // A match on a unit-variant parameter compares the pattern
        // structurally.
        (
            format!(
                "pub enum K {{ A, B }}\npub fn cost(k: K) -> u32 {{\n    match k {{\n        K::A => 1,\n        K::B => 2,\n    }}\n}}\n{module}"
            ),
            "K::B => 2",
            SeamKind::MatchArm,
            arm("K::B => 2"),
            Some("(the changed code returns 2; keep the intended one)"),
        ),
        // A computed result on the taken path is not a constant.
        (
            format!(
                "pub fn tax(n: u32) -> u32 {{\n    if n >= 10 {{ n * 2 }} else {{ 0 }}\n}}\n{module}"
            ),
            "n >= 10",
            SeamKind::PredicateBoundary,
            boundary("n == 10"),
            None,
        ),
        // A `let` in the body may change what the path reads.
        (
            format!(
                "pub fn tier(qty: u32) -> u32 {{\n    let q = qty;\n    if q > 10 {{ 1 }} else {{ 0 }}\n}}\n{module}"
            ),
            "q > 10",
            SeamKind::PredicateBoundary,
            boundary("q == 10"),
            None,
        ),
        // A guard on an earlier arm leaves the arm undecided.
        (
            format!(
                "pub fn secs(n: u8) -> u32 {{\n    match n {{\n        x if x > 5 => 60,\n        2 => 120,\n        _ => 0,\n    }}\n}}\n{module}"
            ),
            "2 => 120",
            SeamKind::MatchArm,
            arm("2 => 120"),
            None,
        ),
        // A binding arm (`b => b`) may shadow a parameter or be a constant.
        (
            format!(
                "pub fn secs(n: u8) -> u32 {{\n    match n {{\n        1 => 60,\n        2 => 120,\n        other => 0,\n    }}\n}}\n{module}"
            ),
            "2 => 120",
            SeamKind::MatchArm,
            arm("2 => 120"),
            Some("(the changed code returns 120; keep the intended one)"),
        ),
        (
            format!(
                "pub fn secs(n: u8) -> u32 {{\n    match n {{\n        1 => 60,\n        other => 1,\n        2 => 120,\n    }}\n}}\n{module}"
            ),
            "2 => 120",
            SeamKind::MatchArm,
            arm("2 => 120"),
            None,
        ),
        // A condition on an input the stub did not write.
        (
            format!(
                "pub fn pick(a: u32, b: u32) -> u32 {{\n    if b > 3 {{ return 9; }}\n    if a > 10 {{ 1 }} else {{ 0 }}\n}}\n{module}"
            ),
            "a > 10",
            SeamKind::PredicateBoundary,
            boundary("a == 10"),
            None,
        ),
        // A statement-position `if` discards its value: the owner returns 3.
        (
            format!(
                "pub fn f(qty: u32) -> u32 {{\n    if qty > 10 {{ 1 }} else {{ 2 }};\n    3\n}}\n{module}"
            ),
            "qty > 10",
            SeamKind::PredicateBoundary,
            boundary("qty == 10"),
            None,
        ),
        // An attribute may remove a statement from the test build.
        (
            format!(
                "pub fn f(qty: u32) -> u32 {{\n    if qty > 10 {{ return 5; }}\n    #[cfg(not(test))]\n    return 1;\n    2\n}}\n{module}"
            ),
            "qty > 10",
            SeamKind::PredicateBoundary,
            boundary("qty == 10"),
            None,
        ),
        // An attribute on a tuple field may remove it from the test build.
        (
            format!(
                "pub fn f(qty: u32) -> (u32, u32) {{\n    if qty > 10 {{ return (#[cfg(not(test))] qty, #[cfg(test)] qty); }}\n    (0, 0)\n}}\n{module}"
            ),
            "qty > 10",
            SeamKind::PredicateBoundary,
            boundary("qty == 10"),
            None,
        ),
        // A redeclared or imported prelude name (`Ok`) refuses for the file.
        (
            format!(
                "#[derive(Debug, PartialEq)]\npub enum R {{ Ok(u8), No }}\nuse R::Ok;\npub fn f(n: u8) -> R {{\n    if n > 3 {{ Ok(1) }} else {{ R::No }}\n}}\n{module}"
            ),
            "n > 3",
            SeamKind::PredicateBoundary,
            boundary("n == 3"),
            None,
        ),
    ];
    for (source, needle, kind, required, hint) in cases {
        let seam = seam_at("src/lib.rs", &source, needle, kind, required)?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        // The value itself always stays a fill-in.
        let expected = stub
            .fill_ins
            .iter()
            .find(|f| f.contains("should return"))
            .ok_or_else(|| format!("{needle}: expected fill-in: {:?}", stub.fill_ins))?;
        assert!(
            stub.text.contains("let expected: ") && stub.text.contains("= todo!("),
            "{needle}: {}",
            stub.text
        );
        match hint {
            Some(hint) => assert!(expected.ends_with(hint), "{needle}: {expected}"),
            None => assert!(
                !expected.contains("the changed code returns"),
                "{needle}: {expected}"
            ),
        }
    }
    Ok(())
}

#[test]
fn length_boundary_writes_a_string_of_the_boundary_length() -> Result<(), String> {
    let module = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n}\n";
    let longest = format!("let s: &str = \"{}\";", "x".repeat(64));
    // (source, changed comparison, written lines; empty = stays a fill-in)
    let cases: Vec<(String, &str, Vec<&str>)> = vec![
        // Two parameters' lengths: equal lengths, different text.
        (
            format!(
                "pub fn longest<'a>(a: &'a str, b: &'a str) -> &'a str {{\n    if a.len() > b.len() {{ a }} else {{ b }}\n}}\n{module}"
            ),
            "a.len() > b.len()",
            vec!["let a: &'_ str = \"ab\";", "let b: &'_ str = \"cd\";"],
        ),
        // A local bound once from `.chars().count()`, against a file-root const.
        (
            format!(
                "pub const MIN_LEN: usize = 3;\npub fn long_enough(s: &str) -> bool {{\n    let n = s.chars().count();\n    if n <= MIN_LEN {{\n        return false;\n    }}\n    true\n}}\n{module}"
            ),
            "n <= MIN_LEN",
            vec!["let s: &str = \"xxx\";"],
        ),
        // A literal bound.
        (
            format!("pub fn short(s: &str) -> bool {{\n    s.len() < 4\n}}\n{module}"),
            "s.len() < 4",
            vec!["let s: &str = \"xxxx\";"],
        ),
        // A local that is reassigned is not the length.
        (
            format!(
                "pub fn f(s: &str) -> bool {{\n    let mut n = s.len();\n    n += 1;\n    n > 3\n}}\n{module}"
            ),
            "n > 3",
            vec![],
        ),
        // A body-local const shadows the file-root one.
        (
            format!(
                "pub const LIMIT: usize = 3;\npub fn f(s: &str) -> bool {{\n    const LIMIT: usize = 9;\n    s.len() > LIMIT\n}}\n{module}"
            ),
            "s.len() > LIMIT",
            vec![],
        ),
        // Too long to write out.
        (
            format!("pub fn f(s: &str) -> bool {{\n    s.len() > 100\n}}\n{module}"),
            "s.len() > 100",
            vec![],
        ),
        // An owned `String` is not a `&str` literal.
        (
            format!("pub fn f(s: String) -> bool {{\n    s.len() > 3\n}}\n{module}"),
            "s.len() > 3",
            vec![],
        ),
        // The parameter is rebound before the comparison.
        (
            format!(
                "pub fn f(s: &str) -> bool {{\n    let s = s.trim();\n    s.len() > 3\n}}\n{module}"
            ),
            "s.len() > 3",
            vec![],
        ),
        // The longest string written, and one past it.
        (
            format!("pub fn f(s: &str) -> bool {{\n    s.len() > 64\n}}\n{module}"),
            "s.len() > 64",
            vec![longest.as_str()],
        ),
        (
            format!("pub fn f(s: &str) -> bool {{\n    s.len() > 65\n}}\n{module}"),
            "s.len() > 65",
            vec![],
        ),
        // A macro may rebind the local (`let $x = 0;` is not hygienic for a
        // passed identifier).
        (
            format!(
                "macro_rules! zero {{ ($x:ident) => {{ let $x = 0usize; }} }}\npub fn f(s: &str) -> bool {{\n    let n = s.len();\n    zero!(n);\n    n > 3\n}}\n{module}"
            ),
            "n > 3",
            vec![],
        ),
        // A macro in the body may define a `const` that shadows the root one.
        (
            format!(
                "pub const LIMIT: usize = 3;\npub fn f(s: &str) -> bool {{\n    limits!();\n    s.len() > LIMIT\n}}\n{module}"
            ),
            "s.len() > LIMIT",
            vec![],
        ),
        // An owner inside an inline module resolves the bare name there;
        // the const is not read (conservatively, even through a glob).
        (
            "pub const LIMIT: usize = 3;\npub mod inner {\n    use super::*;\n    pub fn f(s: &str) -> bool {\n        s.len() > LIMIT\n    }\n    #[cfg(test)]\n    mod tests {\n        use super::*;\n    }\n}\n"
                .to_string(),
            "s.len() > LIMIT",
            vec![],
        ),
        // A `mut` parameter may hold another string at the comparison.
        (
            format!(
                "pub fn f(mut s: &str) -> bool {{\n    s = \"\";\n    s.len() > 3\n}}\n{module}"
            ),
            "s.len() > 3",
            vec![],
        ),
        // A block-level import may shadow the local (or the parameter).
        (
            format!(
                "pub fn f(s: &str) -> bool {{\n    let n = s.len();\n    {{\n        use crate::config::LIMIT as n;\n        n > 3\n    }}\n}}\n{module}"
            ),
            "n > 3",
            vec![],
        ),
        // A glob in the body may supply its own `LIMIT`.
        (
            format!(
                "pub const LIMIT: usize = 3;\npub fn f(s: &str) -> bool {{\n    use crate::config::*;\n    s.len() > LIMIT\n}}\n{module}"
            ),
            "s.len() > LIMIT",
            vec![],
        ),
        // A cfg-gated const may be absent from the test build.
        (
            format!(
                "#[cfg(not(test))]\npub const LIMIT: usize = 3;\npub fn f(s: &str) -> bool {{\n    s.len() > LIMIT\n}}\n{module}"
            ),
            "s.len() > LIMIT",
            vec![],
        ),
        // A gated local may be absent from the test build, and the name then
        // reads the other-gated const instead of the length.
        (
            format!(
                "#[cfg(test)]\nconst N: usize = 9;\npub fn f(s: &str) -> bool {{\n    #[cfg(not(test))]\n    let N = s.len();\n    N > 3\n}}\n{module}"
            ),
            "N > 3",
            vec![],
        ),
        // A byte count of something else (`s.bytes().count()`) is not read.
        (
            format!("pub fn f(s: &str) -> bool {{\n    s.bytes().count() > 3\n}}\n{module}"),
            "s.bytes().count() > 3",
            vec![],
        ),
    ];
    for (source, needle, written) in cases {
        let seam = seam_at(
            "src/lib.rs",
            &source,
            needle,
            SeamKind::PredicateBoundary,
            boundary(needle),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        let choose = stub.fill_ins.iter().any(|f| f.starts_with("ripr: choose"));
        if written.is_empty() {
            assert!(choose, "{needle}: {:?}\n{}", stub.fill_ins, stub.text);
        } else {
            assert!(!choose, "{needle}: {:?}\n{}", stub.fill_ins, stub.text);
            for line in written {
                assert!(stub.text.contains(line), "{needle}: {line}\n{}", stub.text);
            }
            // The value stays the reader's to write.
            assert!(stub.text.contains("= todo!("), "{needle}: {}", stub.text);
        }
    }
    Ok(())
}

#[test]
fn boundary_stub_copies_other_inputs_from_an_existing_test() -> Result<(), String> {
    let owner = "pub fn tax(amount: i64, rate: u32) -> Result<i64, u8> {\n    if amount < 0 {\n        return Err(1);\n    }\n    if rate >= 10_000 {\n        return Err(2);\n    }\n    Ok(amount)\n}\n";
    let module_for = |owner: &str, body: &str| {
        format!("{owner}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n{body}\n}}\n")
    };
    let module = |body: &str| module_for(owner, body);
    // (source, the copied `amount` and the test it came from; None = stays a fill-in)
    let cases: Vec<(String, Option<(&str, &str)>)> = vec![
        // An assertion's direct argument.
        (
            module(
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(10_000, 825), Ok(10_000));\n    }\n",
            ),
            Some(("10_000", "computes")),
        ),
        // A statement call with a negative literal is the first in source
        // order, but `amount = -5` returns `Err(1)` before the changed
        // comparison, so the next call's value is copied instead.
        (
            module(
                "    #[test]\n    fn rejects() {\n        let r = tax(-5, 100);\n        assert!(r.is_err());\n    }\n    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            Some(("7", "computes")),
        ),
        // A negative literal that passes the guard is copied from a `let`.
        (
            module_for(
                &owner.replace("amount < 0", "amount < -10"),
                "    #[test]\n    fn small() {\n        let r = tax(-5, 100);\n        assert!(r.is_ok());\n    }\n",
            ),
            Some(("-5", "small")),
        ),
        // Calls that may never run are skipped: an assertion's message
        // argument, a call under `if`, one behind `&&`, and an ignored test.
        // `assert_eq!` with square brackets reads like parentheses.
        (
            module(
                "    #[test]\n    fn message() {\n        assert!(true, \"{:?}\", tax(1, 825));\n    }\n    #[test]\n    fn branch() {\n        if false {\n            let _ = tax(2, 825);\n        }\n    }\n    #[test]\n    fn short() {\n        let ok = false && tax(3, 825).is_ok();\n        assert!(!ok);\n    }\n    #[test]\n    #[ignore]\n    fn ignored() {\n        let _ = tax(4, 825);\n    }\n    #[test]\n    fn computes() {\n        assert_eq![tax(9, 825), Ok(9)];\n    }\n",
            ),
            Some(("9", "computes")),
        ),
        // A local bound from the copied value steers a guard the evaluator
        // cannot decide.
        (
            module_for(
                &owner.replace(
                    "    if amount < 0 {",
                    "    let doubled = amount * 2;\n    if doubled < 0 {",
                ),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // A `let` rebinding the copied name, a `let ... else` that may
        // return, and a format capture are refused before the comparison.
        (
            module_for(
                &owner.replace("    if amount < 0 {", "    let amount = amount * 100;\n    if amount > 50 {"),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        (
            module_for(
                &owner.replace(
                    "    if amount < 0 {",
                    "    let Some(_left) = amount.checked_sub(10) else {\n        return Err(3);\n    };\n    if amount < 0 {",
                ),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        (
            module_for(
                &owner.replace(
                    "    if amount < 0 {",
                    "    if format!(\"{amount}\").len() < 2 {\n        return Err(3);\n    }\n    if amount < 0 {",
                ),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // A `let` rebinding a written input before the guard.
        (
            module_for(
                &owner.replace("    if amount < 0 {", "    let rate = rate / 2;\n    if amount < 0 {"),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // A `return` or `?` earlier in the test, and a non-standard derive in
        // the module, block the copy.
        (
            module(
                "    #[test]\n    fn early() -> Result<(), u8> {\n        tax(1, 1)?;\n        let _ = tax(2, 825);\n        Ok(())\n    }\n",
            ),
            None,
        ),
        (
            module(
                "    #[derive(Debug, Gen)]\n    struct S;\n    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // So does an attribute macro on a test, which may rewrite it.
        (
            module(
                "    #[test]\n    #[traced]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // A comparison inside a closure runs only when the closure is
        // called, which the copied value may decide after the comparison.
        (
            module_for(
                &owner.replace(
                    "    if rate >= 10_000 {",
                    "    if std::iter::repeat_with(|| rate >= 10_000).take(amount as usize).any(|b| b) {",
                ),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // A `?` inside an assertion before the call may leave the test.
        (
            module(
                "    #[test]\n    fn early() -> Result<(), u8> {\n        assert_eq!(tax(1, 1)?, 1);\n        let _ = tax(2, 825);\n        Ok(())\n    }\n",
            ),
            None,
        ),
        // A standard derive does not.
        (
            module(
                "    #[derive(Debug, Clone, PartialEq)]\n    struct S;\n    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            Some(("7", "computes")),
        ),
        // The copied value is read inside the changed statement before the
        // comparison.
        (
            module_for(
                &owner.replace(
                    "    if rate >= 10_000 {",
                    "    if amount > 5 && rate >= 10_000 {",
                ),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // A parameter whose declared type is an alias takes no copied value.
        (
            module_for(
                &format!(
                    "pub type Cents = i64;\n{}",
                    owner.replace("amount: i64", "amount: Cents")
                ),
                "    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        // `#[macro_use]`, a rename to a standard macro's name, and a
        // `macro` item may each replace `assert_eq!`.
        (
            module(
                "    #[macro_use]\n    mod helpers {}\n    #[test]\n    fn computes() {\n        assert_eq!(tax(7, 825), Ok(7));\n    }\n",
            ),
            None,
        ),
        (
            module(
                "    use crate::checks::equal as assert_eq;\n    #[test]\n    fn computes() {\n        let _ = tax(7, 825);\n    }\n",
            ),
            None,
        ),
        (
            format!(
                "macro assert_eq($a:expr, $b:expr) {{}}\n{}",
                module("    #[test]\n    fn computes() {\n        let _ = tax(7, 825);\n    }\n")
            ),
            None,
        ),
        // A `should_panic` test, a closure, a cfg-gated statement, a helper
        // that is not a test, and a ripr stub are skipped for a later call.
        (
            module(
                "    #[test]\n    #[should_panic]\n    fn panics() {\n        assert_eq!(tax(1, 1), Ok(9));\n    }\n    #[test]\n    fn lazy() {\n        let f = || tax(2, 1);\n        let _ = f;\n    }\n    #[test]\n    fn gated() {\n        #[cfg(feature = \"x\")]\n        let _ = tax(3, 1);\n    }\n    fn helper() {\n        let _ = tax(4, 1);\n    }\n    #[test]\n    fn stub() {\n        let _ = tax(5, 1);\n        let _: i64 = todo!(\"ripr: choose `x`\");\n    }\n    #[test]\n    fn computes() {\n        assert_eq!(tax(6, 825), Ok(6));\n    }\n",
            ),
            Some(("6", "computes")),
        ),
        // A non-literal argument.
        (
            module(
                "    const A: i64 = 1;\n    #[test]\n    fn computes() {\n        assert_eq!(tax(A, 825), Ok(1));\n    }\n",
            ),
            None,
        ),
        // A qualified call, or one wrapped in a method call.
        (
            module(
                "    #[test]\n    fn computes() {\n        assert_eq!(super::tax(1, 825), Ok(1));\n        assert!(tax(1, 825).is_ok());\n    }\n",
            ),
            None,
        ),
        // A binding of the owner's name in the module takes the call.
        (
            module(
                "    #[test]\n    fn computes() {\n        let tax = |a: i64, _r: u32| -> Result<i64, u8> { Ok(a) };\n        assert_eq!(tax(1, 825), Ok(1));\n    }\n",
            ),
            None,
        ),
        // A raw identifier binds the same name.
        (
            module(
                "    #[test]\n    fn computes() {\n        let r#tax = |a: i64, _r: u32| -> Result<i64, u8> { Ok(a) };\n        assert_eq!(tax(1, 825), Ok(1));\n    }\n",
            ),
            None,
        ),
        // So does a function of that name declared in the module.
        (
            module(
                "    fn tax(a: i64, _r: u32) -> Result<i64, u8> { Ok(a) }\n    #[test]\n    fn computes() {\n        assert_eq!(tax(1, 825), Ok(1));\n    }\n",
            ),
            None,
        ),
        // A file-local `assert_eq!` is not the standard assertion.
        (
            format!(
                "macro_rules! assert_eq {{ ($a:expr, $b:expr) => {{}}; }}\n{}",
                module(
                    "    #[test]\n    fn computes() {\n        assert_eq!(tax(1, 825), Ok(1));\n    }\n"
                )
            ),
            None,
        ),
        // A statement macro may emit a function of the owner's name.
        (
            format!(
                "macro_rules! fake {{ () => {{ fn tax(a: &str, _r: u32) -> Result<i64, u8> {{ Ok(a.len() as i64) }} }}; }}\n{}",
                module(
                    "    #[test]\n    fn computes() {\n        fake!();\n        assert_eq!(tax(\"abc\", 825), Ok(3));\n    }\n"
                )
            ),
            None,
        ),
        // An imported `assert_eq!` may never type-check its arguments.
        (
            module(
                "    use crate::testing::assert_eq;\n    #[test]\n    fn computes() {\n        assert_eq!(tax(1, 825), Ok(1));\n    }\n",
            ),
            None,
        ),
        // A standard macro beside the call does not block the copy.
        (
            module(
                "    #[test]\n    fn computes() {\n        println!(\"{}\", 1);\n        let v = vec![1];\n        assert_eq!(tax(8, 825), Ok(8));\n        assert_eq!(v.len(), 1);\n    }\n",
            ),
            Some(("8", "computes")),
        ),
        // No existing test module: nothing to copy from.
        (owner.to_string(), None),
    ];
    for (source, copied) in cases {
        let needle = "rate >= 10_000";
        let seam = seam_at(
            "src/lib.rs",
            &source,
            needle,
            SeamKind::PredicateBoundary,
            boundary(needle),
        )?;
        let stub =
            rust_test_stub(&seam, None, &source, None).map_err(|r| r.reason().to_string())?;
        assert!(
            stub.text.contains("let rate: u32 = 10_000;"),
            "{}",
            stub.text
        );
        // The expected value always stays the developer's to write.
        assert!(
            stub.text.contains("let expected: Result<i64, u8> = todo!("),
            "{}",
            stub.text
        );
        match copied {
            Some((amount, test)) => {
                assert!(
                    stub.text.contains(&format!("let amount: i64 = {amount};")),
                    "{source}\n{}",
                    stub.text
                );
                assert!(
                    stub.text.contains(&format!(
                        "// ripr: `amount` as in the existing test `{test}`; change it if this case needs another value"
                    )),
                    "{}",
                    stub.text
                );
                assert_eq!(
                    stub.input_setup.as_deref(),
                    Some(format!("amount = {amount}, as in the existing test `{test}`").as_str())
                );
                assert_eq!(stub.derived_inputs, vec!["rate = 10_000".to_string()]);
                assert!(
                    !stub.fill_ins.iter().any(|f| f.starts_with("ripr: choose")),
                    "{:?}",
                    stub.fill_ins
                );
                let expected = stub
                    .fill_ins
                    .iter()
                    .find(|f| f.starts_with("ripr: write the value"))
                    .ok_or("expected fill-in")?;
                assert!(
                    expected.contains(&format!("for amount = {amount}, rate = 10_000")),
                    "{expected}"
                );
            }
            None => {
                assert!(
                    stub.text.contains(" = todo!(\"ripr: choose `amount`\");"),
                    "{}",
                    stub.text
                );
                assert_eq!(stub.input_setup, None);
            }
        }
    }
    Ok(())
}

#[test]
fn copied_literal_must_fit_the_parameter_type() {
    use super::reused_inputs::literal_fits;
    let fits = [
        ("i64", "10_000"),
        ("i64", "-3"),
        ("u8", "0x1e"),
        ("u32", "5u32"),
        ("f64", "1.5"),
        ("f64", "2.0"),
        ("f64", "2f64"),
        ("u8", "255"),
        ("usize", "5usize"),
        ("isize", "-1_000isize"),
        ("i8", "-128"),
        ("f32", "1e3"),
        ("bool", "true"),
        ("char", "'c'"),
        ("&str", "\"w\""),
        ("&'_ str", "\"w\""),
        ("&'static str", "r\"w\""),
        ("&[u8]", "b\"ab\""),
    ];
    for (ty, literal) in fits {
        assert!(literal_fits(ty, literal), "{ty} takes {literal}");
    }
    let misfits = [
        ("u32", "-3"),
        ("f64", "2"),
        ("u8", "300"),
        ("i8", "-129"),
        ("u32", "4294967296"),
        ("u32", "5u8"),
        ("i64", "1.5"),
        ("i64", "\"w\""),
        ("f32", "1f64"),
        ("f64", "3i32"),
        ("bool", "1"),
        ("&str", "'c'"),
        ("&[u8]", "\"ab\""),
        ("Cents", "5"),
        ("impl Into<i64>", "5"),
        ("&mut i64", "5"),
        ("Option<i64>", "5"),
    ];
    for (ty, literal) in misfits {
        assert!(!literal_fits(ty, literal), "{ty} refuses {literal}");
    }
}
