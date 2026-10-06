use super::*;
use crate::analysis::facts::FunctionItemFact;
use crate::analysis::rust_index::summarize_file;
use crate::domain::{DeltaKind, ProbeId, SourceLocation, SymbolId};
use std::path::Path;

const LIB: &str = "src/lib.rs";
const TESTS: &str = "tests/buf_tests.rs";

/// The bytes 7930d93 shape: a trait default method whose changed tail is
/// the only `Ok(..)` and whose `?` exits early with an `Err(..)`.
const BUF_LIB: &str = r#"pub struct TryGetError;

fn sign_extend(value: u64, nbytes: usize) -> i64 {
    value as i64 >> nbytes
}

pub trait Buf {
    fn remaining(&self) -> usize;

    fn try_get_uint(&mut self, nbytes: usize) -> Result<u64, TryGetError> {
        if self.remaining() < nbytes {
            return Err(TryGetError);
        }
        Ok(nbytes as u64)
    }

    fn try_get_int(&mut self, nbytes: usize) -> Result<i64, TryGetError> {
        Ok(sign_extend(self.try_get_uint(nbytes)?, nbytes))
    }
}

impl Buf for &[u8] {
    fn remaining(&self) -> usize {
        self.len()
    }
}
"#;

const BUF_CHANGED: &str = "Ok(sign_extend(self.try_get_uint(nbytes)?, nbytes))";

const BUF_TESTS: &str = r#"use demo::{Buf, TryGetError};

#[test]
fn pins() {
    let mut a = &[0xff, 0xff, 0xff][..];
    assert_eq!(a.try_get_int(3), Ok(-1));
    assert_eq!(Err(TryGetError), a.try_get_int(4));
    assert_eq!(a.remaining(), 0);
    assert_ne!(a.try_get_int(3), Ok(0));
    assert_eq!(a.try_get_int(3).is_ok(), true);
    assert_eq!(Buf::try_get_int(&mut a, 3), Ok(-1));
}
"#;

fn index(files: &[(&str, &str)]) -> RustIndex {
    let mut index = RustIndex::default();
    index.package_names.insert("demo".to_string());
    for (path, text) in files {
        let facts = summarize_file(PathBuf::from(path), (*text).to_string());
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from(path), facts);
    }
    index
}

fn return_probe(owner: &FunctionSummary, expression: &str) -> Probe {
    Probe {
        id: ProbeId("probe".to_string()),
        location: SourceLocation::new(owner.file.clone(), owner.start_line + 1, 1),
        owner: Some(SymbolId(owner.id.0.clone())),
        family: ProbeFamily::ReturnValue,
        delta: DeltaKind::Value,
        before: None,
        after: None,
        expression: expression.to_string(),
        expected_sinks: Vec::new(),
        required_oracles: Vec::new(),
    }
}

fn owner<'a>(index: &'a RustIndex, name: &str) -> &'a FunctionSummary {
    let found = index
        .functions()
        .iter()
        .find(|function| function.name == name && function.file == Path::new(LIB));
    assert!(found.is_some(), "owner `{name}` must be indexed from {LIB}");
    found.unwrap_or(index.functions().at(0))
}

fn establish(index: &RustIndex, name: &str, expression: &str) -> Option<OwnerReturnPin> {
    let owner = owner(index, name);
    OwnerReturnPin::establish(&return_probe(owner, expression), owner, index)
}

/// Per assertion of the one test in `index`, whether the pin admits it.
fn admitted(index: &RustIndex, pin: &OwnerReturnPin) -> Vec<(String, bool)> {
    assert_eq!(index.tests().len(), 1, "fixture must hold exactly one test");
    let test = index.tests().at(0);
    assert!(
        !test.assertions.is_empty(),
        "the test's assertions must parse"
    );
    let syntax = OwnerPinSyntax::default();
    test.assertions
        .iter()
        .map(|assertion| {
            let admitted = pin.admits(
                test,
                assertion,
                index,
                &|file, name| {
                    index.files().get(file).is_some_and(|facts| {
                        file_imports_foreign_callee_name(&facts.source, name, &index.package_names)
                    })
                },
                &syntax,
            );
            (assertion.text.clone(), admitted)
        })
        .collect()
}

fn admitted_texts(index: &RustIndex, pin: &OwnerReturnPin) -> Vec<String> {
    admitted(index, pin)
        .into_iter()
        .filter_map(|(text, admitted)| admitted.then_some(text))
        .collect()
}

#[test]
fn trait_default_pinned_through_a_byte_slice_receiver() {
    let index = index(&[(LIB, BUF_LIB), (TESTS, BUF_TESTS)]);
    let owner = owner(&index, "try_get_int");
    assert_eq!(
        owner.item.container,
        FunctionContainer::Trait {
            trait_name: "Buf".to_string()
        }
    );
    assert!(owner.item.has_self_param && owner.item.has_body);
    let pin = establish(&index, "try_get_int", BUF_CHANGED);
    assert!(pin.is_some(), "the bytes shape must establish a pin");
    let Some(pin) = pin else { return };
    // Only the `Ok(..)` pin: the `Err(..)` pin observes the `?` exit, the
    // other assertions name another method, pin nothing, chain after the
    // call, or call through a qualified path.
    assert_eq!(
        admitted_texts(&index, &pin),
        vec!["assert_eq!(a.try_get_int(3), Ok(-1));".to_string()]
    );
}

#[test]
fn trait_method_needs_its_trait_imported_from_the_workspace() {
    for imports in [
        "use demo::TryGetError;",
        "use other_crate::{Buf, TryGetError};",
        "use demo::*;",
    ] {
        let tests = BUF_TESTS.replace("use demo::{Buf, TryGetError};", imports);
        let index = index(&[(LIB, BUF_LIB), (TESTS, &tests)]);
        let pin = establish(&index, "try_get_int", BUF_CHANGED);
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert!(
            admitted_texts(&index, &pin).is_empty(),
            "{imports}: the trait is not in scope from the workspace"
        );
    }
    // An anonymous import brings the trait's methods into scope, and a
    // `::`-rooted path names the same crate (bytes' own tests use it).
    for imports in [
        "use demo::Buf as _;\nuse demo::TryGetError;",
        "use ::demo::{Buf, TryGetError};",
    ] {
        let tests = BUF_TESTS.replace("use demo::{Buf, TryGetError};", imports);
        let index = index(&[(LIB, BUF_LIB), (TESTS, &tests)]);
        let admitted = establish(&index, "try_get_int", BUF_CHANGED)
            .map(|pin| admitted_texts(&index, &pin))
            .unwrap_or_default();
        assert_eq!(admitted.len(), 1, "{imports}: {admitted:?}");
    }
}

#[test]
fn another_definition_of_the_method_defeats_the_pin() {
    // An override in another impl, an inherent method of the same name, and
    // a `macro_rules!` body the parser never indexes each compete.
    for competitor in [
        "pub struct Chain;\nimpl Buf for Chain {\n    fn remaining(&self) -> usize { 0 }\n    fn try_get_int(&mut self, nbytes: usize) -> Result<i64, TryGetError> { Ok(nbytes as i64) }\n}\n",
        "pub struct Other;\nimpl Other {\n    pub fn try_get_int(&mut self, nbytes: usize) -> Result<i64, TryGetError> { Ok(nbytes as i64) }\n}\n",
        "macro_rules! forward {\n    () => {\n        fn try_get_int(&mut self, nbytes: usize) -> Result<i64, TryGetError> {\n            Ok(nbytes as i64)\n        }\n    };\n}\n",
    ] {
        let lib = format!("{BUF_LIB}\n{competitor}");
        let index = index(&[(LIB, &lib), (TESTS, BUF_TESTS)]);
        assert!(
            establish(&index, "try_get_int", BUF_CHANGED).is_none(),
            "{competitor}"
        );
    }
    // A pure `(**self)` forward dispatches to the pointee's method, and a
    // free function of the same name cannot take a method call.
    for bystander in [
        "macro_rules! forward {\n    () => {\n        fn try_get_int(&mut self, nbytes: usize) -> Result<i64, TryGetError> {\n            (**self).try_get_int(nbytes)\n        }\n    };\n}\n",
        "pub fn try_get_int(nbytes: usize) -> i64 {\n    nbytes as i64\n}\n",
    ] {
        let lib = format!("{BUF_LIB}\n{bystander}");
        let index = index(&[(LIB, &lib), (TESTS, BUF_TESTS)]);
        assert!(
            establish(&index, "try_get_int", BUF_CHANGED).is_some(),
            "{bystander}"
        );
    }
}

