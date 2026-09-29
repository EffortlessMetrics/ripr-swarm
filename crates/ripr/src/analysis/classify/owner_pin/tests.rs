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
        index.functions.extend(facts.functions.iter().cloned());
        index.tests.extend(facts.tests.iter().cloned());
        index.files.insert(PathBuf::from(path), facts);
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
        .functions
        .iter()
        .find(|function| function.name == name && function.file == Path::new(LIB));
    assert!(found.is_some(), "owner `{name}` must be indexed from {LIB}");
    found.unwrap_or(&index.functions[0])
}

fn establish(index: &RustIndex, name: &str, expression: &str) -> Option<OwnerReturnPin> {
    let owner = owner(index, name);
    OwnerReturnPin::establish(&return_probe(owner, expression), owner, index)
}

/// Per assertion of the one test in `index`, whether the pin admits it.
fn admitted(index: &RustIndex, pin: &OwnerReturnPin) -> Vec<(String, bool)> {
    assert_eq!(index.tests.len(), 1, "fixture must hold exactly one test");
    let test = &index.tests[0];
    assert!(
        !test.assertions.is_empty(),
        "the test's assertions must parse"
    );
    test.assertions
        .iter()
        .map(|assertion| (assertion.text.clone(), pin.admits(test, assertion, index)))
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
fn bare_call_names_only_a_module_level_function() {
    // B2: `decode(..)` in a test names the free function, so an associated
    // `Codec::decode` owner never takes a bare call.
    let lib = "pub struct Codec;\n\nimpl Codec {\n    pub fn decode(input: u32) -> u32 {\n        input.rotate_left(3)\n    }\n}\n\npub fn decode(input: u32) -> u32 {\n    input + 1\n}\n";
    let tests = "use demo::decode;\n\n#[test]\nfn decodes() {\n    assert_eq!(decode(8), 9);\n}\n";
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let associated = index.functions.iter().find(|function| {
        function.name == "decode"
            && matches!(function.item.container, FunctionContainer::Inherent { .. })
    });
    assert!(associated.is_some_and(|function| !function.item.has_self_param));
    let Some(associated) = associated else { return };
    let probe = return_probe(associated, "input.rotate_left(3)");
    assert!(OwnerReturnPin::establish(&probe, associated, &index).is_none());
    // The free function does take the bare call, and the associated
    // function of the same name does not compete with it.
    let free = index.functions.iter().find(|function| {
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
        // A macro may hide a `return`.
        (
            "fn f(x: i32) -> Result<i32, E> {\n    ensure!(x > 0);\n    Ok(x * 2)\n}",
            "Ok(x * 2)",
        ),
    ] {
        assert!(gate(body, changed).is_none(), "{body}");
    }
}

#[test]
fn lexical_fallback_owner_is_not_established() {
    let mut index = index(&[(LIB, BUF_LIB), (TESTS, BUF_TESTS)]);
    if let Some(facts) = index.files.get_mut(Path::new(LIB)) {
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
