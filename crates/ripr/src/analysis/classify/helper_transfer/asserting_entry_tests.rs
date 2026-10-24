//! The asserting entry of a method-hop chain (#9329, RIPR-SPEC-0158).

use super::*;
use crate::analysis::facts::OwnedRustIndex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn index_of(files: &[(&str, &str)]) -> Result<RustIndex, String> {
    let mut by_file = BTreeMap::new();
    let mut functions = Vec::new();
    let mut tests = Vec::new();
    for (path, source) in files {
        let facts =
            crate::analysis::syntax::ra::summarize_file_with_parser(Path::new(path), source)?;
        functions.extend(facts.functions.clone());
        tests.extend(facts.tests.clone());
        by_file.insert(PathBuf::from(path), facts);
    }
    Ok(RustIndex::from_owned(OwnedRustIndex {
        functions,
        tests,
        files: by_file,
        ..Default::default()
    }))
}

fn chain_of(source: &str) -> Result<(HelperChain, RustIndex), String> {
    let index = index_of(&[("src/lib.rs", source)])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    Ok((chain, index))
}

const BODY: &str = "assert_eq!(expected.to_string(), b.to_string_as(si));";
const METHOD_BODY: &str = "fmt_size(self.0, si_unit)";

fn source(helper_body: &str, method_body: &str) -> String {
    format!(
        "pub struct ByteSize(pub u64);\n\
pub fn fmt_size(bytes: u64, si: bool) -> String {{\n    if si {{ bytes.to_string() }} else {{ String::new() }}\n}}\n\
impl ByteSize {{\n    pub fn to_string_as(&self, si_unit: bool) -> String {{\n        {method_body}\n    }}\n}}\n\
#[cfg(test)]\nmod tests {{\n    use super::*;\n    fn check(expected: &str, b: ByteSize, si: bool) {{\n        {helper_body}\n    }}\n    #[test]\n    fn t() {{\n        check(\"x\", ByteSize(1), true);\n    }}\n}}\n"
    )
}

fn admitted(helper_body: &str, method_body: &str) -> Result<bool, String> {
    let (chain, index) = chain_of(&source(helper_body, method_body))?;
    assert_eq!(chain.hops.len(), 2, "{chain:?}");
    Ok(asserting_method_entry("fmt_size", &chain.hops, &index).is_some())
}

#[test]
fn a_single_assertion_on_the_method_call_over_a_forwarding_method_is_admitted() -> Result<(), String>
{
    assert!(admitted(BODY, METHOD_BODY)?);
    // Operand order and a borrowed operand do not matter.
    assert!(admitted(
        "assert_eq!(&b.to_string_as(si), expected);",
        METHOD_BODY
    )?);
    // A trailing semicolon is optional.
    assert!(admitted(
        "assert_eq!(expected, b.to_string_as(si))",
        METHOD_BODY
    )?);
    Ok(())
}

#[test]
fn a_method_that_transforms_the_result_is_not_admitted() -> Result<(), String> {
    for method in [
        "fmt_size(self.0, si_unit).to_uppercase()",
        "let shown = fmt_size(self.0, si_unit);\n        shown",
        "let _ = fmt_size(self.0, si_unit);\n        String::new()",
        "format!(\"{}!\", fmt_size(self.0, si_unit))",
    ] {
        assert!(!admitted(BODY, method)?, "{method}");
    }
    Ok(())
}

