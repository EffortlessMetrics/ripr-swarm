//! Method-hop admit/decline rules (#9329, RIPR-SPEC-0158).

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

const OWNER: &str = "pub fn fmt_size(bytes: u64, si: bool) -> String {\n    if si { bytes.to_string() } else { String::new() }\n}\n";
const METHOD: &str = "impl ByteSize {\n    pub fn to_string_as(&self, si_unit: bool) -> String {\n        fmt_size(self.0, si_unit)\n    }\n}\n";
const HELPER: &str =
    "fn check(b: ByteSize, si: bool) {\n    assert_eq!(\"x\", b.to_string_as(si));\n}\n";

/// A crate with the owner `fmt_size`, the inherent method `to_string_as`
/// forwarding `self.0`, and a `#[cfg(test)]` helper.
fn crate_with(prelude: &str, method: &str, helper: &str) -> String {
    format!(
        "{prelude}pub struct ByteSize(pub u64);\n{OWNER}{method}#[cfg(test)]\nmod tests {{\n    use super::*;\n{helper}\n    #[test]\n    fn t() {{\n        check(ByteSize(1), true);\n    }}\n}}\n"
    )
}

/// The caller names and receivers of the chain above `fmt_size`, plus the
/// stop above it.
type HopSummary = Vec<(String, Option<String>)>;

fn chain_of(source: &str) -> Result<(HopSummary, Option<String>), String> {
    let index = index_of(&[("src/lib.rs", source)])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    Ok((
        chain
            .hops
            .iter()
            .map(|hop| (hop.caller.name.clone(), hop.receiver.clone()))
            .collect(),
        chain.stop_above,
    ))
}

fn assert_declined(source: &str, label: &str) -> Result<(), String> {
    let (hops, stop) = chain_of(source)?;
    assert_eq!(
        hops,
        vec![("to_string_as".to_string(), None)],
        "{label}: only the direct hop may resolve"
    );
    assert!(
        stop.as_deref()
            .is_some_and(|edge| edge.contains("is not statically splittable")),
        "{label}: {stop:?}"
    );
    Ok(())
}

#[test]
fn receiver_call_on_a_typed_parameter_resolves_a_method_hop() -> Result<(), String> {
    let index = index_of(&[("src/lib.rs", &crate_with("", METHOD, HELPER))])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    assert_eq!(chain.hops.len(), 2, "{chain:?}");
    assert_eq!(chain.hops[0].caller.name, "to_string_as");
    assert_eq!(chain.hops[0].receiver, None);
    assert_eq!(chain.hops[0].arguments, vec!["self.0", "si_unit"]);
    // The method hop's arguments stay positional and exclude the receiver.
    assert_eq!(chain.hops[1].caller.name, "check");
    assert_eq!(chain.hops[1].receiver.as_deref(), Some("b"));
    assert_eq!(chain.hops[1].arguments, vec!["si"]);
    assert_eq!(chain.entry_name(), Some("check"));
    Ok(())
}

#[test]
fn reference_parameter_and_every_self_receiver_form_resolve() -> Result<(), String> {
    for helper_signature in ["b: &ByteSize", "b: &mut ByteSize", "b: &  mut  ByteSize"] {
        let helper = format!(
            "fn check({helper_signature}, si: bool) {{\n    assert_eq!(\"x\", b.to_string_as(si));\n}}\n"
        );
        let (hops, _) = chain_of(&crate_with("", METHOD, &helper))?;
        assert_eq!(hops.len(), 2, "{helper_signature}");
    }
    for receiver in ["&self", "self"] {
        let method = METHOD.replace("&self", receiver);
        let (hops, _) = chain_of(&crate_with("", &method, HELPER))?;
        assert_eq!(hops.len(), 2, "{receiver}");
    }
    // A `&mut self` method may change the field before the owner reads it.
    assert_declined(
        &crate_with("", &METHOD.replace("&self", "&mut self"), HELPER),
        "&mut self receiver",
    )?;
    Ok(())
}

