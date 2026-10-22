//! Multi-caller binding (#9329, RIPR-SPEC-0158): a callee with several
//! direct callers binds through the one caller a test reaches.

use super::*;
use crate::analysis::facts::OwnedRustIndex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn index_of(source: &str) -> Result<RustIndex, String> {
    let facts =
        crate::analysis::syntax::ra::summarize_file_with_parser(Path::new("src/lib.rs"), source)?;
    let mut by_file = BTreeMap::new();
    by_file.insert(PathBuf::from("src/lib.rs"), facts.clone());
    Ok(RustIndex::from_owned(OwnedRustIndex {
        functions: facts.functions.clone(),
        tests: facts.tests.clone(),
        files: by_file,
        ..Default::default()
    }))
}

const PRELUDE: &str = "pub struct ByteSize(pub u64);\n\
pub fn fmt_size(bytes: u64, si: bool) -> String {\n\
    if si { bytes.to_string() } else { String::new() }\n\
}\n\
impl ByteSize {\n\
    pub fn to_string_as(&self, si_unit: bool) -> String {\n\
        fmt_size(self.0, si_unit)\n\
    }\n\
}\n";
/// A second production caller of the owner, shaped like `Display::fmt`.
const OTHER_CALLER: &str = "pub fn render(b: &ByteSize) -> String {\n    fmt_size(b.0, true)\n}\n";
const CHECK: &str = "fn check(expected: &str, b: ByteSize, si: bool) {\n        assert_eq!(expected, b.to_string_as(si));\n    }\n";

fn crate_with(extra: &str, tests_body: &str) -> String {
    format!(
        "{PRELUDE}{extra}#[cfg(test)]\nmod tests {{\n    use super::*;\n    {CHECK}    #[test]\n    fn t() {{\n{tests_body}\n    }}\n}}\n"
    )
}

fn chain_of(source: &str) -> Result<HelperChain, String> {
    Ok(resolve_chain("fmt_size", &index_of(source)?, true, &[]))
}

fn callers(chain: &HelperChain) -> Vec<&str> {
    chain
        .hops
        .iter()
        .map(|hop| hop.caller.name.as_str())
        .collect()
}

const CALLS: &str = "        check(\"215 B\", ByteSize(215), true);";

#[test]
fn a_caller_no_test_reaches_neither_binds_nor_blocks() -> Result<(), String> {
    let chain = chain_of(&crate_with(OTHER_CALLER, CALLS))?;
    assert_eq!(callers(&chain), vec!["to_string_as", "check"], "{chain:?}");
    assert_eq!(chain.hops[1].receiver.as_deref(), Some("b"));
    Ok(())
}

#[test]
fn the_reached_caller_is_chosen_whichever_is_declared_first() -> Result<(), String> {
    let source = crate_with("", CALLS).replace(
        "impl ByteSize {",
        &format!("{OTHER_CALLER}impl ByteSize {{"),
    );
    let chain = chain_of(&source)?;
    assert_eq!(callers(&chain), vec!["to_string_as", "check"], "{chain:?}");
    Ok(())
}

#[test]
fn a_test_reaching_a_second_caller_declines_the_chain() -> Result<(), String> {
    for body in [
        // One test calls both entries.
        "        check(\"215 B\", ByteSize(215), true);\n        render(&ByteSize(1));",
        // The second caller is reached by a test of its own.
        "        check(\"215 B\", ByteSize(215), true);\n    }\n    #[test]\n    fn u() {\n        render(&ByteSize(1));",
    ] {
        let chain = chain_of(&crate_with(OTHER_CALLER, body))?;
        assert!(chain.hops.is_empty(), "{chain:?}");
        assert!(
            chain
                .stop_above
                .as_deref()
                .is_some_and(|edge| edge.contains("more than one of them")),
            "{chain:?}"
        );
    }
    Ok(())
}

#[test]
fn a_test_calling_the_callee_directly_keeps_the_decline() -> Result<(), String> {
    let body = format!("{CALLS}\n        let _ = fmt_size(1, true);");
    let chain = chain_of(&crate_with("", &body))?;
    assert!(chain.hops.is_empty(), "{chain:?}");
    assert!(
        chain
            .stop_above
            .as_deref()
            .is_some_and(|edge| edge.contains("multi-caller binding is not transferred")),
        "{chain:?}"
    );
    Ok(())
}

#[test]
fn no_reached_caller_declines_with_the_multi_caller_edge() -> Result<(), String> {
    let chain = chain_of(&crate_with(OTHER_CALLER, "        let _ = 1;"))?;
    assert!(chain.hops.is_empty(), "{chain:?}");
    assert!(
        chain
            .stop_above
            .as_deref()
            .is_some_and(|edge| edge.contains("no related test reaches any of them")),
        "{chain:?}"
    );
    Ok(())
}

#[test]
fn a_reached_caller_whose_call_does_not_bind_carries_its_own_stop() -> Result<(), String> {
    // `to_string_as` mentions the owner twice: the hop cannot split it.
    let source = crate_with(OTHER_CALLER, CALLS).replace(
        "fmt_size(self.0, si_unit)\n",
        "let _ = fmt_size(self.0, si_unit);\n        fmt_size(self.0, si_unit)\n",
    );
    let chain = chain_of(&source)?;
    assert!(chain.hops.is_empty(), "{chain:?}");
    assert!(
        chain
            .stop_above
            .as_deref()
            .is_some_and(|edge| edge.contains("called 2 times")),
        "{chain:?}"
    );
    Ok(())
}

#[test]
fn more_callers_than_the_binding_bound_decline() -> Result<(), String> {
    let mut extra = String::new();
    for n in 0..MAX_MULTI_CALLER_BINDINGS {
        extra.push_str(&format!(
            "pub fn render{n}(b: &ByteSize) -> String {{\n    fmt_size(b.0, true)\n}}\n"
        ));
    }
    let chain = chain_of(&crate_with(&extra, CALLS))?;
    assert!(chain.hops.is_empty(), "{chain:?}");
    Ok(())
}