#[test]
fn a_helper_that_does_not_compare_the_call_result_alone_is_not_admitted() -> Result<(), String> {
    for helper in [
        // The result is dropped.
        "b.to_string_as(si);\n        assert!(!expected.is_empty());",
        "let _ = b.to_string_as(si);\n        assert_eq!(expected, expected);",
        // Another statement can reshape or shadow the call.
        "let b = ByteSize(2);\n        assert_eq!(expected, b.to_string_as(si));",
        // The result is transformed before the comparison.
        "assert_eq!(expected, b.to_string_as(si).to_uppercase());",
        "assert_eq!(expected.len(), b.to_string_as(si).len());",
        // The expected side is computed from the same call or receiver.
        "assert_eq!(b.to_string_as(si), b.to_string_as(si));",
        "assert_eq!(b.0.to_string(), b.to_string_as(si));",
        // The call sits under a branch the macro does not cover.
        "if si { assert_eq!(expected, b.to_string_as(si)); }",
        // The expected side calls the owner directly.
        "assert_eq!(fmt_size(1, true), b.to_string_as(si));",
    ] {
        let (chain, index) = chain_of(&source(helper, METHOD_BODY))?;
        assert!(
            asserting_method_entry("fmt_size", &chain.hops, &index).is_none(),
            "{helper}: {chain:?}"
        );
    }
    Ok(())
}

#[test]
fn a_production_entry_is_never_an_asserting_entry() -> Result<(), String> {
    let source = "pub struct ByteSize(pub u64);\n\
pub fn fmt_size(bytes: u64, si: bool) -> String {\n    if si { bytes.to_string() } else { String::new() }\n}\n\
impl ByteSize {\n    pub fn to_string_as(&self, si_unit: bool) -> String {\n        fmt_size(self.0, si_unit)\n    }\n}\n\
pub fn describe(b: &ByteSize, si: bool) -> String {\n    assert_eq!(\"x\", b.to_string_as(si))\n}\n";
    let (chain, index) = chain_of(source)?;
    assert_eq!(chain.entry_name(), Some("describe"), "{chain:?}");
    assert!(asserting_method_entry("fmt_size", &chain.hops, &index).is_none());
    Ok(())
}

const MIN_OWNER: &str = "pub struct Size(pub u64);\n\
pub fn min(bytes: u64, si: u64) -> u64 {\n    if bytes <= si { 1 } else { 2 }\n}\n\
mod display;\n\
#[cfg(test)]\nmod tests {\n    use super::*;\n    fn check(want: u64, size: Size, si: u64) {\n        assert_eq!(want, size.shown(si));\n    }\n    #[test]\n    fn t() {\n        check(1, Size(1), 1);\n    }\n}\n";

fn min_chain(display: &str) -> Result<(HelperChain, RustIndex), String> {
    let index = index_of(&[("src/lib.rs", MIN_OWNER), ("src/display.rs", display)])?;
    let chain = resolve_chain("min", &index, true, &[]);
    Ok((chain, index))
}

#[test]
fn a_hop_calling_the_owner_name_resolves_to_the_workspace_function_is_admitted()
-> Result<(), String> {
    let (chain, index) = min_chain(
        "use super::Size;\nimpl Size {\n    pub fn shown(&self, si: u64) -> u64 {\n        min(self.0, si)\n    }\n}\n",
    )?;
    assert_eq!(chain.hops.len(), 2, "{chain:?}");
    assert!(asserting_method_entry("min", &chain.hops, &index).is_some());
    Ok(())
}

#[test]
fn a_hop_whose_file_imports_a_foreign_item_of_the_owner_name_is_not_admitted() -> Result<(), String>
{
    // The method calls `std::cmp::min`, so the result it returns is not the
    // changed `min`'s (Steven's review of #9642).
    let (chain, index) = min_chain(
        "use super::Size;\nuse std::cmp::min;\nimpl Size {\n    pub fn shown(&self, si: u64) -> u64 {\n        min(self.0, si)\n    }\n}\n",
    )?;
    assert_eq!(chain.hops.len(), 2, "{chain:?}");
    assert!(asserting_method_entry("min", &chain.hops, &index).is_none());
    // A body that rebinds the name is just as unresolved.
    let (chain, index) = min_chain(
        "use super::Size;\nimpl Size {\n    pub fn shown(&self, si: u64) -> u64 {\n        let min = |a: u64, b: u64| a + b;\n        min(self.0, si)\n    }\n}\n",
    )?;
    assert!(asserting_method_entry("min", &chain.hops, &index).is_none());
    Ok(())
}