#[test]
fn receiver_that_is_not_a_parameter_keeps_the_stop() -> Result<(), String> {
    let local = "fn check(si: bool) {\n    let b = ByteSize(1);\n    assert_eq!(\"x\", b.to_string_as(si));\n}\n";
    assert_declined(&crate_with("", METHOD, local), "local receiver")?;
    let field =
        "fn check(w: Wrapper, si: bool) {\n    assert_eq!(\"x\", w.inner.to_string_as(si));\n}\n";
    assert_declined(
        &crate_with(
            "pub struct Wrapper { pub inner: ByteSize }\n",
            METHOD,
            field,
        ),
        "field receiver",
    )?;
    let call_result = "fn check(si: bool) {\n    assert_eq!(\"x\", make().to_string_as(si));\n}\n";
    assert_declined(&crate_with("", METHOD, call_result), "call-result receiver")?;
    let path = "fn check(si: bool) {\n    assert_eq!(\"x\", ByteSize::to_string_as(&ByteSize(1), si));\n}\n";
    assert_declined(&crate_with("", METHOD, path), "path-qualified call")?;
    let unnamed =
        "fn check(_: ByteSize, si: bool) {\n    assert_eq!(\"x\", _.to_string_as(si));\n}\n";
    assert_declined(&crate_with("", METHOD, unnamed), "wildcard receiver")
}

#[test]
fn parameter_whose_type_is_not_exactly_the_impl_type_keeps_the_stop() -> Result<(), String> {
    for (declared, prelude) in [
        ("b: Other", "pub struct Other(pub u64);\n"),
        ("b: &Other", "pub struct Other(pub u64);\n"),
        ("b: Box<ByteSize>", ""),
        ("b: &'static ByteSize", ""),
        ("b: Option<ByteSize>", ""),
        ("b: &&ByteSize", ""),
        ("b: impl Into<ByteSize>", ""),
        ("b: &dyn Show", "pub trait Show {}\n"),
        ("b: shapes::ByteSize", ""),
        ("b: ByteSizeX", "pub struct ByteSizeX(pub u64);\n"),
    ] {
        let helper = format!(
            "fn check({declared}, si: bool) {{\n    assert_eq!(\"x\", b.to_string_as(si));\n}}\n"
        );
        assert_declined(&crate_with(prelude, METHOD, &helper), declared)?;
    }
    let generic = "fn check<T>(b: T, si: bool) {\n    assert_eq!(\"x\", b.to_string_as(si));\n}\n";
    assert_declined(&crate_with("", METHOD, generic), "generic parameter")?;
    let tuple = "fn check((b, _n): (ByteSize, u8), si: bool) {\n    assert_eq!(\"x\", b.to_string_as(si));\n}\n";
    assert_declined(&crate_with("", METHOD, tuple), "destructured parameter")
}

#[test]
fn rebound_receiver_keeps_the_stop() -> Result<(), String> {
    for body in [
        "let b = ByteSize(2);\n    assert_eq!(\"x\", b.to_string_as(si));",
        "let (b, _) = (ByteSize(2), 0);\n    assert_eq!(\"x\", b.to_string_as(si));",
        "b = ByteSize(2);\n    assert_eq!(\"x\", b.to_string_as(si));",
        "if let Some(b) = Some(ByteSize(2)) {\n        assert_eq!(\"x\", b.to_string_as(si));\n    }",
        "for b in [ByteSize(2)] {\n        assert_eq!(\"x\", b.to_string_as(si));\n    }",
    ] {
        let helper = format!("fn check(mut b: ByteSize, si: bool) {{\n    {body}\n}}\n");
        assert_declined(&crate_with("", METHOD, &helper), body)?;
    }
    Ok(())
}