#[test]
fn receiver_must_be_bound_to_a_type_that_dispatches_to_the_owner() {
    for receiver in [
        // No `impl Buf for Vec<u8>`.
        "let mut a = vec![0xff, 0xff, 0xff];",
        // A type ripr cannot read.
        "let mut a = make_buf();",
        // Bound by a pattern ripr does not read, or bound twice to
        // different types.
        "let (mut a, _) = (&[0xff][..], 0);",
        "let mut a = &[0xff][..];\n    let drain = |a: &mut Vec<u8>| a.clear();",
        "let mut a = &[0xff][..];\n    for a in [1] {}",
        "let mut a = &[0xff][..];\n    let mut a = vec![1];",
    ] {
        let tests = BUF_TESTS.replace("let mut a = &[0xff, 0xff, 0xff][..];", receiver);
        let index = index(&[(LIB, BUF_LIB), (TESTS, &tests)]);
        let pin = establish(&index, "try_get_int", BUF_CHANGED);
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert!(
            admitted_texts(&index, &pin).is_empty(),
            "{receiver}: the receiver's type is not established"
        );
    }
    // Byte strings are byte slices too, and a reassignment cannot change a
    // binding's type.
    let tests = BUF_TESTS.replace(
        "let mut a = &[0xff, 0xff, 0xff][..];",
        "let mut a = &b\"\\xff\\xff\\xff\"[..];\n    a = &a[..];",
    );
    let index = index(&[(LIB, BUF_LIB), (TESTS, &tests)]);
    let admitted = establish(&index, "try_get_int", BUF_CHANGED)
        .map(|pin| admitted_texts(&index, &pin))
        .unwrap_or_default();
    assert_eq!(admitted.len(), 1, "{admitted:?}");
}

#[test]
fn a_slice_method_name_resolves_to_the_slice_first() {
    // `len` on `&[u8]` is the slice's own method, whatever the trait says.
    let lib = BUF_LIB.replace("fn try_get_int(", "fn len(").replace(
        "fn remaining(&self) -> usize {\n        self.len()",
        "fn remaining(&self) -> usize {\n        0",
    );
    let tests = BUF_TESTS.replace("a.try_get_int(3), Ok(-1)", "a.len(3), Ok(-1)");
    let index = index(&[(LIB, &lib), (TESTS, &tests)]);
    let pin = establish(&index, "len", BUF_CHANGED);
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert!(admitted_texts(&index, &pin).is_empty());
}

#[test]
fn inherent_method_needs_a_receiver_of_its_own_type() {
    let lib = "pub struct Stack {\n    items: Vec<u32>,\n}\n\nimpl Stack {\n    pub fn new() -> Self {\n        Stack { items: Vec::new() }\n    }\n\n    pub fn depth(&self) -> usize {\n        self.items.len() + 1\n    }\n}\n";
    let changed = "self.items.len() + 1";
    let owner_fn = {
        let index = index(&[(LIB, lib)]);
        owner(&index, "depth").item.container.clone()
    };
    assert_eq!(
        owner_fn,
        FunctionContainer::Inherent {
            self_ty: "Stack".to_string()
        }
    );
    for (binding, admitted) in [
        ("let stack = Stack::new();", 1),
        ("let stack = Stack { items: Vec::new() };", 1),
        ("let stack: Stack = Default::default();", 1),
        // Another type's method of the same name is not the owner.
        ("let stack = Vec::<u32>::new();", 0),
        ("let stack: Vec<u32> = Vec::new();", 0),
        ("let stack = Stack::new().items;", 0),
        // An arbitrary associated function may return another type.
        ("let stack = Stack::items_of(3);", 0),
    ] {
        let tests = format!(
            "use demo::Stack;\n\n#[test]\nfn depth_counts() {{\n    {binding}\n    assert_eq!(stack.depth(), 1);\n}}\n"
        );
        let index = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index, "depth", changed);
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert_eq!(admitted_texts(&index, &pin).len(), admitted, "{binding}");
    }
}

#[test]
fn an_inline_constructor_types_the_receiver_like_a_binding() {
    // bytesize's `assert_eq!(ByteSize::b(3).as_whole_units(2), None)`.
    let lib = "pub struct Stack {\n    items: Vec<u32>,\n}\n\nimpl Stack {\n    pub fn new() -> Self {\n        Stack { items: Vec::new() }\n    }\n\n    pub fn with(n: u32) -> Stack {\n        Stack { items: vec![n] }\n    }\n\n    pub fn depth(&self) -> usize {\n        self.items.len() + 1\n    }\n}\n";
    let changed = "self.items.len() + 1";
    for (call, admitted) in [
        ("Stack::new().depth()", 1),
        ("Stack::with(3).depth()", 1),
        // A constructor ripr cannot see returns some other type.
        ("Stack::items_of(3).depth()", 0),
        ("Vec::<u32>::new().depth()", 0),
        // Something chained after the owner call is not its return value.
        ("Stack::new().depth().pow(2)", 0),
        ("Stack::new().clone().depth()", 0),
    ] {
        let tests = format!(
            "use demo::Stack;\n\n#[test]\nfn depth_counts() {{\n    assert_eq!({call}, 1);\n}}\n"
        );
        let index = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index, "depth", changed);
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert_eq!(admitted_texts(&index, &pin).len(), admitted, "{call}");
    }
}

#[test]
fn bare_call_names_only_a_module_level_function() {
    // B2: `decode(..)` in a test names the free function, so an associated
    // `Codec::decode` owner never takes a bare call.
    let lib = "pub struct Codec;\n\nimpl Codec {\n    pub fn decode(input: u32) -> u32 {\n        input.rotate_left(3)\n    }\n}\n\npub fn decode(input: u32) -> u32 {\n    input + 1\n}\n";
    let tests = "use demo::decode;\n\n#[test]\nfn decodes() {\n    assert_eq!(decode(8), 9);\n}\n";
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let associated = index.functions().iter().find(|function| {
        function.name == "decode"
            && matches!(function.item.container, FunctionContainer::Inherent { .. })
    });
    assert!(associated.is_some_and(|function| !function.item.has_self_param));
    let Some(associated) = associated else { return };
    let probe = return_probe(associated, "input.rotate_left(3)");
    assert!(OwnerReturnPin::establish(&probe, associated, &index).is_none());
    // The free function does take the bare call, and the associated
    // function of the same name does not compete with it.
    let free = index.functions().iter().find(|function| {
        function.name == "decode" && function.item.container == FunctionContainer::Free
    });
    assert!(free.is_some());
    let Some(free) = free else { return };
    let pin = OwnerReturnPin::establish(&return_probe(free, "input + 1"), free, &index);
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

#[test]
fn bare_call_is_defeated_by_another_free_function_or_a_local_binding() {
    let lib = "pub fn scaled(x: i32) -> i32 {\n    x * 10\n}\n\npub mod other {\n    pub fn scaled(x: i32) -> i32 {\n        x\n    }\n}\n";
    let index_with_twin = index(&[(LIB, lib)]);
    assert!(establish(&index_with_twin, "scaled", "x * 10").is_none());

    let lib = "pub fn scaled(x: i32) -> i32 {\n    x * 10\n}\n";
    for body in [
        "let scaled = |value: i32| value * 10;\n    assert_eq!(scaled(3), 30);",
        "fn scaled(value: i32) -> i32 { value * 10 }\n    assert_eq!(scaled(3), 30);",
    ] {
        let tests = format!("use demo::scaled;\n\n#[test]\nfn scales() {{\n    {body}\n}}\n");
        let index = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index, "scaled", "x * 10");
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert!(admitted_texts(&index, &pin).is_empty(), "{body}");
    }
}

/// The gate with the changed line at the last occurrence of `changed`.
fn gate(body: &str, changed: &str) -> Option<ReturnPathGate> {
    let at = body.rfind(changed.trim_end_matches(';')).unwrap_or(0);
    return_path_gate(body, changed, body[..at].matches('\n').count())
}

#[test]
fn an_early_return_is_pinned_when_it_is_the_only_source_of_its_value() {
    // bytesize's `as_whole_units`: the one `return None` is the only `None`.
    assert!(matches!(
        gate(
            "fn f(&self, unit: u64) -> Option<u64> {\n    if unit == 0 || self.0 % unit != 0 {\n        return None;\n    }\n    Some(self.0 / unit)\n}",
            "return None;"
        ),
        Some(ReturnPathGate::Exact("None"))
    ));
    assert!(matches!(
        gate(
            "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(E::Negative);\n    }\n    if x == 0 {\n        return Ok(0);\n    }\n    Ok(x * 2)\n}",
            "return Err(E::Negative);"
        ),
        Some(ReturnPathGate::Head("Err"))
    ));
    for body in [
        // A second `return None` is another source of `None`.
        "fn f(x: u64) -> Option<u64> {\n    if x == 1 {\n        return None;\n    }\n    if x == 2 {\n        return None;\n    }\n    Some(x)\n}",
        // `?` can produce `None` too.
        "fn f(x: u64) -> Option<u64> {\n    if x == 1 {\n        return None;\n    }\n    Some(g(x)?)\n}",
        // The tail can produce `None`.
        "fn f(x: u64) -> Option<u64> {\n    if x == 1 {\n        return None;\n    }\n    if x > 2 { Some(x) } else { None }\n}",
        "fn f(x: u64) -> Option<u64> {\n    if x == 1 {\n        return None;\n    }\n    lookup(x)\n}",
        // A `return` inside a closure leaves only the closure.
        "fn f(x: u64) -> Option<u64> {\n    let c = || {\n        return None;\n    };\n    Some(x)\n}",
        // A macro may hide a `return`.
        "fn f(x: u64) -> Option<u64> {\n    if x == 1 {\n        return None;\n    }\n    bail_if!(x);\n    Some(x)\n}",
        // A `return` in an `async` or `const` block, or a nested `fn`, ends
        // only that inner body.
        "fn f(x: u64) -> Option<u64> {\n    let _ = async {\n        return None;\n    };\n    Some(x)\n}",
        "fn f(x: u64) -> Option<u64> {\n    let _ = const {\n        return None;\n    };\n    Some(x)\n}",
        "fn f(x: u64) -> Option<u64> {\n    fn g() -> Option<u64> {\n        return None;\n    }\n    Some(x)\n}",
    ] {
        let changed = "return None;";
        let at = body.find(changed).unwrap_or(0);
        assert!(
            return_path_gate(body, changed, body[..at].matches('\n').count()).is_none(),
            "{body}"
        );
    }
}

