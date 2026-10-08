use super::*;
use crate::analysis::facts::{FunctionItemFact, SourceRoleProvenance, SourceRoleProvenanceEdge};
use crate::analysis::rust_index::summarize_file;
use crate::analysis::syntax::macro_binding_candidates;
use crate::domain::{DeltaKind, ProbeId, SourceLocation, SymbolId};
use std::path::Path;

const LIB: &str = "src/lib.rs";
const TESTS: &str = "tests/buf_tests.rs";
const HELPERS: &str = "src/helpers.rs";
const CHILD: &str = "src/helpers/stack_tests.rs";

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

/// [`index`] with composed module provenance: `provenance` sets
/// `role_provenance` per file, exactly as role composition records it for
/// the `mod` declarations in `files` (outermost edge first).
fn index_with_provenance(
    files: &[(&str, &str)],
    provenance: &[(&str, SourceRoleProvenance)],
) -> RustIndex {
    let mut index = RustIndex::default();
    index.package_names.insert("demo".to_string());
    for (path, text) in files {
        let mut facts = summarize_file(PathBuf::from(path), (*text).to_string());
        if let Some((_, chain)) = provenance.iter().find(|(file, _)| file == path) {
            facts.role_provenance = chain.clone();
        }
        index.extend_functions(facts.functions.iter().cloned());
        index.extend_tests(facts.tests.iter().cloned());
        index.insert_file_only(PathBuf::from(path), facts);
    }
    index
}

