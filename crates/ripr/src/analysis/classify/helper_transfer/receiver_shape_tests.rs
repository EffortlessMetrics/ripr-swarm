//! Method-call receiver shapes the helper chain binds (#15301): `self` after
//! any non-path character, and a constructed receiver on earlier lines of a
//! rustfmt-chained call.

use super::*;
use crate::analysis::facts::OwnedRustIndex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn index_of(files: &[(&str, &str)]) -> Result<RustIndex, String> {
    let mut functions = Vec::new();
    let mut tests = Vec::new();
    let mut by_file = BTreeMap::new();
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

const TYPE: &str = "pub struct P { s: u64 }\n";

/// The callers resolved above `unit`, nearest first, and the chain.
fn hops_above_unit(files: &[(&str, &str)]) -> Result<(Vec<String>, HelperChain), String> {
    let chain = resolve_chain("unit", &index_of(files)?, true, &[]);
    let names = chain
        .hops
        .iter()
        .map(|hop| hop.caller.name.clone())
        .collect();
    Ok((names, chain))
}

fn lib(go_body: &str, top_body: &str) -> String {
    format!(
        "{TYPE}impl P {{\n    fn go(&self) -> u64 {{\n{go_body}\n    }}\n    fn unit(&self) -> u64 {{ self.s * 2 }}\n}}\npub fn top(n: u64) -> u64 {{\n{top_body}\n}}\n"
    )
}

const ONE_LINE_TOP: &str = "    P { s: n }.go()";

fn expect_chain(source: &str, label: &str) -> Result<(), String> {
    let (names, chain) = hops_above_unit(&[("src/lib.rs", source)])?;
    assert_eq!(names, ["go", "top"], "{label}: {chain:?}");
    assert!(chain.hops.iter().all(|hop| hop.receiver.is_none()), "{label}");
    Ok(())
}

fn expect_stop_at_go(source: &str, label: &str) -> Result<(), String> {
    let (names, chain) = hops_above_unit(&[("src/lib.rs", source)])?;
    assert_eq!(names, ["go"], "{label}: {chain:?}");
    Ok(())
}

#[test]
fn self_receiver_binds_after_any_non_path_character() -> Result<(), String> {
    for go_body in [
        "        let r = self.unit();\n        r",
        "        let r=self.unit();\n        r",
        "        let r=self.unit()?;\n        r",
        "        {self.unit()}",
        "        f(1,self.unit())",
        "        !self.unit()",
        "        (self.unit())",
        "        &self.unit()",
        "        self.unit()",
    ] {
        expect_chain(&lib(go_body, ONE_LINE_TOP), go_body)?;
    }
    Ok(())
}

#[test]
fn self_as_part_of_a_path_or_identifier_does_not_bind() {
    for (text, binds) in [
        ("let r=self.unit();", true),
        ("{self.unit()}", true),
        ("self.field.unit()", false),
        ("myself.unit()", false),
        ("x.self.unit()", false),
        ("Self::unit()", false),
        ("my_self.unit()", false),
        ("a::self.unit()", false),
        ("$self.unit()", false),
    ] {
        assert_eq!(
            matches!(
                method_call_site(text, "unit"),
                Some((_, MethodReceiver::SelfReceiver))
            ),
            binds,
            "{text}"
        );
    }
}

#[test]
fn self_field_receiver_keeps_today_s_decline() -> Result<(), String> {
    // `self.inner` may be another type's value, so `go` is not linked as
    // a caller of `unit` at all.
    let source = lib("        self.inner.unit()", ONE_LINE_TOP);
    let (names, chain) = hops_above_unit(&[("src/lib.rs", &source)])?;
    assert!(names.is_empty(), "self.field.unit(): {chain:?}");
    Ok(())
}

#[test]
fn own_line_constructed_receivers_bind() -> Result<(), String> {
    for (label, top_body) in [
        (
            "struct literal",
            "    Parser {\n        s: n,\n    }\n    .go()",
        ),
        ("type path call", "    P::new(\n        n,\n    )\n    .go()"),
        ("one-line constructor", "    P::new(n)\n        .go()"),
    ] {
        let source = lib("        self.unit()", top_body)
            .replace("Parser {", "P {")
            .replace(
                "impl P {",
                "impl P {\n    fn new(s: u64) -> P { P { s } }",
            );
        expect_chain(&source, label)?;
    }
    Ok(())
}

#[test]
fn own_line_receiver_that_is_not_a_constructed_local_type_stays_unbound() -> Result<(), String> {
    for (label, top_body) in [
        ("variable on its own line", "    let p = P { s: n };\n    p\n    .go()"),
        ("variable call line", "    let p = P { s: n };\n    p.go()"),
        ("call result", "    make()\n    .go()"),
        ("lowercase path call", "    build::new(\n        n,\n    )\n    .go()"),
        ("method result", "    P { s: n }.wrap()\n    .go()"),
        ("other type", "    Q { s: n }\n    .go()"),
        ("argument not receiver", "    check(P { s: n })\n    .go()"),
    ] {
        expect_stop_at_go(&lib("        self.unit()", top_body), label)?;
    }
    Ok(())
}

#[test]
fn imported_type_on_an_own_line_receiver_stays_unbound() -> Result<(), String> {
    let model = format!("{TYPE}impl P {{\n    pub fn go(&self) -> u64 {{ self.unit() }}\n    fn unit(&self) -> u64 {{ self.s * 2 }}\n}}\n");
    let chained = "pub fn top(n: u64) -> u64 {\n    P {\n        s: n,\n    }\n    .go()\n}\n";
    let local = format!("{model}{chained}");
    let (names, _) = hops_above_unit(&[("src/lib.rs", &local)])?;
    assert_eq!(names, ["go", "top"]);
    // The same caller in a file that imports `P` may mean another module's
    // type, so it never binds by bare name.
    let imported = format!("use other::P;\n{chained}");
    let (names, _) = hops_above_unit(&[("src/lib.rs", &model), ("src/top.rs", &imported)])?;
    assert_eq!(names, ["go"]);
    Ok(())
}

#[test]
fn same_named_methods_on_two_impls_stay_unbound() -> Result<(), String> {
    let source = format!(
        "{TYPE}pub struct Q {{ s: u64 }}\nimpl P {{\n    fn go(&self) -> u64 {{ self.unit() }}\n    fn unit(&self) -> u64 {{ self.s * 2 }}\n}}\nimpl Q {{\n    fn go(&self) -> u64 {{ 0 }}\n    fn unit(&self) -> u64 {{ self.s }}\n}}\npub fn top(n: u64) -> u64 {{\n    P {{\n        s: n,\n    }}\n    .go()\n}}\n"
    );
    let (names, chain) = hops_above_unit(&[("src/lib.rs", &source)])?;
    assert!(names.is_empty(), "{chain:?}");
    Ok(())
}

#[test]
fn own_line_receiver_carries_reach_and_no_forwarding() -> Result<(), String> {
    let source = lib(
        "        self.unit()",
        "    P {\n        s: n,\n    }\n    .go()",
    );
    let (_, chain) = hops_above_unit(&[("src/lib.rs", &source)])?;
    let top = chain
        .hops
        .iter()
        .find(|hop| hop.caller.name == "top")
        .ok_or("top hop")?;
    // The recorded call text is the `.go()` line alone: it is not a
    // constructed-receiver method site, so no result is forwarded through it.
    assert_eq!(top.call_text.trim(), ".go()");
    assert!(method_call_site(&top.call_text, "go").is_none());
    assert!(!chain_forwards_owner_result("unit", &chain));
    Ok(())
}