#[test]
fn an_early_err_return_needs_to_be_the_only_err_source() {
    for body in [
        // A second `return Err(..)` is another source of `Err`.
        "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(E::Negative);\n    }\n    if x == 0 {\n        return Err(E::Zero);\n    }\n    Ok(x)\n}",
        // `?` can produce `Err` too.
        "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(E::Negative);\n    }\n    Ok(g(x)?)\n}",
    ] {
        let changed = "return Err(E::Negative);";
        let at = body.find(changed).unwrap_or(0);
        assert!(
            return_path_gate(body, changed, body[..at].matches('\n').count()).is_none(),
            "{body}"
        );
    }
}

#[test]
fn an_early_return_pin_admits_only_the_value_that_return_produces() {
    // End to end: the gate is what keeps a pin on the other exits' value
    // from crediting a changed `return None;` it never reaches.
    let lib = "pub fn whole(x: u64, unit: u64) -> Option<u64> {\n    if unit == 0 || x % unit != 0 { return None; }\n    Some(x / unit)\n}\n";
    let tests = "use demo::whole;\n\n#[test]\nfn wholes() {\n    assert_eq!(whole(3, 2), None);\n    assert_eq!(whole(4, 2), Some(2));\n    assert_eq!(whole(3, 2), Option::None);\n    assert_eq!(whole(3, 2), NONE);\n}\n";
    let none_index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&none_index, "whole", "return None;");
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&none_index, &pin),
        vec!["assert_eq!(whole(3, 2), None);".to_string()]
    );

    let lib = "pub enum E { Negative }\n\npub fn checked(x: i32) -> Result<i32, E> {\n    if x < 0 { return Err(E::Negative); }\n    Ok(x * 2)\n}\n";
    let tests = "use demo::{checked, E};\n\n#[test]\nfn checks() {\n    assert_eq!(checked(-1), Err(E::Negative));\n    assert_eq!(checked(2), Ok(4));\n}\n";
    let err_index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&err_index, "checked", "return Err(E::Negative);");
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&err_index, &pin),
        vec!["assert_eq!(checked(-1), Err(E::Negative));".to_string()]
    );
}

#[test]
fn return_path_gate_needs_the_changed_tail_on_the_pinned_path() {
    // No early exit: any pinned value came through the changed tail.
    assert!(matches!(
        gate(
            "fn f(x: i32) -> i32 {\n    let y = x;\n    y * 3\n}",
            "y * 3"
        ),
        Some(ReturnPathGate::Any)
    ));
    // A block statement before the tail needs no `;`.
    assert!(matches!(
        gate(
            "fn f(x: i32) -> i32 {\n    let mut y = 0;\n    for v in 0..x { y += v; }\n    y * 3\n}",
            "y * 3"
        ),
        Some(ReturnPathGate::Any)
    ));
    // `?` and `return Err(..)` exits: only an `Ok(..)` pin agrees.
    assert!(matches!(
        gate(
            "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(E);\n    }\n    Ok(g(x)? * 2)\n}",
            "Ok(g(x)? * 2)"
        ),
        Some(ReturnPathGate::Head("Ok"))
    ));
    // The final `return` is the tail.
    assert!(matches!(
        gate(
            "fn f(x: i32) -> Option<i32> {\n    let y = g(x)?;\n    return Some(y * 2);\n}",
            "return Some(y * 2);"
        ),
        Some(ReturnPathGate::Head("Some"))
    ));
    // An identical expression elsewhere in the body is not the tail.
    let twin = "fn f(x: i32) -> i32 {\n    let _unused = || x * 3;\n    x * 3\n}";
    assert!(return_path_gate(twin, "x * 3", 1).is_none());
    assert!(matches!(
        return_path_gate(twin, "x * 3", 2),
        Some(ReturnPathGate::Any)
    ));
    for (body, changed) in [
        // Not the tail.
        (
            "fn f(x: i32) -> i32 {\n    let y = x * 3;\n    y\n}",
            "let y = x * 3;",
        ),
        // Inside a branch of the tail.
        (
            "fn f(x: i32) -> i32 {\n    if x > 0 {\n        x * 3\n    } else {\n        0\n    }\n}",
            "x * 3",
        ),
        // Another `Ok(..)` exit.
        (
            "fn f(x: i32) -> Result<i32, E> {\n    if x == 0 {\n        return Ok(0);\n    }\n    Ok(g(x)? * 2)\n}",
            "Ok(g(x)? * 2)",
        ),
        // A `return` of an opaque value.
        (
            "fn f(x: i32) -> Result<i32, E> {\n    if x == 0 {\n        return cached();\n    }\n    Ok(g(x)? * 2)\n}",
            "Ok(g(x)? * 2)",
        ),
        // An `Err(..)` tail shares its constructor with every `?` exit.
        (
            "fn f(x: i32) -> Result<i32, E> {\n    g(x)?;\n    Err(E::Late)\n}",
            "Err(E::Late)",
        ),
        // A macro may hide a `return`, however it is spaced.
        (
            "fn f(x: i32) -> Result<i32, E> {\n    ensure!(x > 0);\n    Ok(x * 2)\n}",
            "Ok(x * 2)",
        ),
        (
            "fn f(x: i32) -> Result<i32, E> {\n    ensure !(x > 0, E);\n    Ok(x * 2)\n}",
            "Ok(x * 2)",
        ),
    ] {
        assert!(gate(body, changed).is_none(), "{body}");
    }
}

#[test]
fn lexical_fallback_owner_is_not_established() {
    let mut index = index(&[(LIB, BUF_LIB), (TESTS, BUF_TESTS)]);
    if let Some(facts) = index.file_data_mut(Path::new(LIB)) {
        facts.used_lexical_fallback = true;
    }
    assert!(establish(&index, "try_get_int", BUF_CHANGED).is_none());
}

#[test]
fn item_container_stops_at_the_nearest_item() {
    let source = "pub fn free() {}\n\nimpl Thing {\n    fn method(&self) {\n        fn helper() {}\n    }\n}\n\nimpl<T> Show for Wrapper<T> {\n    fn show(self) {}\n}\n\ntrait Show {\n    fn show(self);\n}\n";
    let facts = summarize_file(PathBuf::from(LIB), source.to_string());
    let item = |name: &str, has_body: bool| {
        facts
            .functions
            .iter()
            .find(|function| function.name == name && function.item.has_body == has_body)
            .map(|function| function.item.clone())
    };
    assert_eq!(
        item("free", true).map(|item| item.container),
        Some(FunctionContainer::Free)
    );
    assert_eq!(
        item("method", true),
        Some(FunctionItemFact {
            container: FunctionContainer::Inherent {
                self_ty: "Thing".to_string()
            },
            has_self_param: true,
            has_body: true,
        })
    );
    assert_eq!(
        item("helper", true).map(|item| item.container),
        Some(FunctionContainer::Local)
    );
    assert_eq!(
        item("show", true).map(|item| item.container),
        Some(FunctionContainer::TraitImpl {
            trait_path: "Show".to_string(),
            self_ty: "Wrapper<T>".to_string()
        })
    );
    assert_eq!(
        item("show", false),
        Some(FunctionItemFact {
            container: FunctionContainer::Trait {
                trait_name: "Show".to_string()
            },
            has_self_param: true,
            has_body: false,
        })
    );
}

#[test]
fn a_tail_that_skips_its_changed_part_on_some_inputs_is_not_established() {
    // `scaled(None)` returns 0 without running the changed closure, so a
    // pin on it observes nothing of the change.
    for (body, changed) in [
        (
            "fn f(x: Option<i32>) -> i32 {\n    x.map_or(0, |v| v * 3)\n}",
            "x.map_or(0, |v| v * 3)",
        ),
        (
            "fn f(x: Option<i32>) -> i32 {\n    x.unwrap_or(LIMIT * 2)\n}",
            "x.unwrap_or(LIMIT * 2)",
        ),
        (
            "fn f(x: Option<i32>) -> Option<i32> {\n    Option::map(x, triple)\n}",
            "Option::map(x, triple)",
        ),
        (
            "fn f(a: i32, b: i32) -> bool {\n    a > 0 && check(b)\n}",
            "a > 0 && check(b)",
        ),
        (
            "fn f(c: bool, a: i32) -> Result<i32, E> {\n    Ok(if c { a * 3 } else { 0 })\n}",
            "Ok(if c { a * 3 } else { 0 })",
        ),
    ] {
        assert!(gate(body, changed).is_none(), "{body}");
    }
    // Control: an unconditional tail with a `map` in its name is not a
    // combinator.
    assert!(matches!(
        gate("fn f(x: i32) -> i32 {\n    remap(x) * 3\n}", "remap(x) * 3"),
        Some(ReturnPathGate::Any)
    ));
}