/// One composed out-of-line `mod` edge.
fn module_edge(
    parent: &str,
    child: &str,
    name: &str,
    line: usize,
    requires_test: bool,
) -> SourceRoleProvenanceEdge {
    SourceRoleProvenanceEdge {
        kind: SourceRoleProvenanceEdgeKind::Module,
        parent: PathBuf::from(parent),
        child: PathBuf::from(child),
        declaration: format!("mod {name};"),
        line,
        requires_test,
    }
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
    // #6773 review: a receiver whose name also occurs inside `mut`
    // (`let mut m`) is read at its whole-word position.
    for (binding, admitted) in [
        ("let mut m = Stack::new();", 1),
        ("let mut m: Stack = Stack::new();", 1),
        ("let mut m = Vec::<u32>::new();", 0),
    ] {
        let tests = format!(
            "use demo::Stack;\n\n#[test]\nfn depth_counts() {{\n    {binding}\n    assert_eq!(m.depth(), 1);\n}}\n"
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
fn only_plain_pub_use_statements_can_export_to_another_crate() {
    let source = "pub use fastscore::score;\npub(crate) use other::score;\nuse private::score;\n// pub use commented::score;\npub  use  spaced::*;\nfn republish() {}\n";
    assert_eq!(
        public_use_statements(source),
        ["use fastscore::score", "use  spaced::*"]
    );
    let lib = "pub mod a;\npub use self::a::score;\n";
    let root = Path::new("pricing/src/lib.rs");
    let local = index(&[("pricing/src/lib.rs", lib)]);
    assert!(!library_may_export_other(&local, root, "score"));
    let foreign = index(&[(
        "pricing/src/lib.rs",
        "pub mod a;\npub use fastscore::score;\n",
    )]);
    assert!(library_may_export_other(&foreign, root, "score"));
    assert!(!library_may_export_other(&foreign, root, "rebate"));
    let glob = index(&[("pricing/src/lib.rs", "pub mod a;\npub use fastscore::*;\n")]);
    assert!(library_may_export_other(&glob, root, "rebate"));
    // Another crate root's re-exports do not speak for this library.
    let binary = index(&[("pricing/src/main.rs", "pub use fastscore::score;\n")]);
    assert!(!library_may_export_other(&binary, root, "score"));
    for (label, lib, exports) in [
        (
            "local crate path",
            "pub mod a;\npub use crate::a::score;\n",
            false,
        ),
        (
            "split pub use",
            "pub mod a;\npub\nuse fastscore::score;\n",
            true,
        ),
        (
            "string literal",
            "pub mod a;\nconst DOC: &str = \"pub use x::score;\";\n",
            false,
        ),
        // A private alias routed through a `self`/`crate` path.
        (
            "aliased crate",
            "pub mod a;\nuse fastscore as fs;\npub use self::fs::score;\n",
            true,
        ),
        (
            "aliased extern",
            "pub mod a;\nextern crate fastscore as fs;\npub use crate::fs::*;\n",
            true,
        ),
        // A value of the callee's name, or text ripr does not read.
        (
            "const",
            "pub mod a;\npub const score: fn(i64) -> i64 = fastscore::score;\n",
            true,
        ),
        (
            "static mut",
            "pub mod a;\npub static mut score: i64 = 0;\n",
            true,
        ),
        ("include", "pub mod a;\ninclude!(\"exports.rs\");\n", true),
        (
            "unrelated const",
            "pub mod a;\npub const SCORE_MAX: i64 = 9;\n",
            false,
        ),
        // Each path inside a brace group must stay local too.
        (
            "grouped alias path",
            "pub mod a;\nuse fastscore as fs;\npub use self::{fs::score};\n",
            true,
        ),
        (
            "nested grouped alias path",
            "pub mod a;\nuse fastscore as fs;\npub use crate::{a::{rebate, fs::score}};\n",
            true,
        ),
        (
            "grouped local paths",
            "pub mod a;\npub use self::{a::score, a::rebate};\n",
            false,
        ),
        (
            "unbalanced group",
            "pub mod a;\npub use self::{a::score;\n",
            true,
        ),
        // A module name that an import alias also binds.
        (
            "alias shadows a nested module",
            "pub mod a;\nmod unrelated { mod fs {} }\npub use fastscore as fs;\npub use self::fs::score;\n",
            true,
        ),
        (
            "extern alias shadows a module",
            "pub mod a;\nmod b { mod fs {} }\nextern crate fastscore as fs;\npub use crate::fs::*;\n",
            true,
        ),
    ] {
        let library = index(&[("pricing/src/lib.rs", lib)]);
        assert_eq!(
            library_may_export_other(&library, root, "score"),
            exports,
            "{label}"
        );
    }
    // A file under `src/` with no established crate root is unread.
    let unresolved = index(&[
        ("pricing/src/lib.rs", "pub mod a;\n"),
        ("pricing/src/orphan.rs", "pub fn other() {}\n"),
    ]);
    assert!(library_may_export_other(&unresolved, root, "score"));
}

#[test]
fn an_unsafe_block_holding_only_the_owner_call_pins_it() {
    // `assert_eq!(unsafe { byte_at(b"xyz", 1) }, b'y')`: calling an
    // `unsafe fn` needs the block, and its value is the call's value.
    let lib = "pub unsafe fn byte_at(bytes: &[u8], index: usize) -> u8 {\n    *bytes.get_unchecked(index)\n}\n";
    let changed = "*bytes.get_unchecked(index)";
    for (operand, admitted) in [
        ("unsafe { byte_at(b\"xyz\", 1) }", 1),
        ("unsafe{byte_at(b\"xyz\", 1)}", 1),
        ("unsafe {\n        byte_at(b\"xyz\", 1)\n    }", 1),
        ("unsafe { byte_at(b\"xyz\", 1) /* SAFETY: 1 < 3 */ }", 1),
        (
            "unsafe {\n        // SAFETY: 1 < 3.\n        byte_at(b\"xyz\", 1)\n    }",
            1,
        ),
        // A statement in the block means its value is not only the call.
        ("unsafe { let v = byte_at(b\"xyz\", 1); v }", 0),
        ("unsafe { byte_at(b\"xyz\", 1); 121 }", 0),
        // Something chained after the block, or after the call inside it.
        ("unsafe { byte_at(b\"xyz\", 1) }.wrapping_add(0)", 0),
        ("unsafe { byte_at(b\"xyz\", 1).wrapping_add(0) }", 0),
        // Another call wrapping the owner call is not the owner's value.
        ("unsafe { u8::from(byte_at(b\"xyz\", 1)) }", 0),
        // A function merely named like the keyword is not a block.
        ("unsafe_byte_at(b\"xyz\", 1)", 0),
    ] {
        let tests = format!(
            "use demo::byte_at;\n\n#[test]\nfn reads() {{\n    assert_eq!({operand}, b'y');\n}}\n"
        );
        let index = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index, "byte_at", changed);
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert_eq!(admitted_texts(&index, &pin).len(), admitted, "{operand}");
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
    // A pin on one branch of a conditional `Err` never evaluates the other.
    for (body, changed) in [
        (
            "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(if x == -1 { E::A } else { E::B });\n    }\n    Ok(x)\n}",
            "return Err(if x == -1 { E::A } else { E::B });",
        ),
        (
            "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(match x { -1 => E::A, _ => E::B });\n    }\n    Ok(x)\n}",
            "return Err(match x { -1 => E::A, _ => E::B });",
        ),
    ] {
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

/// #6675: a binary bitwise `|` evaluates both operands on every input, like
/// `+`/`*`, so a bitwise tail establishes the pin path. Every other pipe
/// (lazy `||`, a closure's parameter list, `|=`) stays conditional.
#[test]
fn a_bitwise_or_tail_is_unconditional_but_closures_and_lazy_or_are_not() {
    for (body, changed) in [
        (
            "fn pack(hi: u8, lo: u8) -> u16 {\n    u16::from(lo) | (u16::from(hi) << 8)\n}",
            "u16::from(lo) | (u16::from(hi) << 8)",
        ),
        (
            "fn pack(hi: u8, lo: u8) -> u16 {\n    (u16::from(hi) << 8) | u16::from(lo)\n}",
            "(u16::from(hi) << 8) | u16::from(lo)",
        ),
        (
            "fn flag(base: u8) -> u8 {\n    0x80 | base\n}",
            "0x80 | base",
        ),
        (
            "fn mix(a: u8, b: u8) -> u8 {\n    (a & 0x0f) ^ (b >> 4) | a\n}",
            "(a & 0x0f) ^ (b >> 4) | a",
        ),
        (
            "fn bits(xs: &[u8]) -> u8 {\n    xs[0] | first(xs)\n}",
            "xs[0] | first(xs)",
        ),
    ] {
        assert!(gate(body, changed).is_some(), "{body}");
    }
    for (body, changed) in [
        (
            "fn f(a: bool, b: u8) -> bool {\n    a || check(b)\n}",
            "a || check(b)",
        ),
        (
            "fn f(x: Option<u8>) -> u8 {\n    x.map_or(0, |v| v | 1)\n}",
            "x.map_or(0, |v| v | 1)",
        ),
        (
            "fn f(x: u8) -> u8 {\n    apply(x, |v| v | 1)\n}",
            "apply(x, |v| v | 1)",
        ),
        (
            "fn f(x: u8) -> u8 {\n    apply(x, move |v| v | 1)\n}",
            "apply(x, move |v| v | 1)",
        ),
        ("fn f(x: u8) -> u8 {\n    run(x, || 1)\n}", "run(x, || 1)"),
        // Pattern alternatives short-circuit: they are not a bitwise OR.
        (
            "fn open(&self) -> bool {\n    matches!(self.kind, Kind::Open | Kind::Pending)\n}",
            "matches!(self.kind, Kind::Open | Kind::Pending)",
        ),
        (
            "fn known(x: Option<u8>) -> bool {\n    matches!(x, Some(_) | None)\n}",
            "matches!(x, Some(_) | None)",
        ),
        (
            "fn small(x: u8) -> bool {\n    matches!(x, 1 | 2)\n}",
            "matches!(x, 1 | 2)",
        ),
        // A closure in a struct literal or an array is still a closure.
        (
            "fn make() -> Foo {\n    Foo { f: |x| x }\n}",
            "Foo { f: |x| x }",
        ),
        ("fn make() -> [F; 1] {\n    [|x| x]\n}", "[|x| x]"),
    ] {
        assert!(gate(body, changed).is_none(), "{body}");
    }
}

#[test]
fn bitwise_pipe_reading_distinguishes_operand_position() {
    assert!(!has_non_bitwise_pipe("a | b"));
    assert!(!has_non_bitwise_pipe("f(x) | g[0] | h()?"));
    assert!(has_non_bitwise_pipe("|x| x + 1"));
    assert!(has_non_bitwise_pipe("f(|x| x)"));
    assert!(has_non_bitwise_pipe("move |x| x"));
    assert!(has_non_bitwise_pipe("return |x| x"));
    assert!(has_non_bitwise_pipe("a || b"));
    assert!(has_non_bitwise_pipe("a |= b"));
    assert!(has_non_bitwise_pipe("{ a } | b"));
    assert!(has_non_bitwise_pipe("matches!(k, A | B)"));
    assert!(has_non_bitwise_pipe("f(matches!(k, Some(_) | None))"));
    assert!(has_non_bitwise_pipe("{ let (A(x) | B(x)) = v; x }"));
    assert!(!has_non_bitwise_pipe("f(a) | g(b)"));
    // A multibyte character before the operand must not split a slice.
    assert!(!has_non_bitwise_pipe("é_flag | b"));
    assert!(!has_non_bitwise_pipe("(\u{e9}) | b"));
    assert!(has_non_bitwise_pipe("é in | b"));
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
    let shadowed = include_fragment_refusal(
        "shadow",
        LIB_WITH_ASSERTS,
        "macro_rules! assert_eq { ($a:expr, $b:expr) => {} }\n",
    )?;
    assert!(matches!(
        shadowed,
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
    // Control: the same layout whose module defines no macro raises no
    // macro-binding refusal.
    let plain = include_fragment_refusal("plain", LIB_WITH_ASSERTS, "pub fn g() {}\n")?;
    assert!(!matches!(
        plain,
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
    Ok(())
}

const LIB_WITH_ASSERTS: &str = "#[macro_use]\nmod asserts;\npub fn f() -> u32 {\n    1\n}\n";

fn include_fragment_refusal(
    label: &str,
    lib: &str,
    asserts: &str,
) -> Result<Option<AssertionRefusal>, String> {
    let root = std::env::temp_dir().join(format!(
        "ripr-owner-pin-include-fragment-{label}-{}",
        std::process::id()
    ));
    let files = [
        (
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", lib),
        ("src/asserts.rs", asserts),
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
        .find(|test| test.file == Path::new("src/tests.rs"))
        .ok_or("the fixture test must be indexed")?;
    let assertion = test
        .assertions
        .first()
        .ok_or("the fixture assertion must be indexed")?;
    Ok(OwnerPinSyntax::default().refusal(test, assertion, &index))
}

#[test]
fn an_unresolvable_path_attribute_or_lexical_fallback_disables_root_routing() {
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let base = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    assert_eq!(
        TargetRoots::new(&base).root(Path::new(TESTS), &base),
        Some(PathBuf::from(TESTS))
    );
    // `#[cfg_attr(.., path = "..")]` has no static target, so its file
    // records no module edge and could look like a root of its own.
    let declaring = "#[cfg_attr(unix, path = \"unix.rs\")]\nmod platform;\n";
    let unknown = index(&[
        (LIB, WEIGHT_LIB),
        (TESTS, tests),
        ("src/main.rs", declaring),
    ]);
    assert!(
        unknown
            .files()
            .get(Path::new("src/main.rs"))
            .is_some_and(|facts| facts
                .module_declarations
                .iter()
                .any(|declaration| declaration.path_target == ModulePathTarget::Unknown)),
        "the fixture must declare an unresolvable `#[path]`"
    );
    assert_eq!(
        TargetRoots::new(&unknown).root(Path::new(TESTS), &unknown),
        None
    );
    let mut fallback = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    let mut facts = summarize_file(PathBuf::from("src/other.rs"), String::new());
    facts.used_lexical_fallback = true;
    fallback.insert_file_only(PathBuf::from("src/other.rs"), facts);
    assert_eq!(
        TargetRoots::new(&fallback).root(Path::new(TESTS), &fallback),
        None
    );
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
        // An unresolved `#[macro_use] mod` may `#[macro_export]` its macros.
        ("benches/b.rs", "#[macro_use]\nmod generated;"),
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
fn a_for_loop_over_a_constant_row_table_runs_its_assertion() {
    let refusal = |body: &str| {
        weight_refusal(
            &format!("use demo::weight;\n#[test]\nfn weighs() {{\n{body}\n}}\n"),
            &[],
        )
    };
    let conditional = |construct: &'static str| {
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::ConditionalPath(construct),
        ))
    };
    let pin = "assert_eq!(weight(x), want);";
    for admitted in [
        format!("let cases = [(1, 3), (4, 12)];\nfor (x, want) in cases {{ {pin} }}"),
        format!("let cases: [(u32, u32); 1] = [(4, 12)];\nfor (x, want) in &cases {{ {pin} }}"),
        format!("for (x, want) in [(4, 12), (-1, -3)] {{ {pin} }}"),
        format!("for (x, want) in &[(4, Some(12)), (0, None)] {{ {pin} }}"),
        format!("for (x, want) in [(&[4], vec![12]), (&[], vec![])] {{ {pin} }}"),
        format!("for (x, want) in [(4, Ok(12)), (0, Err(Kind::Empty))] {{ {pin} }}"),
        format!("for (x, want) in [(4, 12)] {{ {pin} if x > 9 {{ break; }} }}"),
    ] {
        assert_eq!(refusal(&admitted), None, "{admitted}");
    }
    let zero = conditional("a `for` loop, which may run zero times");
    for refused in [
        // May be empty, or its length is not visible.
        format!("let cases: [(u32, u32); 0] = [];\nfor (x, want) in cases {{ {pin} }}"),
        format!("for (x, want) in [(4, 12); 0] {{ {pin} }}"),
        format!("for (x, want) in rows() {{ {pin} }}"),
        format!("for x in 0..4 {{ let want = x * 3; {pin} }}"),
        // A row could hold the owner's own output.
        format!("for (x, want) in [(4, weight(4))] {{ {pin} }}"),
        format!("for (x, want) in [(4, EXPECTED)] {{ {pin} }}"),
        format!("for (x, want) in [(4, Wrap::of(12))] {{ {pin} }}"),
        format!("for (x, want) in [(4, Expected(4))] {{ {pin} }}"),
        format!("for (x, want) in [(4, Twelve)] {{ {pin} }}"),
        // A `cfg` may remove every row.
        format!("for (x, want) in [#[cfg(any())] (4, 0)] {{ {pin} }}"),
        format!(
            "let cases: [(u32, u32); 0] = [#[cfg(any())] (4, 0)];\nfor (x, want) in cases {{ {pin} }}"
        ),
        format!("for (x, want) in [(4, #[cfg(any())] 0)] {{ {pin} }}"),
        format!("for (x, want) in [(4, vec![weight(4)])] {{ {pin} }}"),
        // The bound rows may change or are not the ones iterated.
        format!("let mut cases = [(4, 12)];\ncases[0].1 = 0;\nfor (x, want) in cases {{ {pin} }}"),
        format!(
            "let cases = [(4, 12)];\nlet cases = [(4, 0)];\nfor (x, want) in cases {{ {pin} }}"
        ),
        format!("let cases = [(4, 12)];\nfor (x, want) in cases.iter().skip(1) {{ {pin} }}"),
        format!("let cases = rows();\nfor (x, want) in cases {{ {pin} }}"),
        format!(
            "let cases = [(4, 12)];\nlet r#cases: [(u32, u32); 0] = [];\nfor (x, want) in cases {{ {pin} }}"
        ),
        format!("for (x, want) in &mut [(4, 12)] {{ {pin} }}"),
        format!("'rows: for (x, want) in [(4, 12)] {{ {pin} }}"),
    ] {
        assert_eq!(refusal(&refused), zero, "{refused}");
    }
    assert_eq!(
        refusal(&format!(
            "for (x, want) in [(4, 12), (0, 0)] {{ if x == 0 {{ continue; }} {pin} }}"
        )),
        conditional("a `for` loop after a `break` or `continue` that can skip it")
    );
    // Only the loop body is admitted; the iterable is not on its path.
    assert!(refusal("for _ in [assert_eq!(weight(4), 12)] {}").is_some());
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
        let (ambiguous, _, _) =
            trusted_macro_sites_in(&index, &TargetRoots::new(&index), &unresolved);
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

const WINDOW_LIB: &str = "#[derive(Debug, PartialEq, Eq)]\npub struct Window {\n    start: u32,\n    end: u32,\n}\n\nimpl Window {\n    pub fn new(start: u32, end: u32) -> Self {\n        Window { start, end }\n    }\n}\n\nimpl Clone for Window {\n    fn clone(&self) -> Self {\n        Window {\n            start: self.start,\n            end: self.end,\n        }\n    }\n}\n";

const WINDOW_TESTS: &str = "use demo::Window;\n\n#[test]\nfn a_clone_equals_its_original() {\n    let window = Window::new(3, 9);\n    let other = Window::new(3, 9);\n    assert_eq!(window.clone(), window);\n    assert_eq!(window, window.clone());\n    assert_ne!(window.clone(), Window::new(0, 0));\n    assert_eq!(window.clone(), other);\n    assert_eq!(window.clone(), Window::new(3, 9));\n}\n";

/// A `field_construction` probe on `start: self.start,` in `Window::clone`.
fn clone_field_pin(lib: &str, tests: &str) -> (RustIndex, Option<OwnerReturnPin>) {
    clone_field_pin_at(lib, tests, "start: self.start,", "start: self.start,")
}

/// A `field_construction` probe with `expression` on the (first) line of
/// `clone` whose trimmed text is `line_text`.
fn clone_field_pin_at(
    lib: &str,
    tests: &str,
    line_text: &str,
    expression: &str,
) -> (RustIndex, Option<OwnerReturnPin>) {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = field_pin(&index, lib, line_text, expression);
    (index, pin)
}

/// A `field_construction` probe on `line_text` of `clone` when production
/// and its `mod tests` share one file (#6905).
fn same_file_clone_field_pin(
    source: &str,
    line_text: &str,
    expression: &str,
) -> (RustIndex, Option<OwnerReturnPin>) {
    let index = index(&[(LIB, source)]);
    let pin = field_pin(&index, source, line_text, expression);
    (index, pin)
}

fn field_pin(
    index: &RustIndex,
    lib: &str,
    line_text: &str,
    expression: &str,
) -> Option<OwnerReturnPin> {
    let owner = owner(index, "clone");
    let line = lib
        .lines()
        .enumerate()
        .position(|(offset, line)| offset + 1 > owner.start_line && line.trim() == line_text)
        .map_or(0, |offset| offset + 1);
    assert!(
        line > owner.start_line,
        "fixture: the field line must parse"
    );
    let mut probe = return_probe(owner, expression);
    probe.family = ProbeFamily::FieldConstruction;
    probe.location = SourceLocation::new(owner.file.clone(), line, 1);
    OwnerReturnPin::establish(&probe, owner, index)
}

/// #6692: `assert_eq!(recv.clone(), recv)` through a derived `PartialEq`
/// compares every field of a hand-written clone with the original, so it
/// pins a changed field of the returned literal. `assert_ne!` and a
/// comparison with any other value do not.
#[test]
fn a_clone_compared_with_its_own_receiver_pins_its_fields() {
    let (index, pin) = clone_field_pin(WINDOW_LIB, WINDOW_TESTS);
    assert!(pin.is_some(), "the clone field pin must establish");
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&index, &pin),
        vec![
            "assert_eq!(window.clone(), window);".to_string(),
            "assert_eq!(window, window.clone());".to_string(),
        ]
    );
}

/// #6692 negative controls: no derived `PartialEq` (a hand-written one may
/// ignore fields), a derive behind `cfg_attr`, a workspace `trait Clone`,
/// another exit, or a field outside the returned literal leaves the pin
/// unestablished.
#[test]
fn a_clone_field_pin_needs_derived_equality_and_the_returned_literal() {
    let manual_eq = WINDOW_LIB.replace("PartialEq, Eq", "Eq")
        + "impl PartialEq for Window {\n    fn eq(&self, other: &Self) -> bool { self.end == other.end }\n}\n";
    let gated_derive = WINDOW_LIB.replace(
        "#[derive(Debug, PartialEq, Eq)]",
        "#[cfg_attr(test, derive(Debug, PartialEq, Eq))]",
    );
    let local_trait = WINDOW_LIB.to_string() + "pub trait Clone {}\n";
    let early_exit = WINDOW_LIB.replace(
        "        Window {\n            start",
        "        if self.end == 0 {\n            return Window::new(0, 0);\n        }\n        Window {\n            start",
    );
    let bound_first = WINDOW_LIB.replace(
        "        Window {\n            start: self.start,\n            end: self.end,\n        }\n",
        "        let copy = Window {\n            start: self.start,\n            end: self.end,\n        };\n        Window::new(copy.end, copy.start)\n",
    );
    // #6773 review: a manual `PartialEq<Rhs>` beside the derive, a shadowed
    // `PartialEq` derive (imported from elsewhere, or a qualified derive
    // path), a derived `Clone` beside the hand-written one, and generic
    // parameters on the type all fail closed.
    let manual_rhs_eq = WINDOW_LIB.to_string()
        + "impl PartialEq<u32> for Window {\n    fn eq(&self, other: &u32) -> bool { self.end == *other }\n}\n";
    let imported_derive = WINDOW_LIB.to_string() + "use some_crate::PartialEq;\n";
    let renamed_derive = WINDOW_LIB.to_string() + "use some_crate::Thing as PartialEq;\n";
    let qualified_derive = WINDOW_LIB.replace(
        "#[derive(Debug, PartialEq, Eq)]",
        "#[derive(Debug, some_crate::PartialEq, Eq)]",
    );
    let derived_clone = WINDOW_LIB.replace(
        "#[derive(Debug, PartialEq, Eq)]",
        "#[derive(Debug, PartialEq, Eq, Clone)]",
    );
    let defaulted_generic =
        WINDOW_LIB.replace("pub struct Window {", "pub struct Window<T = u32> {");
    // #6773 review: a foreign `Clone` on the owner side.
    let foreign_clone_path =
        WINDOW_LIB.replace("impl Clone for Window", "impl dupe::Clone for Window");
    let foreign_clone_import = WINDOW_LIB.to_string() + "use dupe::Clone;\n";
    let std_clone_path =
        WINDOW_LIB.replace("impl Clone for Window", "impl std::clone::Clone for Window");
    let renamed_std_clone = std_clone_path.clone() + "extern crate dupe as std;\n";
    for lib in [
        manual_eq,
        gated_derive,
        local_trait,
        early_exit,
        bound_first,
        manual_rhs_eq,
        imported_derive,
        renamed_derive,
        qualified_derive,
        derived_clone,
        defaulted_generic,
        foreign_clone_path,
        foreign_clone_import,
        renamed_std_clone,
    ] {
        let (_, pin) = clone_field_pin(&lib, WINDOW_TESTS);
        assert!(pin.is_none(), "{lib}");
    }
    // Fixture controls: the plain library and a `std`-rooted `Clone` path
    // still establish.
    assert!(clone_field_pin(WINDOW_LIB, WINDOW_TESTS).1.is_some());
    assert!(clone_field_pin(&std_clone_path, WINDOW_TESTS).1.is_some());
}

/// #6773 review: `struct W<String> { x: String }` names a type parameter
/// `String`, and `impl Clone for W<Foo>` instantiates it with a type whose
/// `PartialEq` may ignore the value; a renamed `impl Eq2 for W<Foo>` may
/// also stand beside the derive. A generic declaration or an instantiated
/// self type never establishes.
#[test]
fn a_clone_field_pin_refuses_generic_types_and_instantiated_impls() {
    let generic = "#[derive(Debug, PartialEq)]\npub struct W<String> {\n    pub x: String,\n}\n\npub struct Foo;\n\nimpl PartialEq for Foo {\n    fn eq(&self, _: &Self) -> bool {\n        true\n    }\n}\n\nimpl Clone for W<Foo> {\n    fn clone(&self) -> Self {\n        W {\n            x: Foo,\n        }\n    }\n}\n";
    let renamed_eq = generic.replace(
        "impl PartialEq for Foo",
        "use std::cmp::PartialEq as Eq2;\n\nimpl Eq2 for W<Foo> {\n    fn eq(&self, _: &Self) -> bool {\n        true\n    }\n}\n\nimpl PartialEq for Foo",
    );
    let tests = "use demo::W;\n\n#[test]\nfn compares() {\n    let w = W { x: demo::Foo };\n    assert_eq!(w.clone(), w);\n}\n";
    for lib in [generic.to_string(), renamed_eq] {
        let (_, pin) = clone_field_pin_at(&lib, tests, "x: Foo,", "x: Foo,");
        assert!(pin.is_none(), "{lib}");
    }
    // Same-shape positive control: a non-generic `W` with a value-compared
    // field establishes and admits the clone comparison.
    let plain = "#[derive(Debug, PartialEq)]\npub struct W {\n    pub x: u32,\n}\n\nimpl Clone for W {\n    fn clone(&self) -> Self {\n        W {\n            x: self.x,\n        }\n    }\n}\n";
    let plain_tests = "use demo::W;\n\n#[test]\nfn compares() {\n    let w = W { x: 3 };\n    assert_eq!(w.clone(), w);\n}\n";
    let (index, pin) = clone_field_pin_at(plain, plain_tests, "x: self.x,", "x: self.x,");
    assert!(pin.is_some(), "the non-generic control must establish");
    if let Some(pin) = pin {
        assert_eq!(admitted_texts(&index, &pin).len(), 1);
    }
}

/// #6692 (RIPR-SPEC-0225 rule 4): the changed field's own type must
/// compare by value. A workspace field type with derived equality all the
/// way down credits; one with a hand-written `PartialEq` (which may ignore
/// the value), an attribute on the field, or an unknown type does not.
#[test]
fn a_clone_field_pin_needs_a_field_type_that_compares_by_value() {
    let with_start_type = |ty: &str, extra: &str| {
        WINDOW_LIB.replace("    start: u32,\n", &format!("    start: {ty},\n")) + extra
    };
    let derived_point = "#[derive(Debug, PartialEq, Eq, Clone, Copy)]\npub struct Point {\n    x: u32,\n    tag: Option<Vec<String>>,\n}\n";
    let manual_point = "#[derive(Debug, Eq, Clone, Copy)]\npub struct Point {\n    x: u32,\n}\nimpl PartialEq for Point {\n    fn eq(&self, _: &Self) -> bool { true }\n}\n";
    for (lib, credits) in [
        (with_start_type("Point", derived_point), true),
        (with_start_type("Option<(u8, [u16; 2])>", ""), true),
        (with_start_type("Point", manual_point), false),
        (with_start_type("Unknown", ""), false),
        (with_start_type("Vec<Unknown>", ""), false),
        // #6773 review: a standard base name must be the standard type. A
        // path rooted outside std/core/alloc, or a declaring file that
        // imports, globs, renames or aliases the name from elsewhere, may
        // bind a foreign type whose `==` ignores the value.
        (with_start_type("String", ""), true),
        (with_start_type("std::string::String", ""), true),
        (with_start_type("Vec<u8>", "use std::vec::Vec;\n"), true),
        (
            with_start_type(
                "String",
                "#[cfg(test)]\nmod tests {\n    use super::*;\n}\n",
            ),
            true,
        ),
        (with_start_type("foreign::String", ""), false),
        (with_start_type("other::Vec<u8>", ""), false),
        (with_start_type("crate::String", ""), false),
        (with_start_type("Vec<u8>", "use foreign::Vec;\n"), false),
        (
            with_start_type("Vec<u8>", "use foreign::{Other, Vec};\n"),
            false,
        ),
        (
            with_start_type("String", "use x::Thing as String;\n"),
            false,
        ),
        (with_start_type("String", "use foreign::*;\n"), false),
        (
            with_start_type("String", "use crate::text::String;\n"),
            false,
        ),
        (with_start_type("String", "type String = Loose;\n"), false),
        (
            with_start_type("std::string::String", "extern crate other as std;\n"),
            false,
        ),
        (
            with_start_type("Point", &format!("{derived_point}use foreign::Point;\n")),
            false,
        ),
        (
            WINDOW_LIB.replace(
                "    start: u32,\n",
                "    #[derivative(PartialEq = \"ignore\")]\n    start: u32,\n",
            ),
            false,
        ),
    ] {
        let (_, pin) = clone_field_pin(&lib, WINDOW_TESTS);
        assert_eq!(pin.is_some(), credits, "{lib}");
    }
}

/// #6692 review: the receiver must not itself come from the clone under
/// test (an idempotent wrong field, `start: 0` or `start: self.end`, would
/// then survive) or be changed after it is bound. A literal or the type's
/// own non-cloning constructor credits.
#[test]
fn a_clone_field_pin_needs_a_receiver_built_without_the_clone() {
    let lib = WINDOW_LIB.replace(
        "impl Window {\n",
        "impl Window {\n    pub fn copied(other: &Window) -> Self {\n        other.clone()\n    }\n    pub fn built(start: u32, end: u32) -> Self {\n        copy_of(&Window { start, end })\n    }\n",
    ) + "fn copy_of(window: &Window) -> Window {\n    window.clone()\n}\nimpl Default for Window {\n    fn default() -> Self {\n        Window::new(0, 0)\n    }\n}\nimpl From<u32> for Window {\n    fn from(start: u32) -> Self {\n        Window::new(start, start)\n    }\n}\n";
    let test = |body: &str| {
        format!(
            "use demo::Window;\n\n#[test]\nfn compares() {{\n{body}\n    assert_eq!(w.clone(), w);\n}}\n"
        )
    };
    for (body, credits) in [
        ("    let w = Window::new(3, 9);", true),
        ("    let w: Window = Window::new(3, 9);", true),
        ("    let w = Window { start: 3, end: 9 };", true),
        (
            "    let base = Window::new(3, 9);\n    let w: Window = base.clone();",
            false,
        ),
        (
            "    let base = Window::new(3, 9);\n    let w: Window = base.to_owned();",
            false,
        ),
        (
            "    let mut w = Window::new(3, 9);\n    w = w.clone();",
            false,
        ),
        (
            "    let mut w = Window::new(3, 9);\n    reset(&mut w);",
            false,
        ),
        (
            "    let mut w = Window::new(3, 9);\n    w.start = w.end;",
            false,
        ),
        (
            "    let mut w = Window::new(3, 9);\n    let start = &mut w.start;\n    *start = 9;",
            false,
        ),
        (
            "    let mut w = Window::new(3, 9);\n    w.set_start(9);",
            false,
        ),
        ("    let mut w = Window::new(3, 9);", false),
        ("    let mut\tw = Window::new(3, 9);", false),
        // #6773 review: a helper, a local or an update base may carry a
        // wrong clone's output into the receiver; only literals and
        // constants are independent.
        ("    let w = Window::new(u32::MAX, 9 as u32);", true),
        ("    let w = Window { start: LIMIT, end: 9 };", true),
        (
            "    let base = Window::new(3, 9);\n    let w = Window::new(make(&base), 9);",
            false,
        ),
        (
            "    let base = Window::new(3, 9);\n    let w = Window { start: helper(&base), end: 9 };",
            false,
        ),
        (
            "    let base = Window::new(3, 9);\n    let w = Window { start: base.start, end: 9 };",
            false,
        ),
        (
            "    let base = Window::new(3, 9);\n    let w = Window { start: 3, ..base };",
            false,
        ),
        ("    let s = 3;\n    let w = Window::new(s, 9);", false),
        ("    let w = Window::new(*START, 9);", false),
        ("    let w = Window::new(start!(), 9);", false),
        ("    let w = Window::built(3, 9);", false),
        // Assignment detection on its own (no `mut` anywhere).
        (
            "    let w = Window::new(3, 9);\n    w = Window::new(0, 0);",
            false,
        ),
        ("    let w = Window::new(3, 9);\n    w += 1;", false),
        (
            "    let w = Window::new(3, 9);\n    let w = w.clone();",
            false,
        ),
        ("    let w = Window::default();", false),
        ("    let w = Window::from(3);", false),
        ("    let w = Window::copied(&Window::new(3, 9));", false),
        ("    let w: Window = make_window();", false),
    ] {
        let (index, pin) = clone_field_pin(&lib, &test(body));
        assert!(pin.is_some(), "the clone field pin must establish");
        let Some(pin) = pin else { return };
        assert_eq!(!admitted_texts(&index, &pin).is_empty(), credits, "{body}");
    }
}

/// #6692 review: a changed line inside a nested literal or a call within
/// the returned literal is not that literal's own field value.
#[test]
fn a_clone_field_pin_needs_the_changed_line_at_the_literals_own_depth() {
    let tail = |fields: &str| {
        WINDOW_LIB.replace(
            "        Window {\n            start: self.start,\n            end: self.end,\n        }\n",
            &format!("        Window {{\n{fields}            end: self.end,\n        }}\n"),
        )
    };
    let nested = tail(
        "            start: Raw {\n                start: self.start,\n            }\n            .start,\n",
    );
    let called = tail(
        "            start: normalize(Raw {\n                start: self.start,\n            }),\n",
    );
    for lib in [nested, called] {
        let (_, pin) = clone_field_pin(&lib, WINDOW_TESTS);
        assert!(pin.is_none(), "{lib}");
    }
    let one_line = tail("            start: normalize(Raw { start: self.start }),\n");
    let (_, pin) = clone_field_pin_at(
        &one_line,
        WINDOW_TESTS,
        "start: normalize(Raw { start: self.start }),",
        "start: self.start",
    );
    assert!(pin.is_none(), "{one_line}");
    // The outer field itself, written over several lines, is at depth 1 only
    // on its first line, and that line is not the whole initializer.
    let (_, pin) = clone_field_pin_at(
        &one_line,
        WINDOW_TESTS,
        "start: normalize(Raw { start: self.start }),",
        "start: normalize(Raw { start: self.start }),",
    );
    assert!(
        pin.is_none(),
        "a nested literal on the changed line fails closed"
    );
}

/// #6692 review negative controls: a competing `fn clone`, a foreign
/// `Clone` import in the test's file, a `?` or an unbounded macro in the
/// clone body, a second declaration of the type, and an equality-changing
/// type attribute. `Self { .. }` is the returned literal too.
#[test]
fn a_clone_field_pin_refuses_competitors_and_unreadable_shapes() {
    let self_literal = WINDOW_LIB.replace(
        "        Window {\n            start: self.start,",
        "        Self {\n            start: self.start,",
    );
    let (index, pin) = clone_field_pin(&self_literal, WINDOW_TESTS);
    assert!(pin.is_some(), "a `Self {{ .. }}` tail establishes the pin");
    if let Some(pin) = pin {
        assert_eq!(admitted_texts(&index, &pin).len(), 2);
    }
    let competitor = WINDOW_LIB.to_string()
        + "pub struct Other;\nimpl Other {\n    pub fn clone(&self) -> u8 {\n        0\n    }\n}\n";
    let tried = WINDOW_LIB.replace(
        "            end: self.end,\n        }\n    }\n}\n",
        "            end: check(self.end)?,\n        }\n    }\n}\n",
    );
    let macro_field = WINDOW_LIB.replace(
        "            end: self.end,\n        }\n    }\n}\n",
        "            end: pick!(self.end),\n        }\n    }\n}\n",
    );
    let duplicate = WINDOW_LIB.to_string()
        + "pub mod other {\n    #[derive(Debug, PartialEq, Eq)]\n    pub struct Window {\n        start: u32,\n    }\n}\n";
    let serde_attr = WINDOW_LIB.replace(
        "#[derive(Debug, PartialEq, Eq)]\npub struct Window",
        "#[derive(Debug, PartialEq, Eq)]\n#[serde(rename_all = \"camelCase\")]\npub struct Window",
    );
    for lib in [competitor, tried, macro_field, duplicate, serde_attr] {
        let (_, pin) = clone_field_pin(&lib, WINDOW_TESTS);
        assert!(pin.is_none(), "{lib}");
    }
    let foreign_clone = WINDOW_TESTS.replace(
        "use demo::Window;\n",
        "use demo::Window;\nuse dupe::Clone;\n",
    );
    let (index, pin) = clone_field_pin(WINDOW_LIB, &foreign_clone);
    assert!(pin.is_some());
    if let Some(pin) = pin {
        assert!(admitted_texts(&index, &pin).is_empty(), "{foreign_clone}");
    }
}

/// #6692 review: lint-tool attributes (`clippy::`, `rustfmt::`) do not
/// change equality, so they keep the derived-equality reading.
#[test]
fn a_clone_field_pin_allows_lint_tool_attributes_on_the_type() {
    for attribute in ["#[rustfmt::skip]", "#[clippy::has_significant_drop]"] {
        let lib = WINDOW_LIB.replace(
            "#[derive(Debug, PartialEq, Eq)]\npub struct Window",
            &format!("#[derive(Debug, PartialEq, Eq)]\n{attribute}\npub struct Window"),
        );
        let (_, pin) = clone_field_pin(&lib, WINDOW_TESTS);
        assert!(pin.is_some(), "{lib}");
    }
}

/// #6905: production `Window` with a hand-written `Clone`, and a `mod
/// tests` in the same file declaring its own same-name `Window` with a
/// derived `Clone`. The test's `window.clone()` runs the test-local clone,
/// never the changed owner.
const SHADOWED_WINDOW: &str = "#[derive(Debug, PartialEq, Eq)]\npub struct Window {\n    start: u32,\n    end: u32,\n}\n\nimpl Window {\n    pub fn new(start: u32, end: u32) -> Self {\n        Window { start, end }\n    }\n}\n\nimpl Clone for Window {\n    fn clone(&self) -> Self {\n        Window {\n            start: self.start,\n            end: self.end,\n        }\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[derive(Debug, Clone, PartialEq, Eq)]\n    struct Window {\n        start: u32,\n        end: u32,\n    }\n\n    #[test]\n    fn a_clone_equals_its_original() {\n        let window = Window { start: 3, end: 9 };\n        assert_eq!(window.clone(), window);\n    }\n}\n";

/// #6905: the pin establishes from the production type (equality reads
/// past the test-module definition), but the shadowed receiver names the
/// test-local type, so no assertion is admitted. The same file without the
/// shadowing declaration still pins.
#[test]
fn a_test_module_shadow_of_the_receiver_refuses_the_clone_pin() {
    let (index, pin) =
        same_file_clone_field_pin(SHADOWED_WINDOW, "start: self.start,", "start: self.start,");
    assert!(
        pin.is_some(),
        "the pin establishes from the production type"
    );
    let Some(pin) = pin else { return };
    assert!(
        admitted_texts(&index, &pin).is_empty(),
        "a shadowed receiver names the test-local type, not the owner"
    );
    let unshadowed = SHADOWED_WINDOW
        .replace(
            "    #[derive(Debug, Clone, PartialEq, Eq)]\n    struct Window {\n        start: u32,\n        end: u32,\n    }\n\n",
            "",
        )
        .replace(
            "let window = Window { start: 3, end: 9 };",
            "let window = Window::new(3, 9);",
        );
    let (index, pin) =
        same_file_clone_field_pin(&unshadowed, "start: self.start,", "start: self.start,");
    assert!(pin.is_some(), "the unshadowed control must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

/// #6948 review: the parsed module item for a macro-generated type is the
/// macro, not the struct it emits, so a test-module `macro_rules!`
/// definition plus invocation emitting `Window` shadows the receiver the
/// same way a direct declaration does. Precision control: a macro emitting
/// an unrelated name does not shadow, so the pin still admits.
#[test]
fn a_macro_generated_test_module_shadow_of_the_receiver_refuses_the_clone_pin() {
    let shadowed = "#[derive(Debug, PartialEq, Eq)]\npub struct Window {\n    start: u32,\n    end: u32,\n}\n\nimpl Window {\n    pub fn new(start: u32, end: u32) -> Self {\n        Window { start, end }\n    }\n}\n\nimpl Clone for Window {\n    fn clone(&self) -> Self {\n        Window {\n            start: self.start,\n            end: self.end,\n        }\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    macro_rules! define_window {\n        () => {\n            #[derive(Debug, Clone, PartialEq, Eq)]\n            struct Window {\n                start: u32,\n                end: u32,\n            }\n        };\n    }\n    define_window!();\n\n    #[test]\n    fn a_clone_equals_its_original() {\n        let window = Window { start: 3, end: 9 };\n        assert_eq!(window.clone(), window);\n    }\n}\n";
    let (index, pin) =
        same_file_clone_field_pin(shadowed, "start: self.start,", "start: self.start,");
    assert!(
        pin.is_some(),
        "the pin establishes from the production type"
    );
    let Some(pin) = pin else { return };
    assert!(
        admitted_texts(&index, &pin).is_empty(),
        "a macro-generated shadow names the test-local type, not the owner"
    );
    let unrelated = shadowed.replace("            struct Window", "            struct Pane");
    let (index, pin) =
        same_file_clone_field_pin(&unrelated, "start: self.start,", "start: self.start,");
    assert!(pin.is_some(), "the unrelated-macro control must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

/// #6905 for rules 1-2 method pins: the receiver type is established by
/// name, so a test-module shadow refuses it the same way. Gate-level
/// control: the shadow is methodless (a same-name method would already
/// compete), which pins the receiver-identity refusal itself.
#[test]
fn a_test_module_shadow_of_the_receiver_refuses_a_method_pin() {
    let shadowed = "pub struct Stack {\n    items: Vec<u32>,\n}\n\nimpl Stack {\n    pub fn depth(&self) -> usize {\n        self.items.len() + 1\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    struct Stack {\n        items: Vec<u32>,\n    }\n\n    #[test]\n    fn depth_counts() {\n        let stack = Stack { items: Vec::new() };\n        assert_eq!(stack.depth(), 1);\n    }\n}\n";
    let outer = "pub struct Stack {\n    items: Vec<u32>,\n}\n\nimpl Stack {\n    pub fn depth(&self) -> usize {\n        self.items.len() + 1\n    }\n}\n\nmod holder {\n    struct Stack {\n        items: Vec<u32>,\n    }\n\n    #[cfg(test)]\n    mod tests {\n        #[test]\n        fn depth_counts() {\n            let stack = Stack { items: Vec::new() };\n            assert_eq!(stack.depth(), 1);\n        }\n    }\n}\n";
    for source in [shadowed, outer] {
        let index = index(&[(LIB, source)]);
        let pin = establish(&index, "depth", "self.items.len() + 1");
        assert!(
            pin.is_some(),
            "the pin establishes from the production type: {source}"
        );
        let Some(pin) = pin else { return };
        assert!(
            admitted_texts(&index, &pin).is_empty(),
            "a shadowed receiver names the test-local type: {source}"
        );
    }
    // A raw-identifier shadow (`r#Stack` denotes `Stack`) refuses the same
    // way, including across files where it is the test file's only
    // declaration of the name.
    let lib = "pub struct Stack {\n    items: Vec<u32>,\n}\n\nimpl Stack {\n    pub fn depth(&self) -> usize {\n        self.items.len() + 1\n    }\n}\n";
    let tests = "use demo::Stack;\n\nmod tests {\n    struct r#Stack {\n        items: Vec<u32>,\n    }\n\n    #[test]\n    fn depth_counts() {\n        let stack = Stack { items: Vec::new() };\n        assert_eq!(stack.depth(), 1);\n    }\n}\n";
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "depth", "self.items.len() + 1");
    assert!(pin.is_some(), "the raw-identifier control must establish");
    let Some(pin) = pin else { return };
    assert!(
        admitted_texts(&index, &pin).is_empty(),
        "a raw-identifier shadow names the test-local type"
    );
}

/// #6905 precision: a same-name type outside the test's own module scope
/// does not shadow the receiver, so the pin still admits.
#[test]
fn a_same_name_type_outside_the_test_module_does_not_shadow() {
    let lib = "pub struct Stack {\n    items: Vec<u32>,\n}\n\nimpl Stack {\n    pub fn depth(&self) -> usize {\n        self.items.len() + 1\n    }\n}\n\npub mod other {\n    pub struct Stack {\n        items: Vec<u32>,\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::Stack;\n\n    #[test]\n    fn depth_counts() {\n        let stack = Stack { items: Vec::new() };\n        assert_eq!(stack.depth(), 1);\n    }\n}\n";
    let index = index(&[(LIB, lib)]);
    let pin = establish(&index, "depth", "self.items.len() + 1");
    assert!(pin.is_some(), "the sibling control must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

/// #6905 precision (review): a sibling module sharing the test's line
/// does not shadow the receiver. Byte-exact ancestry, not line spans,
/// decides, so the valid production-owner assertion keeps its pin.
#[test]
fn a_same_line_sibling_module_does_not_shadow() {
    let lib = "pub struct Stack {\n    items: Vec<u32>,\n}\n\nimpl Stack {\n    pub fn depth(&self) -> usize {\n        self.items.len() + 1\n    }\n}\n\nmod shadow { struct Stack; } #[test] fn depth_counts() { let stack = Stack { items: Vec::new() }; assert_eq!(stack.depth(), 1); }\n";
    let index = index(&[(LIB, lib)]);
    let pin = establish(&index, "depth", "self.items.len() + 1");
    assert!(pin.is_some(), "the same-line control must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

/// #6957: when the production declaration sits in an enclosing non-root
/// module, that module is the owner's own scope, not a test-local shadow,
/// so the nested layout keeps its pin.
#[test]
fn a_nested_production_module_does_not_shadow_its_own_owner_pin() {
    let lib = "mod inner {\n    pub struct Stack {\n        items: Vec<u32>,\n    }\n\n    impl Stack {\n        pub fn depth(&self) -> usize {\n            self.items.len() + 1\n        }\n    }\n\n    #[cfg(test)]\n    mod tests {\n        use super::*;\n\n        #[test]\n        fn depth_counts() {\n            let stack = Stack { items: Vec::new() };\n            assert_eq!(stack.depth(), 1);\n        }\n    }\n}\n";
    let index = index(&[(LIB, lib)]);
    let pin = establish(&index, "depth", "self.items.len() + 1");
    assert!(pin.is_some(), "the nested-owner pin must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

/// #6957 negative control: a same-name declaration in the test's own `mod
/// tests` still refuses even when the nested production declaration is
/// exempt, in plain and `r#` forms.
#[test]
fn a_nested_test_module_shadow_alongside_a_nested_production_declaration_refuses() {
    let shadowed = "mod inner {\n    pub struct Stack {\n        items: Vec<u32>,\n    }\n\n    impl Stack {\n        pub fn depth(&self) -> usize {\n            self.items.len() + 1\n        }\n    }\n\n    #[cfg(test)]\n    mod tests {\n        use super::*;\n\n        struct Stack {\n            items: Vec<u32>,\n        }\n\n        #[test]\n        fn depth_counts() {\n            let stack = Stack { items: Vec::new() };\n            assert_eq!(stack.depth(), 1);\n        }\n    }\n}\n";
    let raw = shadowed.replace("        struct Stack {", "        struct r#Stack {");
    assert!(
        raw.contains("struct r#Stack {"),
        "fixture: the shadow must take the raw-identifier form"
    );
    for source in [shadowed, raw.as_str()] {
        let index = index(&[(LIB, source)]);
        let pin = establish(&index, "depth", "self.items.len() + 1");
        assert!(
            pin.is_some(),
            "the pin establishes from the production type: {source}"
        );
        let Some(pin) = pin else { return };
        assert!(
            admitted_texts(&index, &pin).is_empty(),
            "a nested test-module shadow names the test-local type: {source}"
        );
    }
}

/// #6957: cross-file owners keep the fail-closed shadow check. The
/// exemption resolves only in the test file's own parse, so a same-name
/// declaration in the test file's `mod tests` still refuses a nested
/// owner's pin, in plain and `r#` forms.
#[test]
fn a_cross_file_test_module_shadow_of_a_nested_owner_still_refuses() {
    let lib = "pub mod inner {\n    pub struct Stack {\n        pub items: Vec<u32>,\n    }\n\n    impl Stack {\n        pub fn depth(&self) -> usize {\n            self.items.len() + 1\n        }\n    }\n}\n";
    let tests = "use demo::inner::Stack;\n\nmod tests {\n    struct Stack {\n        items: Vec<u32>,\n    }\n\n    #[test]\n    fn depth_counts() {\n        let stack = Stack { items: Vec::new() };\n        assert_eq!(stack.depth(), 1);\n    }\n}\n";
    let raw = tests.replace("    struct Stack {", "    struct r#Stack {");
    assert!(
        raw.contains("struct r#Stack {"),
        "fixture: the shadow must take the raw-identifier form"
    );
    for tests in [tests, raw.as_str()] {
        let index = index(&[(LIB, lib), (TESTS, tests)]);
        let pin = establish(&index, "depth", "self.items.len() + 1");
        assert!(
            pin.is_some(),
            "the cross-file control must establish: {tests}"
        );
        let Some(pin) = pin else { return };
        assert!(
            admitted_texts(&index, &pin).is_empty(),
            "a cross-file shadow names the test-local type: {tests}"
        );
    }
}

/// #6950: production `Stack`/`depth` in `src/lib.rs` with an out-of-line
/// `#[cfg(test)] mod helpers;` (`mod` token on line 12).
const LIB_OUT_OF_LINE: &str = r#"pub struct Stack {
    items: Vec<u32>,
}

impl Stack {
    pub fn depth(&self) -> usize {
        self.items.len() + 1
    }
}

#[cfg(test)]
mod helpers;
"#;

/// #6950: the `helpers` test module declares its own methodless `Stack`
/// (a same-name method would already compete, as in #6905) above its
/// `mod stack_tests;` (`mod` token on line 5).
const HELPERS_SHADOW: &str = r#"pub struct Stack {
    items: Vec<u32>,
}

mod stack_tests;
"#;

/// #6950 precision: the same `helpers` module without the shadow
/// (`mod` token on line 1).
const HELPERS_PLAIN: &str = "mod stack_tests;\n";

/// #6950 review: the `helpers` module rebinds the receiver with a
/// root-level `use ... as Stack` (`mod` token on line 3).
const HELPERS_RENAME: &str = "use crate::other::Gauge as Stack;\n\nmod stack_tests;\n";

/// #6950 review: the same rebinding spelled with a raw identifier
/// (`r#Stack` denotes `Stack`; `mod` token on line 3).
const HELPERS_RENAME_RAW: &str = "use crate::other::Gauge as r#Stack;\n\nmod stack_tests;\n";

/// #6950 review precision: the `helpers` module plainly imports the
/// receiver, which may re-export production (`mod` token on line 3).
const HELPERS_IMPORT: &str = "use crate::Stack;\n\nmod stack_tests;\n";

/// #6950: the nested child test binds `Stack` (the `helpers` shadow, via
/// `use super::*;`) and asserts `depth`.
const CHILD_TEST: &str = r#"use super::*;

#[test]
fn depth_counts() {
    let stack = Stack { items: Vec::new() };
    assert_eq!(stack.depth(), 1);
}
"#;

/// #6950: the child file declares nothing, so the single-file prefilter
/// is blind — the parent chain (`helpers` declares the receiver) must
/// refuse the pin.
#[test]
fn an_out_of_line_parent_module_shadow_of_the_receiver_refuses_the_pin() {
    let index = index_with_provenance(
        &[
            (LIB, LIB_OUT_OF_LINE),
            (HELPERS, HELPERS_SHADOW),
            (CHILD, CHILD_TEST),
        ],
        &[
            (
                HELPERS,
                SourceRoleProvenance {
                    edges: vec![module_edge(LIB, HELPERS, "helpers", 12, true)],
                    earliest_unresolved_reason: None,
                },
            ),
            (
                CHILD,
                SourceRoleProvenance {
                    edges: vec![
                        module_edge(LIB, HELPERS, "helpers", 12, true),
                        module_edge(HELPERS, CHILD, "stack_tests", 5, false),
                    ],
                    earliest_unresolved_reason: None,
                },
            ),
        ],
    );
    let pin = establish(&index, "depth", "self.items.len() + 1");
    assert!(
        pin.is_some(),
        "the pin establishes from the production type"
    );
    let Some(pin) = pin else { return };
    assert!(
        admitted_texts(&index, &pin).is_empty(),
        "a parent-chain shadow names the test-local type, not the owner"
    );
}

/// #6950 precision: the same nested layout without the parent shadow
/// keeps its pin. The walk skips `helpers` (no declaration) and the
/// production root stays exempt as the owner's own scope.
#[test]
fn an_out_of_line_test_without_a_parent_shadow_keeps_its_pin() {
    let index = index_with_provenance(
        &[
            (LIB, LIB_OUT_OF_LINE),
            (HELPERS, HELPERS_PLAIN),
            (CHILD, CHILD_TEST),
        ],
        &[
            (
                HELPERS,
                SourceRoleProvenance {
                    edges: vec![module_edge(LIB, HELPERS, "helpers", 12, true)],
                    earliest_unresolved_reason: None,
                },
            ),
            (
                CHILD,
                SourceRoleProvenance {
                    edges: vec![
                        module_edge(LIB, HELPERS, "helpers", 12, true),
                        module_edge(HELPERS, CHILD, "stack_tests", 1, false),
                    ],
                    earliest_unresolved_reason: None,
                },
            ),
        ],
    );
    let pin = establish(&index, "depth", "self.items.len() + 1");
    assert!(pin.is_some(), "the unshadowed control must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

/// #6950 review: a root-level `use ... as Stack` in the parent module
/// rebinds the receiver to a different type, so the nested child test
/// names that type, not the production one — the pin is refused. The raw
/// spelling (`as r#Stack`) rebinds the same name.
#[test]
fn a_parent_root_rename_of_the_receiver_refuses_the_pin() {
    for helpers in [HELPERS_RENAME, HELPERS_RENAME_RAW] {
        let index = index_with_provenance(
            &[
                (LIB, LIB_OUT_OF_LINE),
                (HELPERS, helpers),
                (CHILD, CHILD_TEST),
            ],
            &[
                (
                    HELPERS,
                    SourceRoleProvenance {
                        edges: vec![module_edge(LIB, HELPERS, "helpers", 12, true)],
                        earliest_unresolved_reason: None,
                    },
                ),
                (
                    CHILD,
                    SourceRoleProvenance {
                        edges: vec![
                            module_edge(LIB, HELPERS, "helpers", 12, true),
                            module_edge(HELPERS, CHILD, "stack_tests", 3, false),
                        ],
                        earliest_unresolved_reason: None,
                    },
                ),
            ],
        );
        let pin = establish(&index, "depth", "self.items.len() + 1");
        assert!(
            pin.is_some(),
            "the pin establishes from the production type: {helpers}"
        );
        let Some(pin) = pin else { return };
        assert!(
            admitted_texts(&index, &pin).is_empty(),
            "a parent-root rename names a different type than the owner: {helpers}"
        );
    }
}

/// #6950 review precision: a plain root-level `use` of the receiver in
/// the parent module may re-export production, so it is not a shadow —
/// the nested child keeps its pin.
#[test]
fn a_parent_root_plain_import_of_the_receiver_keeps_its_pin() {
    let index = index_with_provenance(
        &[
            (LIB, LIB_OUT_OF_LINE),
            (HELPERS, HELPERS_IMPORT),
            (CHILD, CHILD_TEST),
        ],
        &[
            (
                HELPERS,
                SourceRoleProvenance {
                    edges: vec![module_edge(LIB, HELPERS, "helpers", 12, true)],
                    earliest_unresolved_reason: None,
                },
            ),
            (
                CHILD,
                SourceRoleProvenance {
                    edges: vec![
                        module_edge(LIB, HELPERS, "helpers", 12, true),
                        module_edge(HELPERS, CHILD, "stack_tests", 3, false),
                    ],
                    earliest_unresolved_reason: None,
                },
            ),
        ],
    );
    let pin = establish(&index, "depth", "self.items.len() + 1");
    assert!(pin.is_some(), "the plain-import control must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
}

/// #6950 precision (transposed #6957): the production type and its owner
/// live at the parent file's root, so that root is the owner's own scope,
/// not a shadow — the nested child keeps its pin.
#[test]
fn an_out_of_line_production_module_at_the_parent_root_keeps_its_pin() {
    let lib = "mod helpers;\n";
    let helpers = r#"pub struct Stack {
    items: Vec<u32>,
}

impl Stack {
    pub fn depth(&self) -> usize {
        self.items.len() + 1
    }
}

#[cfg(test)]
mod stack_tests;
"#;
    let index = index_with_provenance(
        &[(LIB, lib), (HELPERS, helpers), (CHILD, CHILD_TEST)],
        &[
            (
                HELPERS,
                SourceRoleProvenance {
                    edges: vec![module_edge(LIB, HELPERS, "helpers", 1, false)],
                    earliest_unresolved_reason: None,
                },
            ),
            (
                CHILD,
                SourceRoleProvenance {
                    edges: vec![
                        module_edge(LIB, HELPERS, "helpers", 1, false),
                        module_edge(HELPERS, CHILD, "stack_tests", 12, true),
                    ],
                    earliest_unresolved_reason: None,
                },
            ),
        ],
    );
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == "depth" && function.file == Path::new(HELPERS));
    assert!(owner.is_some(), "the owner must be indexed from {HELPERS}");
    let Some(owner) = owner else { return };
    let pin =
        OwnerReturnPin::establish(&return_probe(owner, "self.items.len() + 1"), owner, &index);
    assert!(pin.is_some(), "the nested-owner pin must establish");
    let Some(pin) = pin else { return };
    assert_eq!(admitted_texts(&index, &pin).len(), 1);
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

fn binding_refusal(kind: Option<MacroBindingKind>) -> AssertionRefusal {
    AssertionRefusal::MacroBinding {
        name: "assert_eq".to_string(),
        site: kind.map(|kind| {
            (
                PathBuf::from("src/lib.rs"),
                MacroBindingSite {
                    line: 3,
                    kind,
                    scope: None,
                    crate_local: false,
                },
            )
        }),
    }
}

#[test]
fn analyzer_limit_refusals_are_limits_of_ripr_reading() {
    let syntax = AssertionRefusal::Syntax;
    for refusal in [
        AssertionRefusal::LexicalFallback,
        AssertionRefusal::UnresolvedModule("orphan".to_string()),
        AssertionRefusal::IncludedFile {
            parent: PathBuf::from("src/lib.rs"),
            line: 2,
        },
        syntax(AssertionContextRefusal::UnparsedFile),
        syntax(AssertionContextRefusal::UnidentifiedTest),
        syntax(AssertionContextRefusal::AsyncTest),
        syntax(AssertionContextRefusal::DuplicateSpelling),
        syntax(AssertionContextRefusal::StaleSource),
        syntax(AssertionContextRefusal::TestAttribute(
            "#[cfg(any(feature = \"std\", not(no_core_net)))]".to_string(),
        )),
        syntax(AssertionContextRefusal::TestAttribute(
            "#[ignore_slow]".to_string(),
        )),
        binding_refusal(None),
        binding_refusal(Some(MacroBindingKind::ForeignGlob("other::*".to_string()))),
        binding_refusal(Some(MacroBindingKind::MacroUse("mod helpers;".to_string()))),
        binding_refusal(Some(MacroBindingKind::MacroArgument("rgtest".to_string()))),
        binding_refusal(Some(MacroBindingKind::Unparsed)),
        binding_refusal(Some(MacroBindingKind::NoImplicitPrelude)),
    ] {
        assert!(refusal.is_analyzer_limit(), "{}", refusal.describe());
    }
}

#[test]
fn refusals_that_can_keep_an_assertion_from_running_are_not_limits() {
    let syntax = AssertionRefusal::Syntax;
    for refusal in [
        AssertionRefusal::ModuleDeclaration {
            parent: PathBuf::from("tests/weight_tests.rs"),
            line: 3,
            declaration: "mod dormant;".to_string(),
        },
        syntax(AssertionContextRefusal::TestAttribute(
            "#[ignore]".to_string(),
        )),
        syntax(AssertionContextRefusal::TestAttribute(
            "#[ignore = \"slow\"]".to_string(),
        )),
        syntax(AssertionContextRefusal::TestAttribute(
            "#[cfg(any())]".to_string(),
        )),
        syntax(AssertionContextRefusal::TestAttribute(
            "#[cfg(all(test, any()))]".to_string(),
        )),
        syntax(AssertionContextRefusal::TestAttribute(
            "#[should_panic]".to_string(),
        )),
        syntax(AssertionContextRefusal::TestAttribute(
            "#[should_panic(expected = \"boom\")]".to_string(),
        )),
        syntax(AssertionContextRefusal::NestedItem),
        syntax(AssertionContextRefusal::GatedItem(
            "#[cfg(any())]".to_string(),
        )),
        syntax(AssertionContextRefusal::OpaqueMacro("skip".to_string())),
        syntax(AssertionContextRefusal::MacroOperandExit(
            "assert_eq".to_string(),
        )),
        syntax(AssertionContextRefusal::ClosureExit),
        syntax(AssertionContextRefusal::ConditionalPath("if")),
        syntax(AssertionContextRefusal::MacroBinding(
            "assert_eq".to_string(),
        )),
        binding_refusal(Some(MacroBindingKind::Definition)),
        binding_refusal(Some(MacroBindingKind::Import)),
    ] {
        assert!(!refusal.is_analyzer_limit(), "{}", refusal.describe());
    }
}

#[test]
fn an_outcome_settling_attribute_is_the_refusal_wherever_it_sits() {
    // A feature gate first, or an `async` test, must not hide the `#[ignore]`
    // or `#[should_panic]` that settles the outcome (RIPR-SPEC-0240).
    for (tests, expected) in [
        (
            "use demo::weight;\n#[test]\n#[cfg(feature = \"std\")]\n#[ignore]\nfn weighs() { assert_eq!(weight(4), 12); }\n",
            "#[ignore]",
        ),
        (
            "use demo::weight;\n#[ignore]\n#[tokio::test]\nasync fn weighs() { assert_eq!(weight(4), 12); }\n",
            "#[ignore]",
        ),
        (
            "use demo::weight;\n#[test]\n#[cfg(feature = \"std\")]\n#[should_panic]\nfn weighs() { assert_eq!(weight(4), 12); }\n",
            "#[should_panic]",
        ),
    ] {
        let refusal = weight_refusal(tests, &[]);
        assert!(
            matches!(
                &refusal,
                Some(AssertionRefusal::Syntax(AssertionContextRefusal::TestAttribute(attribute)))
                    if attribute == expected
            ),
            "{tests}: {refusal:?}"
        );
        assert!(
            refusal.is_some_and(|refusal| !refusal.is_analyzer_limit()),
            "{tests}"
        );
    }
}

#[test]
fn the_workspace_site_is_the_first_rebinding_in_path_order_else_the_first_site() {
    // The workspace scan runs per file on the rayon pool; the reported site
    // must still be the first rebinding in path order, or failing that the
    // first may-rebind site, whatever order the files finish in. That site
    // decides whether the refusal is an analyzer limit (RIPR-SPEC-0240).
    let plain = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let macro_use = "fn a() {}\n#[macro_use]\nextern crate other;\n";
    let definition = "#[macro_export]\nmacro_rules! assert_eq { ($a:expr, $b:expr) => {}; }\n";
    let site_of = |files: &[(&str, &str)]| {
        let index = index(files);
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(TESTS))
            .cloned();
        assert!(test.is_some(), "the fixture test must be indexed");
        let test = test?;
        let probe = return_probe(owner(&index, "weight"), "x * 3");
        match OwnerPinSyntax::default().equality_assertion_refusal(
            &probe,
            &test,
            &test.assertions[0],
            &index,
        ) {
            Some(AssertionRefusal::MacroBinding {
                site: Some((path, site)),
                ..
            }) => Some((path, site.kind)),
            _ => None,
        }
    };

    let rebinding_later = site_of(&[
        (LIB, WEIGHT_LIB),
        (TESTS, plain),
        ("src/a.rs", macro_use),
        ("src/b.rs", definition),
        ("src/c.rs", macro_use),
    ]);
    assert_eq!(
        rebinding_later,
        Some((PathBuf::from("src/b.rs"), MacroBindingKind::Definition))
    );

    // Within one file, a may-rebind site ahead of a definition still yields
    // the definition.
    let same_file = format!("{macro_use}{definition}");
    let rebinding_in_same_file = site_of(&[
        (LIB, WEIGHT_LIB),
        (TESTS, plain),
        ("src/a.rs", &same_file),
        ("src/c.rs", macro_use),
    ]);
    assert_eq!(
        rebinding_in_same_file,
        Some((PathBuf::from("src/a.rs"), MacroBindingKind::Definition))
    );

    let only_may_rebind = site_of(&[
        (LIB, WEIGHT_LIB),
        (TESTS, plain),
        ("src/c.rs", macro_use),
        ("src/a.rs", macro_use),
    ]);
    assert_eq!(
        only_may_rebind,
        Some((
            PathBuf::from("src/a.rs"),
            MacroBindingKind::MacroUse("extern crate other;".to_string())
        ))
    );
}

#[test]
fn a_definition_covering_the_test_outranks_a_may_rebind_site_elsewhere() {
    // The test file defines its own `assert_eq!`, a real rebinding that can
    // keep the assertion from checking anything. An unresolved `#[macro_use]`
    // elsewhere in the workspace only may rebind the name, so reporting that
    // site would turn a real rebinding into an analyzer limit (RIPR-SPEC-0240).
    let tests = "macro_rules! assert_eq { ($a:expr, $b:expr) => {}; }\nuse demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let macro_use = "fn a() {}\n#[macro_use]\nextern crate other;\n";
    let refusal_with = |files: &[(&str, &str)]| {
        let index = index(files);
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(TESTS))
            .cloned();
        assert!(test.is_some(), "the fixture test must be indexed");
        let test = test?;
        let probe = return_probe(owner(&index, "weight"), "x * 3");
        OwnerPinSyntax::default().equality_assertion_refusal(
            &probe,
            &test,
            &test.assertions[0],
            &index,
        )
    };
    let site_kind = |refusal: &Option<AssertionRefusal>| match refusal {
        Some(AssertionRefusal::MacroBinding {
            site: Some((path, site)),
            ..
        }) => Some((path.clone(), site.kind.clone())),
        _ => None,
    };

    let both = refusal_with(&[
        (LIB, WEIGHT_LIB),
        (TESTS, tests),
        ("src/other.rs", macro_use),
    ]);
    assert_eq!(
        site_kind(&both),
        Some((PathBuf::from(TESTS), MacroBindingKind::Definition)),
        "{both:?}"
    );
    assert!(both.is_some_and(|refusal| !refusal.is_analyzer_limit()));

    // Control: without the local definition the workspace site is reported,
    // and it is an analyzer limit.
    let plain = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let only_macro_use = refusal_with(&[
        (LIB, WEIGHT_LIB),
        (TESTS, plain),
        ("src/other.rs", macro_use),
    ]);
    assert_eq!(
        site_kind(&only_macro_use),
        Some((
            PathBuf::from("src/other.rs"),
            MacroBindingKind::MacroUse("extern crate other;".to_string())
        ))
    );
    assert!(only_macro_use.is_some_and(|refusal| refusal.is_analyzer_limit()));
}

/// #5830: a comment between the last statement and the tail (`// SAFETY:`
/// before an `unsafe` block) is not part of the tail.
#[test]
fn return_path_gate_reads_the_tail_past_a_comment() {
    let body = "fn family(sku: &str) -> &str {\n    let end = sku.find('-').unwrap_or(sku.len());\n    // SAFETY: `end` is a char boundary.\n    unsafe { sku.get_unchecked(..end) }\n}";
    assert!(matches!(
        gate(body, "unsafe { sku.get_unchecked(..end) }"),
        Some(ReturnPathGate::Any)
    ));
    // The comment does not make a different tail match: the changed text
    // sits on an earlier line, and the tail after the comment differs.
    let other_tail = "fn family(sku: &str) -> &str {\n    let head = unsafe { sku.get_unchecked(..1) };\n    // SAFETY: `head` is a char boundary.\n    unsafe { sku.get_unchecked(head.len()..) }\n}";
    assert!(gate(other_tail, "unsafe { sku.get_unchecked(..1) }").is_none());
    // Masking hides string contents, so a same-length payload on the
    // changed line must still match the real tail text (#6970 review).
    let payload = "fn f() -> Result<&'static str, ()> {\n    // note\n    Ok(\"actual\")\n}";
    assert!(matches!(
        return_path_gate(payload, "Ok(\"actual\")", 2),
        Some(ReturnPathGate::Any)
    ));
    assert!(return_path_gate(payload, "Ok(\"expect\")", 2).is_none());
}

#[test]
fn a_shared_owner_pin_syntax_names_each_tests_own_covering_site() {
    // Two inline modules rebind `assert_eq!` at different lines. The memo is
    // keyed by name and file, so one shared OwnerPinSyntax must still pick
    // the site that covers each test, as a fresh one does.
    let tests = "use demo::weight;\n\
mod first {\n    macro_rules! assert_eq { ($a:expr, $b:expr) => {}; }\n    #[test]\n    fn weighs() { assert_eq!(super::weight(4), 12); }\n}\n\
mod second {\n    use super::weight;\n    macro_rules! assert_eq { ($a:expr, $b:expr) => {}; }\n    #[test]\n    fn weighs_again() { assert_eq!(weight(5), 15); }\n}\n";
    let index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    let probe = return_probe(owner(&index, "weight"), "x * 3");
    let shared = OwnerPinSyntax::default();
    let mut lines = Vec::new();
    for at in 0..2 {
        let test = index.tests().at(at);
        let memoized = shared.equality_assertion_refusal(&probe, test, &test.assertions[0], &index);
        let fresh = OwnerPinSyntax::default().equality_assertion_refusal(
            &probe,
            test,
            &test.assertions[0],
            &index,
        );
        assert_eq!(memoized, fresh, "test {at}");
        let line = match &memoized {
            Some(AssertionRefusal::MacroBinding {
                site: Some((_, site)),
                ..
            }) => Some(site.line),
            _ => None,
        };
        assert!(line.is_some(), "test {at}: {memoized:?}");
        lines.extend(line);
    }
    lines.sort_unstable();
    assert_eq!(lines, [3, 9]);
}

/// The stored candidates (#5363) are a skip filter for the trusted-macro
/// scans, so a name they rule out must have no site under any trusted
/// subset or workspace context. The maximal context is no workspace
/// package, no resolved module and no verified drop-in: each widens the
/// sites the scan reports.
fn sites_in_widest_context(source: &str, trusted: &[&str]) -> Vec<(String, MacroBindingSite)> {
    trusted_macro_binding_sites(source, &BTreeSet::new(), trusted, &|_, _| false, &|_| false)
}

fn stored_candidates(source: &str) -> Result<MacroBindingCandidates, String> {
    crate::analysis::syntax::ra::summarize_file_with_parser(Path::new("src/lib.rs"), source)?
        .macro_candidates
        .map(|candidates| *candidates)
        .ok_or_else(|| "premise: parser-backed facts carry macro candidates".to_string())
}

#[test]
fn stored_macro_candidates_record_each_binding_construct() -> Result<(), String> {
    let any = None;
    let names = |names: &[&str]| Some(names.iter().map(|name| name.to_string()).collect());
    for (source, expected) in [
        // A trusted call's own name is not a site.
        ("fn f() { assert_eq!(1, 1); }\n", names(&[])),
        (
            "macro_rules! assert_eq { () => {} }\n",
            names(&["assert_eq"]),
        ),
        ("use pretty_assertions::assert_eq;\n", names(&["assert_eq"])),
        ("use other::thing as panic;\n", names(&["panic"])),
        (
            "fn f() { wrap!(assert_eq!(1, 1)); }\n",
            names(&["assert_eq"]),
        ),
        // Recorded although the full-set scan skips the trusted `assert!`:
        // a scan for `vec` alone reads its arguments.
        ("fn f() { assert!(vec![1] == vec![1]); }\n", names(&["vec"])),
        ("use super::*;\nuse crate::a::{self, b};\n", names(&[])),
        ("macro assert_eq($a:expr) { $a }\n", names(&["assert_eq"])),
        ("macro_rules! r#panic { () => {} }\n", names(&["panic"])),
        ("use a::{b::{assert_eq}};\n", names(&["assert_eq"])),
        ("use a::assert_eq as _;\n", names(&[])),
        ("use {a::*};\n", any.clone()),
        ("use ::foo::*;\n", any.clone()),
        (
            "#[cfg_attr(test, macro_use)]\nextern crate log;\n",
            any.clone(),
        ),
        ("use std::io::prelude::*;\n", any.clone()),
        ("#[macro_use]\nextern crate log;\n", any.clone()),
        ("#![no_implicit_prelude]\n", any.clone()),
        ("wrap! { #[macro_use] mod m; }\n", any.clone()),
    ] {
        let expected = match expected {
            None => MacroBindingCandidates::Any,
            Some(names) => MacroBindingCandidates::Names(names),
        };
        assert_eq!(stored_candidates(source)?, expected, "{source}");
    }
    // The `vec` case is load-bearing: the single-name scan reports it.
    assert!(
        !sites_in_widest_context("fn f() { assert!(vec![1] == vec![1]); }\n", &["vec"]).is_empty()
    );
    assert!(
        sites_in_widest_context(
            "fn f() { assert!(vec![1] == vec![1]); }\n",
            NON_RETURNING_MACROS
        )
        .is_empty()
    );
    Ok(())
}

#[test]
fn stored_macro_candidates_never_hide_a_site_in_this_crate() -> Result<(), String> {
    // An independent corpus: this crate's own sources, which use every
    // construct the scan reads (definitions, drop-in imports, globs,
    // `macro_rules!` in tests, nested macro calls).
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut pending = vec![root];
    let mut checked = 0;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|err| err.to_string())? {
            let path = entry.map_err(|err| err.to_string())?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
            // Large files cost minutes in a debug build and add no construct.
            if source.len() > 24 * 1024 {
                continue;
            }
            // The producer stores exactly this for a parser-clean file.
            let Some(parse) = parse_clean_source_file(&source) else {
                continue;
            };
            let candidates = macro_binding_candidates(&parse.tree());
            let ruled_out: Vec<&str> = NON_RETURNING_MACROS
                .iter()
                .copied()
                .filter(|name| !candidates.may_bind(name))
                .collect();
            if ruled_out.is_empty() {
                continue;
            }
            checked += 1;
            assert_eq!(
                sites_in_widest_context(&source, &ruled_out),
                Vec::new(),
                "{}",
                path.display()
            );
            for name in ["assert_eq", "vec"] {
                if ruled_out.contains(&name) {
                    assert_eq!(
                        sites_in_widest_context(&source, &[name]),
                        Vec::new(),
                        "{} {name}",
                        path.display()
                    );
                }
            }
        }
    }
    // Most files rule some name out; a vacuous pass would check none.
    assert!(checked > 150, "only {checked} files ruled a name out");
    Ok(())
}

/// #6974: the assertions a `weight` pin admits when `lib` (the crate root,
/// holding the owner and a `#[cfg(test)] mod tests`) and `tests` (an
/// integration test) are the workspace.
fn path_admitted(lib: &str, owner_line: &str, tests: Option<&str>) -> Vec<String> {
    let mut files = vec![(LIB, lib)];
    files.extend(tests.map(|tests| (TESTS, tests)));
    let index = index(&files);
    let owner = owner(&index, "weight");
    let pin = OwnerReturnPin::establish(&return_probe(owner, owner_line), owner, &index);
    assert!(pin.is_some(), "the free owner must establish a pin");
    let Some(pin) = pin else {
        return Vec::new();
    };
    let syntax = OwnerPinSyntax::default();
    index
        .tests()
        .iter()
        .flat_map(|test| {
            test.assertions
                .iter()
                .map(move |assertion| (test, assertion))
        })
        .filter(|(test, assertion)| pin.admits(test, assertion, &index, &|_, _| false, &syntax))
        .map(|(_, assertion)| assertion.text.clone())
        .collect()
}

fn unit_tests(prelude: &str, body: &str) -> String {
    format!(
        "{prelude}pub fn weight(x: u32) -> u32 {{\n    x * 3\n}}\n\n#[cfg(test)]\nmod tests {{\n    #[test]\n    fn weighs() {{\n        {body}\n    }}\n}}\n"
    )
}

#[test]
fn a_path_through_the_owners_own_crate_pins_the_owner() {
    for body in [
        "assert_eq!(crate::weight(4), 12);",
        "assert_eq!(super::weight(4), 12);",
        "assert_eq!(self::super::weight(4), 12);",
        "assert_eq!(12, crate::weight(4));",
    ] {
        let lib = unit_tests("", body);
        assert_eq!(path_admitted(&lib, "x * 3", None), [body], "{body}");
    }
    // Through a module the crate declares.
    let lib = "pub mod scale {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn weighs() {\n        assert_eq!(crate::scale::weight(4), 12);\n        assert_eq!(super::scale::weight(4), 12);\n    }\n}\n";
    assert_eq!(path_admitted(lib, "x * 3", None).len(), 2);
}

#[test]
fn a_path_that_may_leave_the_owners_crate_is_not_a_pin() {
    for body in [
        // An associated function, not the free owner.
        "assert_eq!(Weights::weight(4), 12);",
        // Rooted outside the crate.
        "assert_eq!(std::weight(4), 12);",
        "assert_eq!(heavy::weight(4), 12);",
        "assert_eq!(::demo::weight(4), 12);",
        // Something after the call.
        "assert_eq!(crate::weight(4) + 0, 12);",
        // A segment that is not a declared module.
        "assert_eq!(crate::fast::weight(4), 12);",
        // `super` only leads.
        "assert_eq!(crate::super::weight(4), 12);",
        // Generic arguments.
        "assert_eq!(crate::weight::<u32>(4), 12);",
    ] {
        let lib = unit_tests("pub struct Weights;\n\n", body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // A module some import also binds may be that import.
    let lib = unit_tests(
        "use fastweight as fast;\nmod fast {}\n\n",
        "assert_eq!(crate::fast::weight(4), 12);",
    );
    assert!(path_admitted(&lib, "x * 3", None).is_empty());
    // A foreign glob in the owner's crate stays refused by the shared
    // foreign-glob gate.
    for prelude in ["use fastweight::*;\n\n", "#[cfg(test)]\nuse heavy::*;\n\n"] {
        let lib = unit_tests(prelude, "assert_eq!(crate::weight(4), 12);");
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{prelude}");
    }
    // An import of another `weight` in another module cannot capture a
    // path to the owner's own module, so `crate::weight` still pins and
    // `crate::fast::weight` does not.
    let prelude = "mod fast {\n    pub use fastweight::weight;\n}\n\n";
    let body = "assert_eq!(crate::weight(4), 12);";
    assert_eq!(
        path_admitted(&unit_tests(prelude, body), "x * 3", None),
        [body]
    );
    let lib = unit_tests(prelude, "assert_eq!(crate::fast::weight(4), 12);");
    assert!(path_admitted(&lib, "x * 3", None).is_empty());
    // A local glob that stays in the crate does not.
    let lib = unit_tests(
        "",
        "use super::*;\n        assert_eq!(super::weight(4), 12);",
    );
    assert_eq!(path_admitted(&lib, "x * 3", None).len(), 1);
}

#[test]
fn a_crate_relative_path_in_another_target_is_not_the_owners_crate() {
    // `crate::` in an integration test names the test crate.
    let tests = "#[test]\nfn weighs() {\n    assert_eq!(crate::weight(4), 12);\n}\n";
    assert!(path_admitted(WEIGHT_LIB, "x * 3", Some(tests)).is_empty());
}

#[test]
fn the_owners_library_name_roots_a_path_from_another_crate() -> Result<(), String> {
    let lib = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    let manifest = "[package]\nname = \"demo-lib\"\nversion = \"0.1.0\"\n";
    let root = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-path",
        &[("Cargo.toml", manifest)],
    )?;
    let admitted = |lib: &str, tests: &str| {
        let mut index = index(&[(LIB, lib), (TESTS, tests)]);
        index.member_crates = crate::analysis::facts::member_crates::MemberCrates::new(&root);
        let owner = owner(&index, "weight");
        let pin = OwnerReturnPin::establish(&return_probe(owner, "x * 3"), owner, &index);
        let syntax = OwnerPinSyntax::default();
        let test = index.tests().at(0);
        pin.is_some_and(|pin| {
            test.assertions
                .iter()
                .any(|assertion| pin.admits(test, assertion, &index, &|_, _| false, &syntax))
        })
    };
    let call = "#[test]\nfn weighs() {\n    assert_eq!(demo_lib::weight(4), 12);\n}\n";
    let pinned = admitted(lib, call);
    // Another crate name, a rename to the library's name, and a foreign
    // glob re-exported from the library all refuse; a private foreign glob
    // in the library is out of another crate's reach.
    let other = admitted(
        lib,
        "#[test]\nfn weighs() {\n    assert_eq!(demo::weight(4), 12);\n}\n",
    );
    let renamed = admitted(lib, &format!("extern crate heavy as demo_lib;\n{call}"));
    let reexport = admitted(&format!("pub use fastweight::*;\n{lib}"), call);
    let private = admitted(&format!("use fastweight::*;\n{lib}"), call);
    let _ = std::fs::remove_dir_all(&root);
    assert!(pinned, "`demo_lib::weight(..)` must pin the owner");
    assert!(!other);
    assert!(!renamed);
    assert!(!reexport);
    assert!(private);
    Ok(())
}

/// Items the let-bound negatives lean on: a `const` named like the
/// binding, and helpers a statement between the `let` and its assertion
/// may call.
const LET_BOUND_PRELUDE: &str = "const total: u32 = 12;\nfn touch(_: &u32) {}\nfn assert_eqx() {}\nfn touch_count() -> u32 {\n    1\n}\n";

#[test]
fn a_result_bound_once_and_only_asserted_pins_like_the_call() {
    for body in [
        "let total = crate::weight(4);\n        assert_eq!(total, 12);",
        "let total: u32 = super::weight(4);\n        assert_eq!(12, total);",
        "let total = weight(4);\n        assert_eq!(total, 12);\n        assert_eq!(total, 12);",
    ] {
        let lib = unit_tests("", body);
        let lib = if body.contains("= weight(4)") {
            lib.replace("mod tests {\n", "mod tests {\n    use super::weight;\n")
        } else {
            lib
        };
        // Only the assertion right after the `let` pins.
        let first = body
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("assert_eq!"));
        assert_eq!(
            path_admitted(&lib, "x * 3", None),
            first.into_iter().collect::<Vec<_>>(),
            "{body}"
        );
    }
    for body in [
        // Mutable, borrowed, used elsewhere, or used before the `let`.
        "let mut total = crate::weight(4);\n        total += 0;\n        assert_eq!(total, 12);",
        "let mut total = crate::weight(4);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        touch(&total);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        assert_eq!(total.min(99), 12);",
        "let total = crate::weight(4);\n        assert_eq!(total, total);",
        "assert_eq!(total, 12);\n        let total = crate::weight(4);",
        // Bound twice, destructured, or by something other than the call.
        "let total = crate::weight(4);\n        let total = total + 0;\n        assert_eq!(total, 12);",
        "let (total, _) = (crate::weight(4), 0);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4) + 0;\n        assert_eq!(total, 12);",
        "let total = std::weight(4);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        let check = |total: u32| total;\n        assert_eq!(total, 12);",
        // Another statement sharing the assertion's line comes first.
        "let total = crate::weight(4);\n        assert_eqx(); assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        assert_eq!(touch_count(), 1); assert_eq!(total, 12);",
        // Two assertions of the binding on one line: the use count and the
        // only-statement-on-its-line rule each refuse it.
        "let total = crate::weight(4);\n        assert_eq!(total, 12); touch_count(); assert_eq!(total, 12);",
    ] {
        let lib = unit_tests(LET_BOUND_PRELUDE, body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // Control: the same prelude leaves the plain form pinned, so the cases
    // above are refused by their own guards.
    let body = "let total = crate::weight(4);\n        assert_eq!(total, 12);";
    assert_eq!(
        path_admitted(&unit_tests(LET_BOUND_PRELUDE, body), "x * 3", None),
        ["assert_eq!(total, 12);"]
    );
}

#[test]
fn an_unreadable_expected_binding_is_scanned_only_beside_another_owner_call() {
    // #7061 review: the self-comparison binding scan fails closed on a
    // binding it cannot read, but only a test that names the owner outside
    // the assertion can bind an owner call, so an unrelated destructured
    // expected value keeps the bare-call pin.
    let pinned = "let (want, _) = (12, 0);\n        assert_eq!(weight(4), want);";
    let lib =
        unit_tests("", pinned).replace("mod tests {\n", "mod tests {\n    use super::weight;\n");
    assert_eq!(
        path_admitted(&lib, "x * 3", None),
        ["assert_eq!(weight(4), want);"],
        "{pinned}"
    );
    // A second owner call can fill the same pattern, so the scan refuses.
    let refused = "let (want, _) = (weight(4), 0);\n        assert_eq!(weight(4), want);";
    let lib =
        unit_tests("", refused).replace("mod tests {\n", "mod tests {\n    use super::weight;\n");
    assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{refused}");
}

#[test]
fn a_cfg_gated_owner_is_not_reached_by_a_path() {
    // #7061 review: a complementary cfg may compile a same-named `static`,
    // `const`, `use` or module where the owner was, so "the path names the
    // owner's module" no longer means it reaches the owner.
    let twin = "pub fn helper(_: u32) -> u32 {\n    12\n}\n#[cfg(test)]\n#[allow(non_upper_case_globals)]\npub static weight: fn(u32) -> u32 = helper;\n\n";
    for body in [
        "assert_eq!(crate::weight(4), 12);",
        "assert_eq!(super::weight(4), 12);",
        "let total = super::weight(4);\n        assert_eq!(total, 12);",
    ] {
        let lib =
            unit_tests(twin, body).replace("pub fn weight", "#[cfg(not(test))]\npub fn weight");
        assert!(lib.contains("#[cfg(not(test))]\npub fn weight"), "{lib}");
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // An inner `#![cfg]` in the owner's body gates the whole fn, so a
    // `#[cfg(test)]` re-export may take its name.
    let body_gated = "mod h {\n    pub fn helper(_: u32) -> u32 {\n        12\n    }\n    #[allow(non_upper_case_globals)]\n    pub static wt: fn(u32) -> u32 = helper;\n}\n#[cfg(test)]\npub use h::wt as weight;\npub fn weight(x: u32) -> u32 { #![cfg(not(test))] let y = x;\n    y * 3\n}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn weighs() {\n        assert_eq!(crate::weight(4), 12);\n    }\n}\n";
    assert!(path_admitted(body_gated, "y * 3", None).is_empty());
    // Control: without the cfg (and so without room for the twin) it pins.
    let ungated = body_gated
        .replace("#[cfg(test)]\npub use h::wt as weight;\n", "")
        .replace("#![cfg(not(test))] ", "");
    assert_eq!(
        path_admitted(&ungated, "y * 3", None),
        ["assert_eq!(crate::weight(4), 12);"]
    );
    // An enclosing inline module's cfg gates it too.
    let lib = unit_tests("", "assert_eq!(crate::b::weight(4), 12);").replace(
        "pub fn weight(x: u32) -> u32 {\n    x * 3\n}",
        "#[cfg(not(test))]\npub mod b {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n}",
    );
    assert!(lib.contains("pub mod b"), "{lib}");
    assert!(path_admitted(&lib, "x * 3", None).is_empty());
    // Control: the same module without the cfg pins.
    let lib = lib.replace("#[cfg(not(test))]\npub mod b", "pub mod b");
    assert_eq!(
        path_admitted(&lib, "x * 3", None),
        ["assert_eq!(crate::b::weight(4), 12);"]
    );
}

#[test]
fn review_holes_in_path_and_let_bound_pins_stay_closed() {
    // A lower-case type alias reached as a "module" segment.
    let lib = unit_tests(
        "pub struct Foo;\n#[allow(non_camel_case_types)]\npub type m = Foo;\nmod other {\n    pub mod m {}\n}\n\n",
        "assert_eq!(crate::m::weight(4), 12);",
    );
    assert!(path_admitted(&lib, "x * 3", None).is_empty());
    // A macro invoked with the owner's name may emit a twin under a cfg
    // that drops the owner; the path resolves to the owner's module, so
    // only the crate's macro-invocation guard refuses it.
    let generator = "macro_rules! g {\n    ($n:ident) => {\n        pub fn $n(_: u32) -> u32 { 3 }\n    };\n}\n";
    let lib = unit_tests(
        &format!("{generator}#[cfg(feature = \"alt\")]\ng!(weight);\n\n"),
        "assert_eq!(crate::weight(4), 12);",
    );
    assert!(path_admitted(&lib, "x * 3", None).is_empty());
    // Control: the same macro, never invoked with the name, still pins.
    let lib = unit_tests(
        &format!("{generator}\n"),
        "assert_eq!(crate::weight(4), 12);",
    );
    assert_eq!(path_admitted(&lib, "x * 3", None).len(), 1);
    // A statement between the `let` and its assertion, and a second
    // binding of the owner on the expected side.
    for body in [
        "let total = crate::weight(4);\n        tick();\n        assert_eq!(total, 12);",
        // The expected side binds the owner again, directly or through a
        // second `let` (the assertion is right after the pinned `let`, so
        // only the self-comparison guard can refuse these).
        "let same = crate::weight(4) * 1;\n        let total = crate::weight(4);\n        assert_eq!(total, same);",
        "let same = crate::weight(4) * 1;\n        assert_eq!(crate::weight(4), same);",
        "let a = crate::weight(4);\n        let b = a;\n        assert_eq!(crate::weight(4), b);",
    ] {
        let lib = unit_tests("fn tick() {}\n", body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
}

#[test]
fn a_test_crate_binding_of_the_library_name_shadows_the_dependency() -> Result<(), String> {
    let lib = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    let manifest = "[package]\nname = \"demo-lib\"\nversion = \"0.1.0\"\n";
    let root = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-shadow",
        &[("Cargo.toml", manifest), ("src/lib.rs", "")],
    )?;
    let admitted = |tests: &str| {
        let mut index = index(&[(LIB, lib), (TESTS, tests)]);
        index.member_crates = crate::analysis::facts::member_crates::MemberCrates::new(&root);
        let owner = owner(&index, "weight");
        let pin = OwnerReturnPin::establish(&return_probe(owner, "x * 3"), owner, &index);
        let syntax = OwnerPinSyntax::default();
        let test = index.tests().at(0);
        pin.is_some_and(|pin| {
            test.assertions
                .iter()
                .any(|assertion| pin.admits(test, assertion, &index, &|_, _| false, &syntax))
        })
    };
    let call = "#[test]\nfn weighs() {\n    assert_eq!(demo_lib::weight(4), 12);\n}\n";
    let plain = admitted(call);
    let own_glob = admitted(&format!("use demo_lib::*;\n{call}"));
    let imported = admitted(&format!("use other_dep::demo_lib;\n{call}"));
    let module = admitted(&format!(
        "mod demo_lib {{\n    pub use other_dep::*;\n}}\n{call}"
    ));
    let glob = admitted(&format!("use other_dep::*;\n{call}"));
    let _ = std::fs::remove_dir_all(&root);
    assert!(plain);
    // Even a glob from the library itself may bring a module of its own
    // name; the bare call that glob enables is the pinned form.
    assert!(!own_glob);
    assert!(!imported);
    assert!(!module);
    assert!(!glob);
    Ok(())
}

#[test]
fn a_rename_to_the_owners_name_or_a_type_of_it_defeats_every_path() {
    for (prelude, body) in [
        // A local rename the path then names.
        (
            "fn helper(_: u32) -> u32 {\n    12\n}\n\n",
            "assert_eq!(self::weight(4), 12);",
        ),
        (
            "fn helper(_: u32) -> u32 {\n    12\n}\npub mod m {\n    pub use crate::helper as weight;\n}\n\n",
            "assert_eq!(crate::m::weight(4), 12);",
        ),
        // The path that reaches a lower-case tuple struct named like the
        // owner.
        (
            "pub mod m {\n    #[allow(non_camel_case_types)]\n    pub struct weight(pub u32);\n}\n\n",
            "assert_eq!(crate::m::weight(4), 12);",
        ),
    ] {
        let lib = unit_tests(prelude, body);
        let lib = if body.contains("self::weight") {
            lib.replace(
                "mod tests {\n",
                "mod tests {\n    use crate::helper as weight;\n",
            )
        } else {
            lib
        };
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // `crate::weight` beside that tuple struct names the owner and pins.
    let body = "assert_eq!(crate::weight(4), 12);";
    let lib = unit_tests(
        "pub mod m {\n    #[allow(non_camel_case_types)]\n    pub struct weight(pub u32);\n}\n\n",
        body,
    );
    assert_eq!(path_admitted(&lib, "x * 3", None), [body]);
    // Control: the same crate without the rename pins.
    let lib = unit_tests(
        "fn helper(_: u32) -> u32 {\n    12\n}\npub mod m {}\n\n",
        "assert_eq!(crate::weight(4), 12);",
    );
    assert_eq!(path_admitted(&lib, "x * 3", None).len(), 1);
}

#[test]
fn a_macro_that_may_emit_the_owners_name_defeats_every_path() {
    let helper = "pub fn helper(_: u32) -> u32 {\n    12\n}\n";
    let alias =
        "macro_rules! alias {\n    ($n:ident) => {\n        use crate::helper as $n;\n    };\n}\n";
    for (prelude, body, tests_prelude) in [
        // A rename emitted into the test module.
        (
            format!("{helper}{alias}\n"),
            "assert_eq!(self::weight(4), 12);",
            "    alias!(weight);\n",
        ),
        // A rename emitted into a module the path then names.
        (
            format!("{helper}{alias}pub mod m {{\n    alias!(weight);\n}}\n\n"),
            "assert_eq!(crate::m::weight(4), 12);",
            "",
        ),
        // Items of the owner's name emitted from a fragment.
        (
            format!("{helper}macro_rules! s {{\n    ($n:ident) => {{\n        pub static $n: fn(u32) -> u32 = crate::helper;\n    }};\n}}\npub mod m {{\n    s!(weight);\n}}\n\n"),
            "assert_eq!(crate::m::weight(4), 12);",
            "",
        ),
        (
            "macro_rules! t {\n    ($n:ident) => {\n        #[allow(non_camel_case_types)]\n        pub struct $n(pub u32);\n    };\n}\npub mod m {\n    t!(weight);\n}\n\n".to_string(),
            "assert_eq!(crate::m::weight(4), 12);",
            "",
        ),
        // A macro from elsewhere called with the owner's name.
        (
            "pub mod m {\n    other::alias!(weight);\n}\n\n".to_string(),
            "assert_eq!(crate::m::weight(4), 12);",
            "",
        ),
        // A macro in the test body that names the owner.
        (
            String::new(),
            "alias!(weight);\n        assert_eq!(crate::weight(4), 12);",
            "",
        ),
    ] {
        let lib = unit_tests(&prelude, body)
            .replace("mod tests {\n", &format!("mod tests {{\n{tests_prelude}"));
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}\n{lib}");
    }
    // Control: a macro that emits nothing of the owner's name still pins.
    let lib = unit_tests(
        "macro_rules! twice {\n    ($e:expr) => {\n        $e + $e\n    };\n}\n\n",
        "assert_eq!(crate::weight(4), 12);",
    );
    assert_eq!(path_admitted(&lib, "x * 3", None).len(), 1);
}

#[test]
fn an_expected_binding_ripr_cannot_read_is_not_a_distinct_value() {
    // A destructured `let` may hold another owner call; the self-comparison
    // guard fails closed rather than reading it as no binding.
    let lib = unit_tests(
        "",
        "let (same, _) = (crate::weight(4), 0);\n        assert_eq!(crate::weight(4), same);",
    );
    assert!(path_admitted(&lib, "x * 3", None).is_empty());
    // Control: a plain expected binding with no owner call still pins.
    let body = "let want = 12;\n        assert_eq!(crate::weight(4), want);";
    let lib = unit_tests("", body);
    assert_eq!(
        path_admitted(&lib, "x * 3", None),
        ["assert_eq!(crate::weight(4), want);"]
    );
}

#[test]
fn a_bound_method_result_pins_like_the_method_call() {
    let admitted = |body: &str| {
        let tests = format!(
            "use demo::Counter;\n\n#[test]\nfn counts() {{\n    let c = Counter::new();\n    {body}\n}}\n"
        );
        let index = index(&[(LIB, COUNTER_LIB), (TESTS, &tests)]);
        let pin = establish(&index, "tally", "self.n + 1");
        assert!(pin.is_some(), "tally must establish a pin");
        pin.map(|pin| admitted_texts(&index, &pin))
            .unwrap_or_default()
    };
    assert_eq!(
        admitted("let n = c.tally();\n    assert_eq!(n, 1);"),
        ["assert_eq!(n, 1);"]
    );
    for body in [
        "let mut n = c.tally();\n    n += 0;\n    assert_eq!(n, 1);",
        "let n = c.tally();\n    drop(c);\n    assert_eq!(n, 1);",
        "let n = c.tally() + 0;\n    assert_eq!(n, 1);",
    ] {
        assert!(admitted(body).is_empty(), "{body}");
    }
}

#[test]
fn a_path_must_resolve_to_the_owners_own_module() {
    // The owner in a nested module, reached from its own test module and
    // from the crate root.
    let lib = "pub mod m {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n\n    #[cfg(test)]\n    mod tests {\n        #[test]\n        fn weighs() {\n            assert_eq!(super::weight(4), 12);\n            assert_eq!(crate::m::weight(4), 12);\n            assert_eq!(self::super::super::m::weight(4), 12);\n            assert_eq!(crate::weight(4), 12);\n            assert_eq!(self::weight(4), 12);\n            assert_eq!(super::super::weight(4), 12);\n        }\n    }\n}\n";
    assert_eq!(
        path_admitted(lib, "x * 3", None),
        [
            "assert_eq!(super::weight(4), 12);",
            "assert_eq!(crate::m::weight(4), 12);",
            "assert_eq!(self::super::super::m::weight(4), 12);",
        ]
    );
    // `super` may not climb out of the owner's file.
    let lib = unit_tests("", "assert_eq!(super::super::weight(4), 12);");
    assert!(path_admitted(&lib, "x * 3", None).is_empty());
}

#[test]
fn rev3_review_false_pins_stay_closed() {
    for (prelude, body) in [
        // A: self-comparison through a type-cased binding, directly and
        // through the let-bound arm.
        (
            "",
            "#[allow(non_snake_case)]\n        let W = crate::weight(4) * 1;\n        assert_eq!(crate::weight(4), W);",
        ),
        (
            "",
            "#[allow(non_snake_case)]\n        let W = crate::weight(4) * 1;\n        let total = crate::weight(4);\n        assert_eq!(total, W);",
        ),
        // B1: raw identifiers for a function or a rename of the name.
        (
            "pub mod m {\n    pub fn r#weight(_: u32) -> u32 { 12 }\n}\n\n",
            "assert_eq!(crate::m::weight(4), 12);",
        ),
        (
            "fn helper(_: u32) -> u32 { 12 }\npub mod m {\n    pub use crate::helper as r#weight;\n}\n\n",
            "assert_eq!(crate::m::weight(4), 12);",
        ),
        // B2: an enum variant re-exported through a module-named segment.
        (
            "pub mod e {}\npub mod x {\n    #[derive(Debug)]\n    #[allow(non_camel_case_types)]\n    pub enum e { weight(u32) }\n    impl PartialEq<u32> for e { fn eq(&self, _: &u32) -> bool { true } }\n    pub use self::e::weight;\n}\n\n",
            "assert_eq!(crate::x::weight(4), 12);",
        ),
        // A raw twin in the owner's own module under another cfg: the path
        // resolves to the owner's module, so only the raw guard refuses.
        (
            "#[cfg(feature = \"alt\")]\npub fn r#weight(_: u32) -> u32 { 12 }\n\n",
            "assert_eq!(crate::weight(4), 12);",
        ),
    ] {
        let lib = unit_tests(prelude, body);
        assert!(
            path_admitted(&lib, "x * 3", None).is_empty(),
            "{body}\n{lib}"
        );
    }
    // Control: a type-cased word only in a `let` annotation is not a
    // binding of the owner.
    let body = "let want: Wanted = 12;\n        assert_eq!(crate::weight(4), want);";
    let lib = unit_tests("type Wanted = u32;\n", body);
    assert_eq!(
        path_admitted(&lib, "x * 3", None),
        ["assert_eq!(crate::weight(4), want);"]
    );
}

#[test]
fn an_integration_path_is_closed_to_raw_and_macro_shadows() -> Result<(), String> {
    let lib = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    let manifest = "[package]\nname = \"demo-lib\"\nversion = \"0.1.0\"\n";
    let root = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-raw-root",
        &[("Cargo.toml", manifest), ("src/lib.rs", "")],
    )?;
    let admitted = |tests: &str| {
        let mut index = index(&[(LIB, lib), (TESTS, tests)]);
        index.member_crates = crate::analysis::facts::member_crates::MemberCrates::new(&root);
        let owner = owner(&index, "weight");
        let pin = OwnerReturnPin::establish(&return_probe(owner, "x * 3"), owner, &index);
        let syntax = OwnerPinSyntax::default();
        let test = index.tests().at(0);
        pin.map(|pin| {
            test.assertions
                .iter()
                .filter(|assertion| pin.admits(test, assertion, &index, &|_, _| false, &syntax))
                .map(|assertion| assertion.text.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
    };
    let let_bound = admitted(
        "#[test]\nfn weighs() {\n    let total = demo_lib::weight(4);\n    assert_eq!(total, 12);\n}\n",
    );
    // B3: the test crate shadows the library name with a raw module.
    let raw_module = admitted(
        "mod r#demo_lib {\n    pub fn r#weight(_: u32) -> u32 { 12 }\n}\n#[test]\nfn weighs() {\n    assert_eq!(demo_lib::weight(4), 12);\n}\n",
    );
    // A macro in the test body that names the owner cannot capture a path
    // (its items are block-local), and the shared macro-binding gate
    // refuses the assertion anyway.
    let body_macro = admitted(
        "#[test]\nfn weighs() {\n    shadow!(weight);\n    assert_eq!(demo_lib::weight(4), 12);\n}\n",
    );
    // A type, trait or alias of the library's name shadows the dependency.
    let shadows = [
        "#[allow(non_camel_case_types)]\nstruct demo_lib;\nimpl demo_lib {\n    fn weight(_: u32) -> u32 { 12 }\n}\n",
        "#[allow(non_camel_case_types)]\n#[derive(Debug)]\nenum demo_lib { weight(u32) }\nimpl PartialEq<u32> for demo_lib {\n    fn eq(&self, _: &u32) -> bool { true }\n}\n",
        "#[allow(non_camel_case_types)]\ntrait demo_lib {\n    fn weight(_: u32) -> u32 { 12 }\n}\n",
        "#[allow(non_camel_case_types)]\ntype demo_lib = Fake;\nstruct Fake;\nimpl Fake {\n    fn weight(_: u32) -> u32 { 12 }\n}\n",
    ]
    .map(|prelude| {
        admitted(&format!(
            "{prelude}#[test]\nfn weighs() {{\n    assert_eq!(demo_lib::weight(4), 12);\n}}\n"
        ))
    });
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(let_bound, ["assert_eq!(total, 12);"]);
    assert!(raw_module.is_empty());
    assert!(body_macro.is_empty());
    for shadow in shadows {
        assert!(shadow.is_empty(), "{shadow:?}");
    }
    Ok(())
}

#[test]
fn test_crate_bindings_of_the_root_are_read_from_that_crate_only() {
    let tests_root = Path::new(TESTS);
    for (source, binds) in [
        ("#[test]\nfn t() {}\n", false),
        ("use other_dep::demo_lib;\n", true),
        ("mod demo_lib {}\n", true),
        ("use other_dep::*;\n", true),
        ("use demo_lib::*;\n", true),
        ("use demo_lib::weight;\n", false),
    ] {
        let index = index(&[(LIB, WEIGHT_LIB), (TESTS, source)]);
        let roots = TargetRoots::new(&index);
        assert_eq!(roots.root(tests_root, &index).as_deref(), Some(tests_root));
        assert_eq!(
            test_crate_may_bind(tests_root, "demo_lib", &index, &roots),
            binds,
            "{source}"
        );
    }
    // Only the test crate's own files count. A `tests/` child module file
    // routes to no root, and its binding sits inside `mod helper`, so it can
    // reach the root only through a `use` or glob in the root, which the
    // root scan refuses. The same binding in the library's crate never
    // counts.
    let helper = "tests/buf_tests/helper.rs";
    let lib_binding = "mod demo_lib {}\npub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    for (files, binds) in [
        (
            vec![
                (LIB, WEIGHT_LIB),
                (TESTS, "mod helper;\n"),
                (helper, "use other_dep::demo_lib;\n"),
            ],
            false,
        ),
        (
            vec![
                (LIB, WEIGHT_LIB),
                (TESTS, "mod helper;\nuse helper::*;\n"),
                (helper, "pub use other_dep::demo_lib;\n"),
            ],
            true,
        ),
        (
            vec![(LIB, lib_binding), (TESTS, "#[test]\nfn t() {}\n")],
            false,
        ),
    ] {
        let index = index(&files);
        let roots = TargetRoots::new(&index);
        assert_eq!(roots.root(Path::new(helper), &index), None);
        assert_eq!(
            test_crate_may_bind(tests_root, "demo_lib", &index, &roots),
            binds,
            "{files:?}"
        );
    }
}

/// One `#[test]` over `weight`, with `body` as its statements.
fn weight_test(signature: &str, body: &str) -> String {
    format!("use demo::weight;\n\n#[test]\nfn weighs(){signature} {{\n{body}\n}}\n")
}

/// Per assertion of the one `weight` test, whether the pin admits it,
/// after asserting that an oracle naming `subject` was extracted, so a
/// refusal is the pin's and not a missing fact's.
fn weight_verdicts(tests: &str, subject: &str) -> Vec<(String, bool)> {
    let index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(pin.is_some(), "the free owner must establish a pin");
    let Some(pin) = pin else {
        return Vec::new();
    };
    let verdicts = admitted(&index, &pin);
    assert!(
        verdicts.iter().any(|(text, _)| text.contains(subject)),
        "no oracle names {subject}: {verdicts:?}"
    );
    verdicts
}

fn assert_not_pinned(tests: &str, subject: &str) {
    let verdicts = weight_verdicts(tests, subject);
    assert!(
        verdicts.iter().all(|(_, admitted)| !admitted),
        "{tests}: {verdicts:?}"
    );
}

#[test]
fn a_single_top_level_equality_in_assert_pins_like_assert_eq() {
    // RIPR-SPEC-0197: `assert!(owner(..) == v)` fails exactly when
    // `assert_eq!(owner(..), v)` does, on either side, message or not.
    for assertion in [
        "assert!(weight(4) == 12);",
        "assert!(12 == weight(4));",
        "assert!(weight(4) == 12, \"weight was {}\", 3);",
    ] {
        let tests = weight_test("", &format!("    {assertion}"));
        assert_eq!(
            weight_verdicts(&tests, "weight(4)"),
            vec![(assertion.to_string(), true)],
            "{assertion}"
        );
    }
}

#[test]
fn an_err_return_guard_on_inequality_pins_its_assertion_twin() {
    // RIPR-SPEC-0154 twin: `if owner(..) != v { return Err(..) }` is
    // `assert!(owner(..) == v)` in a test returning `Result`.
    for (body, subject) in [
        (
            "    if weight(4) != 12 {\n        return Err(format!(\"weight was {}\", weight(4)));\n    }\n    Ok(())",
            "if weight(4) != 12",
        ),
        (
            "    if 12 != weight(4) {\n        return Err(\"mismatch\".to_string());\n    }\n    Ok(())",
            "if 12 != weight(4)",
        ),
    ] {
        let verdicts = weight_verdicts(&weight_test(" -> Result<(), String>", body), subject);
        assert_eq!(verdicts.len(), 1, "{body}: {verdicts:?}");
        assert!(
            verdicts
                .iter()
                .all(|(text, admitted)| *admitted && text.starts_with(subject)),
            "{body}: {verdicts:?}"
        );
    }
}

#[test]
fn only_a_lone_top_level_equality_reads_as_a_pin() {
    let refused = [
        // The twin of an `==` guard is `assert!(weight(4) != 12)`.
        (
            "    if weight(4) == 12 {\n        return Err(\"same\".to_string());\n    }\n    Ok(())",
            "if weight(4) == 12",
        ),
        // `!a == b` is `(!a) == b`, never `!(a == b)`.
        (
            "    if !weight(4) == 12 {\n        return Err(\"same\".to_string());\n    }\n    Ok(())",
            "if !weight(4) == 12",
        ),
        (
            "    assert!(weight(4) >= 12);\n    Ok(())",
            "weight(4) >= 12",
        ),
        (
            "    assert!(weight(4) <= 12);\n    Ok(())",
            "weight(4) <= 12",
        ),
        ("    assert!(weight(4) > 11);\n    Ok(())", "weight(4) > 11"),
        ("    assert!(weight(4) < 13);\n    Ok(())", "weight(4) < 13"),
        (
            "    assert!(weight(4) != 13);\n    Ok(())",
            "weight(4) != 13",
        ),
        (
            "    assert!(weight(4) == 12 && weight(1) == 3);\n    Ok(())",
            "&& weight(1)",
        ),
        (
            "    assert!(weight(4) == 12 || weight(1) == 4);\n    Ok(())",
            "|| weight(1)",
        ),
        // One `==` beside a top-level `&&`/`||` is not a lone equality.
        (
            "    let flag = true;\n    assert!(weight(4) == 12 && flag);\n    Ok(())",
            "&& flag",
        ),
        (
            "    let flag = true;\n    assert!(weight(4) == 13 || flag);\n    Ok(())",
            "|| flag",
        ),
        (
            "    assert!(!(weight(4) == 13));\n    Ok(())",
            "!(weight(4) == 13)",
        ),
        (
            "    assert!((weight(4) == 12) == true);\n    Ok(())",
            "(weight(4) == 12) == true",
        ),
        (
            "    assert!([weight(4) == 12][0]);\n    Ok(())",
            "[weight(4) == 12]",
        ),
        (
            "    assert!((|| weight(4))() == 12);\n    Ok(())",
            "(|| weight(4))",
        ),
    ];
    for (body, subject) in refused {
        assert_not_pinned(&weight_test(" -> Result<(), String>", body), subject);
    }
}

#[test]
fn an_equality_whose_expected_side_names_the_owner_is_not_a_pin() {
    for (body, subject) in [
        (
            "    assert!(weight(4) == weight(2) + weight(2));\n    Ok(())",
            "weight(2) + weight(2)",
        ),
        (
            "    assert!(2 * weight(2) == weight(4));\n    Ok(())",
            "2 * weight(2)",
        ),
        (
            "    if weight(4) != weight(2) + weight(2) {\n        return Err(\"mismatch\".to_string());\n    }\n    Ok(())",
            "if weight(4) != weight(2)",
        ),
    ] {
        assert_not_pinned(&weight_test(" -> Result<(), String>", body), subject);
    }
}

#[test]
fn equality_condition_operands_split_one_top_level_equality() {
    assert_eq!(
        equality_condition_operands("assert!(f(a == b, [1 == 2]) == \"x == y\", \"m == n\");"),
        Some(["f(a == b, [1 == 2])".to_string(), "\"x == y\"".to_string()])
    );
    assert_eq!(
        equality_condition_operands("if f(1) != 3 { return Err(..) }"),
        Some(["f(1)".to_string(), "3".to_string()])
    );
    for refused in [
        "assert!(f(1) == 3 && g());",
        "assert!(f(1) == 3 || g());",
        "assert!(f(1) == 3 && flag);",
        "assert!(f(1) == 3 || flag);",
        "assert!(flag || f(1) == 3);",
        "assert!(f(1) == 3 as u8 > 2);",
        "assert!(f(1) != 3);",
        "assert!(f(1) < 3);",
        "assert!(f(1) > 3);",
        "assert!(f(1) <= 3);",
        "assert!(f(1) >= 3);",
        "assert!(f::<u8>() == 3);",
        "assert!(!(f(1) == 3));",
        "assert!(f(1) == 3 == true);",
        "assert!(x = f(1) == 3);",
        "assert!(f(1));",
        "if f(1) == 3 { return Err(..) }",
        "if !(f(1) == 3) { return Err(..) }",
        "if !f(1) == 3 { return Err(..) }",
        "if f(1) != 3 && g() { return Err(..) }",
        "if f(1) != 3 { return Ok(()) }",
        "if f(1) != 3 { log(); return Err(..) }",
        "debug_assert!(f(1) == 3);",
        "assert_eq!(f(1), 3);",
    ] {
        assert_eq!(equality_condition_operands(refused), None, "{refused}");
    }
}

#[test]
fn an_err_return_guard_pins_only_on_an_established_execution_path() {
    for body in [
        // A loop body may never run.
        "    for _ in 0..0 {\n        if weight(4) != 12 {\n            return Err(\"mismatch\".to_string());\n        }\n    }\n    Ok(())",
        // An earlier `return` may leave the test before the guard runs.
        "    if weight(1) != 3 {\n        return Ok(());\n    }\n    if weight(4) != 12 {\n        return Err(\"mismatch\".to_string());\n    }\n    Ok(())",
        // A closure nobody calls never runs it.
        "    let check = || -> Result<(), String> {\n        if weight(4) != 12 {\n            return Err(\"mismatch\".to_string());\n        }\n        Ok(())\n    };\n    let _ = &check;\n    Ok(())",
        // A closure that runs but whose `Err` is discarded: the guard's
        // `return` leaves only the closure, so the test passes regardless.
        // The syntax gate's closure-exit refusal covers these (#7094
        // follow-up review).
        "    let _ = (|| -> Result<(), String> {\n        if weight(4) != 12 {\n            return Err(\"mismatch\".to_string());\n        }\n        Ok(())\n    })();\n    Ok(())",
        "    std::thread::scope(|_| -> Result<(), String> {\n        if weight(4) != 12 {\n            return Err(\"mismatch\".to_string());\n        }\n        Ok(())\n    });\n    Ok(())",
        "    let _ = std::thread::spawn(|| -> Result<(), String> {\n        if weight(4) != 12 {\n            return Err(\"mismatch\".to_string());\n        }\n        Ok(())\n    })\n    .join()\n    .unwrap();\n    Ok(())",
    ] {
        assert_not_pinned(
            &weight_test(" -> Result<(), String>", body),
            "if weight(4) != 12",
        );
    }
    // A guard reads only its condition: an `assert_eq!` inside its body runs
    // only when the guard fires, and here it never does.
    assert_not_pinned(
        &weight_test(
            " -> Result<(), String>",
            "    if weight(4) != weight(4) {\n        return Err({\n            assert_eq!(weight(4), 12);\n            String::new()\n        });\n    }\n    Ok(())",
        ),
        "if weight(4) != weight(4)",
    );
    // `#[should_panic]` and `#[ignore]` settle the outcome.
    for attribute in ["#[should_panic]", "#[ignore]"] {
        let tests = format!(
            "use demo::weight;\n\n#[test]\n{attribute}\nfn weighs() -> Result<(), String> {{\n    if weight(4) != 12 {{\n        return Err(\"mismatch\".to_string());\n    }}\n    Ok(())\n}}\n"
        );
        assert_not_pinned(&tests, "if weight(4) != 12");
    }
}
