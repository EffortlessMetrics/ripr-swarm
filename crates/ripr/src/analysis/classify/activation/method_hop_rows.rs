//! Rows bound through a method hop (#9329, RIPR-SPEC-0158).

use super::*;
use crate::analysis::classify::helper_transfer::resolve_chain;
use crate::analysis::facts::OwnedRustIndex;
use crate::analysis::rust_index::RustIndex;
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

const PRELUDE: &str = "pub const KIB: u64 = 1_024;\n\
pub struct ByteSize(pub u64);\n\
impl ByteSize {\n\
    pub const fn kib(size: u64) -> Self {\n\
        ByteSize(size * KIB)\n\
    }\n\
    pub fn to_string_as(&self, si_unit: bool) -> String {\n\
        fmt_size(self.0, si_unit)\n\
    }\n\
}\n\
pub fn fmt_size(bytes: u64, si: bool) -> String {\n\
    if si { bytes.to_string() } else { String::new() }\n\
}\n";

fn source(helper_call: &str, test_calls: &str) -> String {
    format!(
        "{PRELUDE}#[cfg(test)]\nmod tests {{\n    use super::*;\n    fn check(expected: &str, b: ByteSize, si: bool) {{\n        {helper_call}\n    }}\n    #[test]\n    fn t() {{\n{test_calls}\n    }}\n}}\n"
    )
}

const CALLS: &str = "        check(\"215 B\", ByteSize(215), true);\n        check(\"1 big\", ByteSize::kib(1), true);\n        check(\"1 big\", ByteSize::kib(1), false);";
const HELPER_CALL: &str = "assert_eq!(expected, b.to_string_as(si));";

/// The `(parameter, value)` cells of each owner row.
fn owner_rows(source: &str) -> Result<Vec<Vec<(String, String)>>, String> {
    let index = index_of(source)?;
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == "fmt_size")
        .ok_or("missing owner")?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    let tests: Vec<&TestSummary> = index.tests().iter().collect();
    let rows = helper_transferred_rows(&function_parameters(owner), &chain, &tests, &index);
    Ok(rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|cell| (cell.parameter, cell.value))
                .collect()
        })
        .collect())
}

fn row(bytes: &str, si: &str) -> Vec<(String, String)> {
    vec![
        ("bytes".to_string(), bytes.to_string()),
        ("si".to_string(), si.to_string()),
    ]
}

#[test]
fn receiver_field_binds_the_owner_parameter_for_a_literal_and_a_const_constructor()
-> Result<(), String> {
    assert_eq!(
        owner_rows(&source(HELPER_CALL, CALLS))?,
        vec![
            row("215", "true"),
            row("1024", "true"),
            row("1024", "false")
        ]
    );
    Ok(())
}

#[test]
fn no_receiver_cell_or_pseudo_parameter_reaches_an_owner_row() -> Result<(), String> {
    for row in owner_rows(&source(HELPER_CALL, CALLS))? {
        assert!(
            row.iter()
                .all(|(parameter, _)| parameter == "bytes" || parameter == "si"),
            "{row:?}"
        );
        assert!(row.iter().all(|(_, value)| !value.contains('(')), "{row:?}");
    }
    Ok(())
}

#[test]
fn one_unreadable_receiver_argument_withdraws_every_row() -> Result<(), String> {
    for calls in [
        // A non-literal tuple field.
        "        check(\"215 B\", ByteSize(215), true);\n        let n = 3;\n        check(\"x\", ByteSize(n), true);",
        // A non-literal constructor argument.
        "        check(\"215 B\", ByteSize(215), true);\n        let n = 3;\n        check(\"x\", ByteSize::kib(n), true);",
        // A constructor argument that is an expression.
        "        check(\"215 B\", ByteSize(215), true);\n        check(\"x\", ByteSize::kib(1 + 1), true);",
        // A second constructor path the evaluator cannot read.
        "        check(\"215 B\", ByteSize(215), true);\n        check(\"x\", ByteSize::default(), true);",
        // A binding rather than a construction.
        "        let b = ByteSize(1);\n        check(\"x\", b, true);",
    ] {
        assert_eq!(
            owner_rows(&source(HELPER_CALL, calls))?,
            Vec::<Vec<(String, String)>>::new(),
            "{calls}"
        );
    }
    Ok(())
}

#[test]
fn only_self_dot_a_tuple_index_in_range_binds() -> Result<(), String> {
    let bind = |expression: &str| -> Result<Vec<Vec<(String, String)>>, String> {
        let source = source(HELPER_CALL, CALLS).replace(
            "fmt_size(self.0, si_unit)",
            &format!("fmt_size({expression}, si_unit)"),
        );
        owner_rows(&source)
    };
    assert_eq!(bind("self.0")?.len(), 3);
    for expression in [
        // Out of range for a one-field tuple struct.
        "self.1",
        // Not a tuple index.
        "self.n",
        "self.0.clone()",
        "self.00",
        "self",
        "(self.0)",
        "self.0 + 1",
        // `self` is not a parameter of the method.
        "other.0",
        "b.0",
        // A literal still binds, independent of the receiver.
    ] {
        assert!(bind(expression)?.is_empty(), "{expression}");
    }
    assert_eq!(bind("2048")?.len(), 3);
    Ok(())
}

#[test]
fn named_field_struct_receiver_binds_nothing() -> Result<(), String> {
    let source = source(HELPER_CALL, CALLS)
        .replace(
            "pub struct ByteSize(pub u64);",
            "pub struct ByteSize { pub n: u64 }",
        )
        .replace("ByteSize(size * KIB)", "ByteSize { n: size * KIB }")
        .replace("self.0", "self.n");
    assert!(owner_rows(&source)?.is_empty());
    Ok(())
}

#[test]
fn production_entry_taking_a_borrowed_receiver_binds_through_the_method_hop() -> Result<(), String>
{
    let source = format!(
        "{PRELUDE}pub fn describe(b: &ByteSize, si: bool) -> String {{\n    b.to_string_as(si)\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {{\n        describe(&ByteSize(215), true);\n        describe(&ByteSize::kib(1), false);\n    }}\n}}\n"
    );
    let chain = resolve_chain("fmt_size", &index_of(&source)?, true, &[]);
    assert_eq!(chain.hops.len(), 2, "{chain:?}");
    assert_eq!(chain.entry_name(), Some("describe"));
    assert_eq!(chain.hops[1].receiver.as_deref(), Some("b"));
    assert_eq!(
        owner_rows(&source)?,
        vec![row("215", "true"), row("1024", "false")]
    );
    Ok(())
}

#[test]
fn receiver_cell_never_binds_through_a_plain_forwarding_hop() -> Result<(), String> {
    // The entry `check` only forwards `b` to `inner`, which makes the method
    // call. `check` is a test helper, so the chain is topped by `inner`, which
    // no test calls: nothing binds.
    let source = source(HELPER_CALL, CALLS).replace(
        "fn check(expected: &str, b: ByteSize, si: bool) {",
        "fn check(expected: &str, b: ByteSize, si: bool) {\n        inner(expected, b, si);\n    }\n    fn inner(expected: &str, b: ByteSize, si: bool) {",
    );
    let chain = resolve_chain("fmt_size", &index_of(&source)?, true, &[]);
    assert_eq!(chain.entry_name(), Some("inner"), "{chain:?}");
    assert!(owner_rows(&source)?.is_empty());
    Ok(())
}