const WEIGHT_LIB: &str = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";

fn weight_admitted(tests: &str) -> Vec<String> {
    let index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(pin.is_some(), "the free owner must establish a pin");
    let Some(pin) = pin else {
        return Vec::new();
    };
    admitted_texts(&index, &pin)
}

#[test]
fn only_a_plain_assert_eq_against_an_owner_free_value_pins() {
    let tests = "use demo::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), weight(2) + weight(2));\n    assert_eq!(weight(2), 2 * weight(1));\n    debug_assert_eq!(weight(4), 12);\n    assert_eq!(weight(4), 12);\n}\n";
    assert_eq!(
        weight_admitted(tests),
        vec!["assert_eq!(weight(4), 12);".to_string()]
    );
    // A `#[should_panic]` test passes exactly when the values differ.
    let tests = "use demo::weight;\n\n#[test]\n#[should_panic]\nfn weighs() {\n    assert_eq!(weight(4), 13);\n}\n";
    assert!(weight_admitted(tests).is_empty());
}

#[test]
fn an_assertion_outside_the_test_body_is_not_its_pin() {
    // A harness trial's `body` is only its registration; an assertion the
    // index attributes to it from a helper does not see the helper's
    // bindings.
    let tests = "use demo::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    let test = index.tests().at(0);
    assert_eq!(test.assertions.len(), 1);
    let no_foreign_import = |_: &Path, _: &str| false;
    assert!(pin.admits(
        test,
        &test.assertions[0],
        &index,
        &no_foreign_import,
        &OwnerPinSyntax::default()
    ));
    let mut helper_assertion = test.assertions[0].clone();
    helper_assertion.line = test.end_line + 3;
    assert!(!pin.admits(
        test,
        &helper_assertion,
        &index,
        &no_foreign_import,
        &OwnerPinSyntax::default()
    ));
}

#[test]
fn a_bare_call_through_any_other_binding_of_the_name_is_not_a_pin() {
    let control =
        "use demo::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    assert_eq!(weight_admitted(control).len(), 1);
    for tests in [
        // A `for` pattern.
        "use demo::weight;\n\n#[test]\nfn weighs() {\n    for weight in [heavy] {\n        assert_eq!(weight(4), 12);\n    }\n}\n",
        // A closure parameter.
        "use demo::weight;\n\n#[test]\nfn weighs() {\n    let check = |weight: fn(u32) -> u32| assert_eq!(weight(4), 12);\n    check(heavy);\n}\n",
        // The test's own parameter.
        "use demo::weight;\n\n#[test]\nfn weighs(weight: fn(u32) -> u32) {\n    assert_eq!(weight(4), 12);\n}\n",
        // A workspace item renamed to the owner's name.
        "use demo::legacy::triple as weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n",
        // A macro that binds a pattern.
        "use demo::weight;\n\n#[test]\nfn weighs() {\n    let_assert!(Ok(weight) = pick());\n    assert_eq!(weight(4), 12);\n}\n",
    ] {
        assert!(weight_admitted(tests).is_empty(), "{tests}");
    }
}

const COUNTER_LIB: &str = "pub struct Counter {\n    n: usize,\n}\n\nimpl Counter {\n    pub fn new() -> Self {\n        Counter { n: 0 }\n    }\n\n    pub fn try_new(n: usize) -> Result<Self, String> {\n        Ok(Counter { n })\n    }\n\n    pub fn count(&self) -> usize {\n        self.n + 1\n    }\n\n    pub fn tally(&self) -> usize {\n        self.n + 1\n    }\n}\n";

fn counter_admitted(owner_name: &str, prelude: &str, binding: &str) -> usize {
    let tests = format!(
        "{prelude}use demo::Counter;\n\n#[test]\nfn counts() {{\n    {binding}\n    assert_eq!(c.{owner_name}(), 1);\n}}\n"
    );
    let index = index(&[(LIB, COUNTER_LIB), (TESTS, &tests)]);
    let pin = establish(&index, owner_name, "self.n + 1");
    assert!(pin.is_some(), "{owner_name} must establish a pin");
    let Some(pin) = pin else { return 0 };
    admitted_texts(&index, &pin).len()
}

#[test]
fn a_by_value_prelude_method_name_may_take_the_call_first() {
    // `Iterator::count(self)` is tried at the receiver type itself, before
    // the inherent `count(&self)` at `&Counter`, whenever `Counter` is an
    // iterator; ripr cannot see that, so the name never pins.
    assert_eq!(counter_admitted("count", "", "let c = Counter::new();"), 0);
    assert_eq!(counter_admitted("tally", "", "let c = Counter::new();"), 1);
}

#[test]
fn a_constructor_binds_its_type_only_when_its_signature_returns_it() {
    for (binding, admitted) in [
        ("let c = Counter::new();", 1),
        ("let c = Counter::new().unwrap();", 0),
        ("let c = Counter::try_new(0);", 0),
        ("let c = Counter::try_new(0)?;", 1),
        ("let c = Counter::try_new(0).unwrap();", 1),
        ("let c = Counter::try_new(0).expect(\"valid\");", 1),
        ("let c = Counter::try_from(0);", 0),
        ("let c = Counter::try_from(0).unwrap();", 1),
        ("let c = Counter::from(0);", 1),
        // Not an inherent definition ripr can read.
        ("let c = Counter::from_bytes(b\"0\");", 0),
        ("let c = Counter { n: 0 };", 1),
    ] {
        assert_eq!(
            counter_admitted("tally", "", binding),
            admitted,
            "{binding}"
        );
    }
}

#[test]
fn a_receiver_name_bound_or_typed_elsewhere_is_not_established() {
    for (prelude, binding) in [
        // A macro binds the receiver.
        (
            "",
            "let c = Counter::new();\n    let_assert!(Ok(c) = pick());",
        ),
        // The test file aliases or renames the type name.
        (
            "type Counter = Vec<usize>;\n",
            "let c: Counter = Vec::new();",
        ),
        ("use other::Tally as Counter;\n", "let c = Counter::new();"),
    ] {
        assert_eq!(counter_admitted("tally", prelude, binding), 0, "{binding}");
    }
    // The test's parameter binds the receiver even when a nested `let`
    // types another binding of the same name.
    let tests = "use demo::Counter;\n\n#[test]\nfn counts(c: Vec<usize>) {\n    {\n        let c = Counter::new();\n        drop(c);\n    }\n    assert_eq!(c.tally(), 1);\n}\n";
    let index = index(&[(LIB, COUNTER_LIB), (TESTS, tests)]);
    let pin = establish(&index, "tally", "self.n + 1");
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert!(admitted_texts(&index, &pin).is_empty());
}

#[test]
fn owner_pin_requires_an_executed_assertion_context() {
    for body in [
        "let check = || assert_eq!(weight(4), 12);",
        "let check = || assert_eq!(weight(4), 12); let _later = || check();",
        "let check = || assert_eq!(weight(4), 12); if false { check(); }",
        "let check = || assert_eq!(weight(4), 12); let check = || {}; check();",
        "let mut check: fn() = || assert_eq!(weight(4), 12); check = || {}; check();",
        "let check = || assert_eq!(weight(4), 12); let alias = check; alias();",
        "let check = || assert_eq!(weight(4), 12); let _borrow = &check; check();",
        "let check = || assert_eq!(weight(4), 12); false && { check(); true };",
        "let check = || assert_eq!(weight(4), 12); #[cfg(any())] check();",
        "#[cfg(any())] assert_eq!(weight(4), 12);",
        "if true { return; } assert_eq!(weight(4), 12);",
        "let _later = async { assert_eq!(weight(4), 12); };",
        "fn later() { assert_eq!(weight(4), 12); }",
        "if false { assert_eq!(weight(4), 12); }",
        "return; assert_eq!(weight(4), 12);",
        "assert_eq!(weight({ return; 4 }), 12);",
        "macro_rules! skip { () => { return; } } skip!(); assert_eq!(weight(4), 12);",
        "macro_rules! skip { () => { return; } } let _ = dbg!(skip!()); assert_eq!(weight(4), 12);",
        "let _ = dbg!(other::assert!()); assert_eq!(weight(4), 12);",
        "use other as std; std::dbg!(); assert_eq!(weight(4), 12);",
        "macro_rules! assert { () => { return; } } assert!(); assert_eq!(weight(4), 12);",
        "let check = || { return; assert_eq!(weight(4), 12); }; check();",
    ] {
        let tests = format!("use demo::weight;\n#[test]\nfn weighs() {{ {body} }}\n");
        assert!(weight_admitted(&tests).is_empty(), "{body}");
    }
    for body in [
        "assert_eq!(weight(4), 12);",
        "{ assert_eq!(weight(4), 12); }",
        "assert!(!false); assert_eq!(weight(4), 12);",
        "assert!(if !(false) { true } else { false }); assert_eq!(weight(4), 12);",
        "assert_eq!(weight(!0u32 & 4), 12);",
        "let check = || assert_eq!(weight(4), 12); check();",
        "let check = || { assert_eq!(weight(4), 12); }; check();",
        "(|| assert_eq!(weight(4), 12))();",
    ] {
        let tests = format!("use demo::weight;\n#[test]\nfn weighs() {{ {body} }}\n");
        assert_eq!(weight_admitted(&tests).len(), 1, "{body}");
    }
}

