//! #11738: the memoized exit record types give every owner the drop answer
//! the direct parse gave, and an owner text is parsed once per scope.

use super::EXIT_RECORD_READS;
use crate::analysis::classify::OwnerReturnPin;
use crate::analysis::rust_index::{FunctionSummary, RustIndex, summarize_file};
use crate::analysis::syntax::{parse_clean_source_file, run_memo};
use ra_ap_syntax::{AstNode, ast};
use std::path::{Path, PathBuf};

const LIB: &str = "src/lib.rs";

/// `returned_types_drop_plain` as it was before #11738, verbatim.
fn reference_returned_types_drop_plain(owner: &FunctionSummary, index: &RustIndex) -> bool {
    let source = owner.body.as_str();
    let Some(parse) = parse_clean_source_file(source) else {
        return false;
    };
    let Some(function) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return false;
    };
    let Some(body) = function.body() else {
        return false;
    };
    super::super::exit_position_records(&body)
        .iter()
        .all(|record| {
            record
                .path()
                .and_then(|path| path.segment())
                .is_some_and(|segment| {
                    crate::analysis::classify::element_field_pin::drop_is_plain(
                        index,
                        &segment.syntax().text().to_string(),
                    )
                })
        })
}

fn index(text: &str) -> RustIndex {
    let mut index = RustIndex::default();
    index.package_names.insert("demo".to_string());
    let facts = summarize_file(PathBuf::from(LIB), text.to_string());
    index.extend_functions(facts.functions.iter().cloned());
    index.extend_tests(facts.tests.iter().cloned());
    index.insert_file_only(PathBuf::from(LIB), facts);
    crate::analysis::facts::compose_fixture_source_roles(&mut index, Path::new(""));
    index
}

const HEADER: &str = "#[derive(Debug, PartialEq)]\npub struct Cfg {\n    pub n: u32,\n}\n\nimpl Drop for Cfg {\n    fn drop(&mut self) {\n        std::process::exit(0);\n    }\n}\n\n#[derive(Debug, PartialEq)]\npub struct Plain {\n    pub n: u32,\n}\n\npub mod deep {\n    #[derive(Debug, PartialEq)]\n    pub struct Cfg2 {\n        pub n: u32,\n    }\n}\n\n";

const BODIES: &[&str] = &[
    "pub fn make(x: u32) -> u32 {\n    let _guard = Cfg { n: 0 };\n    x * 3\n}\n",
    "pub fn make(x: u32) -> u32 {\n    sink(Cfg { n: 0 });\n    x * 3\n}\nfn sink(_c: Cfg) {}\n",
    "pub fn make(x: u32) -> Cfg {\n    Cfg { n: x * 3 }\n}\n",
    "pub fn make(x: u32) -> Cfg {\n    return Cfg { n: x * 3 };\n}\n",
    "pub fn make(x: u32) -> Plain {\n    Plain { n: x * 3 }\n}\n",
    "pub fn make(x: u32) -> Plain {\n    if x > 0 { Plain { n: x } } else { Plain { n: 0 } }\n}\n",
    "pub fn make(x: u32) -> Result<Plain, Cfg> {\n    if x > 0 { return Err(Cfg { n: 0 }); }\n    Ok(Plain { n: x })\n}\n",
    "pub fn make(x: u32) -> deep::Cfg2 {\n    deep::Cfg2 { n: x }\n}\n",
    "pub fn make(x: u32) -> (Plain, Cfg) {\n    (Plain { n: x }, Cfg { n: 1 })\n}\n",
    "pub fn make(x: u32) -> Plain {\n    let c = Plain { n: x * 3 };\n    c\n}\n",
    "pub fn make(x: u32) -> Plain {\n    match x { 0 => Plain { n: 0 }, _ => Plain { n: x } }\n}\n",
    "pub fn make(x: u32) -> u32 {\n    x * 3\n}\n",
];

fn make(index: &RustIndex) -> Option<&FunctionSummary> {
    index.functions().iter().find(|owner| owner.name == "make")
}

fn compare_every_body() -> Result<(usize, usize), String> {
    let (mut plain, mut refused) = (0, 0);
    for body in BODIES {
        let index = index(&format!("{HEADER}{body}"));
        let owner = make(&index).ok_or_else(|| format!("fixture lost `make`:\n{body}"))?;
        let expected = reference_returned_types_drop_plain(owner, &index);
        assert_eq!(
            OwnerReturnPin::returned_types_drop_plain(owner, &index),
            expected,
            "{body}"
        );
        if expected {
            plain += 1;
        } else {
            refused += 1;
        }
    }
    Ok((plain, refused))
}

#[test]
fn memoized_exit_types_answer_as_the_direct_parse_inside_and_outside_a_scope() -> Result<(), String>
{
    let outside = compare_every_body()?;
    // Both answers must occur, or the table proves nothing.
    assert!(outside.0 >= 3 && outside.1 >= 3, "{outside:?}");
    let _scope = run_memo::enter();
    assert_eq!(compare_every_body()?, outside);
    assert_eq!(compare_every_body()?, outside, "warm entries");
    Ok(())
}

#[test]
fn an_owner_text_is_parsed_once_per_scope_however_many_pins_ask() -> Result<(), String> {
    let reads = || EXIT_RECORD_READS.with(std::cell::Cell::get);
    let index = index(&format!("{HEADER}{}", BODIES[6]));
    let owner = make(&index).ok_or("fixture lost `make`")?;
    {
        let _scope = run_memo::enter();
        let start = reads();
        for _ in 0..40 {
            assert!(!OwnerReturnPin::returned_types_drop_plain(owner, &index));
        }
        assert_eq!(reads() - start, 1, "forty asks, one parse");
    }
    // A new scope parses again; outside a scope every ask parses.
    let start = reads();
    assert!(!OwnerReturnPin::returned_types_drop_plain(owner, &index));
    assert!(!OwnerReturnPin::returned_types_drop_plain(owner, &index));
    assert_eq!(reads() - start, 2);
    Ok(())
}