#[test]
fn more_than_one_call_site_keeps_the_stop() -> Result<(), String> {
    let two_lines = "fn check(b: ByteSize, si: bool) {\n    assert_eq!(\"x\", b.to_string_as(si));\n    assert_eq!(\"y\", b.to_string_as(!si));\n}\n";
    let (hops, stop) = chain_of(&crate_with("", METHOD, two_lines))?;
    assert_eq!(hops.len(), 1);
    assert!(stop.is_some_and(|edge| edge.contains("is not statically splittable")));
    let one_line = "fn check(b: ByteSize, si: bool) {\n    assert_eq!(b.to_string_as(si), b.to_string_as(!si));\n}\n";
    assert_declined(&crate_with("", METHOD, one_line), "two calls on one line")?;
    let nested = "fn check(b: ByteSize, si: bool) {\n    assert_eq!(\"x\", b.to_string_as(b.to_string_as(si).is_empty()));\n}\n";
    assert_declined(&crate_with("", METHOD, nested), "nested call")
}

#[test]
fn callee_that_is_not_one_inherent_self_method_keeps_the_stop() -> Result<(), String> {
    // Not unique: a second `to_string_as` anywhere stops the hop above it.
    let second = format!(
        "{METHOD}pub struct Other(pub u64);\nimpl Other {{\n    pub fn to_string_as(&self, si_unit: bool) -> String {{ String::new() }}\n}}\n"
    );
    let index = index_of(&[("src/lib.rs", &crate_with("", &second, HELPER))])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    assert_eq!(chain.hops.len(), 1, "{chain:?}");
    assert!(
        chain
            .stop_above
            .is_some_and(|edge| edge.contains("not a unique function"))
    );
    // A trait-impl method of the same name is not an inherent method.
    let trait_method = METHOD
        .replace("impl ByteSize", "impl Show for ByteSize")
        .replace("pub fn", "fn");
    let source = crate_with(
        "pub trait Show {\n    fn to_string_as(&self, si_unit: bool) -> String;\n}\n",
        &trait_method,
        HELPER,
    );
    let index = index_of(&[("src/lib.rs", &source)])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    assert_eq!(chain.hops.len(), 1, "{chain:?}");
    // Associated function without a receiver.
    let associated = METHOD
        .replace("&self, si_unit", "si_unit")
        .replace("self.0", "1");
    assert_declined(&crate_with("", &associated, HELPER), "no self receiver")?;
    // A typed `self` is not one of the three plain receiver forms.
    let typed = METHOD.replace("&self", "self: Box<Self>");
    assert_declined(&crate_with("", &typed, HELPER), "typed self")?;
    // A generic impl has no plain single-segment self type.
    let generic = "pub struct Wrap<T>(pub T);\nimpl<T> Wrap<T> {\n    pub fn to_string_as(&self, si_unit: bool) -> String {\n        fmt_size(1, si_unit)\n    }\n}\n";
    let source = format!(
        "{OWNER}{generic}#[cfg(test)]\nmod tests {{\n    use super::*;\n    fn check(b: Wrap<u8>, si: bool) {{\n        assert_eq!(\"x\", b.to_string_as(si));\n    }}\n}}\n"
    );
    let (hops, _) = chain_of(&source)?;
    assert_eq!(hops.len(), 1, "generic impl");
    Ok(())
}

#[test]
fn imported_type_name_keeps_the_stop() -> Result<(), String> {
    let source = crate_with("use external::ByteSize as Imported;\n", METHOD, HELPER);
    // The name appears in a `use` line (even renamed) of the caller's file.
    let source = source.replace("ByteSize as Imported", "ByteSize");
    assert_declined(&source, "type imported by use")
}

#[test]
fn method_hop_call_in_a_non_test_helper_keeps_the_test_module_exemption_narrow()
-> Result<(), String> {
    // The retained evidence-role entry is a `#[cfg(test)]` helper. A test
    // function that calls the helper is still popped from the chain.
    let (hops, _) = chain_of(&crate_with("", METHOD, HELPER))?;
    assert!(hops.iter().all(|(name, _)| name != "t"), "{hops:?}");
    Ok(())
}