#[test]
fn owner_pin_requires_unambiguous_standard_assert_eq() {
    for (prelude, body) in [
        (
            "",
            "macro_rules! assert_eq { ($actual:expr, $expected:expr) => { std::assert_eq!(1, 1) }; } assert_eq!(weight(4), 12);",
        ),
        (
            "macro_rules! assert_eq { ($actual:expr, $expected:expr) => { std::assert_eq!(1, 1) }; }",
            "assert_eq!(weight(4), 12);",
        ),
        ("use other::assert_eq;", "assert_eq!(weight(4), 12);"),
        (
            "use other::ignore as assert_eq;",
            "assert_eq!(weight(4), 12);",
        ),
        ("use other::*;", "assert_eq!(weight(4), 12);"),
        (
            "#[macro_use] extern crate other;",
            "assert_eq!(weight(4), 12);",
        ),
    ] {
        let tests = format!("{prelude}\nuse demo::weight;\n#[test]\nfn weighs() {{ {body} }}\n");
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
}

#[test]
fn owner_pin_refuses_ambiguous_oracle_coordinates() {
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); let _later = || { assert_eq!(weight(4), 12); }; }\n";
    assert!(weight_admitted(tests).is_empty());
    let tests = "use demo::weight;\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n    let _later = || { assert_eq!(weight(4), 12); };\n}\n";
    assert_eq!(weight_admitted(tests).len(), 1);
}

#[test]
fn owner_pin_macro_ambiguity_in_other_files_and_run_memo() {
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    for other in [
        "macro_rules! assert_eq { ($a:expr, $b:expr) => {} }",
        "#[macro_use] extern crate other;",
        "use other::{nested::*};",
        "make!(assert_eq);",
    ] {
        let index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests), ("src/other.rs", other)]);
        let pin = establish(&index, "weight", "x * 3");
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert!(admitted_texts(&index, &pin).is_empty(), "{other}");
    }
    let tests = tests.replace("use demo::weight;", "use demo::*;\n// macro_rules! assert_eq; macro_use\nconst NOTE: &str = \"assert_eq macro_use\";\nuse std::fs::write;\nmacro_rules! helper { () => { assert_eq!(1, 1); }; }");
    let index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests)]);
    let Some(pin) = establish(&index, "weight", "x * 3") else {
        return;
    };
    let syntax = OwnerPinSyntax::default();
    for _ in 0..2 {
        assert!(pin.admits(
            index.tests().at(0),
            &index.tests().at(0).assertions[0],
            &index,
            &|_, _| false,
            &syntax
        ));
    }
    // The private `use std::fs::write;` binds only in the test's own crate.
    assert_eq!(
        syntax.ambiguous_macro_bindings.borrow().as_ref(),
        Some(&BTreeSet::new())
    );
    assert_eq!(
        syntax.crate_macro_bindings.borrow().as_ref(),
        Some(&BTreeMap::from([(
            PathBuf::from(TESTS),
            BTreeSet::from(["write".to_string()])
        )]))
    );
    assert_eq!(syntax.by_file.borrow().len(), 1);
}

#[test]
fn owner_pin_closure_call_must_share_the_bindings_live_scope() {
    for body in [
        "{ let check = || assert_eq!(weight(4), 12); } check();",
        "let _later = || { let check = || assert_eq!(weight(4), 12); }; check();",
    ] {
        let tests =
            format!("use demo::weight;\nfn check() {{}}\n#[test]\nfn weighs() {{ {body} }}\n");
        assert!(weight_admitted(&tests).is_empty(), "{body}");
    }
}

#[test]
fn shared_return_admission_uses_the_outer_invocation_identity() {
    for assertion in [
        "assert_eq!(weight(input), 12);",
        "#[cfg(any())] assert_eq!(weight(input), 12);",
        "assert_eq!(weight(input), 12, \"{}\", stringify!(value));",
    ] {
        assert!(is_bare_assert_eq_invocation(assertion), "{assertion}");
    }
    for assertion in [
        "assert!({ assert_eq!(weight(input), 12); true });",
        "std::assert_eq!(weight(input), 12);",
        "matches!(value, Some(_));",
        "match value { _ => assert_eq!(weight(input), 12) }",
    ] {
        assert!(!is_bare_assert_eq_invocation(assertion), "{assertion}");
    }
}

#[test]
fn owner_pin_requires_test_item_ancestry_and_enabled_cfg() {
    for module in [
        "mod nested { #![cfg(any())] BODY }",
        "#[cfg(any())] mod nested { BODY }",
        "#[cfg(not(test))] mod nested { BODY }",
        "#[cfg(test)] #[cfg(any())] mod nested { BODY }",
        "#[cfg(feature = \"unknown\")] mod nested { BODY }",
        "#[cfg_attr(test, cfg(any()))] mod nested { BODY }",
        "#[r#cfg(any())] mod nested { BODY }",
        "#[cfg_attr(test, r#cfg(any()))] mod nested { BODY }",
        "#[r#cfg(test)] mod nested { BODY }",
        "fn outer() { BODY }",
    ] {
        let body = "#[test] fn check() { assert_eq!(weight(4), 12); }";
        let tests = format!("use demo::weight;\n{}", module.replace("BODY", body));
        assert!(weight_admitted(&tests).is_empty(), "{module}");
    }
    for attribute in [
        "",
        "#[cfg(test)]",
        "#[cfg(all())]",
        "#[cfg(any(test, feature = \"unknown\"))]",
        "#[warn(dead_code)] #[doc = \"cfg(any())\"]",
    ] {
        let tests = format!(
            "{attribute} mod outer {{ mod nested {{ use demo::weight; #[test] fn check() {{ assert_eq!(weight(4), 12); }} }} }}"
        );
        assert_eq!(weight_admitted(&tests).len(), 1, "{attribute}");
    }
}

/// The first refusal for the one test's first assertion, with `others`
/// indexed beside the weight library and the test file.
fn weight_refusal(tests: &str, others: &[(&str, &str)]) -> Option<AssertionRefusal> {
    weight_refusal_under(tests, others, None)
}

/// `weight_refusal` with the drop-in manifest authority rooted at `root`.
fn weight_refusal_under(
    tests: &str,
    others: &[(&str, &str)],
    root: Option<&Path>,
) -> Option<AssertionRefusal> {
    let mut files = vec![(LIB, WEIGHT_LIB), (TESTS, tests)];
    files.extend_from_slice(others);
    let mut index = index(&files);
    if let Some(root) = root {
        index.drop_in_manifests = crate::analysis::facts::drop_in::DropInManifests::new(root);
    }
    let test = index
        .tests()
        .iter()
        .find(|test| test.file == Path::new(TESTS));
    assert!(test.is_some(), "the fixture test must be indexed");
    let test = test?;
    assert!(
        !test.assertions.is_empty(),
        "the test's assertions must parse"
    );
    OwnerPinSyntax::default().refusal(test, &test.assertions[0], &index)
}

#[test]
fn a_crate_local_site_another_crate_can_compile_stays_workspace_wide() {
    use crate::analysis::facts::{RustIncludeLimitation, SourceRoleProvenanceEdge};
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let shadow = "macro_rules! assert_eq { ($a:expr, $b:expr) => {} }";
    let edge = |parent: &str, child: &str, kind| SourceRoleProvenanceEdge {
        kind,
        parent: PathBuf::from(parent),
        child: PathBuf::from(child),
        declaration: String::new(),
        line: 1,
        requires_test: false,
    };
    let refused = |helper: &str,
                   edges: Vec<SourceRoleProvenanceEdge>,
                   include_target: bool,
                   unresolved_include: bool| {
        let mut index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
        let mut facts = summarize_file(PathBuf::from(helper), shadow.to_string());
        facts.role_provenance.edges = edges;
        index.insert_file_only(PathBuf::from(helper), facts);
        if include_target {
            index.include_targets.insert(PathBuf::from(helper));
        }
        if unresolved_include {
            index.include_limitations.push(RustIncludeLimitation {
                parent: PathBuf::from("tests/a.rs"),
                line: 1,
                expression: "include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/tests/common.rs\"))"
                    .to_string(),
                reason_code: "rust_include_unresolved".to_string(),
            });
        }
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(TESTS));
        test.and_then(|test| OwnerPinSyntax::default().refusal(test, &test.assertions[0], &index))
            .is_some()
    };
    // Controls: a root of its own, or a module of `src/lib.rs`, is another
    // crate than the test's.
    assert!(!refused("tests/common.rs", Vec::new(), false, false));
    assert!(!refused(
        "src/util.rs",
        vec![edge(
            LIB,
            "src/util.rs",
            SourceRoleProvenanceEdgeKind::Module
        )],
        false,
        false
    ));
    // `include!` pastes the fragment into every includer, edge or not.
    assert!(refused("tests/common.rs", Vec::new(), true, false));
    assert!(refused(
        "src/bin/frag.rs",
        vec![edge(
            "src/bin/a.rs",
            "src/bin/frag.rs",
            SourceRoleProvenanceEdgeKind::Include
        )],
        false,
        false
    ));
    // An unresolved `include!` anywhere may be pulling in any file.
    assert!(refused("tests/common.rs", Vec::new(), false, true));
    assert!(refused(
        "src/util.rs",
        vec![edge(
            LIB,
            "src/util.rs",
            SourceRoleProvenanceEdgeKind::Module
        )],
        false,
        true
    ));
    // A shared `tests/common/mod.rs` keeps only its first owner's edge, but
    // every `tests/*.rs` that declares it compiles its own copy.
    assert!(refused(
        "tests/common/mod.rs",
        vec![edge(
            "tests/alpha.rs",
            "tests/common/mod.rs",
            SourceRoleProvenanceEdgeKind::Module
        )],
        false,
        false
    ));
}