#[test]
fn free_callee_ignores_callers_that_only_make_receiver_calls() -> Result<(), String> {
    assert!(mentions_only_as_receiver_call(
        "assert_eq!(a.fmt_size(), 1)",
        "fmt_size"
    ));
    assert!(mentions_only_as_receiver_call(
        "a.b\n .fmt_size()",
        "fmt_size"
    ));
    assert!(!mentions_only_as_receiver_call(
        "fmt_size(1, true)",
        "fmt_size"
    ));
    assert!(!mentions_only_as_receiver_call(
        "a.fmt_size() + fmt_size(1, true)",
        "fmt_size"
    ));
    assert!(!mentions_only_as_receiver_call(
        "a::fmt_size(1)",
        "fmt_size"
    ));
    assert!(!mentions_only_as_receiver_call(
        "fmt_size::<u8>(1)",
        "fmt_size"
    ));
    assert!(!mentions_only_as_receiver_call("other()", "fmt_size"));
    assert!(mentions_only_as_receiver_call(
        "a.fmt_size() + my_fmt_size(1)",
        "fmt_size"
    ));

    // `expected.fmt_size()` is a method call, never the free function.
    let source = "pub fn fmt_size(bytes: u64) -> String { String::new() }\n\
        pub fn show(bytes: u64) -> String { fmt_size(bytes) }\n\
        pub fn unrelated(x: Thing) -> String { x.fmt_size() }\n";
    let index = index_of(&[("src/lib.rs", source)])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    assert_eq!(
        chain
            .hops
            .iter()
            .map(|hop| hop.caller.name.as_str())
            .collect::<Vec<_>>(),
        vec!["show"],
        "{chain:?}"
    );
    // Two direct callers still stop the chain.
    let two = "pub fn fmt_size(bytes: u64) -> String { String::new() }\n\
        pub fn show(bytes: u64) -> String { fmt_size(bytes) }\n\
        pub fn also(bytes: u64) -> String { fmt_size(bytes) }\n";
    let index = index_of(&[("src/lib.rs", two)])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    assert!(chain.hops.is_empty());
    assert!(
        chain
            .stop_above
            .is_some_and(|edge| edge.contains("2 callers"))
    );
    // A method callee keeps the unfiltered caller set.
    let method = "pub struct T;\nimpl T {\n    pub fn fmt_size(&self) -> String { String::new() }\n}\n\
        pub fn show(t: T) -> String { t.fmt_size() }\n\
        pub fn also(t: T) -> String { t.fmt_size() }\n";
    let index = index_of(&[("src/lib.rs", method)])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    assert!(
        chain
            .stop_above
            .is_some_and(|edge| edge.contains("2 callers"))
    );
    Ok(())
}

#[test]
fn a_method_hop_never_forwards_a_result_or_an_effect_target() -> Result<(), String> {
    // The production wrapper's whole tail is the receiver call, yet no
    // tail-forwarding rule reads a receiver call: the oracle on the wrapper's
    // result is never paired with the owner's.
    let source = "pub struct Size(pub u64);\n\
        impl Size {\n    pub fn render_as(&self, si: bool) -> String { fmt_size(self.0, si) }\n}\n\
        pub fn describe(b: &Size, si: bool) -> String {\n    b.render_as(si)\n}\n\
        pub fn fmt_size(bytes: u64, si: bool) -> String { String::new() }\n";
    let index = index_of(&[("src/lib.rs", source)])?;
    let chain = resolve_chain("fmt_size", &index, true, &[]);
    assert_eq!(chain.hops.len(), 2, "{chain:?}");
    assert_eq!(chain.hops[1].receiver.as_deref(), Some("b"));
    assert!(!chain_forwards_owner_result("fmt_size", &chain));
    // The direct hop alone still forwards; only the method hop does not.
    assert!(hops_forward_owner_result("fmt_size", &chain.hops[..1]));
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == "fmt_size")
        .ok_or("missing owner")?;
    assert!(!chain_passes_effect_target_to_observed_hops(
        owner,
        "bytes.push(1)",
        &chain,
        &[]
    ));
    assert!(chain_has_method_hop(&chain));
    Ok(())
}