#[test]
fn a_module_child_of_an_ambiguous_include_fragment_stays_workspace_wide() -> Result<(), String> {
    // Real composition: `src/lib.rs` is included by two binaries, so the
    // include resolver leaves it without a parent and its `asserts` child
    // composes under it as if it were a crate root of its own.
    let root = std::env::temp_dir().join(format!(
        "ripr-owner-pin-include-fragment-{}",
        std::process::id()
    ));
    let files = [
        (
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "src/lib.rs",
            "#[macro_use]\nmod asserts;\npub fn f() -> u32 {\n    1\n}\n",
        ),
        (
            "src/asserts.rs",
            "macro_rules! assert_eq { ($a:expr, $b:expr) => {} }\n",
        ),
        (
            "src/main.rs",
            "include!(\"lib.rs\");\nfn main() {}\n#[cfg(test)]\nmod tests;\n",
        ),
        (
            "src/bin/tool.rs",
            "include!(\"../lib.rs\");\nfn main() {}\n",
        ),
        (
            "src/tests.rs",
            "#[test]\nfn t() {\n    assert_eq!(super::f(), 1);\n}\n",
        ),
    ];
    let mut paths = Vec::new();
    for (path, text) in files {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&full, text).map_err(|error| error.to_string())?;
        if path.ends_with(".rs") {
            paths.push(PathBuf::from(path));
        }
    }
    let index = crate::analysis::facts::build_index(&root, &paths);
    let _ = std::fs::remove_dir_all(&root);
    let index = index?;
    assert!(
        !index.include_limitations.is_empty(),
        "the two includers must leave `src/lib.rs` unresolved"
    );
    let test = index
        .tests()
        .iter()
        .find(|test| test.file == Path::new("src/tests.rs"));
    assert!(test.is_some(), "the fixture test must be indexed");
    let Some(test) = test else { return Ok(()) };
    assert!(
        OwnerPinSyntax::default()
            .refusal(test, &test.assertions[0], &index)
            .is_some()
    );
    Ok(())
}

#[test]
fn a_withheld_crate_roots_private_glob_is_routed_by_root() {
    let packages = BTreeSet::from(["core".to_string()]);
    let mut withheld = WithheldMacroBindings::default();
    let root = Path::new("e/src/lib.rs");
    assert!(!withheld.absorb(
        root,
        "use proptest::prelude::*;",
        &packages,
        &Default::default()
    ));
    assert!(withheld.trusted.is_empty() && !withheld.any_name);
    assert!(
        withheld
            .by_root
            .get(root)
            .is_some_and(|names| names.contains("assert_eq"))
    );
    // A glob may shadow any name, so a test file's local empty macros stay
    // ambiguous in named mode, as the full scan's empty-macro check is not
    // routed by crate.
    assert!(withheld.root_any_name);
}

#[test]
fn a_crate_local_binding_in_another_target_does_not_reach_the_test() {
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    // humantime's `benches/datetime_format.rs`: a bench is its own crate.
    for other in [
        ("benches/b.rs", "#[macro_use]\nextern crate bencher;"),
        ("benches/b.rs", "use other::assert_eq;"),
        ("benches/b.rs", "use other::*;"),
        (
            "examples/e.rs",
            "macro_rules! assert_eq { ($a:expr, $b:expr) => {} }",
        ),
        ("build.rs", "#![no_implicit_prelude]"),
    ] {
        assert_eq!(weight_refusal(tests, &[other]), None, "{other:?}");
    }
    // Exported or re-exported bindings, and files ripr cannot place in a
    // recognized target root, still reach every test.
    for other in [
        (
            "benches/b.rs",
            "#[macro_export] macro_rules! assert_eq { ($a:expr, $b:expr) => {} }",
        ),
        ("benches/b.rs", "pub use other::assert_eq;"),
        ("src/tests/b.rs", "#[macro_use]\nextern crate bencher;"),
        ("src/other.rs", "#[macro_use]\nextern crate bencher;"),
    ] {
        assert!(
            matches!(
                weight_refusal(tests, &[other]),
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "{other:?}"
        );
    }
    // The test's own crate keeps the binding.
    let own = format!("#[macro_use]\nextern crate bencher;\n{tests}");
    assert!(matches!(
        weight_refusal(&own, &[]),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
}

#[test]
fn any_mention_in_another_macros_arguments_stays_ambiguous() {
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let refused = |other: &str| {
        matches!(
            weight_refusal(tests, &[("src/other.rs", other)]),
            Some(AssertionRefusal::Syntax(
                AssertionContextRefusal::MacroBinding(_)
            ))
        )
    };
    // To the macro a plain invocation is only tokens: `define!(assert_eq!(mod
    // tests;))` can emit `macro_rules! assert_eq` and the module whose tests
    // use it, so ripgrep's `rgtest!(name, |dir, cmd| { assert_eq!(..) })`
    // stays ambiguous too.
    for other in [
        "rgtest!(f, |dir, cmd| { assert_eq!(1, 1); });",
        "wrap! { assert_eq![1, 1] }",
        "wrap!(assert_eq!{1, 1});",
        "make!(assert_eq);",
        "make! { macro_rules! assert_eq { () => {} } }",
        "make! { macro assert_eq() {} }",
        "make!($assert_eq!(1, 1));",
        "make!(assert_eq ! );",
    ] {
        assert!(refused(other), "{other}");
    }
    // `macro_use` or `no_implicit_prelude` in a macro's arguments is only
    // tokens to the parser, but the expansion may apply it to an item.
    for hidden in [
        "wrap! { #[no_implicit_prelude] mod tests; }",
        "wrap! { #[macro_use] extern crate other; }",
    ] {
        let refusal = weight_refusal(tests, &[("src/other.rs", hidden)]);
        assert!(
            matches!(
                refusal,
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "{hidden}"
        );
    }
    // A macro whose arguments name no trusted macro binds nothing.
    assert_eq!(
        weight_refusal(tests, &[("src/other.rs", "wrap!(1 + 1);")]),
        None
    );
}

#[test]
fn a_definition_confined_to_an_inline_module_refuses_only_tests_inside_it() {
    let shadow = "macro_rules! assert_eq { ($a:expr, $b:expr) => {} }";
    let outside = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    // regex-syntax's `mod tests { macro_rules! assert_eq { .. } }`.
    for other in [
        format!("mod tests {{ {shadow} }}"),
        format!("fn helper() {{ {shadow} }}"),
        format!("mod a {{ mod tests {{ {shadow} }} }}"),
    ] {
        assert_eq!(
            weight_refusal(outside, &[("src/other.rs", &other)]),
            None,
            "{other}"
        );
    }
    // The scope leaves its module through `#[macro_use]` or a child file,
    // and a file-level definition reaches every file that file declares.
    for other in [
        shadow.to_string(),
        format!("#[macro_use] mod tests {{ {shadow} }}"),
        format!("#[macro_use] mod a {{ mod tests {{ {shadow} }} }}"),
        format!("mod tests {{ {shadow} mod child; }}"),
        // `#[macro_export]` reaches the crate root from any enclosing item.
        format!("mod helpers {{ #[macro_export] {shadow} }}"),
        format!("fn helper() {{ #[macro_export] {shadow} }}"),
        format!("mod helpers {{ #[cfg_attr(test, macro_export)] {shadow} }}"),
        // Raw identifiers spell the same attributes.
        format!("mod helpers {{ #[r#macro_export] {shadow} }}"),
        format!("fn helper() {{ #[cfg_attr(all(), r#macro_export)] {shadow} }}"),
        format!("#[r#macro_use] mod helpers {{ {shadow} }}"),
    ] {
        assert!(
            matches!(
                weight_refusal(outside, &[("src/other.rs", &other)]),
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "{other}"
        );
    }
    // A test inside the confining module is refused; one after it is not.
    let inside = format!(
        "use demo::weight;\nmod tests {{\n    use super::*;\n    {shadow}\n    #[test]\n    fn weighs() {{ assert_eq!(weight(4), 12); }}\n}}\n"
    );
    assert!(matches!(
        weight_refusal(&inside, &[]),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
    let after = format!(
        "use demo::weight;\nmod tests {{\n    {shadow}\n}}\n#[test]\nfn weighs() {{ assert_eq!(weight(4), 12); }}\n"
    );
    assert_eq!(weight_refusal(&after, &[]), None);
}

#[test]
fn a_private_import_confined_to_an_inline_module_refuses_only_tests_inside_it() {
    let outside = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    // rust-hex's `mod tests { use pretty_assertions::assert_eq; .. }` shape,
    // with a macro ripr does not trust.
    for other in [
        "mod tests { use similar::assert_eq; }".to_string(),
        "mod tests { use proptest::prelude::*; }".to_string(),
        "fn helper() { use similar::assert_eq; }".to_string(),
        "mod a { mod tests { use other::assert_eq as assert_eq; } }".to_string(),
    ] {
        assert_eq!(
            weight_refusal(outside, &[("src/other.rs", &other)]),
            None,
            "{other}"
        );
    }
    // A file-level or re-exported import, or one a child file could reach
    // through `super`, stays ambiguous everywhere.
    for other in [
        "use similar::assert_eq;".to_string(),
        "mod tests { pub use similar::assert_eq; }".to_string(),
        "mod tests { pub(crate) use similar::assert_eq; }".to_string(),
        "mod tests { use similar::assert_eq; mod child; }".to_string(),
        "mod tests { use proptest::prelude::*; mod child; }".to_string(),
    ] {
        assert!(
            matches!(
                weight_refusal(outside, &[("src/other.rs", &other)]),
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "{other}"
        );
    }
    let inside = "use demo::weight;\nmod tests {\n    use super::*;\n    use similar::assert_eq;\n    #[test]\n    fn weighs() { assert_eq!(weight(4), 12); }\n}\n";
    assert!(matches!(
        weight_refusal(inside, &[]),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
    // An import in the test's own body applies however the signature wraps.
    for own_body in [
        "use demo::weight;\n#[test]\nfn weighs(\n) -> Result<(), String> {\n    use other::assert_eq;\n    assert_eq!(weight(4), 12);\n    Ok(())\n}\n",
        "use demo::weight;\n#[test]\nfn weighs()\n{\n    use other::assert_eq;\n    assert_eq!(weight(4), 12);\n}\n",
    ] {
        assert!(
            matches!(
                weight_refusal(own_body, &[]),
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "{own_body}"
        );
    }
    let after = "use demo::weight;\nmod tests {\n    use similar::assert_eq;\n}\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    assert_eq!(weight_refusal(after, &[]), None);
}

#[test]
fn pretty_assertions_imported_under_its_own_name_is_the_standard_assertion() -> Result<(), String> {
    let outside = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let manifest = |dependency: &str| {
        format!(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\n{dependency}\n"
        )
    };
    let plain = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-plain",
        &[("Cargo.toml", &manifest("pretty_assertions = \"1\""))],
    )?;
    // Cargo binds the name to another package; the source cannot tell.
    let aliased = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-aliased",
        &[(
            "Cargo.toml",
            &manifest("pretty_assertions = { package = \"fake-assertions\", version = \"1\" }"),
        )],
    )?;
    let imports = [
        "use pretty_assertions::assert_eq;",
        "use ::pretty_assertions::assert_eq;",
        "use pretty_assertions::{assert_eq, assert_ne};",
        "use pretty_assertions::assert_eq as assert_eq;",
        "mod pretty_assertions {}\nuse ::pretty_assertions::assert_eq;",
    ];
    let mut outcomes = Vec::new();
    for other in imports {
        let others = [("src/other.rs", other)];
        outcomes.push((
            other,
            weight_refusal_under(outside, &others, Some(&plain)),
            weight_refusal_under(outside, &others, Some(&aliased)),
            weight_refusal(outside, &others),
        ));
    }
    // The refusal names the import and what would verify it.
    let mut index = index(&[
        (LIB, WEIGHT_LIB),
        (TESTS, outside),
        ("src/other.rs", "use pretty_assertions::assert_eq;"),
    ]);
    index.drop_in_manifests = crate::analysis::facts::drop_in::DropInManifests::new(&aliased);
    let test = index.tests().at(0);
    let probe = return_probe(owner(&index, "weight"), "x * 3");
    let located = OwnerPinSyntax::default().equality_assertion_refusal(
        &probe,
        test,
        &test.assertions[0],
        &index,
    );
    let _ = std::fs::remove_dir_all(&plain);
    let _ = std::fs::remove_dir_all(&aliased);
    for (other, under_plain, under_alias, unrooted) in outcomes {
        assert_eq!(under_plain, None, "{other}");
        for refusal in [under_alias, unrooted] {
            assert!(
                matches!(
                    refusal,
                    Some(AssertionRefusal::Syntax(
                        AssertionContextRefusal::MacroBinding(_)
                    ))
                ),
                "{other}: {refusal:?}"
            );
        }
    }
    assert!(
        matches!(
            &located,
            Some(AssertionRefusal::MacroBinding { site: Some((_, site)), .. })
                if site.kind == MacroBindingKind::UnverifiedDropIn("pretty_assertions".into())
        ),
        "{located:?}"
    );
    assert_eq!(
        located.as_ref().map(AssertionRefusal::describe).as_deref(),
        Some(
            "src/other.rs:1 imports `assert_eq` from `pretty_assertions`, and no Cargo.toml ripr read declares `pretty_assertions` as the plain registry package; declare it by version only (no `package`, `path`, `git`, `registry` or `[patch]`)"
        )
    );
    for other in [
        "use pretty_assertions::assert_ne as assert_eq;",
        "use pretty_assertions::inner::assert_eq;",
        "use other::pretty_assertions::assert_eq;",
        "use similar::assert_eq;",
        // A local module named after the crate owns that path instead.
        "mod pretty_assertions;\nuse pretty_assertions::assert_eq;",
        "mod pretty_assertions {}\nuse pretty_assertions::assert_eq;",
        "mod r#pretty_assertions {}\nuse pretty_assertions::assert_eq;",
        "use other as pretty_assertions;\nuse pretty_assertions::assert_eq;",
        "extern crate other as pretty_assertions;\nuse ::pretty_assertions::assert_eq;",
    ] {
        assert!(
            matches!(
                weight_refusal(outside, &[("src/other.rs", other)]),
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "{other}"
        );
    }
    Ok(())
}

#[test]
fn each_refusal_names_the_gate_that_failed() {
    let refusal = |body: &str, attrs: &str| {
        weight_refusal(
            &format!("use demo::weight;\n{attrs}\n#[test]\nfn weighs() {{\n{body}\n}}\n"),
            &[],
        )
    };
    let conditional = |construct: &'static str| {
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::ConditionalPath(construct),
        ))
    };
    let pin = "assert_eq!(weight(4), 12);";
    assert_eq!(refusal(pin, ""), None);
    assert_eq!(
        refusal(&format!("for _ in 0..2 {{ {pin} }}"), ""),
        conditional("a `for` loop, which may run zero times")
    );
    assert_eq!(
        refusal(&format!("while false {{ {pin} }}"), ""),
        conditional("a `while` loop, which may run zero times")
    );
    assert_eq!(
        refusal(&format!("if true {{ {pin} }}"), ""),
        conditional("an `if` branch")
    );
    assert_eq!(
        refusal(&format!("return; {pin}"), ""),
        conditional("a block that an earlier `return` can skip")
    );
    assert_eq!(
        refusal(&format!("t!(x); {pin}"), ""),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::OpaqueMacro("t".to_string())
        ))
    );
    assert_eq!(
        refusal("assert_eq!(weight({ return; 4 }), 12);", ""),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroOperandExit("assert_eq".to_string())
        ))
    );
    assert_eq!(
        refusal(pin, "#[cfg(feature = \"std\")]"),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::TestAttribute("#[cfg(feature = \"std\")]".to_string())
        ))
    );
    assert_eq!(
        weight_refusal(
            &format!("use demo::weight;\n#[test]\nasync fn weighs() {{ {pin} }}\n"),
            &[]
        ),
        Some(AssertionRefusal::Syntax(AssertionContextRefusal::AsyncTest))
    );
}

#[test]
fn a_macro_binding_refusal_points_at_its_site() {
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let index = index(&[
        (LIB, WEIGHT_LIB),
        (TESTS, tests),
        (
            "src/other.rs",
            "fn a() {}\n#[macro_use]\nextern crate other;\n",
        ),
    ]);
    let test = index.tests().at(0);
    let probe = return_probe(owner(&index, "weight"), "x * 3");
    let refusal = OwnerPinSyntax::default().equality_assertion_refusal(
        &probe,
        test,
        &test.assertions[0],
        &index,
    );
    let located = match &refusal {
        Some(AssertionRefusal::MacroBinding {
            name,
            site: Some((path, site)),
        }) => Some((name, path, site)),
        _ => None,
    };
    assert!(located.is_some(), "expected a located refusal: {refusal:?}");
    let Some((name, path, site)) = located else {
        return;
    };
    assert_eq!(name, "assert_eq");
    assert_eq!(path, Path::new("src/other.rs"));
    assert_eq!(site.line, 3);
    assert_eq!(
        site.kind,
        MacroBindingKind::MacroUse("extern crate other;".to_string())
    );
    assert_eq!(
        refusal.as_ref().map(AssertionRefusal::describe).as_deref(),
        Some(
            "`#[macro_use] extern crate other;` at src/other.rs:3 imports macros from code ripr did not index, which may redefine `assert_eq!`"
        )
    );
    // An admitted assertion has no refusal to disclose.
    let index = index_without_other(tests);
    let test = index.tests().at(0);
    assert_eq!(
        OwnerPinSyntax::default().equality_assertion_refusal(
            &probe,
            test,
            &test.assertions[0],
            &index
        ),
        None
    );
}

fn index_without_other(tests: &str) -> RustIndex {
    index(&[(LIB, WEIGHT_LIB), (TESTS, tests)])
}

#[test]
fn trusted_macro_scan_skips_files_only_once_every_name_is_ambiguous() {
    let unresolved = |_: &Path, _: usize, _: &str| false;
    let full_scan = |index: &RustIndex| {
        index
            .files()
            .values()
            .flat_map(|facts| {
                trusted_macro_binding_sites(
                    &facts.data().source,
                    index.macro_scope_crates(),
                    NON_RETURNING_MACROS,
                    &|_, _| false,
                    &|_| false,
                )
            })
            .filter(|(_, site)| site.scope.is_none())
            .map(|(name, _)| name)
            .collect::<BTreeSet<_>>()
    };
    let shadowing = "macro_rules! assert_eq { ($($tokens:tt)*) => {} }\n";
    for (files, saturated, assert_eq_ambiguous) in [
        // A foreign glob saturates the set before the defining file is read.
        (
            vec![
                ("src/a.rs", "use std::io::prelude::*;\n"),
                ("src/b.rs", shadowing),
            ],
            true,
            true,
        ),
        // Workspace-owned globs do not saturate it, so the later file counts.
        (
            vec![
                ("src/a.rs", "use super::*;\nfn f() {}\n"),
                ("src/b.rs", shadowing),
            ],
            false,
            true,
        ),
        (
            vec![("src/a.rs", "fn f() { assert_eq!(1, 1); }\n")],
            false,
            false,
        ),
    ] {
        let index = index(&files);
        let (ambiguous, _, _) = trusted_macro_sites_in(&index, &unresolved);
        assert_eq!(ambiguous, full_scan(&index), "{files:?}");
        assert_eq!(
            ambiguous.len() == NON_RETURNING_MACROS.len(),
            saturated,
            "{files:?}"
        );
        assert_eq!(
            ambiguous.contains("assert_eq"),
            assert_eq_ambiguous,
            "{files:?}"
        );
    }
}

#[test]
fn trusted_macro_names_are_distinct_so_a_full_set_means_saturated() {
    let distinct = NON_RETURNING_MACROS.iter().collect::<BTreeSet<_>>();
    assert_eq!(distinct.len(), NON_RETURNING_MACROS.len());
}

#[test]
fn saturation_hint_skips_workspace_owned_globs() {
    for (source, hinted) in [
        ("use super::*;\n", false),
        ("use self::*;\nuse crate::*;\n", false),
        ("use std::io::prelude::*;\n", true),
        ("use super::*;\nuse rayon::prelude::*;\n", true),
        ("#[macro_use]\nmod macros;\n", true),
        ("#![no_implicit_prelude]\n", true),
        ("fn f(x: &i32) -> i32 { *x }\n", false),
    ] {
        assert_eq!(may_saturate_macro_ambiguity(source), hinted, "{source}");
    }
}

const GAUGE_LIB: &str = "pub trait Gauge {\n    fn base(&self) -> usize;\n\n    fn tally(&self) -> usize {\n        self.base() + 1\n    }\n}\n\npub struct Meter;\n\nimpl Meter {\n    pub fn new() -> Self {\n        Meter\n    }\n}\n\nimpl Gauge for Meter {\n    fn base(&self) -> usize {\n        0\n    }\n}\n";

/// #5320: a default method's receiver binding `let m = Meter::new()` pins
/// only while `Meter::new` is the one inherent constructor of that name, so
/// a same-named type's constructor in a file that never spells the trait
/// still decides the pin. The dependent scope admits such files through
/// `trait_impl_self_type_names`.
#[test]
fn a_trait_receiver_pins_only_through_its_one_constructor() {
    let tests = "use demo::{Gauge, Meter};\n\n#[test]\nfn meter_tallies() {\n    let meter = Meter::new();\n    assert_eq!(meter.tally(), 1);\n}\n";
    let admitted_with = |others: &[(&str, &str)]| {
        let mut files = vec![(LIB, GAUGE_LIB), (TESTS, tests)];
        files.extend_from_slice(others);
        let index = index(&files);
        let pin = establish(&index, "tally", "self.base() + 1");
        assert!(pin.is_some(), "the default method must establish a pin");
        pin.map(|pin| admitted_texts(&index, &pin).len())
            .unwrap_or_default()
    };
    assert_eq!(admitted_with(&[]), 1);
    let other_meter =
        "pub struct Meter;\n\nimpl Meter {\n    pub fn new() -> Self {\n        Meter\n    }\n}\n";
    assert_eq!(admitted_with(&[("src/other.rs", other_meter)]), 0);
    assert_eq!(
        trait_impl_self_type_names(GAUGE_LIB, "Gauge"),
        BTreeSet::from(["Meter".to_string()])
    );
    assert!(trait_impl_self_type_names(other_meter, "Gauge").is_empty());
}

const GATE_LIB: &str = "pub fn gate(value: u32) -> bool {\n    10 <= value\n}\n\npub fn level(value: u32) -> u32 {\n    10 + value\n}\n";

fn predicate_probe(owner: &FunctionSummary, expression: &str) -> Probe {
    Probe {
        family: ProbeFamily::Predicate,
        ..return_probe(owner, expression)
    }
}

#[test]
fn a_bare_assert_pins_a_bool_owner_to_true_or_false() {
    let tests = "use demo::{gate, level};\n\n#[test]\nfn pins() {\n    assert!(gate(10));\n    assert!(!gate(9), \"nine is below\");\n    assert!(gate(10) && gate(11));\n    assert!(gate(10).then_some(1).is_some());\n    assert!(!!gate(10));\n    debug_assert!(gate(10));\n    assert!(level(1) > 0);\n}\n";
    let index = index(&[(LIB, GATE_LIB), (TESTS, tests)]);
    let owner = owner(&index, "gate");
    // The pin is the same for the return value and for a predicate that is
    // the bool owner's whole tail.
    for probe in [
        return_probe(owner, "10 <= value"),
        predicate_probe(owner, "10 <= value"),
    ] {
        let pin = OwnerReturnPin::establish(&probe, owner, &index);
        assert!(pin.is_some(), "{:?}: a bool tail pins", probe.family);
        let Some(pin) = pin else { return };
        // Only a whole call of the owner, optionally negated once: a
        // conjunction, a chained call, a double negation and another macro
        // pin nothing about the owner's result.
        assert_eq!(
            admitted_texts(&index, &pin),
            vec![
                "assert!(gate(10));".to_string(),
                "assert!(!gate(9), \"nine is below\");".to_string(),
            ],
            "{:?}",
            probe.family
        );
    }
}

#[test]
fn a_bare_assert_pins_nothing_on_a_non_bool_owner() {
    let tests = "use demo::level;\n\n#[test]\nfn pins() {\n    assert!(level(1));\n    assert_eq!(level(1), 11);\n}\n";
    let index = index(&[(LIB, GATE_LIB), (TESTS, tests)]);
    let owner = owner(&index, "level");
    // A predicate on a non-bool owner is not its return value.
    assert!(
        OwnerReturnPin::establish(&predicate_probe(owner, "10 + value"), owner, &index).is_none()
    );
    let pin = OwnerReturnPin::establish(&return_probe(owner, "10 + value"), owner, &index);
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&index, &pin),
        vec!["assert_eq!(level(1), 11);".to_string()]
    );
}

#[test]
fn a_bare_assert_keeps_the_owner_binding_defeats() {
    // A test-local binding of the owner's name takes the call.
    let tests = "use demo::gate;\n\n#[test]\nfn pins() {\n    let gate = |value: u32| value > 3;\n    assert!(gate(10));\n}\n";
    let shadowed = index(&[(LIB, GATE_LIB), (TESTS, tests)]);
    let gate = owner(&shadowed, "gate");
    let pin = OwnerReturnPin::establish(&predicate_probe(gate, "10 <= value"), gate, &shadowed);
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert!(admitted_texts(&shadowed, &pin).is_empty());
    // A predicate that is only part of the tail is not the return value.
    let lib = "pub fn gate(value: u32) -> bool {\n    10 <= value && value < 99\n}\n";
    let partial = index(&[
        (LIB, lib),
        (
            TESTS,
            "#[test]\nfn pins() {\n    assert!(demo::gate(10));\n}\n",
        ),
    ]);
    let gate = owner(&partial, "gate");
    assert!(
        OwnerReturnPin::establish(&predicate_probe(gate, "10 <= value"), gate, &partial).is_none()
    );
}
