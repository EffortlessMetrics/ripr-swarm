mod json_construction;

use super::*;
use crate::analysis::facts::{FunctionItemFact, SourceRoleProvenance, SourceRoleProvenanceEdge};
use crate::analysis::rust_index::summarize_file;
use crate::analysis::syntax::macro_binding_candidates;
use crate::domain::{DeltaKind, ProbeId, SourceLocation, SymbolId};
use std::path::{Path, PathBuf};

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

/// #7083: a trait default method kept by a unit struct, called on the
/// struct's own name or on a `let` bound to it.
const KEPT_DEFAULT_LIB: &str = r#"pub trait Counter {
    fn step(&self) -> u32;

    fn advance(&self) -> u32 {
        4 * self.step()
    }
}

pub struct Unit;

impl Counter for Unit {
    fn step(&self) -> u32 {
        2
    }
}
"#;

/// The kept-default library with its tests in the same file.
fn kept_default_with_tests(body: &str) -> String {
    format!(
        "{KEPT_DEFAULT_LIB}\n#[cfg(test)]\nmod wrapped {{\n    use super::*;\n\n    #[test]\n    fn advances() {{\n{body}\n    }}\n}}\n"
    )
}

#[test]
fn unit_struct_receiver_is_typed_by_its_own_name() {
    let lib = kept_default_with_tests(
        "        let bound = Unit;\n        assert_eq!(Unit.advance(), 8);\n        assert_eq!(bound.advance(), 8);\n        assert_eq!(unit.advance(), 8);",
    );
    let index = index(&[(LIB, &lib)]);
    let pin = establish(&index, "advance", "4 * self.step()");
    assert!(pin.is_some(), "the kept default must establish a pin");
    let Some(pin) = pin else { return };
    // A lower-case receiver no `let` binds is not a unit struct.
    assert_eq!(
        admitted_texts(&index, &pin),
        vec![
            "assert_eq!(Unit.advance(), 8);".to_string(),
            "assert_eq!(bound.advance(), 8);".to_string(),
        ]
    );
}

/// #7083 review: the 2024 prelude's `IntoFuture::into_future(self)` is found
/// at the by-value step, before a `&self` trait default of the same name.
#[test]
fn edition_2024_into_future_is_a_by_value_prelude_method() {
    let lib = kept_default_with_tests("        assert_eq!(Unit.into_future(), 8);")
        .replace("fn advance(&self)", "fn into_future(&self)");
    let index = index(&[(LIB, &lib)]);
    let pin = establish(&index, "into_future", "4 * self.step()");
    assert!(pin.is_some(), "the kept default must establish a pin");
    let Some(pin) = pin else { return };
    assert!(admitted_texts(&index, &pin).is_empty());
}

/// #7083 review: `Iterator`'s comparisons take `self`, so `Unit.ne(..)` on an
/// iterator type runs `Iterator::ne` before a `&self` trait default `ne`.
#[test]
fn iterator_by_value_comparisons_are_prelude_methods() {
    for name in ["cmp", "eq", "ge", "gt", "le", "lt", "ne", "partial_cmp"] {
        let lib = kept_default_with_tests(&format!("        assert_eq!(Unit.{name}(), 8);"))
            .replace("fn advance(&self)", &format!("fn {name}(&self)"));
        let index = index(&[(LIB, &lib)]);
        let pin = establish(&index, name, "4 * self.step()");
        assert!(
            pin.is_some(),
            "the kept default `{name}` must establish a pin"
        );
        let Some(pin) = pin else { continue };
        assert!(admitted_texts(&index, &pin).is_empty(), "`{name}` admitted");
    }
}

/// #7098 review: `Iterator::is_partitioned` is still unstable on the
/// supported 1.95 toolchain, so a custom default of that name is the only
/// possible callee and its test is a genuine discriminator: the pin
/// admits it.
#[test]
fn unstable_is_partitioned_custom_default_is_admitted() {
    let lib = kept_default_with_tests("        assert_eq!(Unit.is_partitioned(), 8);")
        .replace("fn advance(&self)", "fn is_partitioned(&self)");
    let index = index(&[(LIB, &lib)]);
    let pin = establish(&index, "is_partitioned", "4 * self.step()");
    assert!(pin.is_some(), "the kept default must establish a pin");
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&index, &pin),
        vec!["assert_eq!(Unit.is_partitioned(), 8);".to_string()]
    );
}

#[test]
fn unit_struct_value_admits_only_spellings_nothing_else_can_bind() {
    let lib = kept_default_with_tests("        assert_eq!(Unit.advance(), 8);");
    let decide = |extra: &str| {
        let index = index(&[(LIB, &lib), (HELPERS, extra)]);
        // The memo is scoped to its index, so each fresh index decides
        // with a fresh run-scoped state, as production does per run.
        let syntax = OwnerPinSyntax::default();
        unit_struct_value("Unit", Path::new(LIB), &index, &syntax)
    };
    // Comments, other names, method calls and `let` initializers.
    assert!(decide(
        "mod inner {\n    use super::*;\n    use crate::inner::*;\n}\n"
    ));
    assert!(decide(
        "// struct Unit { x: u32 }\npub struct Units;\nconst UNIT: u32 = 1;\nfn n() -> u32 {\n    let v = Unit;\n    Unit.advance() + v.advance()\n}\nimpl Unit {}\n"
    ));
    for rival in [
        // Another declaration, or a non-unit or generic one.
        "pub struct Unit;\n",
        "pub struct Unit(u32);\n",
        "pub struct Unit<T>;\n",
        "pub enum Unit { A }\n",
        "pub union Unit { a: u32 }\n",
        // Rivals in the value namespace, renames and imports.
        "pub const Unit: u32 = 1;\n",
        "pub static Unit: u32 = 1;\n",
        "pub static mut Unit: u32 = 1;\n",
        "pub fn Unit() {}\n",
        "pub type Unit = u32;\n",
        "pub use other::Thing as Unit;\n",
        "pub use std::u32::MAX as Unit;\n",
        "pub use crate::m::Unit;\n",
        "pub enum Text { Unit }\n",
        "use self::Text::Unit;\n",
        "const r#Unit: &str = \"\";\n",
        "fn f(x: Kind) -> u32 { match x { Unit => 1 } }\n",
        // Type positions and macro input are not read.
        "fn take(u: &mut Unit) {}\n",
        "value!(Unit);\n",
        "constant!(= Unit);\n",
        // A macro that may declare any name it is given.
        "macro_rules! constant { ($name:ident) => {}; }\n",
        "macro_rules! constant { ($name : tt) => {}; }\n",
        // #7083 round 3: a left-to-right mark is whitespace to rustc, and
        // `include!`/`#[path]` splice in text the scan never reads.
        "macro_rules! constant { (= $name:\u{200E}ident;) => {}; }\n",
        "include!(\"shadow.in\");\n",
        "#[path = \"other.rs\"]\nmod other;\n",
        "# [ path = \"other.rs\"]\nmod other;\n",
        "#\n[path = \"other.rs\"]\nmod other;\n",
        // #7098 review: rustc accepts the raw-identifier spelling too
        // (`r#` directly before the ident; verified by compile probe).
        "#[r#path = \"shadow.in\"]\nmod shadow;\n",
        "#[r#path   =   \"shadow.in\"]\nmod shadow;\n",
        // A foreign glob in any file can reach the test through `crate::*`.
        "pub use std::u32::*;\n",
        // #7083 round 4: an outside module renamed or re-exported into the
        // crate, then globbed through a `crate::` path.
        "use std::u32 as nums;\nuse crate::nums::*;\n",
        "pub use std::u32;\nuse crate::u32::*;\n",
        "extern crate core as k;\nuse crate::k::u32::*;\n",
        "use crate::Kind::*;\n",
        "use crate::{helpers::*, Counter};\n",
        // #7098 review: a same-named module declared anywhere else does not
        // make the imported name a workspace module.
        "mod inner { mod nums {} }\nuse std::u32 as nums;\nuse crate::nums::*;\n",
        "mod inner { mod u32 {} }\npub use std::u32;\nuse crate::u32::*;\n",
        "mod a { mod k {} }\nextern crate core as k;\nuse crate::k::*;\n",
        "mod b { mod nums {} }\nuse std::{u32 as nums};\nuse crate::nums::*;\n",
        // #7083 round 5: one spelling in a macro body declares a struct per
        // invocation.
        "macro_rules! unit { () => { pub struct Other; }; }\nfn n() -> u32 {\n    Unit.advance()\n}\n",
    ] {
        assert!(!decide(rival), "{rival}");
    }
    // A declaration outside the test's file, or a foreign glob in it.
    // A prelude value is in scope without an import.
    let prelude =
        kept_default_with_tests("        assert_eq!(None.advance(), 8);").replace("Unit", "None");
    let prelude_index = index(&[(LIB, &prelude)]);
    let prelude_syntax = OwnerPinSyntax::default();
    assert!(!unit_struct_value(
        "None",
        Path::new(LIB),
        &prelude_index,
        &prelude_syntax
    ));
    let own = index(&[(LIB, &lib)]);
    let own_syntax = OwnerPinSyntax::default();
    assert!(!unit_struct_value(
        "Unit",
        Path::new(HELPERS),
        &own,
        &own_syntax
    ));
    let glob = lib.replace("use super::*;", "use super::*;\n    use std::u32::*;");
    let foreign = index(&[(LIB, &glob)]);
    let foreign_syntax = OwnerPinSyntax::default();
    assert!(!unit_struct_value(
        "Unit",
        Path::new(LIB),
        &foreign,
        &foreign_syntax
    ));
}

/// #7172: per-assertion unit-struct pin admission must not rescan the
/// workspace. `ASSERTIONS` admissions over one `FILLER_FILES`-file index
/// share one run-scoped [`OwnerPinSyntax`], exactly as production shares
/// it across probes; without memoization each admission pays a full
/// workspace scan, so the repeats cost `ASSERTIONS` x one cold admission.
/// Self-calibrating: both sides scale with machine speed, and the bound
/// sits far from both shapes (tens of x when rescanning, low single
/// digits when memoized).
#[test]
fn unit_struct_admission_memoizes_workspace_scans() {
    const FILLER_FILES: usize = 300;
    const ASSERTIONS: usize = 40;
    const RATIO_BOUND: u32 = 10;
    let mut body = String::new();
    for receiver in 0..ASSERTIONS {
        body.push_str(&format!(
            "        let unit{receiver} = Unit;\n        assert_eq!(unit{receiver}.advance(), 8);\n"
        ));
    }
    let lib = kept_default_with_tests(&body);
    let mut owned: Vec<(String, String)> = Vec::with_capacity(FILLER_FILES + 1);
    owned.push((LIB.to_string(), lib));
    for file in 0..FILLER_FILES {
        let mut filler = format!("mod filler_{file} {{\n    use super::filler_{file}::*;\n");
        for function in 0..25 {
            filler.push_str(&format!(
                "    pub fn work_{function}(n: u32) -> u32 {{\n        let path_len = n + {file};\n        path_len + {function}\n    }}\n"
            ));
        }
        filler.push_str("}\n");
        owned.push((format!("src/filler_{file}.rs"), filler));
    }
    let refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect();
    let index = index(&refs);
    let pin = establish(&index, "advance", "4 * self.step()");
    assert!(pin.is_some(), "the kept default must establish a pin");
    let Some(pin) = pin else { return };
    assert_eq!(index.tests().len(), 1, "fixture must hold exactly one test");
    let test = index.tests().at(0);
    assert_eq!(
        test.assertions.len(),
        ASSERTIONS,
        "fixture must hold one assertion per receiver"
    );
    let imports_foreign = |file: &Path, name: &str| {
        index.files().get(file).is_some_and(|facts| {
            file_imports_foreign_callee_name(&facts.source, name, &index.package_names)
        })
    };
    let syntax = OwnerPinSyntax::default();
    assert!(
        pin.admits(test, &test.assertions[0], &index, &imports_foreign, &syntax),
        "the fixture must admit on the success path"
    );
    let started = std::time::Instant::now();
    for assertion in &test.assertions {
        assert!(
            pin.admits(test, assertion, &index, &imports_foreign, &syntax),
            "every receiver assertion must admit on the success path"
        );
    }
    let repeated = started.elapsed();
    // Structural pin for the same claim: the shared run-scoped state now
    // holds the workspace scan, so the repeats above reused it instead of
    // rescanning. This holds regardless of machine timing noise.
    assert!(
        syntax.unit_struct_facts.get().is_some(),
        "the shared syntax state must memoize the workspace scan"
    );
    let cold = OwnerPinSyntax::default();
    let started = std::time::Instant::now();
    assert!(pin.admits(test, &test.assertions[0], &index, &imports_foreign, &cold));
    let single = started.elapsed();
    eprintln!(
        "#7172: {ASSERTIONS} shared-syntax admissions took {repeated:?}; one cold admission took {single:?}"
    );
    assert!(
        repeated < single * RATIO_BOUND,
        "{ASSERTIONS} shared-syntax admissions took {repeated:?}, more than {RATIO_BOUND}x one cold admission ({single:?}): the workspace scan is not memoized"
    );
}

#[test]
fn unit_struct_fact_collection_stops_at_the_first_refusal() {
    // The refusal predicates are name-independent, so the scan stops once
    // one holds and the caller refuses before reading `macro_files`. A
    // later macro-defining file must never be scanned into the set, which
    // pins the early stop structurally instead of by timing.
    let index = index(&[
        (
            "src/a_refusing.rs",
            "macro_rules! takes_name { ($t:tt) => {}; }\n",
        ),
        ("src/z_macro.rs", "macro_rules! later { () => {}; }\n"),
    ]);
    let facts = UnitStructFacts::collect(&index);
    assert!(facts.refused(), "the `:tt` matcher must refuse every name");
    let refusing = PathBuf::from("src/a_refusing.rs");
    let later = PathBuf::from("src/z_macro.rs");
    assert!(
        facts.macro_files.contains(&refusing),
        "the file that set the refusal must be recorded as macro-defining"
    );
    assert!(
        !facts.macro_files.contains(&later),
        "the scan must stop at the first refusal: the later macro file (scanned second in the sorted index) must not be recorded"
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
fn library_export_import_extraction_is_bounded_to_selected_files() {
    let source = "pub mod a { pub fn score() -> i64 { 1 } }\nmod b {}\nmod c {}\nmod d {}\npub use self::a::score;\n";
    let library = index(&[(LIB, source)]);
    assert_eq!(library.files().len(), 1);
    assert_eq!(library.functions().len(), 1);
    LIBRARY_IMPORT_EXTRACTIONS.with(|count| count.set(0));

    assert!(!library_may_export_other(&library, Path::new(LIB), "score"));
    assert_eq!(
        LIBRARY_IMPORT_EXTRACTIONS.with(std::cell::Cell::get),
        1,
        "each selected source is extracted once, regardless of module count"
    );
}

#[test]
fn library_export_without_module_names_does_not_extract_imports() {
    let library = index(&[(LIB, "pub fn score() -> i64 { 1 }\n")]);
    assert_eq!(library.functions().len(), 1);
    LIBRARY_IMPORT_EXTRACTIONS.with(|count| count.set(0));

    assert!(!library_may_export_other(&library, Path::new(LIB), "score"));
    assert_eq!(LIBRARY_IMPORT_EXTRACTIONS.with(std::cell::Cell::get), 0);
}

#[test]
fn library_export_imports_in_another_file_still_defeat_a_module_name() {
    let lib =
        "pub mod fs { pub fn score() -> i64 { 1 } }\nmod helpers;\npub use self::fs::score;\n";
    let provenance = SourceRoleProvenance {
        edges: vec![module_edge(LIB, HELPERS, "helpers", 2, false)],
        ..Default::default()
    };
    for (helpers, expected) in [
        ("use other_crate as fs;\n", true),
        ("extern crate other_crate as fs;\n", true),
        (
            "// use other_crate as fs;\nconst DOC: &str = \"use other_crate as fs;\";\n",
            false,
        ),
    ] {
        let library = index_with_provenance(
            &[(LIB, lib), (HELPERS, helpers)],
            &[(HELPERS, provenance.clone())],
        );
        assert_eq!(library.files().len(), 2);
        assert_eq!(library.functions().len(), 1);
        LIBRARY_IMPORT_EXTRACTIONS.with(|count| count.set(0));

        assert_eq!(
            library_may_export_other(&library, Path::new(LIB), "score"),
            expected,
            "{helpers}"
        );
        assert!(
            LIBRARY_IMPORT_EXTRACTIONS.with(std::cell::Cell::get) <= 2,
            "cross-file checks reuse each file's own extracted imports"
        );
    }
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
    // `Codec::decode` owner never takes a bare call. #6617 decision: the
    // associated owner now establishes a `Codec::decode(..)` pin
    // (RIPR-SPEC-0172), so B2 is asserted directly on the test side: the
    // pin exists and the bare `decode(8)` assertion is not admitted.
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
    let associated_pin = OwnerReturnPin::establish(&probe, associated, &index);
    assert!(associated_pin.is_some());
    let Some(associated_pin) = associated_pin else {
        return;
    };
    assert!(admitted_texts(&index, &associated_pin).is_empty());
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
    // #6544: a same-package module-level twin no longer refuses the pin at
    // `establish`; each test's import or path must settle the bare call on
    // the owner, and one that settles on the twin (or on nothing) is still
    // defeated.
    for (import, pinned) in [
        ("use demo::other::scaled;", false),
        ("use demo::other::*;", false),
        // The crate-name glob brings the crate root's own `scaled`.
        ("use demo::*;", true),
        ("use demo::scaled;", true),
    ] {
        let tests =
            format!("{import}\n\n#[test]\nfn scales() {{\n    assert_eq!(scaled(3), 30);\n}}\n");
        let index_with_twin = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index_with_twin, "scaled", "x * 10");
        assert!(pin.is_some(), "{import}");
        let admitted = pin
            .map(|pin| admitted_texts(&index_with_twin, &pin).len())
            .unwrap_or_default();
        assert_eq!(admitted, usize::from(pinned), "{import}");
    }
    // A twin outside the owner's package is not settled per test and still
    // refuses the pin outright.
    let foreign_twin = "pub fn scaled(x: i32) -> i32 {\n    x\n}\n";
    let index_with_foreign_twin = index(&[(LIB, lib), ("crates/other/src/lib.rs", foreign_twin)]);
    assert!(establish(&index_with_foreign_twin, "scaled", "x * 10").is_none());

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

/// #6578: with the changed line's old text, only the part that differs
/// must run on every input. An unchanged closure elsewhere in the tail does
/// not make the change conditional.
#[test]
fn an_unchanged_closure_leaves_the_changed_operand_unconditional() {
    let body = |tail: &str| format!("fn total(lines: &[Line]) -> u64 {{\n    {tail}\n}}");
    let gate_from =
        |after: &str, before: Option<&str>| return_path_gate_from(&body(after), after, before, 1);
    let tail = |fee: &str| format!("lines.iter().map(|l| l.cents).sum::<u64>() + {fee}");
    assert!(matches!(
        gate_from(&tail("25 * 2"), Some(&tail("50"))),
        Some(ReturnPathGate::Any)
    ));
    // Every enclosing operator, parenthesis and cast evaluates the change.
    for (after, before) in [
        (
            "(lines.iter().map(|l| l.cents).sum::<u64>() * 2 + 51) as u64",
            "(lines.iter().map(|l| l.cents).sum::<u64>() * 2 + 50) as u64",
        ),
        (
            "lines.iter().map(|l| l.cents).sum::<u64>() + u64::from(fee(51))",
            "lines.iter().map(|l| l.cents).sum::<u64>() + u64::from(fee(50))",
        ),
    ] {
        assert!(
            matches!(gate_from(after, Some(before)), Some(ReturnPathGate::Any)),
            "{after}"
        );
    }
    // Without the old text the whole tail is read, closure included.
    assert!(gate_from(&tail("25 * 2"), None).is_none());
    for (after, before) in [
        // The change is inside the closure.
        (
            "lines.iter().map(|l| l.cents + 1).sum::<u64>()",
            "lines.iter().map(|l| l.cents + 2).sum::<u64>()",
        ),
        // The changed part is an argument a combinator may skip, sits in a
        // branch, or behind `&&`/`||`.
        ("x.map_or(51, |v| v)", "x.map_or(50, |v| v)"),
        (
            "if ready() { 51 } else { lines.iter().map(|l| l.cents).sum::<u64>() }",
            "if ready() { 50 } else { lines.iter().map(|l| l.cents).sum::<u64>() }",
        ),
        (
            "u64::from(lines.iter().any(|l| l.cents > 0) && f(51))",
            "u64::from(lines.iter().any(|l| l.cents > 0) && f(50))",
        ),
        (
            "Option::map(x, |v| v).unwrap_or(51)",
            "Option::map(x, |v| v).unwrap_or(50)",
        ),
        (
            "Option::map_or(x, 51, |v| v) + 1",
            "Option::map_or(x, 50, |v| v) + 1",
        ),
        (
            "(Option::map_or)(x, 51, |v| v)",
            "(Option::map_or)(x, 50, |v| v)",
        ),
        (
            "x.unwrap_or(51) + lines.iter().map(|l| l.cents).sum::<u64>()",
            "x.unwrap_or(50) + lines.iter().map(|l| l.cents).sum::<u64>()",
        ),
        // A name bound outside the closure carries the change into it.
        (
            "{ let k = 3; lines.iter().map(|l| l.cents * k).sum::<u64>() }",
            "{ let k = 2; lines.iter().map(|l| l.cents * k).sum::<u64>() }",
        ),
        (
            "{ const K: u64 = 3; lines.iter().map(|l| l.cents * K).sum::<u64>() }",
            "{ const K: u64 = 2; lines.iter().map(|l| l.cents * K).sum::<u64>() }",
        ),
        // The changed value flows through a closure beside it in a call.
        (
            "items.iter().map(|l| l.cents).sum::<u64>() + 50",
            "lines.iter().map(|l| l.cents).sum::<u64>() + 50",
        ),
        (
            "lines[..1].iter().map(|l| l.cents).sum::<u64>() + 50",
            "lines.iter().map(|l| l.cents).sum::<u64>() + 50",
        ),
        (
            "lines.iter().fold(51, |acc, l| acc + l.cents)",
            "lines.iter().fold(50, |acc, l| acc + l.cents)",
        ),
        // The changed method name, turbofish, or a deletion beside the
        // closure.
        (
            "lines.iter().map(|l| l.cents).product::<u64>()",
            "lines.iter().map(|l| l.cents).sum::<u64>()",
        ),
        (
            "lines.iter().map(|l| l.cents).sum::<u32>() as u64",
            "lines.iter().map(|l| l.cents).sum::<u64>() as u64",
        ),
        (
            "lines.iter().map(|l| l.cents).sum::<u64>()",
            "lines.iter().skip(1).map(|l| l.cents).sum::<u64>()",
        ),
        // The change removed a closure, macro or block that may have given
        // the same value.
        (
            "lines.iter().map(|l| l.cents).sum::<u64>() + 51",
            "lines.iter().map(|l| l.cents).sum::<u64>() + (|| 51)()",
        ),
        (
            "lines.iter().map(|l| l.cents).sum::<u64>() + 51",
            "lines.iter().map(|l| l.cents).sum::<u64>() + m!(51)",
        ),
        (
            "lines.iter().map(|l| l.cents).sum::<u64>() + 51",
            "lines.iter().map(|l| l.cents).sum::<u64>() + { 51 }",
        ),
        // String whitespace changed alongside the operand.
        (
            "\"a  b\".len() as u64 + lines.iter().map(|l| l.cents).sum::<u64>() + 1",
            "\"a b\".len() as u64 + lines.iter().map(|l| l.cents).sum::<u64>() + 2",
        ),
        // A changed comment beside the change.
        (
            "lines.iter().map(|l| l.cents).sum::<u64>() + 51 /* fee v2 */",
            "lines.iter().map(|l| l.cents).sum::<u64>() + 50 /* fee v1 */",
        ),
        // A block around the change may skip it.
        (
            "'a: { if lines.is_empty() { break 'a 0; } lines.iter().map(|l| l.cents).sum::<u64>() + 51 }",
            "'a: { if lines.is_empty() { break 'a 0; } lines.iter().map(|l| l.cents).sum::<u64>() + 50 }",
        ),
    ] {
        assert!(gate_from(after, Some(before)).is_none(), "{after}");
    }
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
    // A second `return Err(..)` is another source of `Err`, so the `Err`
    // head gate refuses it; #7063's variant gate then admits only a pin
    // that names the changed variant, which no other exit builds.
    let sibling = "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(E::Negative);\n    }\n    if x == 0 {\n        return Err(E::Zero);\n    }\n    Ok(x)\n}";
    let at = sibling.find("return Err(E::Negative);").unwrap_or(0);
    let gate = return_path_gate(
        sibling,
        "return Err(E::Negative);",
        sibling[..at].matches('\n').count(),
    );
    assert!(
        matches!(&gate, Some(ReturnPathGate::ErrVariant { qualifier, variant, .. })
            if qualifier == "E" && variant == "Negative"),
        "{}",
        gate.is_some()
    );
    assert!(
        gate.as_ref()
            .is_some_and(|gate| !gate.admits("Err(E::Zero)"))
    );
    for body in [
        // A second `return` of the same variant is another source of it.
        "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(E::Negative);\n    }\n    if x == 0 {\n        return Err(E::Negative);\n    }\n    Ok(x)\n}",
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

/// #7063: a changed early `return Err(Type::Variant { .. });` beside other
/// `return Err(..)` exits is pinned when no other exit can build that
/// variant, and the pin admits only an expected `Err(..)` of that variant.
#[test]
fn an_early_err_variant_return_is_pinned_when_no_other_exit_builds_it() {
    let body = |other: &str, tail: &str| {
        format!(
            "fn f(x: i32) -> Result<i32, E> {{\n    if x < 0 {{\n        return Err(E::Negative {{ at: x - 1 }});\n    }}\n    if x == 0 {{\n        {other}\n    }}\n    {tail}\n}}"
        )
    };
    let changed = "return Err(E::Negative { at: x - 1 });";
    let gate_of = |text: &str| gate(text, changed);
    let positive = gate_of(&body("return Err(E::Zero { at: x });", "Ok(x)"));
    assert!(
        matches!(&positive, Some(ReturnPathGate::ErrVariant { qualifier, variant, .. })
            if qualifier == "E" && variant == "Negative"),
        "the sibling-variant exit must leave the variant gate"
    );
    let Some(positive) = positive else { return };
    assert!(positive.admits("Err(E::Negative { at: 7 })"));
    assert!(positive.admits(" Err(crate::E::Negative { at: 7 }) "));
    for expected in [
        "Err(E::Zero { at: 7 })",
        "Err(F::Negative { at: 7 })",
        "Ok(7)",
        "Err(e)",
        "Err(E::Negative { at: 7 }).map(|v| v)",
        // A foreign enum that only shares the trailing names.
        "Err(external::E::Negative { at: 7 })",
    ] {
        assert!(!positive.admits(expected), "{expected}");
    }
    for (other, tail) in [
        // A second return of the same variant.
        ("return Err(E::Negative { at: 0 });", "Ok(x)"),
        // An opaque error may hold the changed variant.
        ("return Err(zero_error());", "Ok(x)"),
        ("return Err(e);", "Ok(x)"),
        // Another enum's variant is not read as a sibling.
        ("return Err(F::Zero);", "Ok(x)"),
        // A `?` converts a callee error into any variant.
        ("return Err(E::Zero { at: x });", "Ok(g(x)?)"),
        // A tail that is not one `Ok(..)` (or a sibling variant) call.
        ("return Err(E::Zero { at: x });", "lookup(x)"),
        (
            "return Err(E::Zero { at: x });",
            "Err(E::Negative { at: 0 })",
        ),
    ] {
        let text = body(other, tail);
        assert!(gate_of(&text).is_none(), "{text}");
    }
    // A changed construction of a foreign `external::E` names no local enum.
    let foreign = "fn f(x: i32) -> Result<i32, external::E> {\n    if x < 0 {\n        return Err(external::E::Negative { at: x - 1 });\n    }\n    if x == 0 {\n        return Err(external::E::Zero { at: x });\n    }\n    Ok(x)\n}";
    assert!(gate(foreign, "return Err(external::E::Negative { at: x - 1 });").is_none());
    // A bare variant names no enum.
    let bare = "fn f(x: i32) -> Result<i32, E> {\n    if x < 0 {\n        return Err(Negative { at: x });\n    }\n    if x == 0 {\n        return Err(E::Zero);\n    }\n    Ok(x)\n}";
    assert!(gate(bare, "return Err(Negative { at: x });").is_none());
}

/// The config parser behind `config-missing-equals-line` (#7063).
const MISSING_EQUALS_LIB: &str = "#[derive(Debug, Clone, PartialEq, Eq)]\npub enum ConfigError {\n    MissingEquals { line: usize },\n    EmptyKey { line: usize },\n}\n\npub fn parse_pairs(text: &str) -> Result<Vec<String>, ConfigError> {\n    let mut keys = Vec::new();\n    for (idx, raw) in text.lines().enumerate() {\n        let line_no = idx + 1;\n        let Some((key, _value)) = raw.split_once('=') else {\n            return Err(ConfigError::MissingEquals { line: idx + 1 });\n        };\n        if key.trim().is_empty() {\n            return Err(ConfigError::EmptyKey { line: line_no });\n        }\n        keys.push(key.trim().to_string());\n    }\n    Ok(keys)\n}\n";

const MISSING_EQUALS_TESTS: &str = "use demo::{parse_pairs, ConfigError};\n\n#[test]\nfn missing_equals_reports_its_line() {\n    assert_eq!(parse_pairs(\"a = 1\\nport 9000\"), Err(ConfigError::MissingEquals { line: 2 }));\n    assert_eq!(parse_pairs(\"= 1\"), Err(ConfigError::EmptyKey { line: 1 }));\n    assert!(matches!(parse_pairs(\"port\"), Err(ConfigError::MissingEquals { .. })));\n    assert_eq!(parse_pairs(\"a = 1\"), Ok(vec![\"a\".to_string()]));\n}\n";

/// The owner pin for a probe of `family` on the changed `MissingEquals`
/// line of [`MISSING_EQUALS_LIB`] (or `lib`).
fn config_pin(
    lib: &str,
    family: ProbeFamily,
    expression: &str,
) -> (RustIndex, Option<OwnerReturnPin>) {
    config_pin_with(lib, MISSING_EQUALS_TESTS, family, expression)
}

/// [`config_pin`] read against the test file `tests`.
fn config_pin_with(
    lib: &str,
    tests: &str,
    family: ProbeFamily,
    expression: &str,
) -> (RustIndex, Option<OwnerReturnPin>) {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = {
        let owner = owner(&index, "parse_pairs");
        let line = owner.start_line
            + owner
                .body
                .lines()
                .position(|line| line.contains("ConfigError::MissingEquals"))
                .unwrap_or(0);
        let probe = Probe {
            family,
            location: SourceLocation::new(owner.file.clone(), line, 1),
            ..return_probe(owner, expression)
        };
        OwnerReturnPin::establish(&probe, owner, &index)
    };
    (index, pin)
}

/// #7063: the error-path, return-value and field-construction probes of a
/// changed `return Err(E::V { line: .. });` are pinned only by an
/// `assert_eq!` expecting that variant, and only when the enum's `==` is
/// the derived one, so the pin compares the payload.
#[test]
fn an_early_err_variant_pin_admits_only_its_variant_through_derived_equality() {
    let changed = "return Err(ConfigError::MissingEquals { line: idx + 1 });";
    let expected = vec![
        "assert_eq!(parse_pairs(\"a = 1\\nport 9000\"), Err(ConfigError::MissingEquals { line: 2 }));"
            .to_string(),
    ];
    for (family, expression) in [
        (ProbeFamily::ErrorPath, changed),
        (ProbeFamily::ReturnValue, changed),
        (ProbeFamily::FieldConstruction, "line: idx + 1"),
    ] {
        let (index, pin) = config_pin(MISSING_EQUALS_LIB, family.clone(), expression);
        assert!(pin.is_some(), "{family:?} must establish a pin");
        let Some(pin) = pin else { return };
        // Not the sibling `EmptyKey` pin, the variant-only `matches!`, or
        // the `Ok` pin.
        assert_eq!(admitted_texts(&index, &pin), expected, "{family:?}");
    }
    // A hand-written `==` may ignore the payload (or the variant).
    let manual = MISSING_EQUALS_LIB.replace(
        "#[derive(Debug, Clone, PartialEq, Eq)]",
        "#[derive(Debug, Clone)]",
    ) + "\nimpl PartialEq for ConfigError {\n    fn eq(&self, _other: &Self) -> bool {\n        true\n    }\n}\n";
    // A second `MissingEquals` exit is another source of the variant.
    let second = MISSING_EQUALS_LIB.replace(
        "return Err(ConfigError::EmptyKey { line: line_no });",
        "return Err(ConfigError::MissingEquals { line: 0 });",
    );
    for lib in [manual.as_str(), second.as_str()] {
        for (family, expression) in [
            (ProbeFamily::ErrorPath, changed),
            (ProbeFamily::FieldConstruction, "line: idx + 1"),
        ] {
            assert!(
                config_pin(lib, family.clone(), expression).1.is_none(),
                "{family:?}\n{lib}"
            );
        }
    }
    // A field the construction does not initialize, or a construction
    // spread over several lines, establishes no field pin.
    assert!(
        config_pin(
            MISSING_EQUALS_LIB,
            ProbeFamily::FieldConstruction,
            "line: line_no"
        )
        .1
        .is_none()
    );
    let spread = MISSING_EQUALS_LIB.replace(
        "return Err(ConfigError::MissingEquals { line: idx + 1 });",
        "return Err(ConfigError::MissingEquals {\n                line: idx + 1,\n            });",
    );
    assert!(
        config_pin(&spread, ProbeFamily::FieldConstruction, "line: idx + 1,")
            .1
            .is_none()
    );
}

/// #7063: an early error-variant pin reads `ConfigError::MissingEquals` as
/// the one construction of that variant only when nothing else in the owner
/// can build it unseen: no `use` brings the variant in under a bare or alias
/// spelling, no import or alias rebinds the enum, every sibling exit names a
/// declared variant ending at its payload, and the derived `==` compares
/// each field by value.
#[test]
fn an_early_err_variant_pin_refuses_what_can_build_its_variant_unseen() {
    let changed = "return Err(ConfigError::MissingEquals { line: idx + 1 });";
    let families = [
        (ProbeFamily::ErrorPath, changed),
        (ProbeFamily::ReturnValue, changed),
        (ProbeFamily::FieldConstruction, "line: idx + 1"),
    ];
    let pinned = |lib: &str| {
        families
            .iter()
            .map(|(family, expression)| config_pin(lib, family.clone(), expression).1.is_some())
            .collect::<Vec<_>>()
    };
    assert_eq!(pinned(MISSING_EQUALS_LIB), vec![true, true, true]);
    let first_exit = |exit: &str| {
        MISSING_EQUALS_LIB.replace(
            "    let mut keys = Vec::new();\n",
            &format!("    if text.is_empty() {{\n        {exit}\n    }}\n    let mut keys = Vec::new();\n"),
        )
    };
    let with_use = |statement: &str, exit: &str| format!("{statement}\n{}", first_exit(exit));
    let refused = [
        // A bare or aliased spelling of the variant builds it unseen.
        with_use(
            "use ConfigError::MissingEquals;",
            "return Err(MissingEquals { line: 0 });",
        ),
        with_use(
            "use ConfigError::*;",
            "return Err(MissingEquals { line: 0 });",
        ),
        with_use(
            "use self::ConfigError::MissingEquals as M;",
            "return Err(M { line: 0 });",
        ),
        // The import alone already opens that door, even through a
        // workspace path that does not rebind the enum itself.
        format!("use ConfigError::MissingEquals;\n{MISSING_EQUALS_LIB}"),
        format!("use crate::ConfigError::MissingEquals;\n{MISSING_EQUALS_LIB}"),
        format!("use self::ConfigError::*;\n{MISSING_EQUALS_LIB}"),
        // An import or alias rebinds the enum name the probe reads.
        format!("use other::ConfigError;\n{MISSING_EQUALS_LIB}"),
        // A sibling that is not a declared variant (an associated constant
        // may hold any variant), or one whose argument goes on past the
        // payload into a call that may return any variant.
        first_exit("return Err(ConfigError::NO_EQUALS);"),
        first_exit("return Err(ConfigError::EmptyKey { line: 0 }.relabel());"),
        first_exit("return Err(ConfigError::EmptyKey { line: 0 }).map(|v| v);"),
        // A field whose `==` is not by value, or a field attribute.
        MISSING_EQUALS_LIB.replace(
            "MissingEquals { line: usize },",
            "MissingEquals { line: Span },",
        ) + "\n#[derive(Debug, Clone)]\npub struct Span(usize);\n\nimpl PartialEq for Span {\n    fn eq(&self, _other: &Self) -> bool {\n        true\n    }\n}\n\nimpl Eq for Span {}\n",
        MISSING_EQUALS_LIB.replace(
            "MissingEquals { line: usize },",
            "MissingEquals {\n        #[serde(skip)]\n        line: usize,\n    },",
        ),
    ];
    for lib in &refused {
        assert_eq!(pinned(lib), vec![false, false, false], "{lib}");
    }
    // A declared sibling variant on another exit leaves the pin in place.
    let sibling = first_exit("return Err(ConfigError::EmptyKey { line: 0 });");
    assert_eq!(pinned(&sibling), vec![true, true, true]);
    // The only `Err` exit, under the `Err` head gate, is held to the same
    // derived `==`: a hand-written one may ignore the payload.
    let only = MISSING_EQUALS_LIB.replace(
        "        if key.trim().is_empty() {\n            return Err(ConfigError::EmptyKey { line: line_no });\n        }\n",
        "",
    );
    assert_ne!(only, MISSING_EQUALS_LIB);
    assert_eq!(pinned(&only)[..2], [true, true]);
    let only_manual = only.replace(
        "#[derive(Debug, Clone, PartialEq, Eq)]",
        "#[derive(Debug, Clone)]",
    ) + "\nimpl PartialEq for ConfigError {\n    fn eq(&self, _other: &Self) -> bool {\n        true\n    }\n}\n";
    assert_eq!(pinned(&only_manual)[..2], [false, false]);
}

/// #7063: the on-line error witness completes only for a construction whose
/// qualifier names, in the owner's file, a declared workspace enum that
/// declares the variant. An alias, a `type` alias, `Self::` or a module path
/// keeps main's candidate witness.
#[test]
fn only_a_declared_enum_variant_completes_the_error_witness() {
    let declared = |lib: &str, path: &str| {
        let index = index(&[(LIB, lib)]);
        owner_builds_declared_error_variant(path, owner(&index, "parse_pairs"), &index)
    };
    for path in [
        "ConfigError::MissingEquals",
        "crate::ConfigError::MissingEquals",
    ] {
        assert!(declared(MISSING_EQUALS_LIB, path), "{path}");
    }
    let refused = [
        // A sibling the enum does not declare.
        (MISSING_EQUALS_LIB.to_string(), "ConfigError::Unknown"),
        // An import alias.
        (
            format!("use crate::ConfigError as CE;\n{MISSING_EQUALS_LIB}"),
            "CE::MissingEquals",
        ),
        // A `type` alias.
        (
            format!("{MISSING_EQUALS_LIB}\npub type CE = ConfigError;\n"),
            "CE::MissingEquals",
        ),
        // `Self::` inside the enum's own impl.
        (MISSING_EQUALS_LIB.to_string(), "Self::MissingEquals"),
        // A module path.
        (
            format!(
                "{MISSING_EQUALS_LIB}\npub mod errors {{\n    pub struct MissingEquals {{\n        pub line: usize,\n    }}\n}}\n"
            ),
            "errors::MissingEquals",
        ),
        // An import that rebinds the enum name itself.
        (
            format!("use other::ConfigError;\n{MISSING_EQUALS_LIB}"),
            "ConfigError::MissingEquals",
        ),
        // A foreign path sharing the declared enum's trailing names.
        (
            MISSING_EQUALS_LIB.to_string(),
            "external::ConfigError::MissingEquals",
        ),
        (
            MISSING_EQUALS_LIB.to_string(),
            "::ConfigError::MissingEquals",
        ),
        (
            MISSING_EQUALS_LIB.to_string(),
            "super::ConfigError::MissingEquals",
        ),
    ];
    for (lib, path) in &refused {
        assert!(!declared(lib, path), "{path}\n{lib}");
    }
}

/// #7063: a test file that rebinds the expected variant's enum name
/// (`use other::ConfigError;`) may compare another type, so the early
/// error-variant pin admits none of its equalities.
#[test]
fn an_early_err_variant_pin_refuses_a_test_file_that_rebinds_the_enum() {
    let changed = "return Err(ConfigError::MissingEquals { line: idx + 1 });";
    let admitted_in = |imports: &str| {
        let tests = format!(
            "{imports}\n\n#[test]\nfn reads() {{\n    assert_eq!(parse_pairs(\"port\"), Err(ConfigError::MissingEquals {{ line: 1 }}));\n}}\n"
        );
        let (index, pin) =
            config_pin_with(MISSING_EQUALS_LIB, &tests, ProbeFamily::ErrorPath, changed);
        assert!(pin.is_some(), "the owner-side pin must establish");
        pin.map(|pin| admitted_texts(&index, &pin))
            .unwrap_or_default()
    };
    assert_eq!(
        admitted_in("use demo::{parse_pairs, ConfigError};").len(),
        1,
        "the workspace enum's equality is the pin"
    );
    assert!(admitted_in("use demo::parse_pairs;\nuse other::ConfigError;").is_empty());
}

/// #7063: the early error-variant pin admits an equality only when one of
/// its operands is the owner call or a local bound directly from it
/// (`let r = owner(..);`, `let e = owner(..).unwrap_err();` or
/// `.expect_err("..")`), never an equality on another function's result
/// beside an unrelated owner assertion.
#[test]
fn an_early_err_variant_pin_reads_only_the_owner_result() {
    let changed = "return Err(ConfigError::MissingEquals { line: idx + 1 });";
    let lib = format!(
        "{MISSING_EQUALS_LIB}\npub fn parse_section(text: &str) -> Result<String, ConfigError> {{\n    Err(ConfigError::MissingEquals {{ line: text.len() }})\n}}\n"
    );
    let admitted_in = |body: &str| {
        let tests = format!(
            "use demo::{{parse_pairs, parse_section, ConfigError}};\n\n#[test]\nfn reads() {{\n{body}\n}}\n"
        );
        let (index, pin) = config_pin_with(&lib, &tests, ProbeFamily::ErrorPath, changed);
        assert!(pin.is_some(), "the pin must establish for {body}");
        pin.map(|pin| admitted_texts(&index, &pin))
            .unwrap_or_default()
    };
    for (body, expected) in [
        (
            "    let r = parse_pairs(\"port\");\n    assert_eq!(r, Err(ConfigError::MissingEquals { line: 1 }));",
            "assert_eq!(r, Err(ConfigError::MissingEquals { line: 1 }));",
        ),
        (
            "    let e = parse_pairs(\"port\").unwrap_err();\n    assert_eq!(e, ConfigError::MissingEquals { line: 1 });",
            "assert_eq!(e, ConfigError::MissingEquals { line: 1 });",
        ),
        (
            "    let e = parse_pairs(\"port\").expect_err(\"no equals\");\n    assert_eq!(ConfigError::MissingEquals { line: 1 }, e);",
            "assert_eq!(ConfigError::MissingEquals { line: 1 }, e);",
        ),
        (
            "    assert_eq!(parse_pairs(\"port\").unwrap_err(), ConfigError::MissingEquals { line: 1 });",
            "assert_eq!(parse_pairs(\"port\").unwrap_err(), ConfigError::MissingEquals { line: 1 });",
        ),
    ] {
        assert_eq!(admitted_in(body), vec![expected.to_string()], "{body}");
    }
    for body in [
        // Another function's result beside an owner assertion that
        // compares nothing.
        "    assert!(parse_pairs(\"port\").is_err());\n    assert_eq!(parse_section(\"[x\"), Err(ConfigError::MissingEquals { line: 7 }));",
        "    let e = parse_section(\"[x\").unwrap_err();\n    assert_eq!(e, ConfigError::MissingEquals { line: 2 });",
        // The owner's result, but a sibling variant or the `Ok` side.
        "    let e = parse_pairs(\"= 1\").unwrap_err();\n    assert_eq!(e, ConfigError::EmptyKey { line: 1 });",
        "    let e = parse_pairs(\"a = 1\").unwrap();\n    assert_eq!(e, vec![\"a\".to_string()]);",
        // An `unwrap_err` binding read through a derived value.
        "    let e = parse_pairs(\"port\").unwrap_err().clone();\n    assert_eq!(e, ConfigError::MissingEquals { line: 1 });",
    ] {
        assert_eq!(admitted_in(body), Vec::<String>::new(), "{body}");
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

    // The gate reads `None` as the prelude variant: a `const None` in the
    // owner's file can be a second `Some` exit (#7581 review).
    let lib = "pub fn lookup(k: u32) -> Option<u32> {\n    if k == 1 { return Some(k + 2); }\n    None\n}\n";
    let some_index = index(&[(LIB, lib)]);
    assert!(establish(&some_index, "lookup", "return Some(k + 2);").is_some());
    let shadowed =
        format!("{lib}\n#[allow(non_upper_case_globals)]\nconst None: Option<u32> = Some(3);\n");
    let shadowed_index = index(&[(LIB, &shadowed)]);
    assert!(establish(&shadowed_index, "lookup", "return Some(k + 2);").is_none());
    // The same for an early `Ok` beside `Err` exits and a rebound `Err`.
    let lib = "pub fn parse(k: u32) -> Result<u32, u32> {\n    if k == 1 { return Ok(k + 2); }\n    Err(k)\n}\n";
    let ok_index = index(&[(LIB, lib)]);
    assert!(establish(&ok_index, "parse", "return Ok(k + 2);").is_some());
    let shadowed = format!(
        "{lib}\n#[allow(non_snake_case)]\nfn Err(k: u32) -> Result<u32, u32> {{\n    Ok(k)\n}}\n"
    );
    let shadowed_index = index(&[(LIB, &shadowed)]);
    assert!(establish(&shadowed_index, "parse", "return Ok(k + 2);").is_none());

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

/// An early `return Err(String::from("big"))` beside an `Ok(x)` tail.
fn lib_twin() -> &'static str {
    "pub fn check(x: u32) -> Result<u32, String> {\n    if x > 10 { return Err(String::from(\"big\")); }\n    Ok(x)\n}\n"
}

#[test]
fn an_error_path_probe_is_pinned_like_a_changed_err_return() {
    // RIPR-SPEC-0231 example 5: ripr files a changed `return Err(..)` as an
    // `error_path` probe. The early-return gate is the one a `return_value`
    // probe of the same line gets, so `assert_eq!(f(..), Err(..))` pins it.
    let lib = "pub fn check(x: u32) -> Result<u32, String> {\n    if x > 10 { return Err(String::from(\"b\") + \"ig\"); }\n    Ok(x)\n}\n";
    let tests = "use demo::check;\n\n#[test]\nfn checks() {\n    assert_eq!(check(20), Err(String::from(\"big\")));\n    assert_eq!(check(2), Ok(2));\n}\n";
    let err_index = index(&[(LIB, lib), (TESTS, tests)]);
    let error_probe = |index: &RustIndex, name: &str, expression: &str| {
        let owner = owner(index, name);
        let probe = Probe {
            family: ProbeFamily::ErrorPath,
            ..return_probe(owner, expression)
        };
        OwnerReturnPin::establish(&probe, owner, index)
    };
    let changed = "return Err(String::from(\"b\") + \"ig\");";
    let pin = error_probe(&err_index, "check", changed);
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&err_index, &pin),
        vec!["assert_eq!(check(20), Err(String::from(\"big\")));".to_string()]
    );

    // A `?` is a second `Err` source, so an early `return Err(..)` beside
    // one pins nothing for an `error_path` probe either.
    let lib = "pub fn parse(s: &str) -> Result<u32, String> {\n    if s.is_empty() { return Err(String::from(\"empty\")); }\n    let n: u32 = s.parse().map_err(|_| String::from(\"nan\"))?;\n    Ok(n)\n}\n";
    let question_index = index(&[(LIB, lib)]);
    assert!(
        error_probe(
            &question_index,
            "parse",
            "return Err(String::from(\"empty\"));"
        )
        .is_none()
    );

    // A test-local `fn Err` that returns `Ok` makes the expected value
    // something else, so the test's own scope refuses the pin; a glob the
    // scan cannot see through does not (a test module's routine
    // `use super::sub::*;`).
    let pinned = |tests: &str| {
        let test_index = index(&[(LIB, lib_twin()), (TESTS, tests)]);
        let pin = error_probe(&test_index, "check", "return Err(String::from(\"big\"));");
        pin.map(|pin| admitted_texts(&test_index, &pin))
    };
    let assertion = "    assert_eq!(check(2), Err(String::from(\"big\")));\n";
    let rebound = format!(
        "use demo::check;\n\n#[allow(non_snake_case)]\nfn Err(_: String) -> Result<u32, String> {{\n    Ok(2)\n}}\n\n#[test]\nfn checks() {{\n{assertion}}}\n"
    );
    assert_eq!(pinned(&rebound), Some(Vec::new()));
    let globbed = format!(
        "use demo::check;\nuse demo::retail::*;\n\n#[test]\nfn checks() {{\n{assertion}}}\n"
    );
    assert_eq!(pinned(&globbed), Some(vec![assertion.trim().to_string()]));

    // The gate reads the tail `Ok(x)` as the prelude variant: a rebound
    // `Ok` in the owner's file can be a second `Err` exit.
    let shadowed = format!(
        "{}\n#[allow(non_snake_case)]\nfn Ok(x: u32) -> Result<u32, String> {{\n    Err(x.to_string())\n}}\n",
        lib_twin()
    );
    let shadowed_index = index(&[(LIB, &shadowed)]);
    assert!(
        error_probe(
            &shadowed_index,
            "check",
            "return Err(String::from(\"big\"));"
        )
        .is_none()
    );
    // A `None =>` arm names the variant, not a binding (E0530), so an
    // ordinary match elsewhere in the file keeps an early `Some` pin.
    let matched = "pub fn lookup(k: u32) -> Option<u32> {\n    if k == 1 { return Some(k + 2); }\n    None\n}\n\npub fn describe(v: Option<u32>) -> u32 {\n    match v { Some(n) => n, None => 0 }\n}\n";
    let matched_index = index(&[(LIB, matched)]);
    assert!(establish(&matched_index, "lookup", "return Some(k + 2);").is_some());
    // A comment that only mentions the attribute changes nothing.
    let mentioned = format!("// no_implicit_prelude is not set here\n{matched}");
    let mentioned_index = index(&[(LIB, &mentioned)]);
    assert!(establish(&mentioned_index, "lookup", "return Some(k + 2);").is_some());
    // Without the prelude, the same arm binds a fresh `None`.
    let no_prelude = format!("#![no_implicit_prelude]\n{matched}");
    let no_prelude_index = index(&[(LIB, &no_prelude)]);
    assert!(establish(&no_prelude_index, "lookup", "return Some(k + 2);").is_none());
    // An early `return None;` keeps its long-standing pin beside a glob the
    // binding scan cannot see through: only the `Err` gate this change
    // opened reads the other exits by spelling.
    let glob = "use crate::prelude::*;\n\npub fn whole(x: u64) -> Option<u64> {\n    if x == 0 { return None; }\n    Some(x)\n}\n";
    let glob_index = index(&[(LIB, glob)]);
    assert!(establish(&glob_index, "whole", "return None;").is_some());
}

#[test]
fn an_early_some_or_ok_return_is_pinned_beside_opposite_exits() {
    // RIPR-SPEC-0231 example 23: an early `return Some(..)` beside a `None`
    // tail is the only source of `Some`, so a `Some(..)` pin observes it.
    assert!(matches!(
        gate(
            "fn lookup(k: u32) -> Option<u32> {\n    if k == 1 {\n        return Some(1 + 2);\n    }\n    None\n}",
            "return Some(1 + 2);"
        ),
        Some(ReturnPathGate::EarlyHead("Some"))
    ));
    // A second `return None;` and a `?` build only `None`, so neither
    // competes with an early `Some`.
    assert!(matches!(
        gate(
            "fn f(k: u32) -> Option<u32> {\n    let j = k.checked_sub(1)?;\n    if j == 7 {\n        return None;\n    }\n    if j == 0 {\n        return Some(3);\n    }\n    None\n}",
            "return Some(3);"
        ),
        Some(ReturnPathGate::EarlyHead("Some"))
    ));
    // A `?` builds only `Err`, so it does not compete with an early `Ok`.
    assert!(matches!(
        gate(
            "fn f(x: i32) -> Result<i32, E> {\n    if x == 0 {\n        return Ok(1);\n    }\n    g(x)?;\n    Err(E::Late)\n}",
            "return Ok(1);"
        ),
        Some(ReturnPathGate::EarlyHead("Ok"))
    ));
    for (body, changed) in [
        // A second `return Some(..)` is another source of `Some`.
        (
            "fn f(k: u32) -> Option<u32> {\n    if k == 1 {\n        return Some(3);\n    }\n    if k == 2 {\n        return Some(4);\n    }\n    None\n}",
            "return Some(3);",
        ),
        // The tail can build `Some` too.
        (
            "fn f(k: u32) -> Option<u32> {\n    if k == 1 {\n        return Some(3);\n    }\n    Some(k)\n}",
            "return Some(3);",
        ),
        (
            "fn f(k: u32) -> Option<u32> {\n    if k == 1 {\n        return Some(3);\n    }\n    lookup(k)\n}",
            "return Some(3);",
        ),
        (
            "fn f(k: u32) -> Option<u32> {\n    if k == 1 {\n        return Some(3);\n    }\n    if k > 2 { Some(k) } else { None }\n}",
            "return Some(3);",
        ),
        // A path-qualified `None` is not read as the prelude variant.
        (
            "fn f(k: u32) -> Option<u32> {\n    if k == 1 {\n        return Some(3);\n    }\n    Option::None\n}",
            "return Some(3);",
        ),
        // Another `return` of an opaque value.
        (
            "fn f(x: i32) -> Result<i32, E> {\n    if x == 0 {\n        return Ok(1);\n    }\n    if x == 1 {\n        return cached();\n    }\n    Err(E::Late)\n}",
            "return Ok(1);",
        ),
        // A pin on one branch of a conditional `Some` never evaluates the other.
        (
            "fn f(k: u32) -> Option<u32> {\n    if k == 1 {\n        return Some(if k > 0 { 3 } else { 4 });\n    }\n    None\n}",
            "return Some(if k > 0 { 3 } else { 4 });",
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
fn a_non_plain_drop_on_the_returned_type_lends_no_pin() {
    // #8480: dropping the receiver while unwinding from a failed
    // assertion may exit 0 before the failure is reported.
    let lib = |drop_body: &str| {
        format!(
            "#[derive(Debug, PartialEq)]\npub struct Cfg {{\n    pub n: u32,\n}}\n\nimpl Drop for Cfg {{\n    fn drop(&mut self) {{{drop_body}\n    }}\n}}\n\npub fn make(x: u32) -> Cfg {{\n    Cfg {{ n: x * 3 }}\n}}\n"
        )
    };
    let tests = "use demo::make;\nuse demo::Cfg;\n\n#[test]\nfn makes() {\n    assert_eq!(make(4), Cfg { n: 12 });\n}\n";
    let admitted = |drop_body: &str| {
        let index = index(&[(LIB, &lib(drop_body)), (TESTS, tests)]);
        let pin = establish(&index, "make", "Cfg { n: x * 3 }");
        assert!(pin.is_some(), "the struct owner must establish a pin");
        pin.map(|pin| admitted_texts(&index, &pin).len())
            .unwrap_or_default()
    };
    assert_eq!(admitted("\n        std::process::exit(0);"), 0);
    assert_eq!(admitted(""), 1);
}

#[test]
fn a_local_non_plain_drop_guard_lends_a_pin() {
    // #8480 exit-position narrowing: a non-plain Drop record built only
    // in a local, non-returned position drops inside the owner, so it
    // must not refuse the pin for the plain record the owner returns.
    let lib = "#[derive(Debug, PartialEq)]\npub struct Cfg {\n    pub n: u32,\n}\n\nimpl Drop for Cfg {\n    fn drop(&mut self) {\n    }\n}\n\npub struct Guard {\n    pub n: u32,\n}\n\nimpl Drop for Guard {\n    fn drop(&mut self) {\n        self.n = self.n.wrapping_add(1);\n    }\n}\n\npub fn make(x: u32) -> Cfg { let _guard = Guard { n: 0 };\n    Cfg { n: x * 3 }\n}\n";
    let tests = "use demo::make;\nuse demo::Cfg;\n\n#[test]\nfn makes() {\n    assert_eq!(make(4), Cfg { n: 12 });\n}\n";
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "make", "Cfg { n: x * 3 }");
    assert!(pin.is_some(), "the struct owner must establish a pin");
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&index, &pin),
        vec!["assert_eq!(make(4), Cfg { n: 12 });".to_string()]
    );
}

#[test]
fn returned_types_drop_plain_scans_only_exit_positions() {
    // #8480 exit-position narrowing, direct owner-level table.
    let header = "#[derive(Debug, PartialEq)]\npub struct Cfg {\n    pub n: u32,\n}\n\nimpl Drop for Cfg {\n    fn drop(&mut self) {\n        std::process::exit(0);\n    }\n}\n\n";
    let verdict = |lib: &str| -> bool {
        let index = index(&[(LIB, lib)]);
        let owner = owner(&index, "make");
        OwnerReturnPin::returned_types_drop_plain(owner, &index)
    };
    // Local-only constructions: not exit positions.
    assert!(verdict(&format!(
        "{header}pub fn make(x: u32) -> u32 {{\n    let _guard = Cfg {{ n: 0 }};\n    x * 3\n}}\n"
    )));
    assert!(verdict(&format!(
        "{header}pub fn make(x: u32) -> u32 {{\n    let _ = Cfg {{ n: 0 }};\n    x * 3\n}}\n"
    )));
    assert!(verdict(&format!(
        "{header}fn sink(_c: Cfg) {{}}\npub fn make(x: u32) -> u32 {{\n    sink(Cfg {{ n: 0 }});\n    x * 3\n}}\n"
    )));
    assert!(verdict(&format!(
        "{header}pub fn make(x: u32) -> u32 {{\n    if (Cfg {{ n: 0 }}).n == 0 {{ x * 3 }} else {{ 0 }}\n}}\n"
    )));
    assert!(verdict(&format!(
        "{header}pub fn make(x: u32) -> u32 {{\n    loop {{ let _g = Cfg {{ n: 0 }}; break; }}\n    x * 3\n}}\n"
    )));
    // Exit positions: still demote.
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Cfg {{\n    Cfg {{ n: x * 3 }}\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Cfg {{\n    return Cfg {{ n: x * 3 }};\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Cfg {{\n    if x > 0 {{ Cfg {{ n: x * 3 }} }} else {{ Cfg {{ n: 0 }} }}\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Result<Cfg, ()> {{\n    Ok(Cfg {{ n: x * 3 }})\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Cfg {{\n    match x {{ _ => Cfg {{ n: x * 3 }} }}\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> (Cfg, u32) {{\n    (Cfg {{ n: x * 3 }}, 1)\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Cfg {{\n    (Cfg {{ n: x * 3 }})\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Cfg {{\n    let c = Cfg {{ n: x * 3 }};\n    c\n}}\n"
    )));
    assert!(!verdict(&format!(
        "{header}pub fn make(x: u32) -> Cfg {{\n    match x {{ _ => {{ Cfg {{ n: x * 3 }} }} }}\n}}\n"
    )));
}

#[test]
fn a_test_written_expected_file_lends_no_weight_pin() {
    // #8097: the test rewrites its expected file from the owner before
    // comparing, inline or through a plain (annotated) binding.
    for body in [
        "    std::fs::write(\"w.txt\", weight(4).to_string()).unwrap();\n    assert_eq!(weight(4), std::fs::read_to_string(\"w.txt\").unwrap().parse::<u32>().unwrap());",
        "    std::fs::write(\"w.txt\", weight(4).to_string()).unwrap();\n    let want: String = std::fs::read_to_string(\"w.txt\").unwrap();\n    assert_eq!(weight(4), want.parse::<u32>().unwrap());",
        "    std::fs::write(\"w.txt\", weight(4).to_string()).unwrap();\n    let want = std::fs::read_to_string(\"w.txt\").unwrap();\n    let got = want;\n    assert_eq!(weight(4), got.parse::<u32>().unwrap());",
    ] {
        let tests = format!("use demo::weight;\n\n#[test]\nfn weighs() {{\n{body}\n}}\n");
        assert!(weight_admitted(&tests).is_empty(), "{body}");
    }
    // A committed golden read with no writer still pins, and assigning a
    // file read into a *field* does not bind the plain local of the same
    // name.
    let tests = "use demo::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), std::fs::read_to_string(\"w.txt\").unwrap().parse::<u32>().unwrap());\n}\n";
    assert_eq!(
        weight_admitted(tests),
        vec![
            "assert_eq!(weight(4), std::fs::read_to_string(\"w.txt\").unwrap().parse::<u32>().unwrap());"
                .to_string()
        ]
    );
    let tests = "use demo::weight;\n\n#[test]\nfn weighs() {\n    std::fs::write(\"w.txt\", \"x\").unwrap();\n    let want = 12;\n    holder.want = std::fs::read_to_string(\"w.txt\").unwrap().parse::<u32>().unwrap();\n    assert_eq!(weight(4), want);\n}\n";
    assert_eq!(
        weight_admitted(tests),
        vec!["assert_eq!(weight(4), want);".to_string()]
    );
}

#[test]
fn a_constant_initialized_through_the_owner_is_not_an_independent_expected_value() {
    let test = |items: &str| {
        format!(
            "use demo::weight;\n{items}\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), EXPECTED);\n}}\n"
        )
    };
    // RIPR-SPEC-0035: the constant moves with the owner, directly, through
    // a `static`, or through another constant.
    for items in [
        "const EXPECTED: u32 = weight(4);",
        "static EXPECTED: u32 = weight(4);",
        "pub(crate) static mut EXPECTED: u32 = weight(4);",
        "const BASE: u32 = weight(2);\nconst EXPECTED: u32 = BASE * 2;",
        "const BASE: u32 = weight(2);\nconst MID: u32 = BASE + 1;\nconst EXPECTED: u32 = MID * 2;",
    ] {
        let tests = test(items);
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
    // The constant may live in another file, be renamed on import, name
    // the owner through a rename, or be declared by a macro.
    let across = |import: &str, operand: &str, other: &str| {
        let tests = format!(
            "use demo::weight;\n{import}\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), {operand});\n}}\n"
        );
        let index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests), ("src/other.rs", other)]);
        let pin = establish(&index, "weight", "x * 3");
        assert!(pin.is_some(), "the free owner must establish a pin");
        pin.map(|pin| admitted_texts(&index, &pin))
            .unwrap_or_default()
    };
    for (import, operand, other) in [
        (
            "use demo::other::EXPECTED;",
            "EXPECTED",
            "pub const EXPECTED: u32 = crate::weight(4);\n",
        ),
        (
            "use demo::other::E as EXPECTED;",
            "EXPECTED",
            "pub const E: u32 = crate::weight(4);\n",
        ),
        (
            "use demo::other::EXPECTED;",
            "EXPECTED",
            "use crate::weight as w;\npub const EXPECTED: u32 = w(4);\n",
        ),
        (
            "use demo::other::EXPECTED;",
            "EXPECTED",
            "use crate::base::BASE as B;\npub const EXPECTED: u32 = B;\nmod base {\n    pub const BASE: u32 = crate::weight(4);\n}\n",
        ),
        (
            "use demo::other::Table;",
            "Table::EXPECTED",
            "pub struct Table;\nimpl Table {\n    pub const EXPECTED: u32 = crate::weight(4);\n}\n",
        ),
        (
            "use demo::other::EXPECTED;",
            "EXPECTED.with(|e| *e)",
            "thread_local! {\n    pub static EXPECTED: u32 = crate::weight(4);\n}\n",
        ),
        (
            "use demo::other::EXPECTED;",
            "*EXPECTED",
            "lazy_static::lazy_static! {\n    pub static ref EXPECTED: u32 = crate::weight(4);\n}\n",
        ),
    ] {
        assert!(
            across(import, operand, other).is_empty(),
            "{import} {operand} {other}"
        );
    }
    // A constant built by some other type's same-named function, a method of
    // that name, or a literal stays an independent expected value.
    for other in [
        "pub const EXPECTED: u32 = 12;\n",
        "pub const EXPECTED: u32 = Table::weight(4);\n",
        "pub const EXPECTED: u32 = TABLE.weight(4);\n",
        "pub const EXPECTED: u32 = TABLE.weight;\n",
    ] {
        assert_eq!(
            across("use demo::other::EXPECTED;", "EXPECTED", other),
            vec!["assert_eq!(weight(4), EXPECTED);".to_string()],
            "{other}"
        );
    }
    let tests = test("const EXPECTED: u32 = 12; // not weight(4)");
    assert_eq!(
        weight_admitted(&tests),
        vec!["assert_eq!(weight(4), EXPECTED);".to_string()]
    );
}

#[test]
fn a_constant_initializer_reaches_the_owner_only_in_the_pins_call_shape() {
    let bare = PinCall::Bare;
    let associated = PinCall::Associated {
        type_name: "Table".to_string(),
    };
    let method = PinCall::Method {
        receivers: vec![ReceiverType::Named("Counter".to_string())],
        trait_scope: None,
    };
    for (initializer, shape, calls) in [
        // A free function: bare, module-path, turbofish and value uses.
        ("weight(4)", &bare, true),
        ("crate::weight(4)", &bare, true),
        ("super::weight (4)", &bare, true),
        ("weight::<u32>(4)", &bare, true),
        ("apply(weight, 4)", &bare, true),
        ("<T as Tr>::weight(4)", &bare, true),
        // Another type's function, a method, a field or a struct field.
        ("Table::weight(4)", &bare, false),
        ("Self::weight(4)", &bare, false),
        ("TABLE.weight(4)", &bare, false),
        ("TABLE.weight", &bare, false),
        ("Limits { weight: 4 }", &bare, false),
        // A non-ASCII qualifier is read whole, not split mid-character.
        ("módulo::weight(4)", &bare, true),
        ("Größe::weight(4)", &bare, false),
        ("Größe::weight(4)", &associated, false),
        // An associated function needs its own type, `Self` or an
        // unreadable `<..>` qualifier.
        ("Table::weight(4)", &associated, true),
        ("Self::weight(4)", &associated, true),
        ("Table::<u8>::weight(4)", &associated, true),
        ("Other::weight(4)", &associated, false),
        ("weight(4)", &associated, false),
        ("Table::weight", &associated, false),
        // A method needs a method call or its receiver's type.
        ("COUNTER.weight()", &method, true),
        ("Counter::weight(&COUNTER)", &method, true),
        ("Other::weight(1)", &method, false),
        ("weight(1)", &method, false),
    ] {
        assert_eq!(
            initializer_calls_owner(initializer, "weight", shape),
            calls,
            "{initializer}"
        );
    }
}

#[test]
fn text_declared_constants_read_macro_items_and_skip_non_declarations() {
    assert_eq!(
        text_declared_constants(
            "pub static ref EXP: u32 = quota(2); static mut r#M: u8 = 1; const FOO: u8 = 2;"
        ),
        vec![
            ("FOO".to_string(), ": u8 = 2".to_string()),
            ("EXP".to_string(), ": u32 = quota(2)".to_string()),
            ("M".to_string(), ": u8 = 1".to_string()),
        ]
    );
    assert_eq!(
        text_declared_constants(
            "extern { static X: u32; } static A: [u32; 2] = [quota(1); 2]; const Y: u8 = 1; \
             static B: Box<dyn Iterator<Item = u32>> = { let a = 2; quota(a) };"
        ),
        vec![
            ("Y".to_string(), ": u8 = 1".to_string()),
            ("A".to_string(), ": [u32; 2] = [quota(1); 2]".to_string()),
            (
                "B".to_string(),
                ": Box<dyn Iterator<Item = u32>> = { let a = 2; quota(a) }".to_string()
            ),
        ]
    );
    for text in [
        "extern { static X: u32; } const fn f() -> u32 { 1 }",
        "fn f(p: *const u8, s: &'static str) -> u8 { 0 }",
        "static ÉTAT: u32 = 1;",
    ] {
        assert!(text_declared_constants(text).is_empty(), "{text}");
    }
}

#[test]
fn a_method_owner_is_reached_only_through_a_method_shaped_constant() {
    let admitted = |other: &str| {
        let tests = "use demo::Counter;\nuse demo::other::EXPECTED;\n\n#[test]\nfn counts() {\n    let c = Counter::new();\n    assert_eq!(c.tally(), EXPECTED);\n}\n";
        let index = index(&[(LIB, COUNTER_LIB), (TESTS, tests), ("src/other.rs", other)]);
        let pin = establish(&index, "tally", "self.n + 1");
        assert!(pin.is_some(), "tally must establish a pin");
        pin.map(|pin| admitted_texts(&index, &pin).len())
            .unwrap_or_default()
    };
    assert_eq!(admitted("pub const EXPECTED: usize = 1;\n"), 1);
    assert_eq!(admitted("pub const EXPECTED: usize = tally(1);\n"), 1);
    assert_eq!(
        admitted("pub const EXPECTED: usize = Other::tally(1);\n"),
        1
    );
    assert_eq!(
        admitted("pub static EXPECTED: usize = crate::Counter::new().tally();\n"),
        0
    );
}

#[test]
fn a_constant_counts_only_from_a_package_the_test_belongs_to_tests_or_names() {
    let lib = "crates/core/src/lib.rs";
    let admitted = |import: &str, constant_file: &str, initializer: &str| {
        let tests = format!(
            "use demo::weight;\n{import}\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), EXPECTED);\n}}\n"
        );
        let constant = format!("pub const EXPECTED: u32 = {initializer};\n");
        let index = index(&[
            (lib, WEIGHT_LIB),
            ("crates/app/tests/weighs.rs", &tests),
            (constant_file, &constant),
        ]);
        let owner = index
            .functions()
            .iter()
            .find(|function| function.name == "weight" && function.file == Path::new(lib));
        assert!(owner.is_some(), "the owner must be indexed");
        let Some(owner) = owner else { return 0 };
        let pin = OwnerReturnPin::establish(&return_probe(owner, "x * 3"), owner, &index);
        assert!(pin.is_some(), "the free owner must establish a pin");
        pin.map(|pin| admitted_texts(&index, &pin).len())
            .unwrap_or_default()
    };
    for (import, constant_file) in [
        // The owner's package, the test's own package, and a named
        // test-support crate.
        ("use demo::EXPECTED;", "crates/core/src/expected.rs"),
        ("use common::EXPECTED;", "crates/app/tests/common.rs"),
        ("use test_kit::EXPECTED;", "crates/test-kit/src/lib.rs"),
    ] {
        // A literal constant there pins; one built by the owner does not.
        assert_eq!(admitted(import, constant_file, "12"), 1, "{constant_file}");
        assert_eq!(
            admitted(import, constant_file, "demo::weight(4)"),
            0,
            "{constant_file}"
        );
    }
    // A same-named constant in a package the test never names does not
    // withhold the pin from the test's own literal constant.
    assert_eq!(
        admitted(
            "const EXPECTED: u32 = 12;",
            "crates/other/src/lib.rs",
            "demo::weight(4)"
        ),
        1
    );
}

#[test]
fn the_reveal_side_reads_an_expected_name_only_through_one_readable_let_or_constant() {
    let value = |items: &str, body: &str, operand: &str| -> Option<String> {
        let tests = format!("use demo::weight;\n{items}\n#[test]\nfn weighs() {{\n{body}\n}}\n");
        let index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests)]);
        assert_eq!(index.tests().len(), 1, "fixture must hold exactly one test");
        let owner = index
            .functions()
            .iter()
            .find(|function| function.name == "weight");
        assert!(owner.is_some(), "the owner must be indexed");
        let owner = owner?;
        expected_operand_value(&index, index.tests().at(0), operand, owner)
            .map(|value| value.trim().to_string())
    };
    // RIPR-SPEC-0035: a readable `let`, a chain of them, or one constant
    // yields its value.
    assert_eq!(
        value("", "let e = weight(2);\nassert_eq!(weight(2), e);", "e"),
        Some("weight(2)".to_string())
    );
    assert_eq!(
        value(
            "",
            "let a = weight(2);\nlet b: u32 = a;\nassert_eq!(weight(2), b);",
            "b"
        ),
        Some("weight(2)".to_string())
    );
    assert_eq!(
        value(
            "const EXPECTED: u32 = weight(4);",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED"
        ),
        Some("weight(4)".to_string())
    );
    assert_eq!(
        value(
            "const EXPECTED: u32 = ::demo::weight(4);",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED"
        ),
        Some("::demo::weight(4)".to_string())
    );
    assert_eq!(
        value(
            "thread_local! { static EXPECTED: u32 = weight(4); }",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED"
        ),
        Some("weight(4)".to_string())
    );
    // #8112: a `lazy_static!` `static ref` of plain value type reads like
    // any other static.
    assert_eq!(
        value(
            "lazy_static! { static ref EXPECTED: u32 = weight(4); }",
            "assert_eq!(weight(4), *EXPECTED);",
            "EXPECTED"
        ),
        Some("weight(4)".to_string())
    );
    // #8112: the declared type decides, not words the initializer mentions:
    // a scalar static whose block merely names an atomic stays readable,
    // as does a scalar `let` and a `const` of atomic type (rebuilt per use).
    assert_eq!(
        value(
            "static EXPECTED: u32 = { let _t = AtomicUsize::new(0); weight(4) };",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED"
        ),
        Some("{ let _t = AtomicUsize::new(0); weight(4) }".to_string())
    );
    assert_eq!(
        value(
            "",
            "let e: u32 = { let _t = AtomicUsize::new(0); weight(2) };\nassert_eq!(weight(2), e);",
            "e"
        ),
        Some("{ let _t = AtomicUsize::new(0); weight(2) }".to_string())
    );
    // A grouped `use` import of `u32` may rebind it to a user type: not
    // followed; the same file without the import is followed.
    assert_eq!(
        value(
            "use crate::types::{Counter, u32};\nstatic EXPECTED: u32 = Counter(AtomicUsize::new(weight(4)));",
            "assert_eq!(weight(4), EXPECTED.0);",
            "EXPECTED"
        ),
        None
    );
    assert_eq!(
        value(
            "use crate::types::Counter;\nstatic EXPECTED: u32 = { let _t = AtomicUsize::new(0); weight(4) };",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED"
        ),
        Some("{ let _t = AtomicUsize::new(0); weight(4) }".to_string())
    );
    // A plain cast `n as u32` elsewhere in the file does not rebind `u32`.
    assert_eq!(
        value(
            "fn narrow(n: u64) -> u32 { n as u32 }\nstatic EXPECTED: u32 = { let _t = AtomicUsize::new(0); weight(4) };",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED"
        ),
        Some("{ let _t = AtomicUsize::new(0); weight(4) }".to_string())
    );
    assert_eq!(
        value(
            "const EXPECTED: AtomicUsize = AtomicUsize::new(weight(4));",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED"
        ),
        Some("AtomicUsize::new(weight(4))".to_string())
    );
    // An operand that is not a plain name, a name the test binds twice or
    // outside a `let` (a loop variable), the owner's own name, and a name
    // nothing binds yield nothing to read.
    for (items, body, operand) in [
        (
            "",
            "let e = weight(2);\nassert_eq!(weight(2), e + 1);",
            "e + 1",
        ),
        (
            "",
            "let e = weight(2);\nassert_eq!(weight(2), fixture.e);",
            "fixture.e",
        ),
        (
            "",
            "let e = weight(2);\nlet e = 6;\nassert_eq!(weight(2), e);",
            "e",
        ),
        ("", "for e in [6] {\n    assert_eq!(weight(2), e);\n}", "e"),
        (
            "",
            "let weight = 6;\nassert_eq!(super::weight(2), weight);",
            "weight",
        ),
        ("", "assert_eq!(weight(2), EXPECTED);", "EXPECTED"),
        // A `mut` binding may change after its `let`; an attribute may
        // compile the `let` away; two same-named constants are ambiguous.
        (
            "",
            "let mut e = weight(2);\ne = 6;\nassert_eq!(weight(2), e);",
            "e",
        ),
        (
            "",
            "#[cfg(any())]\nlet e = weight(2);\nassert_eq!(weight(2), e);",
            "e",
        ),
        (
            "const EXPECTED: u32 = weight(4);\nmod other { pub const EXPECTED: u32 = 12; }",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED",
        ),
        // #8112: an interior-mutable `let` and a `static mut` may hold a
        // value other than their initializer at the assertion.
        (
            "",
            "let e = std::cell::Cell::new(weight(2));\ne.set(6);\nassert_eq!(std::cell::Cell::new(weight(2)), e);",
            "e",
        ),
        (
            "",
            "let e = std::rc::Rc::new(RefCell::new(weight(2)));\nassert_eq!(weight(2), e);",
            "e",
        ),
        (
            "",
            "let e = std::sync::Mutex::new(weight(2));\nassert_eq!(weight(2), e);",
            "e",
        ),
        (
            "",
            "let e = std::sync::atomic::AtomicUsize::new(weight(2));\nassert_eq!(weight(2), e);",
            "e",
        ),
        (
            "static EXPECTED: AtomicUsize = AtomicUsize::new(weight(4));",
            "EXPECTED.store(6, Ordering::SeqCst);\nassert_eq!(weight(4), EXPECTED);",
            "EXPECTED",
        ),
        (
            "lazy_static! { static ref EXPECTED: AtomicUsize = AtomicUsize::new(weight(4)); }",
            "EXPECTED.store(6, Ordering::SeqCst);\nassert_eq!(weight(4), EXPECTED);",
            "EXPECTED",
        ),
        // #8112: the declared type refuses even when the initializer text
        // shows no interior mutability.
        (
            "const ZERO: usize = 0;\nstatic EXPECTED: AtomicUsize = ZERO;",
            "EXPECTED.store(6, Ordering::SeqCst);\nassert_eq!(weight(4), EXPECTED);",
            "EXPECTED",
        ),
        // `make()` is intentionally opaque: these refuse from the declared type.
        (
            "static EXPECTED: Cell<u32> = make();",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED",
        ),
        (
            "const INIT: AtomicUsize = AtomicUsize::new(0);\nstatic EXPECTED: AtomicUsize = INIT;",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED",
        ),
        (
            "lazy_static! { static ref EXPECTED: Mutex<u32> = make(); }",
            "assert_eq!(weight(4), *EXPECTED.lock().unwrap());",
            "EXPECTED",
        ),
        (
            "",
            "let e: Cell<u32> = make();\nassert_eq!(weight(2), e);",
            "e",
        ),
        // A file that rebinds a primitive name no longer proves a scalar.
        (
            "struct Counter(AtomicUsize);\ntype u32 = Counter;\nstatic EXPECTED: u32 = Counter(AtomicUsize::new(weight(4)));",
            "assert_eq!(weight(4), EXPECTED.0);",
            "EXPECTED",
        ),
        (
            "struct Counter(AtomicUsize);\ntype u32 = Counter;",
            "let e: u32 = Counter(AtomicUsize::new(weight(2)));\nassert_eq!(weight(2), e);",
            "e",
        ),
        // A user type holding an atomic is refused only when its type or
        // initializer names a known interior-mutable type (documented limit).
        (
            "static EXPECTED: Counter = Counter::new(AtomicUsize::new(weight(4)));",
            "assert_eq!(weight(4), EXPECTED);",
            "EXPECTED",
        ),
        (
            "static mut EXPECTED: u32 = weight(4);",
            "unsafe { EXPECTED = 6; }\nassert_eq!(weight(4), unsafe { EXPECTED });",
            "EXPECTED",
        ),
    ] {
        assert_eq!(value(items, body, operand), None, "{body}");
    }
}

#[test]
fn serial_test_locks_keep_the_pin_and_other_attributes_refuse_it() {
    for attributes in [
        "use serial_test::serial;\n#[test]\n#[serial]",
        "use serial_test::serial;\n#[serial]\n#[test]",
        "#[test]\n#[serial_test::serial(env)]",
        "#[test]\n#[serial_test::file_serial]",
        "use serial_test::parallel;\n#[test]\n#[parallel]",
        "use serial_test::{parallel, serial};\n#[test]\n#[serial]",
        "use ::serial_test::serial as locked;\n#[test]\n#[locked]",
        "use serial_test::file_serial;\n#[test]\n#[file_serial(db)]",
        "use serial_test::serial;\n#[derive(Debug, Clone)]\nstruct Fixture;\n#[test]\n#[serial]",
        "extern crate serial_test;\n#[test]\n#[serial_test::serial]",
        "use serial_test;\n#[test]\n#[serial_test::serial]",
        // `macro_rules!` alone emits nothing at item position.
        "macro_rules! helper { () => {}; }\n#[test]\n#[serial_test::serial]",
        // A rooted path names the same crate.
        "#[test]\n#[::serial_test::serial]",
        "#[::serial_test::serial]\nfn helper() {}\n#[test]\n#[serial_test::serial]",
        // A local `tokio` matters only to a module that uses tokio's attribute.
        "mod tokio {}\n#[test]\n#[serial_test::serial]",
    ] {
        let tests = format!(
            "use demo::weight;\n{attributes}\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert_eq!(weight_admitted(&tests).len(), 1, "{tests}");
    }
    for attributes in [
        // A bare `serial` with no serial_test in the file may be any macro.
        "#[test]\n#[serial]",
        "#[test]\n#[other::serial]",
        "#[test]\n#[ignore]",
        "#[test]\n#[serial_test::serial]\n#[ignore]",
        "#[test]\n#[serial_test::unknown_lock]",
        "#[test]\n#[::serial_test::unknown_lock]",
        "#[test]\n#[::other::serial]",
        // The lock can hand the body to other macros or another runtime.
        "#[test]\n#[serial_test::serial(inner_attrs = [my_macros::skip_body])]",
        "#[test]\n#[serial_test::serial(crate = my_runtime)]",
        "#[test]\n#[serial_test::file_serial(path => \"/tmp/lock\")]",
        // A mention outside a `use` item does not import the macro.
        "// serial_test is not used here\nuse my_macros::skip_body as serial;\n#[test]\n#[serial]",
        // Importing another serial_test item does not bind `serial`.
        "use serial_test::parallel;\nuse my_macros::serial;\n#[test]\n#[serial]",
        "use serial_test::parallel;\n#[test]\n#[serial]",
        // A competing binding of the same name in the module refuses,
        // raw spellings included.
        "use serial_test::serial;\nuse my_macros::skip_body as serial;\n#[test]\n#[serial]",
        "use serial_test::serial;\nuse my_macros::r#serial;\n#[test]\n#[serial]",
        // Only the test's own module binds its attribute: an import in a
        // function body, a sibling module or a macro token tree does not.
        "mod a { use serial_test::serial; }\nuse my_macros::serial as r#serial;\n#[test]\n#[serial]",
        "use my_macros::r#serial;\nfn helper() { use serial_test::serial; }\n#[test]\n#[serial]",
        "macro_rules! bring { () => { use my_macros::serial; }; }\nbring!();\nfn helper() { use serial_test::serial; }\n#[test]\n#[serial]",
        // A local `serial_test` shadows the crate for a qualified lock.
        "mod serial_test { pub use my_macros::serial; }\n#[test]\n#[serial_test::serial]",
        "use my_macros as serial_test;\n#[test]\n#[serial_test::serial]",
        "extern crate my_macros as serial_test;\n#[test]\n#[serial_test::serial]",
        "extern crate my_macros as serial_test;\n#[test]\n#[::serial_test::serial]",
        // A glob, an item macro or an attribute or derive macro may bring
        // one.
        "use crate::common::*;\n#[test]\n#[serial_test::serial]",
        "bring!();\n#[test]\n#[serial_test::serial]",
        "#[my_macros::emit_mod]\nfn helper() {}\n#[test]\n#[serial_test::serial]",
        "#[derive(my_macros::EmitMod)]\nstruct Marker;\n#[test]\n#[serial_test::serial]",
        "#[cfg_attr(all(), my_macros::emit_mod)]\nfn helper() {}\n#[test]\n#[serial_test::serial]",
        "use my_macros::serial;\n#[serial]\nfn helper() {}\n#[test]\n#[serial_test::serial]",
        // An import can rebind a prelude derive or `derive` itself.
        "use my_macros::EmitMod as Debug;\n#[derive(Debug)]\nstruct Marker;\n#[test]\n#[serial_test::serial]",
        "use my_macros::emit_mod as derive;\n#[derive(Debug)]\nstruct Marker;\n#[test]\n#[serial_test::serial]",
        "use serial_test::serial;\nuse my_macros::EmitMod as Debug;\n#[derive(Debug)]\nstruct Marker;\n#[test]\n#[serial]",
        "extern crate my_macros as Debug;\n#[test]\n#[serial_test::serial]",
        // The bare import's `serial_test` must be the crate too, and an
        // out-of-line module's `super::*` may bring another one.
        "use super::*;\nuse serial_test::serial;\n#[test]\n#[serial]",
        "mod serial_test { pub use my_macros::serial; }\nuse serial_test::serial;\n#[test]\n#[serial]",
        "extern crate my_macros as serial_test;\nuse serial_test::serial;\n#[test]\n#[serial]",
        // A foreign glob may also bind the owner name.
        "use serial_test::serial;\nuse my_macros::*;\n#[test]\n#[serial]",
        "use serial_test::serial as _;\n#[test]\n#[serial]",
        "use serial_test::*;\n#[test]\n#[serial]",
    ] {
        let tests = format!(
            "use demo::weight;\n{attributes}\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
}

#[test]
fn a_rebound_test_attribute_refuses_the_pin_and_the_builtin_keeps_it() {
    for items in [
        "",
        "use super::*;",
        // A glob of the workspace's own crate, or of its own modules.
        "use demo::*;",
        "use crate::common::*;",
        "use serial_test::serial;",
        "use my_macros::skip_body as other_name;",
        // An import in a function body or a sibling module binds nothing here.
        "mod a { use my_macros::skip_body as test; }",
        "fn helper() { use my_macros::skip_body as test; }",
    ] {
        let tests = format!(
            "use demo::weight;\n{items}\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert_eq!(weight_admitted(&tests).len(), 1, "{tests}");
    }
    for items in [
        // The issue's repro, raw and plain spellings, and a re-export.
        "use skipper::serial as test;",
        "use skipper::serial as r#test;",
        "use skipper::test;",
        "use skipper::{serial as test, other};",
        "use std::prelude::v1::test;",
        // A glob of a crate outside the workspace may bring one, and so may
        // a prelude glob of std.
        "use skipper::*;",
        "use std::prelude::v1::*;",
        "extern crate skipper as test;",
    ] {
        let tests = format!(
            "use demo::weight;\n{items}\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
    // A macro-generated test's module is not in the file's text, so the
    // file's own rebinding answers for it.
    for (outer, refused) in [
        ("use skipper::serial as test;", true),
        ("extern crate skipper as test;", true),
        ("", false),
    ] {
        let tests = format!(
            "use demo::weight;\n{outer}\nmacro_rules! generate {{\n    ($name:ident) => {{\n        #[test]\n        fn $name() {{\n            assert_eq!(weight(4), 12);\n        }}\n    }};\n}}\ngenerate!(weighs);\n"
        );
        let mut index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests)]);
        crate::analysis::facts::register_macro_generated_tests_for_test(&mut index);
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(TESTS));
        let assertion = test.and_then(|test| test.assertions.first());
        assert!(assertion.is_some(), "{tests}");
        let (Some(test), Some(assertion)) = (test, assertion) else {
            continue;
        };
        let refusal = OwnerPinSyntax::default().refusal(test, assertion, &index);
        assert_eq!(refusal.is_some(), refused, "{tests}");
    }
    // `super::*` reaches the enclosing module's private imports, however
    // many levels up.
    let tests = "use demo::weight;\nuse skipper::serial as test;\nmod outer {\n    mod tests {\n        use super::super::*;\n        #[test]\n        fn weighs() {\n            assert_eq!(weight(4), 12);\n        }\n    }\n}\n";
    assert!(weight_admitted(tests).is_empty(), "{tests}");
    // `super::*` reaches the enclosing module's private imports.
    for (outer, admitted) in [
        ("use skipper::serial as test;", false),
        ("use skipper::*;", false),
        ("use skipper::serial;", true),
    ] {
        let tests = format!(
            "use demo::weight;\n{outer}\nmod tests {{\n    use super::*;\n    #[test]\n    fn weighs() {{\n        assert_eq!(weight(4), 12);\n    }}\n}}\n"
        );
        assert_eq!(
            weight_admitted(&tests).len(),
            usize::from(admitted),
            "{tests}"
        );
    }
}

#[test]
fn a_tokio_test_runtime_runs_an_async_test_body_and_other_async_tests_refuse() {
    for attributes in [
        "#[tokio::test]",
        "#[tokio::test(flavor = \"multi_thread\", worker_threads = 2)]",
        "#[tokio::test(flavor = \"current_thread\", start_paused = true)]",
        "#[tokio::test]\n#[serial_test::serial]",
        "extern crate tokio;\n#[tokio::test]",
    ] {
        let tests = format!(
            "use demo::weight;\n{attributes}\nasync fn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert_eq!(weight_admitted(&tests).len(), 1, "{tests}");
    }
    for attributes in [
        // Another crate's runtime path, and arguments that change what runs.
        "#[test]",
        "#[async_std::test]",
        "#[tokio::test(crate = \"other\")]",
        "#[tokio::test(flavor = \"current_thread\", unhandled_panic = \"shutdown_runtime\")]",
        "#[tokio::test(flavor = some_flavor())]",
        "#[tokio::test(flavor = \"current_thread\", start_paused = true, unhandled_panic = \"shutdown_runtime\")]",
        // A local `tokio` shadows the crate.
        "mod tokio { pub use other::test; }\n#[tokio::test]",
        "use other as tokio;\n#[tokio::test]",
        // A shadowed serial_test lock elsewhere in the module may emit `tokio`.
        "mod serial_test { pub use other::serial; }\n#[serial_test::serial]\nfn helper() {}\n#[tokio::test]",
        "mod tokio { pub use other::test; }\n#[tokio::test]\n#[serial_test::serial]",
        "#[tokio::test]\n#[ignore]",
    ] {
        let tests = format!(
            "use demo::weight;\n{attributes}\nasync fn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
    // A bare `#[case]` that no `#[rstest]` consumes is an unknown attribute
    // macro, which may emit a `tokio` or `serial_test` module.
    for attributes in [
        "use other::case;\n#[case]\nfn decoy() {}\n#[tokio::test]",
        "#[case]\nfn decoy() {}\n#[tokio::test]",
        "#[case::row]\nstruct Decoy;\n#[tokio::test]\n#[serial_test::serial]",
    ] {
        let tests = format!(
            "use demo::weight;\n{attributes}\nasync fn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
    // A sync `fn` under `#[tokio::test]` does not compile.
    let tests =
        "use demo::weight;\n#[tokio::test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    assert_eq!(
        index.tests().len(),
        1,
        "the sync tokio test must be indexed"
    );
    assert!(weight_admitted(tests).is_empty(), "{tests}");
}

#[test]
fn an_rstest_case_table_with_literal_rows_runs_the_body_once_per_row() {
    let case_test = |prelude: &str, attributes: &str, parameters: &str, name: &str| {
        format!(
            "use demo::weight;\n{prelude}\n{attributes}\nfn {name}({parameters}) {{\n    assert_eq!(weight(x), expected);\n}}\n"
        )
    };
    let cases = "#[case] x: u32, #[case] expected: u32";
    for (prelude, attributes) in [
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]\n#[case(5, 15)]",
        ),
        (
            "use rstest::{fixture, rstest};",
            "#[rstest]\n#[case::four(4, 12)]",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]\n#[case(5, 15)]\n#[serial_test::serial]",
        ),
    ] {
        let tests = case_test(prelude, attributes, cases, "weighs");
        assert_eq!(weight_admitted(&tests).len(), 1, "{tests}");
    }
    // A parameterless `#[rstest]` runs its body once, as `#[test]` does.
    let tests = "use demo::weight;\nuse rstest::rstest;\n#[rstest]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    assert_eq!(weight_admitted(tests).len(), 1, "{tests}");
    for (prelude, attributes, parameters, name) in [
        // A row value that is not a literal may compute the owner itself.
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, weight(4))]",
            cases,
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 3 * 4)]",
            cases,
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4)]",
            cases,
            "weighs",
        ),
        // A row above `#[rstest]` is not consumed by it.
        (
            "use rstest::rstest;",
            "#[case(4, 12)]\n#[rstest]",
            cases,
            "weighs",
        ),
        // Arguments, value matrices and other attributes.
        (
            "use rstest::rstest;",
            "#[rstest(foo)]\n#[case(4, 12)]",
            cases,
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[rstest]\n#[case(4, 12)]",
            cases,
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, \"12\")]",
            cases,
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]",
            "#[case] ref x: u32, #[case] expected: u32",
            "weighs",
        ),
        // A row must hold one value per `#[case]` parameter; the unmarked
        // `expected` is a fixture, so this row has one value too many.
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]",
            "#[case] x: u32, expected: u32",
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]",
            "#[values(4, 5)] x: u32, #[case] expected: u32",
            "weighs",
        ),
        // Fixtures may not supply the expected value.
        (
            "use rstest::rstest;",
            "#[rstest]",
            "x: u32, expected: u32",
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]\n#[ignore]",
            cases,
            "weighs",
        ),
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]\n#[timeout(std::time::Duration::from_secs(1))]",
            cases,
            "weighs",
        ),
        // `rstest` is not the crate, or the bare name is another macro.
        (
            "use other::rstest;",
            "#[rstest]\n#[case(4, 12)]",
            cases,
            "weighs",
        ),
        (
            "use rstest::*;",
            "#[rstest]\n#[case(4, 12)]",
            cases,
            "weighs",
        ),
        (
            "use rstest::rstest;\nuse rstest::fixture as case;",
            "#[rstest]\n#[case(4, 12)]",
            cases,
            "weighs",
        ),
        (
            "mod rstest { pub use other::rstest; }\nuse rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]",
            cases,
            "weighs",
        ),
        (
            "use other as rstest;\nuse rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]",
            cases,
            "weighs",
        ),
        // A test named after a checked crate expands to a module that
        // shadows it.
        (
            "use rstest::rstest;",
            "#[rstest]\n#[case(4, 12)]",
            cases,
            "rstest",
        ),
        (
            "use rstest::rstest;\nuse serial_test::serial;",
            "#[rstest]\n#[case(4, 12)]\n#[serial]",
            cases,
            "serial_test",
        ),
    ] {
        let tests = case_test(prelude, attributes, parameters, name);
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
}

/// #6578: an unmarked `#[rstest]` parameter is a fixture, filled once by
/// the same-named `#[fixture]` before the body runs. It may be the owner's
/// argument or receiver (typed by its annotation), never the expected value:
/// the fixture body sits outside the test and may call the owner itself.
#[test]
fn an_rstest_fixture_feeds_the_call_but_never_the_expected_value() {
    let fixture_test = |fixtures: &str, parameters: &str, body: &str| {
        format!(
            "use demo::weight;\nuse rstest::{{fixture, rstest}};\n{fixtures}\n#[rstest]\nfn weighs({parameters}) {{\n    {body}\n}}\n"
        )
    };
    let four = "#[fixture]\nfn x() -> u32 {\n    4\n}";
    let want = "#[fixture]\nfn want() -> u32 {\n    weight(4)\n}";
    for (rows, parameters, body) in [
        ("", "x: u32", "assert_eq!(weight(x), 12);"),
        ("", "mut x: u32", "assert_eq!(weight(x), 12);"),
        // A fixture beside `#[case]` parameters: rows bind only the cases.
        (
            "#[case(12)]\n",
            "#[case] expected: u32, x: u32",
            "assert_eq!(weight(x), expected);",
        ),
    ] {
        let tests = fixture_test(four, parameters, body)
            .replace("#[rstest]\n", &format!("#[rstest]\n{rows}"));
        assert_eq!(weight_admitted(&tests).len(), 1, "{tests}");
    }
    for (parameters, body) in [
        // The expected value is a fixture, directly or through a `let`.
        ("want: u32", "assert_eq!(weight(4), want);"),
        ("want: u32", "let w = want;\n    assert_eq!(weight(4), w);"),
        ("want: u32", "assert_eq!(want, weight(4));"),
        // Any body binding of an expected word fails closed beside a
        // fixture: an assignment, a mutation, a loop or a rebound `#[case]`.
        (
            "want: u32",
            "let mut w = 0;\n    w = want;\n    assert_eq!(weight(4), w);",
        ),
        (
            "want: u32",
            "let mut v = Vec::new();\n    v.push(want);\n    assert_eq!(weight(4), v[0]);",
        ),
        (
            "want: u32",
            "for w in [want] {\n        assert_eq!(weight(4), w);\n    }",
        ),
        ("x: u32", "let w = 12;\n    assert_eq!(weight(x), w);"),
        // Parameter attributes that change what or how often the body runs.
        ("#[from(want)] x: u32", "assert_eq!(weight(x), 12);"),
        ("#[with(4)] x: u32", "assert_eq!(weight(x), 12);"),
        ("#[future] x: u32", "assert_eq!(weight(x), 12);"),
        ("#[values(4)] x: u32", "assert_eq!(weight(x), 12);"),
        ("#[by_ref] x: &u32", "assert_eq!(weight(x), 12);"),
        // An anonymous type parameter rstest may infer from the fixture.
        ("x: impl Into<u32>", "assert_eq!(weight(x.into()), 12);"),
        // Patterns other than a plain binding.
        ("ref x: u32", "assert_eq!(weight(x), 12);"),
        ("(x, _y): (u32, u32)", "assert_eq!(weight(x), 12);"),
    ] {
        let tests = fixture_test(&format!("{four}\n{want}"), parameters, body);
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
    // A `mut` `#[case]` parameter the body reassigns from a fixture or the
    // owner: an assignment binds nothing (#6578 review).
    for (fixtures, parameters, body) in [
        (
            want,
            "#[case] mut expected: u32, want: u32",
            "expected = want;\n    assert_eq!(weight(4), expected);",
        ),
        (
            "",
            "#[case] mut expected: u32",
            "expected = weight(4);\n    assert_eq!(weight(4), expected);",
        ),
    ] {
        let tests = fixture_test(fixtures, parameters, body)
            .replace("#[rstest]\n", "#[rstest]\n#[case(0)]\n");
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }
    // A `#[case]` parameter the body rebinds from a fixture.
    let tests = fixture_test(
        want,
        "#[case] expected: u32, want: u32",
        "let expected = want;\n    assert_eq!(weight(4), expected);",
    )
    .replace("#[rstest]\n", "#[rstest]\n#[case(0)]\n");
    assert!(weight_admitted(&tests).is_empty(), "{tests}");
    // A generic test may infer the annotated type from the fixture.
    for generics in ["<T: Copy>", ""] {
        let tests = fixture_test(four, "x: u32", "assert_eq!(weight(x), 12);")
            .replace("fn weighs(", &format!("fn weighs{generics}("));
        let tests = if generics.is_empty() {
            tests.replace(") {\n    assert", ") where u32: Copy {\n    assert")
        } else {
            tests
        };
        assert!(weight_admitted(&tests).is_empty(), "{tests}");
    }

    // A fixture receiver is typed by its annotation, as
    // `let c: Counter = ..;` is.
    let counter = |parameters: &str, body: &str| {
        let tests = format!(
            "use demo::Counter;\nuse rstest::{{fixture, rstest}};\n#[fixture]\nfn c() -> Counter {{\n    Counter::new()\n}}\n#[rstest]\nfn counts({parameters}) {{\n    {body}\n}}\n"
        );
        let index = index(&[(LIB, COUNTER_LIB), (TESTS, &tests)]);
        let pin = establish(&index, "tally", "self.n + 1");
        assert!(pin.is_some(), "tally must establish a pin");
        let Some(pin) = pin else { return 0 };
        admitted_texts(&index, &pin).len()
    };
    assert_eq!(counter("c: Counter", "assert_eq!(c.tally(), 1);"), 1);
    assert_eq!(counter("mut c: Counter", "assert_eq!(c.tally(), 1);"), 1);
    // A borrowed or rebound receiver is not typed by the annotation.
    assert_eq!(counter("c: &Counter", "assert_eq!(c.tally(), 1);"), 0);
    // The annotation, not the fixture or the method name, types the call.
    assert_eq!(counter("c: Other", "assert_eq!(c.tally(), 1);"), 0);
    assert_eq!(counter("c: Counter", "assert_eq!(c.tally(), c.n + 1);"), 0);
    assert_eq!(
        counter(
            "c: Counter",
            "let c = Other::new();\n    assert_eq!(c.tally(), 1);"
        ),
        0
    );
    assert_eq!(
        counter(
            "c: Counter",
            "if let Some(c) = None::<Other> {}\n    assert_eq!(c.tally(), 1);"
        ),
        0
    );
}

#[test]
fn an_async_owner_is_read_through_one_await_and_a_sync_owner_is_not() {
    let lib = "pub async fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    let tests = "use demo::weight;\n#[tokio::test]\nasync fn weighs() {\n    assert_eq!(weight(4).await, 12);\n    assert_eq!(weight(4), 12);\n    assert_eq!(weight(4).await.await, 12);\n    assert_eq!(async { weight(4).await }.await, 12);\n    let heavy = weight(4).await;\n    assert_eq!(heavy, 12);\n    let pending = weight(5);\n    assert_eq!(pending, 15);\n}\n";
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(pin.is_some(), "the async owner must establish a pin");
    let Some(pin) = pin else {
        return;
    };
    assert_eq!(
        admitted_texts(&index, &pin),
        vec![
            "assert_eq!(weight(4).await, 12);".to_string(),
            "assert_eq!(heavy, 12);".to_string()
        ]
    );
    // `async` in a comment does not make the owner async.
    let lib = "/* async */ pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    let tests = "use demo::weight;\n#[tokio::test]\nasync fn weighs() {\n    assert_eq!(weight(4).await, 12);\n}\n";
    let index = self::index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(pin.is_some(), "the sync owner must establish a pin");
    if let Some(pin) = pin {
        assert!(admitted_texts(&index, &pin).is_empty());
    }
    // A sync owner's `.await` is a different value (an `IntoFuture` impl).
    let tests = "use demo::weight;\n#[tokio::test]\nasync fn weighs() {\n    assert_eq!(weight(4).await, 12);\n}\n";
    assert!(weight_admitted(tests).is_empty(), "{tests}");
}

#[test]
fn serial_test_locks_resolve_in_the_tests_own_module() {
    let module = |body: &str| {
        format!(
            "use demo::weight;\nmod tests {{\n{body}\n#[test]\n#[serial_test::serial]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n}}\n"
        )
    };
    // `use super::*` reaches an enclosing module of this file, which binds
    // no `serial_test`.
    assert_eq!(weight_admitted(&module("use super::*;")).len(), 1);
    // The common inline test module: `#[cfg(test)]` emits no items.
    let common = "use demo::weight;\n#[cfg(test)]\nmod tests {\nuse super::*;\nuse serial_test::serial;\n#[test]\n#[serial]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n}\n";
    assert_eq!(weight_admitted(common).len(), 1, "{common}");
    let tests = "use demo::weight;\nmod serial_test { pub use my_macros::serial; }\nmod tests {\nuse super::*;\n#[test]\n#[serial_test::serial]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n}\n";
    assert!(weight_admitted(tests).is_empty(), "{tests}");
    let bare = "use demo::weight;\nuse serial_test::serial;\nmod tests {\nuse super::*;\n#[test]\n#[serial]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n}\n";
    let shadowed = "use demo::weight;\nmod serial_test { pub use my_macros::serial; }\nmod tests {\nuse super::*;\nuse serial_test::serial;\n#[test]\n#[serial]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n}\n";
    assert!(weight_admitted(shadowed).is_empty(), "{shadowed}");
    let rebound = "use demo::weight;\nuse my_macros::EmitMod as Debug;\n#[derive(Debug)]\nstruct Marker;\nmod tests {\nuse super::*;\n#[test]\n#[serial_test::serial]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n}\n";
    assert!(weight_admitted(rebound).is_empty(), "{rebound}");
    // `#[macro_use]` can rebind `Debug` with no import; the file-wide
    // macro-binding scan refuses it.
    let macro_use = "#[macro_use]\nextern crate my_macros;\nuse demo::weight;\nmod tests {\nuse super::*;\n#[derive(Debug)]\nstruct Marker;\n#[test]\n#[serial_test::serial]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n}\n";
    assert!(weight_admitted(macro_use).is_empty(), "{macro_use}");
    // The bare lock is bound only through the glob, which proves nothing.
    assert!(weight_admitted(bare).is_empty(), "{bare}");
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

#[test]
fn a_cfg_gated_owner_is_not_reached_by_a_bare_call() {
    // #7082: under the complementary cfg the bare name reaches the
    // same-named `static`, not the owner.
    let tests = "use demo::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let lib = "pub fn helper(_: u32) -> u32 {\n    12\n}\n#[cfg(feature = \"alt\")]\n#[allow(non_upper_case_globals)]\npub static weight: fn(u32) -> u32 = helper;\n#[cfg(not(feature = \"alt\"))]\npub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    let ix = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&ix, "weight", "x * 3");
    assert!(pin.is_some(), "the owner itself must establish");
    if let Some(pin) = pin {
        assert!(admitted_texts(&ix, &pin).is_empty());
    }
    // An enclosing inline module's cfg gates it the same way.
    let lib = "#[cfg(not(feature = \"alt\"))]\npub mod scale {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n}\n";
    let tests =
        "use demo::scale::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let ix = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&ix, "weight", "x * 3");
    assert!(pin.is_some(), "the nested owner must establish");
    if let Some(pin) = pin {
        assert!(admitted_texts(&ix, &pin).is_empty());
        // Control: the same module without the cfg pins.
        let lib = lib.replace("#[cfg(not(feature = \"alt\"))]\n", "");
        let ix = index(&[(LIB, &lib), (TESTS, tests)]);
        let pin = establish(&ix, "weight", "x * 3");
        assert!(pin.is_some());
        if let Some(pin) = pin {
            assert_eq!(admitted_texts(&ix, &pin), ["assert_eq!(weight(4), 12);"]);
        }
    }
    // The unit-test form: `#[cfg(test)]` swaps in the twin.
    let twin = "pub fn helper(_: u32) -> u32 {\n    12\n}\n#[cfg(test)]\n#[allow(non_upper_case_globals)]\npub static weight: fn(u32) -> u32 = helper;\n\n";
    let unit = unit_tests(twin, "assert_eq!(weight(4), 12);")
        .replace("pub fn weight", "#[cfg(not(test))]\npub fn weight")
        .replace("mod tests {\n", "mod tests {\n    use super::*;\n");
    assert!(
        unit.contains("#[cfg(not(test))]\npub fn weight") && unit.contains("use super::*;"),
        "{unit}"
    );
    let ix = index(&[(LIB, &unit)]);
    let pin = establish(&ix, "weight", "x * 3");
    assert!(pin.is_some());
    if let Some(pin) = pin {
        assert!(admitted_texts(&ix, &pin).is_empty());
    }
    // An out-of-line module behind a cfg, or with a cfg'd `#[path]`, or a
    // file-level `#![cfg]` in the owner's file: a cfg may swap the whole
    // file for a same-named module holding the twin.
    let scale = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n";
    let twin_mod = "#[cfg(feature = \"alt\")]\npub mod scale {\n    pub fn helper(_: u32) -> u32 {\n        12\n    }\n    #[allow(non_upper_case_globals)]\n    pub static weight: fn(u32) -> u32 = helper;\n}\n";
    let tests =
        "use demo::scale::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let file_admitted = |lib: &str, scale: &str, extra: &[(&str, &str)]| {
        let mut files = vec![(LIB, lib), ("src/scale.rs", scale), (TESTS, tests)];
        files.extend_from_slice(extra);
        // Role composition records the `mod scale;` edge for the file.
        let line = lib
            .lines()
            .position(|line| line.contains("mod scale;"))
            .map_or(0, |at| at + 1);
        assert!(line > 0, "{lib}");
        let chain = SourceRoleProvenance {
            edges: vec![module_edge(LIB, "src/scale.rs", "scale", line, false)],
            earliest_unresolved_reason: None,
            ..Default::default()
        };
        let ix = index_with_provenance(&files, &[("src/scale.rs", chain)]);
        let owner = ix.functions().iter().find(|function| {
            function.name == "weight" && function.file == Path::new("src/scale.rs")
        });
        assert!(
            owner.is_some(),
            "the owner must be indexed from src/scale.rs"
        );
        let pin = owner
            .and_then(|owner| OwnerReturnPin::establish(&return_probe(owner, "x * 3"), owner, &ix));
        assert!(pin.is_some(), "the file owner must establish: {lib}");
        pin.map(|pin| admitted_texts(&ix, &pin)).unwrap_or_default()
    };
    let gated_decl = format!("#[cfg(not(feature = \"alt\"))]\npub mod scale;\n{twin_mod}");
    assert!(file_admitted(&gated_decl, scale, &[]).is_empty());
    let alt_file = "pub fn helper(_: u32) -> u32 {\n    12\n}\n#[allow(non_upper_case_globals)]\npub static weight: fn(u32) -> u32 = helper;\n";
    assert!(
        file_admitted(
            "#[cfg_attr(feature = \"alt\", path = \"alt_scale.rs\")]\npub mod scale;\n",
            scale,
            &[("src/alt_scale.rs", alt_file)]
        )
        .is_empty()
    );
    let inner_gated = format!("#![cfg(not(feature = \"alt\"))]\n{scale}");
    assert!(file_admitted("pub mod scale;\n", &inner_gated, &[]).is_empty());
    // A `cfg_attr` that can only toggle lints keeps the pin; one that
    // expands to a `cfg` drops the file.
    let lint_toggle = format!("#![cfg_attr(docsrs, allow(unused))]\n{scale}");
    assert_eq!(
        file_admitted("pub mod scale;\n", &lint_toggle, &[]),
        ["assert_eq!(weight(4), 12);"]
    );
    let doc_toggle = format!("#![cfg_attr(feature = \"alt\", doc = \"path cfg\")]\n{scale}");
    assert_eq!(
        file_admitted("pub mod scale;\n", &doc_toggle, &[]),
        ["assert_eq!(weight(4), 12);"]
    );
    let cfg_toggle = format!("#![cfg_attr(feature = \"alt\", cfg(any()))]\n{scale}");
    assert!(file_admitted("pub mod scale;\n", &cfg_toggle, &[]).is_empty());
    // Control: the plain declaration pins, even beside a cfg'd `mod scale;`
    // that declares another file (only the edge's own declaration counts).
    assert_eq!(
        file_admitted("pub mod scale;\n", scale, &[]),
        ["assert_eq!(weight(4), 12);"]
    );
    assert_eq!(
        file_admitted(
            "pub mod scale;\nmod unused {\n    #[cfg(feature = \"alt\")]\n    mod scale;\n}\n",
            scale,
            &[]
        ),
        ["assert_eq!(weight(4), 12);"]
    );
    // Control: the ungated integration owner pins.
    assert_eq!(
        weight_admitted(
            "use demo::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n"
        ),
        ["assert_eq!(weight(4), 12);"]
    );
}

#[test]
fn an_ancestor_file_a_cfg_may_drop_gates_the_owner() {
    // #7104 review: an inner `#![cfg]` at the top of `src/a.rs` drops
    // `a::scale` with it.
    let tests = "use demo::a::scale::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let owner_file = "src/a/scale.rs";
    let admitted_with = |a: &str| {
        let lib = "pub mod a;\n";
        let files = [
            (LIB, lib),
            ("src/a.rs", a),
            (owner_file, "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n"),
            (TESTS, tests),
        ];
        let line = a
            .lines()
            .position(|line| line.contains("mod scale;"))
            .map_or(0, |at| at + 1);
        assert!(line > 0, "{a}");
        let edges = vec![
            module_edge(LIB, "src/a.rs", "a", 1, false),
            module_edge("src/a.rs", owner_file, "scale", line, false),
        ];
        let chain = |edges: Vec<SourceRoleProvenanceEdge>| SourceRoleProvenance {
            edges,
            earliest_unresolved_reason: None,
            ..Default::default()
        };
        let ix = index_with_provenance(
            &files,
            &[
                ("src/a.rs", chain(edges[..1].to_vec())),
                (owner_file, chain(edges)),
            ],
        );
        let owner = ix
            .functions()
            .iter()
            .find(|function| function.name == "weight" && function.file == Path::new(owner_file));
        assert!(owner.is_some());
        let pin = owner
            .and_then(|owner| OwnerReturnPin::establish(&return_probe(owner, "x * 3"), owner, &ix));
        assert!(pin.is_some(), "the nested file owner must establish");
        pin.map(|pin| admitted_texts(&ix, &pin)).unwrap_or_default()
    };
    assert!(admitted_with("#![cfg(not(feature = \"alt\"))]\npub mod scale;\n").is_empty());
    // Control: the same chain without the cfg pins.
    assert_eq!(
        admitted_with("pub mod scale;\n"),
        ["assert_eq!(weight(4), 12);"]
    );
}

#[test]
fn a_cfg_attr_path_on_the_owners_declaration_gates_it_through_real_composition()
-> Result<(), String> {
    // #7104 review: ripr records no module edge for a `#[path]` under
    // `cfg_attr`, so the owner's file has an empty chain; only a crate
    // root may. An unrelated unresolvable `#[path]` elsewhere leaves
    // other owners pinned.
    let pinned = |label: &str, lib: &str| -> Result<Vec<String>, String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-owner-pin-cfg-attr-path-{label}-{}",
            std::process::id()
        ));
        let files = [
            (
                "Cargo.toml",
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
            ),
            ("src/lib.rs", lib),
            (
                "src/scale.rs",
                "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n",
            ),
            ("src/sys.rs", "pub fn native() {}\n"),
            ("src/sys_win.rs", "pub fn native() {}\n"),
            (
                "src/alt_scale.rs",
                "pub fn helper(_: u32) -> u32 {\n    12\n}\n#[allow(non_upper_case_globals)]\npub static weight: fn(u32) -> u32 = helper;\n",
            ),
            (
                "tests/t.rs",
                "use demo::scale::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n",
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
        let index = index?;
        let owner = index
            .functions()
            .iter()
            .find(|function| {
                function.name == "weight" && function.file == Path::new("src/scale.rs")
            })
            .ok_or("the owner must be indexed from src/scale.rs")?;
        let pin = OwnerReturnPin::establish(&return_probe(owner, "x * 3"), owner, &index)
            .ok_or("the owner must establish")?;
        // The manifest authority reads the workspace from disk at admission
        // time, as in production, so the scratch tree stays until after.
        let admitted = admitted_texts(&index, &pin);
        let _ = std::fs::remove_dir_all(&root);
        Ok(admitted)
    };
    assert!(
        pinned(
            "gated",
            "#[cfg_attr(feature = \"alt\", path = \"alt_scale.rs\")]\npub mod scale;\n"
        )?
        .is_empty()
    );
    // Control: the plain declaration pins, with the edge real composition
    // records.
    assert_eq!(
        pinned("plain", "pub mod scale;\n")?,
        ["assert_eq!(weight(4), 12);"]
    );
    assert_eq!(
        pinned(
            "platform",
            "pub mod scale;\n#[cfg_attr(windows, path = \"sys_win.rs\")]\nmod sys;\n"
        )?,
        ["assert_eq!(weight(4), 12);"]
    );
    Ok(())
}

/// #7104 review: a comma inside a predicate string must not select the
/// payload boundary — `cfg_attr(mode = "a,b", cfg(..))` drops, while a
/// non-dropping attribute with a comma in its predicate string does not.
#[test]
fn cfg_attr_payload_with_comma_in_predicate_string() {
    assert!(
        cfg_attr_payload_may_drop("cfg_attr(mode = \"a,b\", cfg(feature = \"x\"))"),
        "a string comma must not hide the dropping cfg payload"
    );
    assert!(
        !cfg_attr_payload_may_drop("cfg_attr(mode = \"a,b\", allow(dead_code))"),
        "a non-dropping payload stays non-dropping with a string comma"
    );
}

/// #7097: the owner beside a same-named non-fn twin in another module.
const TWIN_STATIC_LIB: &str = "pub mod scale {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n}\n\npub mod alt {\n    pub fn helper(_: u32) -> u32 {\n        12\n    }\n    #[allow(non_upper_case_globals)]\n    pub static weight: fn(u32) -> u32 = helper;\n}\n";
const TWIN_CONST_LIB: &str = "pub mod scale {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n}\n\npub mod alt {\n    pub fn helper(_: u32) -> u32 {\n        12\n    }\n    #[allow(non_upper_case_globals)]\n    pub const weight: fn(u32) -> u32 = helper;\n}\n";
const TWIN_STRUCT_LIB: &str = "pub mod scale {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n}\n\npub mod alt {\n    pub struct weight(pub u32);\n}\n";

fn twin_admitted(lib: &str, tests: &str) -> Vec<String> {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(
        pin.is_some(),
        "the fn owner must establish beside a non-fn twin"
    );
    let Some(pin) = pin else {
        return Vec::new();
    };
    admitted_texts(&index, &pin)
}

fn weighs_test(prelude: &str) -> String {
    format!("{prelude}\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n")
}

#[test]
fn a_bare_call_to_an_imported_same_named_twin_is_not_a_pin() {
    // #7097: the issue's exact repro — `use demo::alt::weight;` imports the
    // twin, so `weight(4)` never reaches `scale::weight`.
    for lib in [TWIN_STATIC_LIB, TWIN_CONST_LIB, TWIN_STRUCT_LIB] {
        for import in ["use demo::alt::weight;", "use demo::alt::{weight};"] {
            assert!(
                twin_admitted(lib, &weighs_test(import)).is_empty(),
                "{import}"
            );
        }
    }
}

#[test]
fn a_bare_call_beside_a_competing_import_is_not_a_pin() {
    // #7097: a second import of the name — cfg'd or not — leaves the bare
    // call's binding ambiguous (or a duplicate-import error).
    for prelude in [
        "use demo::scale::weight;\n#[cfg(feature = \"alt\")]\nuse demo::alt::weight;",
        "#[cfg(not(feature = \"alt\"))]\nuse demo::scale::weight;\n#[cfg(feature = \"alt\")]\nuse demo::alt::weight;",
        "use demo::scale::weight;\nuse demo::alt::weight;",
    ] {
        assert!(
            twin_admitted(TWIN_STATIC_LIB, &weighs_test(prelude)).is_empty(),
            "{prelude}"
        );
    }
}

#[test]
fn a_same_named_item_in_the_test_file_is_not_the_owner() {
    // #7097: a `static`/`const`/tuple-struct of the name in the test file
    // takes the bare call, at file scope, in the test's module, or in its body.
    for item in [
        "#[allow(non_upper_case_globals)]\nstatic weight: fn(u32) -> u32 = helper;",
        "#[allow(non_upper_case_globals)]\nconst weight: fn(u32) -> u32 = helper;",
        "struct weight(u32);",
    ] {
        let file =
            format!("{item}\n\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n");
        assert!(weight_admitted(&file).is_empty(), "file scope: {item}");
        let module = format!(
            "mod tests {{\n    {}\n    #[test]\n    fn weighs() {{\n        assert_eq!(weight(4), 12);\n    }}\n}}\n",
            item.replace('\n', "\n    ")
        );
        assert!(weight_admitted(&module).is_empty(), "module scope: {item}");
        let body = format!(
            "use demo::weight;\n\n#[test]\nfn weighs() {{\n    {}\n    assert_eq!(weight(4), 12);\n}}\n",
            item.replace('\n', "\n    ")
        );
        assert!(weight_admitted(&body).is_empty(), "body scope: {item}");
    }
}

#[test]
fn a_bare_call_through_the_owners_own_import_pins_beside_a_twin() {
    // #7097 control: a twin elsewhere cannot capture an explicit import of
    // the owner's own path.
    for lib in [TWIN_STATIC_LIB, TWIN_CONST_LIB, TWIN_STRUCT_LIB] {
        for import in [
            "use demo::scale::weight;",
            "use demo::scale::{weight};",
            "use demo::{scale::weight};",
        ] {
            assert_eq!(
                twin_admitted(lib, &weighs_test(import)).len(),
                1,
                "{import}"
            );
        }
    }
    // A one-hop re-export of the owner pins through the re-exporting
    // module; a re-export of the twin, or a second hop, refuses.
    let reexport = |target: &str| {
        format!(
            "pub mod scale {{\n    pub fn weight(x: u32) -> u32 {{\n        x * 3\n    }}\n}}\n\npub mod alt {{\n    pub fn helper(_: u32) -> u32 {{\n        12\n    }}\n    #[allow(non_upper_case_globals)]\n    pub static weight: fn(u32) -> u32 = helper;\n}}\n\npub use {target};\n"
        )
    };
    assert_eq!(
        twin_admitted(
            &reexport("crate::scale::weight"),
            &weighs_test("use demo::weight;")
        )
        .len(),
        1
    );
    assert!(
        twin_admitted(
            &reexport("crate::alt::weight"),
            &weighs_test("use demo::weight;")
        )
        .is_empty()
    );
    let two_hop = "pub mod scale {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n}\n\npub mod mid {\n    pub use crate::scale::weight;\n}\n\npub use crate::mid::weight;\n";
    assert!(
        twin_admitted(two_hop, &weighs_test("use demo::weight;")).is_empty(),
        "a second re-export hop refuses"
    );
}

#[test]
fn a_bare_call_through_a_glob_pins_only_past_the_owners_module() {
    // #7097: a glob of the owner's module delivers the owner; a glob of the
    // twin's module, or of a module without the owner, does not pin.
    assert_eq!(
        twin_admitted(TWIN_STATIC_LIB, &weighs_test("use demo::scale::*;")).len(),
        1
    );
    assert!(twin_admitted(TWIN_STATIC_LIB, &weighs_test("use demo::alt::*;")).is_empty());
    assert!(
        twin_admitted(TWIN_STATIC_LIB, &weighs_test("use demo::*;")).is_empty(),
        "the crate root holds no `weight` to import"
    );
    // A rename to the name binds it too: a glob of a module renaming the
    // twin refuses, while one renaming the owner transports it.
    for (target, admitted) in [("super::alt::weight", 0), ("super::scale::weight", 1)] {
        let lib = format!(
            "pub mod scale {{\n    pub fn weight(x: u32) -> u32 {{\n        x * 3\n    }}\n}}\n\npub mod alt {{\n    pub fn helper(_: u32) -> u32 {{\n        12\n    }}\n    #[allow(non_upper_case_globals)]\n    pub static weight: fn(u32) -> u32 = helper;\n}}\n\npub mod mid {{\n    pub use {target} as weight;\n}}\n"
        );
        assert_eq!(
            twin_admitted(&lib, &weighs_test("use demo::mid::*;")).len(),
            admitted,
            "{target}"
        );
    }
    // A prelude glob delivers a re-exported owner, but not a
    // re-exported twin.
    for (target, admitted) in [("crate::scale::weight", 1), ("crate::alt::weight", 0)] {
        let lib = format!(
            "pub mod scale {{\n    pub fn weight(x: u32) -> u32 {{\n        x * 3\n    }}\n}}\n\npub mod alt {{\n    pub fn helper(_: u32) -> u32 {{\n        12\n    }}\n    #[allow(non_upper_case_globals)]\n    pub static weight: fn(u32) -> u32 = helper;\n}}\n\npub mod prelude {{\n    pub use {target};\n}}\n"
        );
        assert_eq!(
            twin_admitted(&lib, &weighs_test("use demo::prelude::*;")).len(),
            admitted,
            "{target}"
        );
    }
    // A nested `use super::*` transports the file-top import: the owner's
    // own import pins, the twin's refuses.
    for (import, admitted) in [
        ("use demo::scale::weight;", 1),
        ("use demo::alt::weight;", 0),
    ] {
        let tests = format!(
            "{import}\nmod wrapped {{\n    use super::*;\n    #[test]\n    fn weighs() {{\n        assert_eq!(weight(4), 12);\n    }}\n}}\n"
        );
        assert_eq!(
            twin_admitted(TWIN_STATIC_LIB, &tests).len(),
            admitted,
            "{import}"
        );
    }
    // The unit-test shape: `use super::*` inside the owner's own module.
    let lib = "pub mod scale {\n    pub fn weight(x: u32) -> u32 {\n        x * 3\n    }\n    mod tests {\n        use super::*;\n        #[test]\n        fn weighs() {\n            assert_eq!(weight(4), 12);\n        }\n    }\n}\n";
    let index = index(&[(LIB, lib)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(pin.is_some(), "the nested owner must establish");
    if let Some(pin) = pin {
        assert_eq!(admitted_texts(&index, &pin).len(), 1);
    }
}

#[test]
fn a_bare_call_without_an_import_pins_only_in_the_owners_module() {
    // #7097: a test beside the owner needs no import; anywhere else an
    // unimported bare name is unresolved (or the prelude), never the owner.
    let lib = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let index = index(&[(LIB, lib)]);
    let pin = establish(&index, "weight", "x * 3");
    assert!(pin.is_some(), "the file-top owner must establish");
    if let Some(pin) = pin {
        assert_eq!(admitted_texts(&index, &pin).len(), 1);
    }
    assert!(
        weight_admitted("#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n").is_empty()
    );
    // A foreign import binds a foreign item, not the owner.
    assert!(
        weight_admitted(
            "use foreign::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n"
        )
        .is_empty()
    );
}

#[test]
fn a_same_named_type_only_item_in_the_test_file_keeps_the_pin() {
    // #7097 precision: items that cannot take a value call share no
    // namespace with it, so the owner's import still pins. A braced
    // struct beside an imported fn compiles and calls the fn
    // (rustc-probed); an `enum weight` never injects `weight` into the
    // module value namespace (rustc-probed).
    for item in [
        "mod weight {}",
        "trait weight {}",
        "type weight = u32;",
        "enum weight { A }",
        "union weight { x: u32 }",
        "struct weight { x: u32 }",
        "struct weight(u32);",
    ] {
        let tests = format!(
            "use demo::weight;\n{item}\n\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        // A tuple struct is callable, so it twins; the rest cannot take the call.
        let expected = usize::from(!item.ends_with("(u32);"));
        assert_eq!(weight_admitted(&tests).len(), expected, "{item}");
    }
}

#[test]
fn a_unit_struct_twin_refuses_the_bare_call() {
    // #7176 review: a unit struct's constructor collides with an imported
    // fn (E0255) and takes the value namespace (E0618, rustc-probed), so
    // `weight(4)` beside one never calls the owner — in the module scope
    // or the test body.
    for tests in [
        "use demo::weight;\nstruct weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n",
        "use demo::weight;\n\n#[test]\nfn weighs() {\n    struct weight;\n    assert_eq!(weight(4), 12);\n}\n",
    ] {
        assert!(
            weight_admitted(tests).is_empty(),
            "a unit struct takes the call: {tests:?}"
        );
    }
}

fn establish_in(
    index: &RustIndex,
    file: &str,
    name: &str,
    expression: &str,
) -> Option<OwnerReturnPin> {
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == name && function.file == Path::new(file))?;
    OwnerReturnPin::establish(&return_probe(owner, expression), owner, index)
}

#[test]
fn an_extern_twin_refuses_the_bare_call() {
    // #7176 review: extern items hang under the block's item list. An
    // extern fn of the name blocks the pin at establish time (a
    // competing workspace definition); an extern static is not a
    // function, so it must refuse at admission through the twin table.
    let decl = "fn weight(x: u32) -> u32;";
    let tests = format!(
        "use demo::weight;\n\nextern \"C\" {{\n    {decl}\n}}\n\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
    );
    let index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests)]);
    assert!(
        establish(&index, "weight", "x * 3").is_none(),
        "an extern fn competes at establish time"
    );
    let decl = "static weight: fn(u32) -> u32;";
    let tests = format!(
        "use demo::weight;\n\nextern \"C\" {{\n    {decl}\n}}\n\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
    );
    assert!(
        weight_admitted(&tests).is_empty(),
        "an extern static takes the call"
    );
}

#[test]
fn a_static_mut_twin_refuses_the_bare_call() {
    // #7176 review: `static mut weight` carries a qualifier between the
    // keyword and the name; it twins in the module scope and the body.
    // A differently-named `mut` item (`mutual`) must not over-refuse.
    for decl in [
        "static mut weight: u32 = 4;",
        "pub static mut weight: u32 = 4;",
    ] {
        let tests = format!(
            "use demo::weight;\n{decl}\n\n#[test]\nfn weighs() {{\n    assert_eq!(weight(4), 12);\n}}\n"
        );
        assert!(
            weight_admitted(&tests).is_empty(),
            "a static mut takes the call: {decl}"
        );
    }
    let body = "use demo::weight;\n\n#[test]\nfn weighs() {\n    static mut weight: u32 = 4;\n    assert_eq!(weight(4), 12);\n}\n";
    assert!(weight_admitted(body).is_empty());
    let other = "use demo::weight;\nstatic mutual: u32 = 4;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    assert_eq!(weight_admitted(other).len(), 1);
}

#[test]
fn an_out_of_line_super_glob_pins_past_the_declaring_parent() {
    // #7176 review: `use super::*;` at the top of an out-of-line
    // `tests.rs` climbs into the inline scope holding its `mod`
    // declaration; the owner's own module delivers the owner.
    let scale = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n\n#[cfg(test)]\nmod tests;\n";
    let tests = "use super::*;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let index = index_with_provenance(
        &[
            (LIB, "pub mod scale;\n"),
            ("src/scale.rs", scale),
            ("src/scale/tests.rs", tests),
        ],
        &[
            (
                "src/scale.rs",
                SourceRoleProvenance {
                    edges: vec![module_edge(LIB, "src/scale.rs", "scale", 1, false)],
                    earliest_unresolved_reason: None,
                    ..Default::default()
                },
            ),
            (
                "src/scale/tests.rs",
                SourceRoleProvenance {
                    edges: vec![module_edge(
                        "src/scale.rs",
                        "src/scale/tests.rs",
                        "tests",
                        6,
                        true,
                    )],
                    earliest_unresolved_reason: None,
                    ..Default::default()
                },
            ),
        ],
    );
    let pin = establish_in(&index, "src/scale.rs", "weight", "x * 3");
    assert!(pin.is_some(), "the out-of-line owner must establish");
    if let Some(pin) = pin {
        assert_eq!(admitted_texts(&index, &pin).len(), 1);
    }
}

#[test]
fn an_out_of_line_super_use_pins_past_the_declaring_parent() {
    // #7176 review: the explicit-`use` twin of the glob control.
    let scale = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n\n#[cfg(test)]\nmod tests;\n";
    let tests = "use super::weight;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let index = index_with_provenance(
        &[
            (LIB, "pub mod scale;\n"),
            ("src/scale.rs", scale),
            ("src/scale/tests.rs", tests),
        ],
        &[
            (
                "src/scale.rs",
                SourceRoleProvenance {
                    edges: vec![module_edge(LIB, "src/scale.rs", "scale", 1, false)],
                    earliest_unresolved_reason: None,
                    ..Default::default()
                },
            ),
            (
                "src/scale/tests.rs",
                SourceRoleProvenance {
                    edges: vec![module_edge(
                        "src/scale.rs",
                        "src/scale/tests.rs",
                        "tests",
                        6,
                        true,
                    )],
                    earliest_unresolved_reason: None,
                    ..Default::default()
                },
            ),
        ],
    );
    let pin = establish_in(&index, "src/scale.rs", "weight", "x * 3");
    assert!(pin.is_some(), "the out-of-line owner must establish");
    if let Some(pin) = pin {
        assert_eq!(admitted_texts(&index, &pin).len(), 1);
    }
}

#[test]
fn an_out_of_line_super_glob_refuses_past_a_parent_twin() {
    // #7176 review negative: the same layout with a twin beside the
    // owner in the declaring parent refuses.
    let scale = "pub fn weight(x: u32) -> u32 {\n    x * 3\n}\n\npub static weight: fn(u32) -> u32 = weight;\n\n#[cfg(test)]\nmod tests;\n";
    let tests = "use super::*;\n\n#[test]\nfn weighs() {\n    assert_eq!(weight(4), 12);\n}\n";
    let index = index_with_provenance(
        &[
            (LIB, "pub mod scale;\n"),
            ("src/scale.rs", scale),
            ("src/scale/tests.rs", tests),
        ],
        &[
            (
                "src/scale.rs",
                SourceRoleProvenance {
                    edges: vec![module_edge(LIB, "src/scale.rs", "scale", 1, false)],
                    earliest_unresolved_reason: None,
                    ..Default::default()
                },
            ),
            (
                "src/scale/tests.rs",
                SourceRoleProvenance {
                    edges: vec![module_edge(
                        "src/scale.rs",
                        "src/scale/tests.rs",
                        "tests",
                        8,
                        true,
                    )],
                    earliest_unresolved_reason: None,
                    ..Default::default()
                },
            ),
        ],
    );
    let pin = establish_in(&index, "src/scale.rs", "weight", "x * 3");
    assert!(pin.is_some(), "the out-of-line owner must establish");
    if let Some(pin) = pin {
        assert!(
            admitted_texts(&index, &pin).is_empty(),
            "a parent twin takes the globbed call"
        );
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

/// #7067: a single-file `use ... as Counter` rebinds the receiver to a
/// different type, so the pin is refused. The raw spelling
/// (`as r#Counter`) rebinds the same name.
#[test]
fn a_single_file_rename_of_the_receiver_refuses_the_pin_in_both_spellings() {
    for prelude in [
        "use other::Tally as Counter;\n",
        "use other::Tally as r#Counter;\n",
    ] {
        assert_eq!(
            counter_admitted("tally", prelude, "let c = Counter::new();"),
            0,
            "{prelude}"
        );
    }
}

/// #7067 precision: a plain single-file `use` of the receiver may
/// re-export production, so it is not a shadow — the pin stays admitted.
#[test]
fn a_single_file_plain_import_of_the_receiver_keeps_its_pin() {
    assert_eq!(counter_admitted("tally", "", "let c = Counter::new();"), 1);
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
        "std::assert_eq!(weight(input), 12);",
        "::core::assert_eq!(weight(input), 12);",
        "pretty_assertions::assert_eq!(weight(input), 12);",
        "similar_asserts :: assert_eq !(weight(input), 12);",
    ] {
        assert!(is_admission_gated_assert_eq(assertion), "{assertion}");
    }
    for assertion in [
        "assert!({ assert_eq!(weight(input), 12); true });",
        "other::assert_eq!(weight(input), 12);",
        "pretty_assertions::assert_ne!(weight(input), 12);",
        "pretty_assertions::nested::assert_eq!(weight(input), 12);",
        "matches!(value, Some(_));",
        "match value { _ => assert_eq!(weight(input), 12) }",
    ] {
        assert!(!is_admission_gated_assert_eq(assertion), "{assertion}");
    }
}

/// #7857: a `std::`/`core::`-qualified `assert_eq!` runs as the standard
/// macro unless its root may be rebound, so it is admitted like the bare
/// form. The proof needs the package manifests (a Cargo rename of `std`),
/// the test's own file (a rebinding module, import or `extern crate`
/// rename, a glob from another file, `no_implicit_prelude`, the invocation
/// of a macro that generates the test) and the usual execution path.
#[test]
fn a_std_qualified_assertion_is_admitted_unless_its_root_may_be_rebound() -> Result<(), String> {
    let manifest = |dependencies: &str| {
        format!("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dependencies}")
    };
    let plain = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-std-plain",
        &[("Cargo.toml", &manifest(""))],
    )?;
    let aliased = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-std-aliased",
        &[(
            "Cargo.toml",
            &manifest(
                "std = { package = \"fake-std\", version = \"1\" }\ncore = { package = \"fake-core\", version = \"1\" }\n",
            ),
        )],
    )?;
    let test = |assertion: &str| format!("#[test]\nfn weighs() {{\n    {assertion}\n}}\n");
    let opaque = |path: &str| {
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::OpaqueMacro(path.to_string()),
        ))
    };
    for (tests, path) in [
        (
            format!(
                "use demo::weight;\n{}",
                test("std::assert_eq!(weight(4), 12);")
            ),
            "std::assert_eq",
        ),
        (
            format!(
                "use demo::weight;\n{}",
                test("::core::assert_eq!(weight(4), 12);")
            ),
            "::core::assert_eq",
        ),
        (
            format!(
                "use demo::weight;\n{}",
                test("core::assert_eq!(weight(4), 12);")
            ),
            "core::assert_eq",
        ),
        // The parser keeps the spacing of the path.
        (
            format!(
                "use demo::weight;\n{}",
                test("std :: assert_eq!(weight(4), 12);")
            ),
            "std::assert_eq",
        ),
        (
            format!(
                "use demo::weight;\nmod tests {{\n    use super::*;\n{}}}\n",
                test("std::assert_eq!(weight(4), 12);")
            ),
            "std::assert_eq",
        ),
    ] {
        assert_eq!(
            weight_refusal_under(&tests, &[], Some(&plain)),
            None,
            "{tests}"
        );
        // Without a manifest, or with `std`/`core` bound to another package,
        // the root is unproved.
        assert_eq!(
            weight_refusal_under(&tests, &[], Some(&aliased)),
            opaque(path),
            "{tests}"
        );
        assert_eq!(weight_refusal(&tests, &[]), opaque(path), "{tests}");
    }
    // The fact reader already drops a call whose root the file rebinds.
    for prefix in [
        "mod std {}\n",
        "use demo as std;\n",
        "extern crate demo as std;\n",
    ] {
        let tests = format!(
            "{prefix}use demo::weight;\n{}",
            test("std::assert_eq!(weight(4), 12);")
        );
        let index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests)]);
        assert!(index.tests().at(0).assertions.is_empty(), "{tests}");
    }
    for (tests, refusal) in [
        // A glob from another file may bring a `std` item.
        (
            format!("use demo::*;\n{}", test("std::assert_eq!(weight(4), 12);")),
            opaque("std::assert_eq"),
        ),
        (
            format!(
                "#![no_implicit_prelude]\nuse demo::weight;\n{}",
                test("::std::assert_eq!(weight(4), 12);")
            ),
            opaque("::std::assert_eq"),
        ),
        (
            format!(
                "use demo::weight;\n{}",
                test("if weight(1) > 9 { std::assert_eq!(weight(4), 12); }")
            ),
            Some(AssertionRefusal::Syntax(
                AssertionContextRefusal::ConditionalPath("an `if` branch"),
            )),
        ),
        // `#![no_std]` without `extern crate std` leaves no `std` root.
        (
            format!(
                "#![no_std]\nuse demo::weight;\n{}",
                test("std::assert_eq!(weight(4), 12);")
            ),
            opaque("std::assert_eq"),
        ),
        // Other drop-in roots need their own manifest check, which this
        // gate does not make.
        (
            format!(
                "use demo::weight;\n{}",
                test("pretty_assertions::assert_eq!(weight(4), 12);")
            ),
            opaque("pretty_assertions::assert_eq"),
        ),
    ] {
        assert_eq!(
            weight_refusal_under(&tests, &[], Some(&plain)),
            refusal,
            "{tests}"
        );
    }
    // An unproved qualified call anywhere in the file may expand to an early
    // return, so a later bare assertion is refused too (#7938 review).
    let bare_after = |root: &Path| {
        let tests = format!(
            "use demo::weight;\n{}",
            test("std::assert_eq!(1, 1);\n    assert_eq!(weight(4), 12);")
        );
        let mut index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests)]);
        index.drop_in_manifests = crate::analysis::facts::drop_in::DropInManifests::new(root);
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(TESTS));
        let assertion = test.and_then(|test| {
            test.assertions
                .iter()
                .find(|assertion| assertion.text.starts_with("assert_eq!(weight"))
        });
        assert!(assertion.is_some(), "{tests}");
        OwnerPinSyntax::default().refusal(test?, assertion?, &index)
    };
    assert_eq!(bare_after(&plain), None);
    assert_eq!(bare_after(&aliased), opaque("std::assert_eq"));
    // A generated test is checked against its expansion, which shows
    // neither the file's glob nor the generating invocation; the real file
    // shows both. The bare form is the control: the same generated test is
    // admitted, so the refusal is the qualified root's.
    let generated = |assertion: &str| {
        let tests = format!(
            "use demo::*;\nmacro_rules! generate {{\n    ($name:ident) => {{\n        #[test]\n        fn $name() {{\n            {assertion}\n        }}\n    }};\n}}\ngenerate!(weighs);\n"
        );
        let mut index = index(&[(LIB, WEIGHT_LIB), (TESTS, &tests)]);
        crate::analysis::facts::register_macro_generated_tests_for_test(&mut index);
        index.drop_in_manifests = crate::analysis::facts::drop_in::DropInManifests::new(&plain);
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(TESTS));
        assert!(
            test.is_some_and(|test| !test.assertions.is_empty()),
            "{tests}"
        );
        let test = test?;
        OwnerPinSyntax::default().refusal(test, &test.assertions[0], &index)
    };
    assert_eq!(generated("assert_eq!(weight(4), 12);"), None);
    assert_eq!(
        generated("std::assert_eq!(weight(4), 12);"),
        opaque("std::assert_eq")
    );
    Ok(())
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
    weight_refusal_nth(tests, others, root, 0)
}

/// `weight_refusal_under` for the test's `nth` assertion.
fn weight_refusal_nth(
    tests: &str,
    others: &[(&str, &str)],
    root: Option<&Path>,
    nth: usize,
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
        test.assertions.len() > nth,
        "the test's assertions must parse"
    );
    OwnerPinSyntax::default().refusal(test, &test.assertions[nth], &index)
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
fn a_bound_trusted_call_hides_attributes_and_nested_calls_from_the_pin() {
    // #8446: the second pass over a bound trusted name's calls must read an
    // attribute in its arguments like the first pass does, and must reach a
    // bound call nested inside another trusted call's token tree.
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let refused = |other: &str| {
        matches!(
            weight_refusal(tests, &[("src/other.rs", other)]),
            Some(AssertionRefusal::Syntax(
                AssertionContextRefusal::MacroBinding(_)
            ))
        )
    };
    for hidden in [
        // Finding 1: an attribute in the arguments of a bound trusted call.
        "use ext::vec;\nvec!(#[no_implicit_prelude] mod tests;);\n",
        "use ext::vec;\nvec!(#[macro_use] mod tests;);\n",
        // Finding 2: a bound call nested in another trusted call.
        "use ext::vec;\nfn f() { dbg!(vec!(assert_eq!(#[cfg(test)] mod tests;))); }\n",
        "use ext::vec;\nfn f() { dbg!(vec!(#[no_implicit_prelude] mod tests;)); }\n",
        "use ext::vec;\nfn f() { dbg!(dbg!(vec![assert_eq!(1, 1)])); }\n",
    ] {
        assert!(refused(hidden), "{hidden}");
    }
    // Discriminating negatives: an inert attribute, an unbound nested name
    // and a bound call that names no trusted macro keep the pin.
    for kept in [
        "use ext::vec;\nvec!(#[derive(Debug)] struct S;);\n",
        "fn f() { dbg!(vec!(assert_eq!(1, 1))); }\n",
        "use ext::vec;\nfn f() { dbg!(vec!(1, 2)); }\n",
    ] {
        assert_eq!(
            weight_refusal(tests, &[("src/other.rs", kept)]),
            None,
            "{kept}"
        );
    }
}

#[test]
fn a_bound_trusted_macros_arguments_are_opaque_to_other_trusted_names() {
    // #5941: a user-defined `vec!` is no longer std's, so `assert_eq!` named
    // only inside its arguments can be defined by the expansion together with
    // the module whose tests use it.
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let define = "macro_rules! vec { ($name:ident ! ($($rest:tt)*)) => { macro_rules! $name { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; } $($rest)* }; }\n";
    let refused = |other: &str| {
        matches!(
            weight_refusal(tests, &[("src/other.rs", other)]),
            Some(AssertionRefusal::Syntax(
                AssertionContextRefusal::MacroBinding(_)
            ))
        )
    };
    // The issue's own shape. Its definition body also binds through
    // `macro_rules! $name`, so this case alone does not isolate the new rule.
    assert!(refused(&format!(
        "{define}vec!(assert_eq!(#[cfg(test)] mod tests;));\n"
    )));
    // Isolating cases: `vec` is bound without its body making `assert_eq`
    // ambiguous, so only the arguments of `vec!` can reach it.
    let not_refused: Vec<&str> = [
        // An import of a foreign `vec`.
        "use ext::vec;\nvec!(assert_eq!(#[cfg(test)] mod tests;));\n",
        // A local `vec` whose body binds only a non-trusted name.
        "macro_rules! vec { () => { macro_rules! other { () => {}; } }; }\nvec!(assert_eq!(#[cfg(test)] mod tests;));\n",
        // Fixpoint: `vec!(format)` binds `format`, which is then opaque, even
        // though its call precedes the `vec!` call that binds it.
        "use ext::vec;\nformat!(assert_eq!(1, 1));\nvec!(format);\n",
    ]
    .into_iter()
    .filter(|other| !refused(other))
    .collect();
    assert!(not_refused.is_empty(), "not refused: {not_refused:?}");
    // Discriminating negatives: an unbound `vec!` keeps its arguments
    // trusted, and a locally defined `vec!` whose expansion cannot bind a
    // name stays transparent (#6648).
    assert_eq!(
        weight_refusal(
            tests,
            &[("src/other.rs", "fn f() { vec![assert_eq!(1, 1)]; }\n")]
        ),
        None
    );
    assert_eq!(
        weight_refusal(
            tests,
            &[(
                "src/other.rs",
                "macro_rules! vec { ($e:expr) => { 0 }; }\nfn f() { vec![assert_eq!(1, 1)]; }\n"
            )]
        ),
        None
    );
}

#[test]
fn a_local_wrapper_that_cannot_bind_a_name_keeps_assert_eq_trusted() {
    // #6648: the corpus subject's own `header_case!` wraps its argument in a
    // closure body; its expansion can define, import or export nothing, so
    // the `assert_eq!` tokens it receives do not make every `assert_eq!` in
    // the crate ambiguous.
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let definition = "macro_rules! case {\n    ($name:ident, |$v:ident| $body:block) => {\n        #[test]\n        fn $name() { let check = |$v: u32| $body; check(1); }\n    };\n}\n";
    let call = "case!(one, |v| { assert_eq!(v, 1); });\n";
    let refusal = |other: &str| weight_refusal(tests, &[("src/other.rs", other)]);
    assert_eq!(refusal(&format!("{definition}{call}")), None);
    assert_eq!(
        refusal(&format!("mod tests {{\n{definition}{call}}}\n")),
        None
    );
    // A doc comment is not an attribute node, so it changes nothing.
    assert_eq!(
        refusal(&format!("/// Runs one case.\n{definition}{call}")),
        None
    );
    // Another inert call of the same wrapper between them, and a built-in
    // test attribute in the arguments, bind nothing either.
    assert_eq!(
        refusal(&format!(
            "{definition}case!(two, |v| {{ assert_eq!(v, 2); }});\ncase!(one, |v| {{ #[allow(unused)] let w = 1; assert_eq!(v, 1); }});\n"
        )),
        None
    );

    // Each way the wrapper may still bind a name keeps the argument
    // ambiguous.
    let bang = definition.replace("check(1);", "check(1); assert!(true);");
    let defines = definition.replace("#[test]", "macro_rules! inner { () => {} }\n#[test]");
    let imports = definition.replace("#[test]", "use other::*;\n#[test]");
    let attributed = format!("#[allow(unused)]\n{definition}");
    for other in [
        // Used before (or without) a definition in this file.
        call.to_string(),
        format!("{call}{definition}"),
        // Defined twice, or also imported under its name.
        format!("{definition}{definition}{call}"),
        format!("use other::case;\n{definition}{call}"),
        format!("use other::wrap as case;\n{definition}{call}"),
        // Reached by path, so not the textual definition.
        format!("{definition}crate::case!(one, |v| {{ assert_eq!(v, 1); }});\n"),
        // The definition invokes a macro or holds a binding word.
        format!("{bang}{call}"),
        format!("{defines}{call}"),
        format!("{imports}{call}"),
        // An attribute on the definition may change it.
        format!("{attributed}{call}"),
        // The arguments carry a binding word.
        format!("{definition}case!(one, |v| {{ macro_rules! assert_eq {{ () => {{}} }} }});\n"),
        format!("{definition}case!(one, |v| {{ use other::assert_eq; assert_eq!(v, 1); }});\n"),
        // The call sits outside the definition's inline module, or the
        // module's scope cannot be bounded, or the definition sits in a
        // function body.
        format!("mod tests {{\n{definition}}}\n{call}"),
        format!("mod tests {{\nmod child;\n{definition}{call}}}\n"),
        format!("#[macro_use]\nmod tests {{\n{definition}{call}}}\n"),
        format!("fn setup() {{\n{definition}{call}}}\n"),
        // The arguments invoke another macro, a trusted name by path, or an
        // attribute that may emit items.
        format!("{definition}case!(one, |v| {{ define!(assert_eq); }});\n"),
        format!("{definition}case!(one, |v| {{ other::assert_eq!(v, 1); }});\n"),
        format!(
            "{definition}case!(one, |v| {{ #[derive(Clone)] struct S; assert_eq!(v, 1); }});\n"
        ),
        format!(
            "{}{call}",
            definition.replace("#[test]", "#[test]\n#[derive(Clone)]")
        ),
        // A macro expanded between the definition and the call may emit a
        // shadowing `macro_rules! case`.
        format!("{definition}generate!();\n{call}"),
        // Even a trusted name may be a local macro that emits one.
        format!("{definition}fn build() -> Vec<u32> {{ vec![1] }}\n{call}"),
        // A module between them may carry a shadowing definition, even
        // without an outer attribute (`#![macro_use]` in its own file).
        format!("{definition}#[macro_use]\nmod shadow;\n{call}"),
        format!("{definition}mod shadow;\n{call}"),
        // A `!` that reads as unary can be spliced after a name by a `tt`
        // fragment or a repetition.
        format!("{definition}case!(one, |v| {{ assert!(!(v == 2)); assert_eq!(v, 1); }});\n"),
        "macro_rules! case { ($m:ident, $b:tt, $n:ident) => { $m $b ($n); } }\ncase!(define, !, assert_eq);\n".to_string(),
        "macro_rules! case { ($($m:ident)*; $n:ident) => { $($m)* ! ($n); } }\ncase!(define; assert_eq);\n".to_string(),
        // A `tt` fragment can lift the `!` off a trusted call and put it
        // after an untrusted name: this expands to `define!(assert_eq);`.
        "macro_rules! case { ($m:ident $n:ident $bang:tt $a:tt) => { $m $bang ($n); } }\ncase!(define assert_eq!(x));\n".to_string(),
        format!("{definition}case!(two, |v| {{ generate!(); }});\n{call}"),
    ] {
        assert!(
            matches!(
                refusal(&other),
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "{other}"
        );
    }
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
        "use demo::weight;\nmod wrapped {{\n    use super::*;\n    {shadow}\n    #[test]\n    fn weighs() {{ assert_eq!(weight(4), 12); }}\n}}\n"
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
fn a_verified_drop_in_import_in_a_macro_body_is_the_standard_assertion() -> Result<(), String> {
    // #8149: the drop-in rule applies to `use` items a macro body emits.
    let outside = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let manifest = |dependency: &str| {
        format!(
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\n{dependency}\n"
        )
    };
    let plain = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-macro-plain",
        &[("Cargo.toml", &manifest("pretty_assertions = \"1\""))],
    )?;
    let aliased = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-macro-aliased",
        &[(
            "Cargo.toml",
            &manifest("pretty_assertions = { package = \"fake-assertions\", version = \"1\" }"),
        )],
    )?;
    let refused = |refusal: Option<AssertionRefusal>| {
        matches!(
            refusal,
            Some(AssertionRefusal::Syntax(
                AssertionContextRefusal::MacroBinding(_)
            ))
        )
    };
    let mut results = Vec::new();
    // Admitted under a verified manifest, refused without one or when aliased.
    for other in [
        "macro_rules! cases { ($n:ident) => { mod $n { use pretty_assertions::assert_eq; #[test] fn t() { assert_eq!(weight(4), 12); } } }; }\n",
        "macro_rules! cases { () => { use ::pretty_assertions::assert_eq; }; }\n",
    ] {
        let others = [("src/other.rs", other)];
        results.push((
            other,
            weight_refusal_under(outside, &others, Some(&plain)),
            weight_refusal_under(outside, &others, Some(&aliased)),
            weight_refusal(outside, &others),
        ));
    }
    // Still refused even under a verified manifest.
    let mut still_refused = Vec::new();
    for other in [
        "macro_rules! m { () => { use other::assert_eq; }; }\n",
        "macro_rules! m { () => { use pretty_assertions::assert_ne as assert_eq; }; }\n",
        "macro_rules! m { () => { use pretty_assertions::inner::assert_eq; }; }\n",
        "macro_rules! m { () => { use pretty_assertions::assert_eq; }; }\nmod pretty_assertions {}\n",
    ] {
        still_refused.push((
            other,
            weight_refusal_under(outside, &[("src/other.rs", other)], Some(&plain)),
        ));
    }
    // A literal `use` in a matcher binds nothing and cannot reach the next `;`.
    let matcher =
        "macro_rules! m { (use other::assert_eq) => { use pretty_assertions::assert_eq; }; }\n";
    let matcher_result = weight_refusal_under(outside, &[("src/other.rs", matcher)], Some(&plain));
    let _ = std::fs::remove_dir_all(&plain);
    let _ = std::fs::remove_dir_all(&aliased);
    for (other, under_plain, under_alias, unrooted) in results {
        assert_eq!(under_plain, None, "{other}");
        assert!(refused(under_alias), "{other}");
        assert!(refused(unrooted), "{other}");
    }
    for (other, refusal) in still_refused {
        assert!(refused(refusal), "{other}");
    }
    assert_eq!(matcher_result, None);
    Ok(())
}

#[test]
fn a_return_guard_that_spares_the_expected_value_is_an_analyzer_limit() {
    // trap-tally-reached-assert-control / trap-steps-early-return.
    let refusal = |body: &str| {
        weight_refusal(
            &format!("use demo::weight;\n#[test]\nfn weighs() {{\n{body}\n}}\n"),
            &[],
        )
    };
    let pin = "assert_eq!(weight(4), 12);";
    for guard in [
        "if weight(4) > 1000 { return; }",
        "if 1000 < weight(4) { return; }",
        "if weight(4) == 0 {\n    return;\n}",
        "if weight( 4 ) >= 13 { return }",
        "if weight(4) > 12 { return; }",
        "if weight(4) <= 11 { return; }",
        "if weight(4) < -1 { return; }",
    ] {
        let found = refusal(&format!("{guard}\n{pin}"));
        assert_eq!(
            found
                .as_ref()
                .and_then(AssertionRefusal::value_guard_subject),
            Some("weight(4)"),
            "{guard}"
        );
        assert!(
            found.is_some_and(|refusal| refusal.is_analyzer_limit()),
            "{guard}"
        );
    }
    for guard in [
        // The guard fires at the expected value: left to the guarded-return
        // refusal.
        "if weight(4) < 1000 { return; }",
        "if weight(4) == 12 { return; }",
        "if weight(4) >= 12 { return; }",
        // The guard returns for every wrong value: a certain gap.
        "if weight(4) != 12 { return }",
        "if 12 != weight(4) { return; }",
        // Another call, another operand shape, or a compound condition.
        "if weight(5) > 1000 { return; }",
        "if other(4) > 1000 { return; }",
        "if weight(4) > LIMIT { return; }",
        "if weight(4) > 1000 && true { return; }",
        "if weight(4) > 10 > false { return; }",
        "if !(weight(4) < 1000) { return; }",
        // Anything else in the guard, an `else`, a second return, a value.
        "if weight(4) > 1000 { println!(\"x\"); return; }",
        "if weight(4) > 1000 { return; } else {}",
        "if weight(4) > 1000 { return; }\nif weight(4) > 2000 { return; }",
        "let _ = (); if weight(4) > 1000 { return (); }",
        "{ if weight(4) > 1000 { return; } }",
        "return;",
    ] {
        let found = refusal(&format!("{guard}\n{pin}"));
        assert!(found.is_some(), "{guard}");
        assert!(
            found
                .as_ref()
                .and_then(AssertionRefusal::value_guard_subject)
                .is_none(),
            "{guard}"
        );
        assert!(
            !found.is_some_and(|refusal| refusal.is_analyzer_limit()),
            "{guard}"
        );
    }
    // The guard reads the call the assertion compares, against a literal.
    let found = refusal("if weight(4) > 1000 { return; }\nassert_eq!(weight(4), TWELVE);");
    assert!(found.is_some_and(|refusal| refusal.value_guard_subject().is_none()));
    let found = refusal("if weight(4) > 1000 { return; }\nassert_ne!(weight(4), 13);");
    assert!(found.is_none_or(|refusal| refusal.value_guard_subject().is_none()));
    // Whitespace inside a literal argument is not layout, and the compared
    // value must be the call's own return: no projection, receiver or path
    // (#7974 review).
    for (guard, pin) in [
        (
            "if weight(4).wrapping_sub(weight(4)) > 1000 { return; }",
            "assert_eq!(weight(4).wrapping_sub(weight(4)), 0);",
        ),
        (
            "if weight(4).min(5) > 1000 { return; }",
            "assert_eq!(weight(4).min(5), 5);",
        ),
        (
            "if other::weight(4) > 1000 { return; }",
            "assert_eq!(other::weight(4), 12);",
        ),
        (
            "if (weight(4)) > 1000 { return; }",
            "assert_eq!((weight(4)), 12);",
        ),
        (
            "if weight(\"a b\") > 1000 { return; }",
            "assert_eq!(weight(\"ab\"), 12);",
        ),
        (
            "if weight(' ') > 1000 { return; }",
            "assert_eq!(weight(' '), 12);",
        ),
    ] {
        let found = refusal(&format!("{guard}\n{pin}"));
        assert!(
            found.is_some_and(|refusal| refusal.value_guard_subject().is_none()),
            "{guard}"
        );
    }
    // ripr does not know the type, so a neighbour of `e` outside the guard's
    // spared side may not exist: an unsigned count at 0, a `u8` at 255.
    for (guard, pin) in [
        ("if weight(4) > 0 { return; }", "assert_eq!(weight(4), 0);"),
        ("if weight(4) >= 1 { return; }", "assert_eq!(weight(4), 0);"),
        (
            "if weight(4) < 255 { return; }",
            "assert_eq!(weight(4), 255);",
        ),
        (
            "if weight(4) <= 254 { return; }",
            "assert_eq!(weight(4), 255);",
        ),
    ] {
        let found = refusal(&format!("{guard}\n{pin}"));
        assert!(
            !found.is_some_and(|refusal| refusal.is_analyzer_limit()),
            "{guard}"
        );
    }
    // A neighbour on the spared side, or one every type holds, is enough.
    for (guard, pin) in [
        ("if weight(4) > 1 { return; }", "assert_eq!(weight(4), 0);"),
        (
            "if weight(4) < 254 { return; }",
            "assert_eq!(weight(4), 255);",
        ),
        (
            "if weight(4) < 100 { return; }",
            "assert_eq!(weight(4), 101);",
        ),
    ] {
        let found = refusal(&format!("{guard}\n{pin}"));
        assert!(
            found.is_some_and(|refusal| refusal.is_analyzer_limit()),
            "{guard}"
        );
    }
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
    // trap-shine-debug-exact-control: a plain nested `format!` expands
    // to a call over its own operands, which the scan reads for exits.
    assert_eq!(
        refusal("assert_eq!(format!(\"{}\", weight(4)), \"12\");", ""),
        None
    );
    for body in [
        "assert_eq!(format!(\"{}\", weight({ return; 4 })), \"12\");",
        "assert_eq!(std::format!(\"{}\", weight(4)), \"12\");",
        "assert_eq!(format!(\"{}\", dbg!(weight(4))), \"12\");",
        // `::` inside a token tree lexes as two `:` tokens.
        "assert_eq!(std::matches!(weight(4), 12), true);",
    ] {
        assert_eq!(
            refusal(body, ""),
            Some(AssertionRefusal::Syntax(
                AssertionContextRefusal::MacroOperandExit("assert_eq".to_string())
            )),
            "{body}"
        );
    }
    assert!(matches!(
        weight_refusal(
            "use demo::weight;\n#[test]\nfn weighs() {\nassert_eq!(format!(\"{}\", weight(4)), \"12\");\n}\n",
            &[(
                "src/other.rs",
                "macro_rules! format { ($($t:tt)*) => { String::new() } }"
            )],
        ),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
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
                    &assertion_context_macros(),
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
                ("src/a.rs", "use rayon::prelude::*;\n"),
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
        let (ambiguous, _, _, _) =
            trusted_macro_sites_in(&index, &TargetRoots::new(&index), &unresolved);
        assert_eq!(ambiguous, full_scan(&index), "{files:?}");
        assert_eq!(
            ambiguous.len() == assertion_context_macros().len(),
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
    assert!(!NON_RETURNING_MACROS.contains(&"json"));
    let context = assertion_context_macros();
    assert_eq!(context.iter().collect::<BTreeSet<_>>().len(), context.len());
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

/// #8255 fixture: every site class the workspace-site selection ranks. The
/// foreign glob in `src/globs.rs` is a global may-rebind site for every
/// trusted name and saturates the ambiguity set from the pass's likely
/// files; the private `matches` definition in `src/lib.rs` is a
/// crate-local rebinding there; and the two `tests/*.rs` files are
/// separate test-target roots the crate-local site must not reach.
const CONVERGENCE_GLOB: &str = "src/globs.rs";
const CONVERGENCE_OTHER_TESTS: &str = "tests/other_tests.rs";

fn convergence_index() -> RustIndex {
    index(&[
        (
            CONVERGENCE_GLOB,
            "use rayon::prelude::*;\npub fn drain(input: &mut Vec<u8>) {\n    input.clear();\n}\n",
        ),
        (
            LIB,
            "macro_rules! matches { ($($tokens:tt)*) => {} }\npub fn gate(value: u32) -> bool {\n    10 <= value\n}\n",
        ),
        (
            TESTS,
            "use demo::gate;\n\n#[test]\nfn pins() {\n    assert_eq!(gate(10), true);\n}\n",
        ),
        (
            CONVERGENCE_OTHER_TESTS,
            "use demo::gate;\n\n#[test]\nfn pins_elsewhere() {\n    assert_eq!(gate(11), true);\n}\n",
        ),
    ])
}

/// #8255: a refused assertion names its site from the single ambiguity
/// pass. For every trusted name and every asking root — including a test
/// with no recognized root — the retained lookup answers exactly what the
/// replaced per-(name, test root) workspace scan answered.
#[test]
fn a_refusal_site_lookup_matches_the_replaced_scan_for_every_name_and_root() {
    let index = convergence_index();
    let syntax = OwnerPinSyntax::default();
    let roots = syntax.target_roots(&index);
    let resolved_modules = syntax.resolved_module_declarations(&index);
    let resolved = move |path: &Path, line: usize, declaration: &str| {
        resolved_modules.contains(&(path.to_path_buf(), line, declaration.to_string()))
    };
    for name in assertion_context_macros() {
        for test_root in [
            None,
            Some(Path::new(TESTS)),
            Some(Path::new(CONVERGENCE_OTHER_TESTS)),
            Some(Path::new(LIB)),
            Some(Path::new("tests/unindexed_root.rs")),
        ] {
            assert_eq!(
                syntax.workspace_macro_site(name, test_root, &index),
                workspace_macro_binding_site(name, test_root, &index, roots, &resolved),
                "{name} at {test_root:?}"
            );
        }
    }
    // A rebinding outranks a may-rebind site wherever each sits: the
    // crate-local `matches` definition in src/lib.rs beats the earlier
    // foreign glob for a test with no recognized root, while a test root
    // sees only the glob.
    let site = |test_root: Option<&Path>| {
        syntax
            .workspace_macro_site("matches", test_root, &index)
            .map(|(path, site)| (path, site.kind))
    };
    assert_eq!(
        site(None),
        Some((PathBuf::from(LIB), MacroBindingKind::Definition))
    );
    for test_root in [
        Some(Path::new(TESTS)),
        Some(Path::new(CONVERGENCE_OTHER_TESTS)),
    ] {
        let located = site(test_root);
        assert!(located.is_some(), "{test_root:?} must still be ambiguous");
        let Some((path, kind)) = located else {
            continue;
        };
        assert_eq!(path, Path::new(CONVERGENCE_GLOB));
        assert!(
            matches!(kind, MacroBindingKind::ForeignGlob(_)),
            "{test_root:?}: {kind:?}"
        );
    }
}

/// #8255: the site lookup is bounded. The ambiguity pass saturates on the
/// foreign glob and skips the rest of the index; the first site lookup
/// completes that pass once, and however many (name, test root) refusals
/// follow — across both test-target roots — this run pays at most two
/// full-index passes instead of one per (name, test root).
#[test]
fn refusal_site_lookups_pay_at_most_two_full_index_passes() {
    let index = convergence_index();
    let syntax = OwnerPinSyntax::default();
    assert!(
        syntax.workspace_site_scan.borrow().as_ref().is_none(),
        "no scan may run before a consumer asks"
    );
    for name in assertion_context_macros() {
        for test_root in [
            None,
            Some(Path::new(TESTS)),
            Some(Path::new(CONVERGENCE_OTHER_TESTS)),
        ] {
            let _ = syntax.workspace_macro_site(name, test_root, &index);
        }
    }
    assert!(
        syntax
            .workspace_site_scan
            .borrow()
            .as_ref()
            .is_some_and(|scan| scan.complete),
        "the retained scan must complete on demand"
    );
    assert!(
        syntax.workspace_scan_passes() <= 2,
        "{} full-index passes for 96 lookups",
        syntax.workspace_scan_passes()
    );
    // The production refusal path pays the same bound: a predicate probe's
    // refused disclosures across both test roots add no third pass.
    let owner = index
        .functions()
        .iter()
        .find(|function| function.name == "gate" && function.file == Path::new(LIB));
    assert!(owner.is_some(), "the owner must be indexed from {LIB}");
    let owner = owner.unwrap_or(index.functions().at(0));
    let probe = predicate_probe(owner, "10 <= value");
    for tests_path in [TESTS, CONVERGENCE_OTHER_TESTS] {
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(tests_path));
        assert!(test.is_some(), "{tests_path} must be indexed");
        let Some(test) = test else { continue };
        let refusal = syntax.equality_assertion_refusal(&probe, test, &test.assertions[0], &index);
        assert!(
            matches!(
                &refusal,
                Some(AssertionRefusal::MacroBinding { name, site: Some((path, _)) })
                    if name == "assert_eq" && path.as_path() == Path::new(CONVERGENCE_GLOB)
            ),
            "{tests_path}: {refusal:?}"
        );
    }
    assert!(
        syntax.workspace_scan_passes() <= 2,
        "{} full-index passes after the refusal disclosures",
        syntax.workspace_scan_passes()
    );
}

/// #8282: the seam walk shares one run-scoped [`OwnerPinSyntax`] per
/// [`CompactGripContext`], so the #8255 two-full-index-pass bound holds
/// for an entire walk however many seams consult the macro-binding
/// authority. Every fixture owner is a `Vec::push` state write whose
/// related test asserts on a differently named binding: the direct
/// collection observer fails (the asserted subject is not the owner's
/// receiver), so each seam's evidence takes the collection-argument path
/// whose first authority check pays the trusted-macro ambiguity pass. A
/// per-seam fresh instance paid it once per seam here.
#[test]
fn seam_walk_pays_at_most_two_full_index_passes() {
    const PUSH_LIB: &str = "pub fn keep_first(v: &mut Vec<u32>, x: u32) {\n    v.push(x * 1);\n}\npub fn keep_second(v: &mut Vec<u32>, x: u32) {\n    v.push(x * 1);\n}\npub fn keep_third(v: &mut Vec<u32>, x: u32) {\n    v.push(x * 1);\n}\n";
    const PUSH_TESTS: &str = "use demo::{keep_first, keep_second, keep_third};\n\n#[test]\nfn keeps_all() {\n    let mut first = Vec::new();\n    keep_first(&mut first, 11);\n    let mut second = Vec::new();\n    keep_second(&mut second, 12);\n    let mut third = Vec::new();\n    keep_third(&mut third, 13);\n    assert_eq!(first, vec![11]);\n    assert_eq!(second, vec![12]);\n    assert_eq!(third, vec![13]);\n}\n";
    const KEEP_TESTS: &str = "tests/keep_tests.rs";
    let index = index(&[(LIB, PUSH_LIB), (KEEP_TESTS, PUSH_TESTS)]);
    let seams =
        crate::analysis::seam_inventory::inventory_seams_from_index(&[PathBuf::from(LIB)], &index);
    assert!(
        seams.len() >= 3,
        "fixture must yield one state-write seam per push owner, got {}",
        seams.len()
    );
    let context = crate::analysis::test_grip_evidence::CompactGripContext::new(&index);
    for seam in &seams {
        crate::analysis::test_grip_evidence::compact_evidence_for_seam(seam, &context);
    }
    let passes = context.workspace_scan_passes();
    assert!(
        passes >= 1,
        "the walk must consult the shared macro-binding authority for the bound to mean anything"
    );
    assert!(
        passes <= 2,
        "{} full-index passes for a {}-seam walk",
        passes,
        seams.len()
    );
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

const SNAP_LIB: &str = "pub mod celsius {\n    pub fn snap(v: u32) -> u32 {\n        (v + 5) / 10 * 10\n    }\n}\n\npub mod fahrenheit {\n    pub fn snap(v: u32) -> u32 {\n        (v + 6) / 12 * 12\n    }\n}\n\n#[cfg(test)]\nmod snap_tests {\n    use super::celsius::snap;\n\n    #[test]\n    fn snap_seventeen_to_twenty() {\n        assert_eq!(snap(17), 20);\n    }\n}\n";

fn module_owner<'a>(index: &'a RustIndex, id_suffix: &str) -> Option<&'a FunctionSummary> {
    index
        .functions()
        .iter()
        .find(|function| function.id.0.ends_with(id_suffix))
}

fn module_pin(index: &RustIndex, id_suffix: &str, expression: &str) -> Option<OwnerReturnPin> {
    let owner = module_owner(index, id_suffix);
    assert!(owner.is_some(), "`{id_suffix}` must be indexed");
    let owner = owner?;
    assert_eq!(owner.item.container, FunctionContainer::Free);
    OwnerReturnPin::establish(&return_probe(owner, expression), owner, index)
}

/// #6544: same-named module-level rivals no longer refuse the pin outright.
/// The test's `use super::celsius::snap` settles the bare call on
/// `celsius::snap`, so its exact assert pins that owner and never the
/// `fahrenheit::snap` rival it does not import.
#[test]
fn an_imported_owner_among_same_named_rivals_is_pinned_only_through_its_import() {
    let index = index(&[(LIB, SNAP_LIB)]);
    let pin = module_pin(&index, "::celsius::snap", "(v + 5) / 10 * 10");
    assert!(
        pin.is_some(),
        "rivals settled per test must not refuse the pin"
    );
    let admitted = pin
        .map(|pin| admitted_texts(&index, &pin))
        .unwrap_or_default();
    assert_eq!(admitted, vec!["assert_eq!(snap(17), 20);".to_string()]);

    let rival_pin = module_pin(&index, "::fahrenheit::snap", "(v + 6) / 12 * 12");
    assert!(rival_pin.is_some());
    let rival_admitted = rival_pin
        .map(|pin| admitted_texts(&index, &pin))
        .unwrap_or_else(|| vec!["no pin".to_string()]);
    assert!(
        rival_admitted.is_empty(),
        "the import names celsius::snap, never fahrenheit::snap: {rival_admitted:?}"
    );
}

/// Review finding 4: the pin resolves the asserted call itself. A test that
/// imports the rival and also calls the owner by its full path pins the
/// rival through the bare `snap(17)`, never the owner; with the imports
/// swapped (control) the same assertion pins the owner. The per-call
/// resolution is the identity gate; the import-reading reach gate (#7097)
/// refuses the same shape independently.
#[test]
fn a_mixed_owner_and_rival_test_pins_only_through_the_asserted_call() {
    let mixed = SNAP_LIB.replace(
        "    use super::celsius::snap;\n\n    #[test]\n    fn snap_seventeen_to_twenty() {\n",
        "    use super::fahrenheit::snap;\n\n    #[test]\n    fn snap_seventeen_to_twenty() {\n        let _ = crate::celsius::snap(1);\n",
    );
    assert_ne!(mixed, SNAP_LIB, "the fixture must be rewritten");
    let mixed_index = index(&[(LIB, mixed.as_str())]);
    let pin = module_pin(&mixed_index, "::celsius::snap", "(v + 5) / 10 * 10");
    assert!(pin.is_some());
    let admitted = pin
        .map(|pin| admitted_texts(&mixed_index, &pin))
        .unwrap_or_else(|| vec!["no pin".to_string()]);
    assert!(
        admitted.is_empty(),
        "`snap(17)` is fahrenheit's; the path call lends it nothing: {admitted:?}"
    );

    let control = SNAP_LIB.replace(
        "    #[test]\n    fn snap_seventeen_to_twenty() {\n",
        "    #[test]\n    fn snap_seventeen_to_twenty() {\n        let _ = crate::fahrenheit::snap(1);\n",
    );
    assert_ne!(control, SNAP_LIB, "the control fixture must be rewritten");
    let control_index = index(&[(LIB, control.as_str())]);
    let pin = module_pin(&control_index, "::celsius::snap", "(v + 5) / 10 * 10");
    let admitted = pin
        .map(|pin| admitted_texts(&control_index, &pin))
        .unwrap_or_default();
    assert_eq!(admitted, vec!["assert_eq!(snap(17), 20);".to_string()]);
}

/// #6544 trap pair: with only `use super::*`, the bare `snap` is bound by
/// no scope ripr can read (both rivals sit in sibling modules), so neither
/// rival is pinned.
#[test]
fn a_bare_call_no_import_settles_pins_neither_rival() {
    let source = SNAP_LIB.replace("    use super::celsius::snap;\n", "    use super::*;\n");
    let index = index(&[(LIB, source.as_str())]);
    let pin = module_pin(&index, "::celsius::snap", "(v + 5) / 10 * 10");
    assert!(pin.is_some());
    let admitted = pin
        .map(|pin| admitted_texts(&index, &pin))
        .unwrap_or_else(|| vec!["no pin".to_string()]);
    assert!(admitted.is_empty(), "{admitted:?}");
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
    field_pin_of(index, lib, "clone", line_text, expression)
}

/// A `field_construction` probe on the first line of owner `name` whose
/// trimmed text is `line_text`.
fn field_pin_of(
    index: &RustIndex,
    lib: &str,
    name: &str,
    line_text: &str,
    expression: &str,
) -> Option<OwnerReturnPin> {
    let owner = owner(index, name);
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

/// Only a `use` or `extern crate` item, a macro's arguments or an attribute
/// renames. A cast to the field's type (`letters as u32`) elsewhere in the
/// workspace binds no name, so it leaves `u32` the standard type, and a
/// qualified path (`<Window as Clone>`) leaves `Clone` the standard trait.
/// The same name renamed in a `use` group, a function-local `use`, a macro
/// body, a macro invocation's arguments or an attribute still refuses
/// (spot-window-clone-field-whole-equality, #7926 review).
#[test]
fn a_cast_or_qualified_path_is_not_a_rename_of_the_field_type() {
    let glob_tests = "#[cfg(test)]\nmod tests {\n    use super::*;\n}\n";
    let with = |extra: &str| format!("{WINDOW_LIB}{glob_tests}{extra}");
    for (lib, credits) in [
        (with(""), true),
        (
            with("pub fn width(letters: usize) -> u32 {\n    letters as u32\n}\n"),
            true,
        ),
        // A raw identifier `r#use` is not the `use` keyword (#7936).
        (
            with(
                "pub fn width(letters: usize) -> u32 {\n    let r#use = letters as u32;\n    r#use\n}\n",
            ),
            true,
        ),
        // Both used to read as renames of `Clone` and `PartialEq`.
        (
            with("pub fn dup(w: &Window) -> Window {\n    <Window as Clone>::clone(w)\n}\n"),
            true,
        ),
        (
            with(
                "pub fn same(a: &Window, b: &Window) -> bool {\n    <Window as PartialEq>::eq(a, b)\n}\n",
            ),
            true,
        ),
        // Precise capturing is not an import, so the cast after it stays a cast.
        (
            with("pub fn width(letters: usize) -> impl Sized + use<> {\n    letters as u32\n}\n"),
            true,
        ),
        // Rooted in `std`, so the use-statement scan admits the path and
        // only the rename itself refuses.
        (with("use std::{num::Wrapping as u32};\n"), false),
        (
            with("pub fn f() {\n    use std::num::Wrapping as u32;\n}\n"),
            false,
        ),
        (
            with(
                "macro_rules! bind {\n    () => {\n        use std::num::Wrapping as u32;\n    };\n}\n",
            ),
            false,
        ),
        (with("extern crate other as u32;\n"), false),
        // A macro may expand its arguments into a `use`.
        (
            with(
                "macro_rules! import {\n    ($p:path as $n:ident) => {\n        use $p as $n;\n    };\n}\nimport!(std::num::Wrapping as u32);\n",
            ),
            false,
        ),
        (with("reexport! { std::num::Wrapping as u32 }\n"), false),
        (
            with("#[import(std::num::Wrapping as u32)]\npub fn f() {}\n"),
            false,
        ),
        // A Unicode macro name expands its arguments too.
        (with("δ!(std::num::Wrapping as u32);\n"), false),
    ] {
        let (_, pin) = clone_field_pin(&lib, WINDOW_TESTS);
        assert_eq!(pin.is_some(), credits, "{lib}");
    }
    // A non-ASCII identifier before a macro `!` must not split a character.
    let (_, pin) = clone_field_pin(
        &with(
            "pub fn größe() -> u32 {\n    let maß = 3;\n    maß as u32\n}\nmacro_rules! größe_m { () => {} }\npub fn g() { größe_m!(); }\n",
        ),
        WINDOW_TESTS,
    );
    assert!(pin.is_some());
    // Another workspace file: `use super::*;` reaches the workspace-wide
    // re-export scan, which reads every file's renames but not its casts.
    let lib = format!("{WINDOW_LIB}{glob_tests}");
    for (other, credits) in [
        (
            "pub fn width(letters: usize) -> u32 {\n    letters as u32\n}\n",
            true,
        ),
        ("extern crate other as u32;\n", false),
    ] {
        let index = index(&[
            (LIB, lib.as_str()),
            ("src/score.rs", other),
            (TESTS, WINDOW_TESTS),
        ]);
        let pin = field_pin(&index, &lib, "start: self.start,", "start: self.start,");
        assert_eq!(pin.is_some(), credits, "{other}");
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
                    ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                        ..Default::default()
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
                        ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
                    ..Default::default()
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
fn a_feature_gated_test_is_refused_for_its_attribute_not_a_macro_rebinding() {
    // #8388, rand 94c9078 `uniform_other.rs` `test_char_bad_deser`: a test
    // carrying `#[cfg(feature = "serde")]` is "assertion not credited" because
    // the feature may be off in the build under analysis, so the test may not
    // run at all. No macro is rebound anywhere; the plain `assert!` and
    // `assert_eq!` spellings are credited without the gate.
    let body = "fn char_bad_deser() {\n    let result = weight(4);\n    assert!(result > 0);\n    assert_eq!(result, 12);\n}\n";
    let prelude = "use demo::weight;\n";
    let gated = format!("{prelude}#[test]\n#[cfg(feature = \"serde\")]\n{body}");
    let gate = "#[cfg(feature = \"serde\")]".to_string();
    let refusal = weight_refusal(&gated, &[]);
    assert_eq!(
        refusal,
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::TestAttribute(gate.clone())
        ))
    );
    // The exact-value `assert_eq!` oracle (the second assertion) is refused
    // for the same attribute, not only the first assertion.
    assert_eq!(weight_refusal_nth(&gated, &[], None, 1), refusal);
    // The gate is unknown availability, not a settled outcome: an analyzer
    // limit that withholds a gap, and the clause names the attribute.
    let clause = refusal.as_ref().map(AssertionRefusal::describe);
    assert!(
        refusal.is_some_and(|refusal| refusal.is_analyzer_limit())
            && clause.as_deref().is_some_and(|clause| {
                clause.contains(&gate) && clause.contains("may change whether or how it runs")
            }),
        "{clause:?}"
    );
    // The same test without the gate is credited, so the attribute alone is
    // the cause.
    let plain = format!("{prelude}#[test]\n{body}");
    assert_eq!(weight_refusal(&plain, &[]), None);
    assert_eq!(weight_refusal_nth(&plain, &[], None, 1), None);
    // Fail closed: a glob of a foreign crate still refuses the ungated test,
    // for the `assert_eq!` oracle as well as the first assertion.
    let foreign = format!("use rayon::prelude::*;\n{plain}");
    for nth in 0..2 {
        assert!(
            matches!(
                weight_refusal_nth(&foreign, &[], None, nth),
                Some(AssertionRefusal::Syntax(
                    AssertionContextRefusal::MacroBinding(_)
                ))
            ),
            "assertion {nth}"
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
        // A macro body's binding items (#5991).
        (
            "macro_rules! m { () => { macro_rules! assert_eq { () => {} } }; }\n",
            names(&["assert_eq"]),
        ),
        (
            "macro_rules! m { () => { use a::check as vec; assert!(true); }; }\n",
            names(&["vec"]),
        ),
        ("macro_rules! m { () => { use other::*; }; }\n", any.clone()),
        (
            "macro_rules! m { () => { #[macro_use] extern crate log; }; }\n",
            any.clone(),
        ),
        (
            "macro_rules! m { () => { use super::*; assert_eq!(1, 1); }; }\n",
            names(&[]),
        ),
        (
            "macro_rules! vec { ($n:ident) => { macro_rules! $n { () => {} } }; }\n",
            any.clone(),
        ),
        (
            "macro_rules! define { ($n:ident) => { macro_rules! $n { () => {} } }; }\n",
            names(&[]),
        ),
        // A matcher matches the invocation and emits nothing, so a literal
        // `use` in it binds nothing.
        (
            "macro_rules! m { (use other::assert_eq;) => {}; }\n",
            names(&[]),
        ),
        ("macro_rules! m { (use other::*) => {}; }\n", names(&[])),
        // A direct fragment leaf names the import; a rename target or a
        // path segment before `::` is what binds instead. The definition's
        // own name stays recorded either way, and a wrongly recorded
        // fragment would widen these to `Any`.
        (
            "macro_rules! vec { ($n:ident) => { use $n; }; }\n",
            any.clone(),
        ),
        (
            "macro_rules! vec { ($n:ident) => { use $n as check; }; }\n",
            names(&["vec"]),
        ),
        (
            "macro_rules! vec { ($n:ident) => { use $n::path; }; }\n",
            names(&["vec"]),
        ),
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

/// RIPR-SPEC-0225: `build` returns a `Config` literal; `parse` returns it
/// inside `Ok(..)` after a `?`.
const CONFIG_LIB: &str = r#"#[derive(Debug, Clone, PartialEq)]
pub struct Count(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub retries: u32,
    pub name: String,
    pub count: Count,
}

pub fn build(n: u32) -> Config {
    Config {
        retries: n + 1,
        name: "x".into(),
        count: Count(n),
    }
}

pub fn parse(s: &str) -> Result<Config, String> {
    let n: u32 = s.parse().map_err(|_| String::from("bad"))?;
    Ok(Config {
        retries: n + 1,
        name: "x".into(),
        count: Count(n),
    })
}
"#;

/// The `retries` field pin of `owner` against `tests` (one test).
fn whole_value_pin(
    lib: &str,
    owner_name: &str,
    tests: &str,
) -> (RustIndex, Option<OwnerReturnPin>) {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = field_pin_of(
        &index,
        lib,
        owner_name,
        "retries: n + 1,",
        "retries: n + 1,",
    );
    (index, pin)
}

fn whole_value_admits(lib: &str, owner_name: &str, body: &str) -> bool {
    let tests = format!("use demo::*;\n#[test]\nfn pins() {{\n{body}\n}}\n");
    let (index, pin) = whole_value_pin(lib, owner_name, &tests);
    assert!(
        pin.is_some(),
        "the whole-value field pin must establish: {lib}"
    );
    pin.is_some_and(|pin| !admitted_texts(&index, &pin).is_empty())
}

/// `CONFIG_LIB` plus a method owner returning a `Self { .. }` tail and an
/// owner returning `Some(..)` against a declared `Option<Config>`.
const CONFIG_VARIANTS_LIB: &str = r#"
impl Config {
    pub fn bumped(&self, n: u32) -> Self {
        Self {
            retries: n + 1,
            name: "x".into(),
            count: Count(n),
        }
    }
}

pub fn maybe(n: u32) -> Option<Config> {
    Some(Config {
        retries: n + 1,
        name: "x".into(),
        count: Count(n),
    })
}
"#;

/// #7066 review: the `Self { .. }` tail of a method resolves to the impl's
/// type and pins through a typed receiver; a `Some(..)` tail pins only
/// against `Some(..)` of the literal, not `Ok(..)` or the bare literal.
#[test]
fn a_whole_value_field_pin_reads_self_tails_methods_and_option_wrappers() {
    let lib = format!("{CONFIG_LIB}{CONFIG_VARIANTS_LIB}");
    let literal = r#"Config { retries: 4, name: "x".into(), count: Count(3) }"#;
    let base = r#"let base = Config { retries: 0, name: "y".into(), count: Count(0) };"#;
    assert!(whole_value_admits(
        &lib,
        "bumped",
        &format!("{base}\nassert_eq!(base.bumped(3), {literal});")
    ));
    // The receiver must type as the impl's `Config`.
    assert!(!whole_value_admits(
        &lib,
        "bumped",
        &format!("let base = load();\nassert_eq!(base.bumped(3), {literal});")
    ));
    assert!(whole_value_admits(
        &lib,
        "maybe",
        &format!("assert_eq!(maybe(3), Some({literal}));")
    ));
    for body in [
        format!("assert_eq!(maybe(3), Ok({literal}));"),
        format!("assert_eq!(maybe(3), {literal});"),
    ] {
        assert!(!whole_value_admits(&lib, "maybe", &body), "{body}");
    }
}

/// RIPR-SPEC-0225 decision 6 (ledger-receipt-remaining): beside an `Ok(..)`
/// tail, an early `return Err(..)` exits as `?` does, and the test may
/// compare `owner(..).unwrap()` or `.expect("..")` with the bare literal.
/// Any other early return, an unwrapped bare-tail owner, `unwrap_or_default`
/// or a computed `expect` message does not pin.
#[test]
fn an_unwrapped_ok_tail_with_early_err_returns_pins_its_field() {
    let question = "    let n: u32 = s.parse().map_err(|_| String::from(\"bad\"))?;\n";
    let with_exit = |exit: &str| {
        CONFIG_LIB.replace(
            question,
            &format!("{question}    if n == 0 {{\n        {exit}\n    }}\n"),
        )
    };
    let guarded = with_exit("return Err(String::from(\"zero\"));");
    let literal = r#"Config { retries: 4, name: "x".into(), count: Count(3) }"#;
    for (body, pins) in [
        (format!("assert_eq!(parse(\"3\"), Ok({literal}));"), true),
        (
            format!("assert_eq!(parse(\"3\").unwrap(), {literal});"),
            true,
        ),
        (
            format!("assert_eq!({literal}, parse(\"3\").unwrap());"),
            true,
        ),
        (
            format!("assert_eq!(parse(\"3\").expect(\"parses\"), {literal});"),
            true,
        ),
        (
            format!("let c = parse(\"3\").unwrap();\nassert_eq!(c, {literal});"),
            true,
        ),
        (
            format!("assert_eq!(parse(\"3\").unwrap(), Ok({literal}));"),
            false,
        ),
        (
            format!("assert_eq!(parse(\"3\").unwrap_or_default(), {literal});"),
            false,
        ),
        (
            format!("assert_eq!(parse(\"3\").expect(&msg), {literal});"),
            false,
        ),
        (format!("assert_eq!(parse(\"3\"), {literal});"), false),
        (
            format!("let mut c = parse(\"3\").unwrap();\nassert_eq!(c, {literal});"),
            false,
        ),
    ] {
        assert_eq!(whole_value_admits(&guarded, "parse", &body), pins, "{body}");
    }
    // #7940 review: an async owner's unwrap reads through `.await`, inline
    // and once-bound alike; the un-awaited future does not.
    let awaited = guarded.replace("pub fn parse", "pub async fn parse");
    assert_ne!(awaited, guarded);
    for (body, pins) in [
        (
            format!("assert_eq!(parse(\"3\").await, Ok({literal}));"),
            true,
        ),
        (
            format!("assert_eq!(parse(\"3\").await.unwrap(), {literal});"),
            true,
        ),
        (
            format!("let c = parse(\"3\").await.unwrap();\nassert_eq!(c, {literal});"),
            true,
        ),
        (
            format!("let c = parse(\"3\").unwrap();\nassert_eq!(c, {literal});"),
            false,
        ),
        (
            format!("let c = parse(\"3\");\nassert_eq!(c, Ok({literal}));"),
            false,
        ),
    ] {
        let tests = format!(
            "use demo::{{parse, Config, Count}};\n#[tokio::test]\nasync fn pins() {{\n{body}\n}}\n"
        );
        let (index, pin) = whole_value_pin(&awaited, "parse", &tests);
        assert!(
            pin.is_some(),
            "the async whole-value field pin must establish"
        );
        let admits = pin.is_some_and(|pin| !admitted_texts(&index, &pin).is_empty());
        assert_eq!(admits, pins, "{body}");
    }
    // A bare-tail owner has no wrapped value to unwrap.
    assert!(!whole_value_admits(
        CONFIG_LIB,
        "build",
        &format!("assert_eq!(build(3).unwrap(), {literal});")
    ));
    // An early exit that may hand back another `Ok` refuses the pin.
    let tests = format!(
        "use demo::*;\n#[test]\nfn pins() {{\n    assert_eq!(parse(\"3\"), Ok({literal}));\n}}\n"
    );
    for exit in [
        "return Ok(Config { retries: 9, name: \"x\".into(), count: Count(n) });",
        "return parse(\"1\");",
        "return Err(String::new()).or(parse(\"1\"));",
    ] {
        let lib = with_exit(exit);
        let (_, pin) = whole_value_pin(&lib, "parse", &tests);
        assert!(pin.is_none(), "{lib}");
    }
    // A `Some(..)` tail admits an early `return None` and nothing else.
    let maybe_tail = "pub fn maybe(n: u32) -> Option<Config> {\n";
    let maybe_with = |exit: &str| {
        format!("{CONFIG_LIB}{CONFIG_VARIANTS_LIB}").replace(
            maybe_tail,
            &format!("{maybe_tail}    if n == 0 {{\n        {exit}\n    }}\n"),
        )
    };
    let unwrap_maybe = format!("assert_eq!(maybe(3).unwrap(), {literal});");
    assert!(whole_value_admits(
        &maybe_with("return None;"),
        "maybe",
        &unwrap_maybe
    ));
    let maybe_tests = format!("use demo::*;\n#[test]\nfn pins() {{\n{unwrap_maybe}\n}}\n");
    for exit in [
        "return Some(Config { retries: 9, name: \"x\".into(), count: Count(n) });",
        "return maybe(1);",
        "return FALLBACK;",
    ] {
        let lib = maybe_with(exit);
        let (_, pin) = whole_value_pin(&lib, "maybe", &maybe_tests);
        assert!(pin.is_none(), "{lib}");
    }
    // Review: an `Err` the workspace rebinds may build an `Ok`, so the early
    // exit is not the other variant.
    let early_literal =
        with_exit("return Err(Config { retries: 4, name: \"x\".into(), count: Count(n) });");
    assert!(whole_value_pin(&early_literal, "parse", &tests).1.is_some());
    for rebinding in [
        "#[allow(non_snake_case)]\npub fn Err(c: Config) -> Result<Config, String> {\n    Ok(c)\n}\n",
        "use std::result::Result::Ok as Err;\n",
    ] {
        let lib = format!("{rebinding}{early_literal}");
        let (_, pin) = whole_value_pin(&lib, "parse", &tests);
        assert!(pin.is_none(), "{lib}");
    }
    // #7940 review: a std-rooted alias inside the owner itself also rebinds.
    let signature = "pub fn parse(s: &str) -> Result<Config, String> {\n";
    let local = early_literal.replace(
        signature,
        &format!("{signature}    use std::result::Result::Ok as Err;\n"),
    );
    assert_ne!(local, early_literal);
    let (_, pin) = whole_value_pin(&local, "parse", &tests);
    assert!(pin.is_none(), "{local}");
    // So does a rebound `None` beside a `Some` tail.
    let none_const = format!(
        "#[allow(non_upper_case_globals)]\npub const None: Option<u32> = Some(1);\n{}",
        maybe_with("return None;")
    );
    let (_, pin) = whole_value_pin(&none_const, "maybe", &maybe_tests);
    assert!(pin.is_none(), "{none_const}");
}

/// RIPR-SPEC-0225 decision 7, acceptance examples 6 and 7: a zero-argument
/// helper returning a bare literal, and a functional update whose base is
/// the one manual `Default` returning a bare literal, read as that literal.
#[test]
fn an_expected_helper_or_default_base_reads_as_its_literal() {
    let admits = |lib: &str, items: &str, body: &str| {
        let tests = format!("use demo::*;\n{items}#[test]\nfn pins() {{\n{body}\n}}\n");
        let (index, pin) = whole_value_pin(lib, "build", &tests);
        assert!(
            pin.is_some(),
            "the whole-value field pin must establish: {lib}"
        );
        pin.is_some_and(|pin| !admitted_texts(&index, &pin).is_empty())
    };
    let helper = |body: &str| format!("fn expected() -> Config {{\n    {body}\n}}\n");
    let literal = r#"Config { retries: 4, name: "x".into(), count: Count(3) }"#;
    let call = "assert_eq!(build(3), expected());";
    assert!(admits(CONFIG_LIB, &helper(literal), call));
    assert!(admits(
        CONFIG_LIB,
        &helper(literal),
        "assert_eq!(expected(), build(3));"
    ));
    // A helper that computes, takes an argument, or is not the only `fn
    // expected` (or not a plain item) gives no credit.
    for (items, body) in [
        (
            helper(&format!("let c = {literal};\n    c")),
            call.to_string(),
        ),
        (
            helper(r#"Config { retries: build(3).retries, name: "x".into(), count: Count(3) }"#),
            call.to_string(),
        ),
        (helper("build(3)"), call.to_string()),
        (
            helper(&format!("let _ = build(3);\n    {literal}")),
            call.to_string(),
        ),
        (
            format!("fn expected(n: u32) -> Config {{\n    {literal}\n}}\n"),
            "assert_eq!(build(3), expected(4));".to_string(),
        ),
        (
            format!(
                "{}mod more {{\n    {}}}\n",
                helper(literal),
                helper(literal)
            ),
            call.to_string(),
        ),
        (
            format!(
                "{}macro_rules! make {{\n    () => {{ {} }};\n}}\nmod hidden {{\n    make!();\n}}\n",
                helper(literal),
                helper(literal)
            ),
            call.to_string(),
        ),
        (
            helper(literal),
            format!("let expected = || build(3);\n{call}"),
        ),
        (format!("#[inline]\n{}", helper(literal)), call.to_string()),
        (format!("async {}", helper(literal)), call.to_string()),
        (
            helper(literal),
            "assert_eq!(build(3), more::expected());".to_string(),
        ),
    ] {
        assert!(!admits(CONFIG_LIB, &items, &body), "{items}{body}");
    }
    // The helper's own literal must name `retries` independently.
    assert!(!admits(
        CONFIG_LIB,
        &helper(r#"Config { name: "x".into(), count: Count(3), retries: four() }"#),
        call
    ));

    let default_of = |body: &str| {
        format!(
            "{CONFIG_LIB}impl Default for Config {{\n    fn default() -> Self {{\n        {body}\n    }}\n}}\n"
        )
    };
    let defaulted = default_of(r#"Config { retries: 4, name: String::new(), count: Count(3) }"#);
    let update = r#"assert_eq!(build(3), Config { name: "x".into(), ..Config::default() });"#;
    assert!(admits(&defaulted, "", update));
    assert!(admits(
        &defaulted,
        "",
        r#"assert_eq!(build(3), Config { name: "x".into(), ..Default::default() });"#
    ));
    assert!(admits(
        &default_of(r#"Self { retries: 4, name: String::new(), count: Count(3) }"#),
        "",
        update
    ));
    // The test's explicit field wins over the default's.
    assert!(!admits(
        &defaulted,
        "",
        r#"assert_eq!(build(3), Config { retries: four(), ..Config::default() });"#
    ));
    for lib in [
        default_of(r#"Config { retries: build(3).retries, name: String::new(), count: Count(3) }"#),
        default_of("build(3)"),
        default_of(
            r#"let c = Config { retries: 4, name: String::new(), count: Count(3) };
        c"#,
        ),
        default_of(r#"Config { retries: 4, ..base() }"#),
        format!(
            "{defaulted}impl Config {{\n    pub fn default() -> Self {{\n        build(0)\n    }}\n}}\n"
        ),
        format!("{defaulted}pub trait Default {{\n    fn default() -> Self;\n}}\n"),
        CONFIG_LIB.replace(
            "#[derive(Debug, Clone, PartialEq)]\npub struct Config",
            "#[derive(Debug, Clone, PartialEq, Default)]\npub struct Config",
        ),
    ] {
        assert!(!admits(&lib, "", update), "{lib}");
    }
    assert!(!admits(
        &defaulted,
        "",
        r#"assert_eq!(build(3), Config { name: "x".into(), ..base() });"#
    ));
    // Review (#7954): a glob from outside the workspace, in the test or
    // reaching it through another workspace file, may bind `expected` or
    // `Default` to an item the index cannot read.
    assert!(!admits(
        CONFIG_LIB,
        &format!("use other::*;\n{}", helper(literal)),
        call
    ));
    // `None` when the pin itself refuses: a foreign glob the library
    // re-exports already withholds the type name's reach.
    let through = |lib: &str, helpers: &str, body: &str| {
        let lib = format!("{lib}pub mod helpers;\npub use crate::helpers::*;\n");
        let tests = format!("use demo::*;\n#[test]\nfn pins() {{\n{body}\n}}\n");
        let index = index(&[(LIB, &lib), (HELPERS, helpers), (TESTS, &tests)]);
        field_pin_of(&index, &lib, "build", "retries: n + 1,", "retries: n + 1,")
            .map(|pin| !admitted_texts(&index, &pin).is_empty())
    };
    let fixtures = format!("{CONFIG_LIB}mod fixtures {{\n    {}}}\n", helper(literal));
    for (lib, body) in [(fixtures.as_str(), call), (defaulted.as_str(), update)] {
        assert_eq!(through(lib, "", body), Some(true), "{lib}");
        assert_ne!(
            through(lib, "pub use other::*;\n", body),
            Some(true),
            "{lib}"
        );
        // A standard-library glob leaves the pin standing; decision 7
        // still refuses to read through a glob it cannot list, and so
        // through a workspace path whose segment an import binds.
        for helpers in [
            "pub use std::fmt::*;\n",
            "extern crate other as ext;\npub use self::ext::*;\n",
            "pub use other::prelude;\npub use self::prelude::*;\n",
        ] {
            assert_eq!(through(lib, helpers, body), Some(false), "{helpers}{lib}");
        }
        assert_eq!(
            through(lib, "pub mod inner {}\npub use self::inner::*;\n", body),
            Some(true),
            "{lib}"
        );
    }
    // Review (#7954): `fn r#default` is `default`; an inherent one wins
    // `Config::default()` over the trait's literal.
    assert!(!admits(
        &format!(
            "{defaulted}impl Config {{\n    pub fn r#default() -> Self {{\n        build(0)\n    }}\n}}\n"
        ),
        "",
        update
    ));
    // Review: a `#[cfg]`-disabled explicit field leaves the default's
    // computed value in force, so it may not override it in the merge.
    let computed =
        default_of(r#"Config { retries: build(3).retries, name: String::new(), count: Count(3) }"#);
    assert!(admits(
        &computed,
        "",
        r#"assert_eq!(build(3), Config { retries: 4, name: "x".into(), ..Config::default() });"#
    ));
    assert!(!admits(
        &computed,
        "",
        r#"assert_eq!(build(3), Config { #[cfg(any())] retries: 4, name: "x".into(), ..Config::default() });"#
    ));
    // A raw-identifier helper, an attributed `impl Default`, or a derived
    // `Default` beside an inherent `default` gives no credit.
    assert!(!admits(
        CONFIG_LIB,
        &format!("fn r#expected() -> Config {{\n    {literal}\n}}\n"),
        "assert_eq!(build(3), r#expected());"
    ));
    // The one `impl Default` is another type's, and an `==` assertion or a
    // borrowed check helper is not decision 7's `assert_eq!` shape.
    assert!(!admits(
        &format!(
            "{CONFIG_LIB}pub struct Other {{\n    pub retries: u32,\n}}\nimpl Default for Other {{\n    fn default() -> Self {{\n        Self {{ retries: 4 }}\n    }}\n}}\n"
        ),
        "",
        update
    ));
    assert!(!admits(
        CONFIG_LIB,
        &helper(literal),
        "assert!(build(3) == expected());"
    ));
    let derived = CONFIG_LIB.replace(
        "#[derive(Debug, Clone, PartialEq)]\npub struct Config",
        "#[derive(Debug, Clone, PartialEq, Default)]\npub struct Config",
    );
    for lib in [
        defaulted.replace(
            "impl Default for Config",
            "#[cfg(any())]\nimpl Default for Config",
        ),
        format!(
            "{derived}impl Config {{\n    pub fn default() -> Self {{\n        Config {{ retries: 4, name: String::new(), count: Count(3) }}\n    }}\n}}\n"
        ),
    ] {
        assert_ne!(lib, defaulted);
        assert!(
            !admits(
                &lib,
                "",
                r#"assert_eq!(build(3), Config { name: "x".into(), ..Default::default() });"#
            ),
            "{lib}"
        );
    }
}

/// RIPR-SPEC-0225 acceptance examples 1, 3, 4 and 5: a whole-value literal
/// that names the changed field with an independent value pins it,
/// directly, through `Ok(..)` and through a once-used `let` of the owner
/// call. (Examples 9, 11, 14 and 16 are verdict-corpus rows.)
#[test]
fn a_whole_value_literal_naming_the_field_pins_it() {
    let literal = r#"Config { retries: 4, name: "x".into(), count: Count(3) }"#;
    for body in [
        format!("assert_eq!(build(3), {literal});"),
        format!("assert_eq!({literal}, build(3));"),
        format!("let c = build(3);\nassert_eq!(c, {literal});"),
        format!("let c: Config = build(3);\nassert_eq!({literal}, c);"),
        format!("let c = build(3);\nlet e = {literal};\nassert_eq!(c, e);"),
        r#"assert_eq!(build(3), Config { retries: (4), name: String::from("x"), count: Count(3) });"#
            .to_string(),
    ] {
        assert!(whole_value_admits(CONFIG_LIB, "build", &body), "{body}");
    }
    assert!(whole_value_admits(
        CONFIG_LIB,
        "parse",
        &format!("assert_eq!(parse(\"3\"), Ok({literal}));")
    ));
}

/// RIPR-SPEC-0225 "No credit" and acceptance examples 8, 10, 12, 13 and
/// 15: a dependent binding, an unresolvable helper or functional-update base
/// (`CONFIG_LIB` declares neither `expected` nor a `Default`), another call,
/// `assert_ne!`, a value copied from the result, a `let mut` or a second use
/// of the binding, a mismatched wrapper, and a dependent expected value give
/// no credit.
#[test]
fn a_whole_value_literal_without_an_independent_field_value_does_not_pin() {
    let literal = r#"Config { retries: 4, name: "x".into(), count: Count(3) }"#;
    for body in [
        format!("let c = build(3);\nlet e = {literal};\nlet _ = &e;\nassert_eq!(c, e);"),
        "assert_eq!(build(3), expected());".to_string(),
        r#"assert_eq!(build(3), Config { name: "x".into(), ..Config::default() });"#.to_string(),
        r#"assert_eq!(build(3), Config { retries: 4, ..base() });"#.to_string(),
        format!("assert_eq!(other(3), {literal});"),
        format!("assert_ne!(build(3), {literal});"),
        r#"let c = build(3);
assert_eq!(c, Config { retries: c.retries, name: "x".into(), count: Count(3) });"#
            .to_string(),
        format!("let mut c = build(3);\nc.retries = 4;\nassert_eq!(c, {literal});"),
        format!("let mut c = build(3);\nassert_eq!(c, {literal});"),
        format!("let c = build(3);\nlet r = c.retries;\nassert_eq!(c, {literal});\nlet _ = r;"),
        format!("let c = build(3);\nlet c = build(4);\nassert_eq!(c, {literal});"),
        format!("assert_eq!(build(3), Some({literal}));"),
        "assert_eq!(build(3), Other { retries: 4 });".to_string(),
        r#"assert_eq!(build(3), Config { retries, name: "x".into(), count: Count(3) });"#
            .to_string(),
        r#"assert_eq!(build(3), Config { retries: RETRIES, name: "x".into(), count: Count(3) });"#
            .to_string(),
        r#"assert_eq!(build(3), Config { retries: four(), name: "x".into(), count: Count(3) });"#
            .to_string(),
        r#"assert_eq!(build(3), Config { retries: 2 + 2, name: "x".into(), count: Count(3) });"#
            .to_string(),
        r#"assert_eq!(build(3), Config { retries: build(3).retries, name: "x".into(), count: Count(3) });"#
            .to_string(),
        r#"assert_eq!(build(3), Config { retries: "4".parse().unwrap(), name: "x".into(), count: Count(3) });"#
            .to_string(),
        r#"assert_eq!(build(3), Config { retries: 4, retries: 5, name: "x".into(), count: Count(3) });"#
            .to_string(),
    ] {
        assert!(!whole_value_admits(CONFIG_LIB, "build", &body), "{body}");
    }
    assert!(!whole_value_admits(
        CONFIG_LIB,
        "parse",
        &format!("assert_eq!(parse(\"3\"), {literal});")
    ));
    // Review: a CamelCase constant or fn is computed, and a `let` in an inner
    // block leaves the operand naming an outer item.
    let camel_const = CONFIG_LIB.to_string()
        + "pub struct Limits;\nimpl Limits {\n    #[allow(non_upper_case_globals)]\n    pub const Max: u32 = 4;\n}\n";
    assert!(!whole_value_admits(
        &camel_const,
        "build",
        r#"assert_eq!(build(3), Config { retries: Limits::Max, name: "x".into(), count: Count(3) });"#
    ));
    assert!(!whole_value_admits(
        CONFIG_LIB,
        "build",
        &format!("{{ let c = build(3); }}\nassert_eq!(c, {literal});")
    ));
    // #7066 review: a fn renamed to a CamelCase alias is still computed.
    let helper = CONFIG_LIB.to_string() + "pub fn four() -> u32 {\n    4\n}\n";
    assert!(!whole_value_admits(
        &helper,
        "build",
        r#"use demo::four as Four;
assert_eq!(build(3), Config { retries: Four(), name: "x".into(), count: Count(3) });"#
    ));
}

/// RIPR-SPEC-0225 owner-side gates, acceptance example 2: a manual
/// `PartialEq` on the type, a field type from outside the workspace, an attribute on the field, a second exit, a conditional field
/// value, a mismatched declared return type, or a tail that is not the
/// literal leaves the pin unestablished.
#[test]
fn a_whole_value_field_pin_needs_derived_equality_and_the_returned_literal() {
    let manual_eq = CONFIG_LIB.replacen(
        "#[derive(Debug, Clone, PartialEq)]\npub struct Config",
        "#[derive(Debug, Clone)]\npub struct Config",
        1,
    ) + "impl PartialEq for Config {\n    fn eq(&self, o: &Self) -> bool { self.name == o.name }\n}\n";
    let foreign_field = CONFIG_LIB.replace("pub retries: u32,", "pub retries: other::Retries,");
    let attributed_field =
        CONFIG_LIB.replace("pub retries: u32,", "#[eq(ignore)]\n    pub retries: u32,");
    let early_return = CONFIG_LIB.replace(
        "pub fn build(n: u32) -> Config {\n",
        "pub fn build(n: u32) -> Config {\n    if n == 0 {\n        return build(1);\n    }\n",
    );
    let conditional = CONFIG_LIB.replace(
        "pub fn build(n: u32) -> Config {\n    Config {\n        retries: n + 1,",
        "pub fn build(n: u32) -> Config {\n    Config {\n        retries: if n > 9 { n } else { n + 1 },",
    );
    let bound_first = CONFIG_LIB
        .replace(
            "pub fn build(n: u32) -> Config {\n    Config {",
            "pub fn build(n: u32) -> Config {\n    let c = Config {",
        )
        .replace(
            "        count: Count(n),\n    }\n}\n\npub fn parse",
            "        count: Count(n),\n    };\n    c\n}\n\npub fn parse",
        );
    let qualified_tail = CONFIG_LIB.replace(
        "pub fn build(n: u32) -> Config {\n    Config {",
        "pub fn build(n: u32) -> Config {\n    crate::Config {",
    );
    let boxed_return = CONFIG_LIB.replace(
        "pub fn build(n: u32) -> Config {",
        "pub fn build(n: u32) -> Box<Config> {",
    );
    for lib in [
        manual_eq,
        foreign_field,
        attributed_field,
        early_return,
        bound_first,
        qualified_tail,
        boxed_return,
    ] {
        let tests = "use demo::*;\n#[test]\nfn pins() {\n    assert_eq!(build(3), Config { retries: 4, name: \"x\".into(), count: Count(3) });\n}\n";
        let (_, pin) = whole_value_pin(&lib, "build", tests);
        assert!(pin.is_none(), "{lib}");
    }
    let tests = "use demo::*;\n#[test]\nfn pins() {\n    assert_eq!(build(3), Config { retries: 4, name: \"x\".into(), count: Count(3) });\n}\n";
    let index = index(&[(LIB, &conditional), (TESTS, tests)]);
    assert!(
        field_pin_of(
            &index,
            &conditional,
            "build",
            "retries: if n > 9 { n } else { n + 1 },",
            "retries: if n > 9 { n } else { n + 1 },",
        )
        .is_none()
    );
    // Fixture control: the plain library establishes.
    assert!(whole_value_pin(CONFIG_LIB, "build", tests).1.is_some());
}

/// #7066 review: the owner's file must name the workspace `Config` (a
/// `use external::Config;` makes the literal another crate's type), a
/// once-used binding must come before the assertion that reads it, and a
/// pointer, `'static` or cast type is not a computed value item.
#[test]
fn whole_value_review_holes_stay_closed() {
    let literal = r#"Config { retries: 4, name: "x".into(), count: Count(3) }"#;
    let tests =
        format!("use demo::*;\n#[test]\nfn pins() {{\n    assert_eq!(build(3), {literal});\n}}\n");
    let foreign_import = CONFIG_LIB.replace(
        "pub fn build(n: u32) -> Config {\n    Config {\n        retries: n + 1,\n        name: \"x\".into(),\n        count: Count(n),\n    }\n}\n",
        "pub mod remote {\n    use external::{Config, Count};\n\n    pub fn build(n: u32) -> Config {\n    Config {\n        retries: n + 1,\n        name: \"x\".into(),\n        count: Count(n),\n    }\n}\n}\n",
    );
    assert_ne!(foreign_import, CONFIG_LIB, "fixture must move the owner");
    assert!(
        whole_value_pin(&foreign_import, "build", &tests)
            .1
            .is_none()
    );
    // Fixture control: the same module importing the workspace type pins.
    let workspace_import = foreign_import.replace(
        "use external::{Config, Count};",
        "use crate::{Config, Count};",
    );
    assert!(
        whole_value_pin(&workspace_import, "build", &tests)
            .1
            .is_some()
    );

    // A binding after the assertion is not the value the assertion reads.
    assert!(!whole_value_admits(
        CONFIG_LIB,
        "build",
        &format!("assert_eq!(current, {literal});\nlet current = build(3);")
    ));
    assert!(whole_value_admits(
        CONFIG_LIB,
        "build",
        &format!("let current = build(3);\nassert_eq!(current, {literal});")
    ));

    // `*const Count`, `&'static Count` and a cast name the type, not a
    // CamelCase value item; an import rename still does.
    let typed = format!(
        "{CONFIG_LIB}pub fn addr(p: *const Count) -> usize {{\n    p as usize\n}}\n\npub static ZERO: &'static Count = &Count(0);\n\npub fn widen(n: u8) -> u32 {{\n    n as Count\n}}\n"
    );
    assert!(whole_value_admits(
        &typed,
        "build",
        &format!("assert_eq!(build(3), {literal});")
    ));
    let renamed = format!("{CONFIG_LIB}use crate::build as Count;\n");
    let index = index(&[(LIB, &renamed)]);
    assert!(camel_case_value_items(&index).contains("Count"));
}

/// #7066 review (second round): a cfg-elided binding, a qualified or
/// test-side rebound expected type, and a workspace `Ok`/`Some` that is not
/// the standard variant each refuse the whole-value pin.
#[test]
fn whole_value_identity_holes_stay_closed() {
    let literal = r#"Config { retries: 4, name: "x".into(), count: Count(3) }"#;
    // `#[cfg(any())] let c = ..;` leaves `c` naming an outer item.
    assert!(!whole_value_admits(
        CONFIG_LIB,
        "build",
        &format!("#[cfg(any())]\nlet current = build(3);\nassert_eq!(current, {literal});")
    ));
    // A qualified expected path may name another type.
    assert!(!whole_value_admits(
        CONFIG_LIB,
        "build",
        &format!("assert_eq!(build(3), external::{literal});")
    ));
    // The test file importing another `Config` rebinds the bare name.
    let tests = format!(
        "use demo::{{build, Count}};\nuse external::Config;\n#[test]\nfn pins() {{\n    assert_eq!(build(3), {literal});\n}}\n"
    );
    let (index, pin) = whole_value_pin(CONFIG_LIB, "build", &tests);
    assert!(pin.is_some_and(|pin| admitted_texts(&index, &pin).is_empty()));
    // Fixture control: the same test importing the workspace type pins.
    let tests = tests.replace("use external::Config;", "use demo::Config;");
    let (index, pin) = whole_value_pin(CONFIG_LIB, "build", &tests);
    assert!(pin.is_some_and(|pin| !admitted_texts(&index, &pin).is_empty()));

    // A bare CamelCase value the workspace never declares may be a local.
    assert!(!whole_value_admits(
        CONFIG_LIB,
        "build",
        "#[allow(non_snake_case)]\nlet Expected = 4;\nassert_eq!(build(3), Config { retries: Expected, name: \"x\".into(), count: Count(3) });"
    ));
    let names = declared_constructor_names(&index);
    for name in ["Config", "Count", "None", "Ok"] {
        assert!(names.contains(name), "{name}");
    }
    let lib = format!(
        "{CONFIG_LIB}pub enum Mode {{\n    #[default]\n    Fast,\n    Slow(Count),\n    Exact = 3,\n}}\n"
    );
    let (mode_index, _) = whole_value_pin(&lib, "build", &tests);
    let names = declared_constructor_names(&mode_index);
    for name in ["Mode", "Fast", "Slow", "Exact"] {
        assert!(names.contains(name), "{name}");
    }
    assert!(!names.contains("Expected"));

    // A workspace `Ok` function or a `Some` variant is not the standard one.
    let ok_body = format!("assert_eq!(parse(\"3\"), Ok({literal}));");
    assert!(whole_value_admits(CONFIG_LIB, "parse", &ok_body));
    for shadow in [
        "#[allow(non_snake_case)]\npub fn Ok(c: Config) -> Result<Config, String> {\n    Result::Ok(c)\n}\n",
        "pub enum Probe {\n    Ok(u8),\n    Fail,\n}\n",
        "pub use other::Ok;\n",
    ] {
        let lib = format!("{CONFIG_LIB}{shadow}");
        let (_, pin) = whole_value_pin(
            &lib,
            "parse",
            &format!("use demo::*;\n#[test]\nfn pins() {{\n{ok_body}\n}}\n"),
        );
        assert!(pin.is_none(), "{shadow}");
    }
}

/// The `name` field pin of `build` against `tests` (one test): the changed
/// line is the `String` field initializer.
fn string_field_pin(lib: &str, tests: &str) -> (RustIndex, Option<OwnerReturnPin>) {
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = field_pin_of(
        &index,
        lib,
        "build",
        "name: render(retries),",
        "name: render(retries),",
    );
    (index, pin)
}

fn string_field_admits(lib: &str, body: &str) -> bool {
    let tests = format!("use demo::*;\n#[test]\nfn pins() {{\n{body}\n}}\n");
    let (index, pin) = string_field_pin(lib, &tests);
    assert!(pin.is_some(), "the string field pin must establish: {lib}");
    pin.is_some_and(|pin| !admitted_texts(&index, &pin).is_empty())
}

const STRING_FIELD_LIB: &str = r#"#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub retries: u32,
    pub name: String,
}

pub fn build(retries: u32) -> Config {
    Config {
        retries,
        name: render(retries),
    }
}

pub fn render(retries: u32) -> String {
    format!("cfg-{retries}")
}
"#;

/// #7066 review (fourth round): a test file's `mod String` makes an expected
/// `String::from(..)` the owner's own output, so both operands move together
/// and the pin refuses the spelling; the standard conversion still pins.
#[test]
fn whole_value_string_from_refuses_a_workspace_string() {
    let body = "assert_eq!(build(3), Config { retries: 3, name: String::from(\"3\") });";
    // Control: the standard `String::from` carries an independent value.
    assert!(string_field_admits(STRING_FIELD_LIB, body));

    // The test file declaring `mod String` rebinds the conversion to a
    // function that returns the owner's own name.
    let shadowed_tests = format!(
        "use demo::*;\n#[test]\nfn pins() {{\n{body}\n}}\nmod String {{\n    pub fn from(value: &str) -> std::string::String {{\n        demo::build(value.trim().parse().unwrap_or(0)).name\n    }}\n}}\n"
    );
    let (index, pin) = string_field_pin(STRING_FIELD_LIB, &shadowed_tests);
    assert!(pin.is_some(), "the pin still establishes under the shadow");
    assert!(pin.is_some_and(|pin| admitted_texts(&index, &pin).is_empty()));
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

/// #7847: an expected word a statement macro binds, or a static the test
/// writes the owner's value into, compares the owner with itself.
#[test]
fn an_expected_value_a_macro_or_a_static_rebinds_from_the_owner_is_not_a_pin() {
    let macro_prelude =
        "macro_rules! s {\n    ($i:ident) => {\n        let $i = crate::weight(4);\n    };\n}\n\n";
    for (prelude, body) in [
        (
            macro_prelude,
            "s!(expected);\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "static EXPECTED: AtomicU32 = AtomicU32::new(0);\n\n",
            "EXPECTED.store(crate::weight(4), Ordering::SeqCst);\n        assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));",
        ),
        (
            "static mut EXPECTED: u32 = 0;\n\n",
            "EXPECTED = crate::weight(4);\n        assert_eq!(crate::weight(4), EXPECTED);",
        ),
        (
            "static EXPECTED: AtomicU32 = AtomicU32::new(0);\n\n",
            "AtomicU32::store(&EXPECTED, crate::weight(4), Ordering::SeqCst);\n        assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));",
        ),
        (
            "thread_local! { static EXPECTED: Cell<u32> = Cell::new(0); }\n\n",
            "EXPECTED.with(|c| c.set(crate::weight(4)));\n        assert_eq!(crate::weight(4), EXPECTED.with(|c| c.get()));",
        ),
        (
            "",
            "let mut expected = 0;\n        expected += crate::weight(4);\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let mut expected = 0;\n        bump(&mut expected, crate::weight(4));\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let mut expected = 0;\n        expected.widen::<u64>(crate::weight(4));\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let mut expected = Wrapper::new();\n        expected.cell?.bump(crate::weight(4));\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "fn write(_: &AtomicU32, _: u32) {}\nstatic EXPECTED: AtomicU32 = AtomicU32::new(0);\n\n",
            "write(&EXPECTED, crate::weight(4));\n        assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));",
        ),
        (
            "",
            "let expected = Cell::new(0);\n        Cell::set(&expected, crate::weight(4));\n        assert_eq!(crate::weight(4), expected.get());",
        ),
        (
            "",
            "let mut expected = 0;\n        (expected, _) = (crate::weight(4), 0);\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let mut expected = 0;\n        [expected, ..] = [crate::weight(4), 0];\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "struct S { f: u32, g: u32 }\n\n",
            "let mut expected = 0;\n        S { f: expected, .. } = S { f: crate::weight(4), g: 0 };\n        assert_eq!(crate::weight(4), expected);",
        ),
        // #8795 review: a struct of literals may `Deref` to an atomic.
        (
            "use std::sync::atomic::{AtomicU32, Ordering};\nstruct E {\n    tag: u32,\n}\nstatic VALUE: AtomicU32 = AtomicU32::new(0);\nimpl std::ops::Deref for E {\n    type Target = AtomicU32;\n    fn deref(&self) -> &Self::Target {\n        &VALUE\n    }\n}\n\n",
            "let expected = E { tag: 0 };\n        AtomicU32::store(&expected, crate::weight(4), Ordering::SeqCst);\n        assert_eq!(crate::weight(4), expected.load(Ordering::SeqCst));",
        ),
        // #8795 review: a loop label before `[` is not an indexed place.
        (
            "",
            "let mut expected = 0;\n        let mut o = 0;\n        'a: {\n            break 'a [expected, o] = [crate::weight(4), 0];\n        }\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let mut expected = 0;\n        let mut o = 0;\n        'a: loop {\n            break 'a [expected, o] = [crate::weight(4), 0];\n        }\n        assert_eq!(crate::weight(4), expected);",
        ),
        // #8795 review: a later `let` shadows the static the earlier
        // shared borrow writes; the assertion before it still names the static.
        (
            "static EXPECTED: AtomicU32 = AtomicU32::new(0);\nfn same(_: u32, _: &AtomicU32) {}\n\n",
            "same(crate::weight(4), &EXPECTED);\n        assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));\n        let EXPECTED = 0;",
        ),
        // #8795 review: a keyword before `[` starts an array pattern, not an index.
        (
            "",
            "let mut expected = 0;\n        let mut o = 0;\n        match [expected, o] = [crate::weight(4), 0] {\n            _ => {}\n        }\n        assert_eq!(crate::weight(4), expected);",
        ),
        // #8795 review: a test-local type may define `replace` / `take`
        // with any arity and mutate the value with the owner's result, and
        // a test-local `String` may hold a `Cell`.
        (
            "struct Counter(u32);\nimpl Counter {\n    fn replace(&mut self, value: u32, _: u32) {\n        self.0 = value;\n    }\n}\n\n",
            "let mut expected = Counter(0);\n        expected.replace(crate::weight(4), 0);\n        assert_eq!(crate::weight(4), expected.0);",
        ),
        (
            "struct Counter(u32);\nimpl Counter {\n    fn take(&mut self, value: u32) {\n        self.0 = value;\n    }\n}\n\n",
            "let mut expected = Counter(0);\n        expected.take(crate::weight(4));\n        assert_eq!(crate::weight(4), expected.0);",
        ),
        (
            "struct String(Cell<u32>);\nimpl String {\n    fn new() -> Self {\n        Self(Cell::new(0))\n    }\n}\n\n",
            "let expected = String::new();\n        Cell::set(&expected.0, crate::weight(4));\n        assert_eq!(crate::weight(4), expected.0.get());",
        ),
        // #8929: an inner-block `static` redeclaring the name after a plain
        // top-level `let` makes the later `&EXPECTED` a static borrow.
        (
            "",
            "let EXPECTED = 0;\n        {\n            static EXPECTED: AtomicU32 = AtomicU32::new(0);\n            same(crate::weight(4), &EXPECTED);\n            assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));\n        }",
        ),
        // #8795 negative controls: a non-plain value still refuses a shared
        // borrow, the one-argument `replace` and the argument-free `take`
        // still write, and a pattern is not an annotation.
        (
            "fn same(_: u32, _: &Cell<u32>) {}\n\n",
            "let expected = Cell::new(0);\n        same(crate::weight(4), &expected);\n        assert_eq!(crate::weight(4), expected.get());",
        ),
        (
            "",
            "let expected = Cell::new(0);\n        let _ = Some(&expected);\n        expected.replace(crate::weight(4));\n        assert_eq!(crate::weight(4), expected.get());",
        ),
        (
            "",
            "let mut expected = Some(0);\n        expected.replace(crate::weight(4));\n        assert_eq!(crate::weight(4), expected.unwrap());",
        ),
        (
            "static EXPECTED: AtomicU32 = AtomicU32::new(0);\nfn same(_: u32, _: &AtomicU32) {}\n\n",
            "same(crate::weight(4), &EXPECTED);\n        assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));",
        ),
        (
            "",
            "let mut expected = (0, 0);\n        let (a, expected2): (u32, u32) = (1, 2);\n        (expected, _) = (crate::weight(4), a + expected2);\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let mut expected = String::new();\n        write!(expected, \"{}\", crate::weight(4)).unwrap();\n        assert_eq!(crate::weight(4).to_string(), expected);",
        ),
        (
            "",
            "let mut expected = String::new();\n        writeln!(expected, \"{}\", crate::weight(4)).unwrap();\n        assert_eq!(crate::weight(4).to_string(), expected);",
        ),
        // Non-atomic writes: a field, a tuple field, an index, a listed
        // mutator that is not a `store`, and a mutator in a struct-literal
        // field value (not a `let` annotation).
        (
            "struct E { v: u32 }\n\n",
            "let mut expected = E { v: 0 };\n        expected.v = crate::weight(4);\n        assert_eq!(crate::weight(4), expected.v);",
        ),
        (
            "",
            "let mut expected = (0, 0);\n        expected.0 = crate::weight(4);\n        assert_eq!(crate::weight(4), expected.0);",
        ),
        (
            "",
            "let mut expected = [0; 2];\n        expected[1] = crate::weight(4);\n        assert_eq!(crate::weight(4), expected[1]);",
        ),
        (
            "",
            "let mut expected = [0; 2];\n        expected.fill(crate::weight(4));\n        assert_eq!(crate::weight(4), expected[0]);",
        ),
        (
            "struct S { f: u32 }\nstatic EXPECTED: AtomicU32 = AtomicU32::new(0);\n\n",
            "let _s = S { f: EXPECTED.swap(crate::weight(4), Ordering::SeqCst) };\n        assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));",
        ),
    ] {
        let lib = unit_tests(prelude, body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // Controls: an independent `let`, and a static the test only reads.
    for (prelude, body) in [
        // #8795: annotation shapes naming the type are not writes.
        (
            "struct Config { v: u32 }\n\n",
            "let expected: crate::Config = Config { v: 12 };\n        assert_eq!(crate::weight(4), expected.v);",
        ),
        (
            "struct Config { v: u32 }\n\n",
            "let expected: &Config = &Config { v: 12 };\n        assert_eq!(crate::weight(4), expected.v);",
        ),
        // #8795: a shared borrow of a plain literal value is a read.
        (
            "fn same(_: u32, _: &u32) {}\n\n",
            "let expected = 12;\n        same(crate::weight(4), &expected);\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let expected = 12;\n        let _ = Some(&expected);\n        assert_eq!(crate::weight(4), expected);",
        ),
        // #8795: the name only indexes another place.
        (
            "",
            "let expected = 12;\n        let mut v = [0; 16];\n        v [expected] = 7;\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let expected = [12; 2];\n        let _ = Some(&expected);\n        assert_eq!(crate::weight(4), expected[0]);",
        ),
        (
            "",
            "let expected = 12;\n        let mut v = [0; 16];\n        v[expected] = 7;\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let expected = 12;\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "",
            "let expected: u32 = 12;\n        assert_eq!(crate::weight(4), expected);",
        ),
        (
            "static EXPECTED: u32 = 12;\n\n",
            "assert_eq!(crate::weight(4), EXPECTED);",
        ),
        (
            "static EXPECTED: u32 = 12;\n\n",
            "let _w = crate::weight(4);\n        let ok = EXPECTED == 12 && EXPECTED != 3 && EXPECTED >= 1 && EXPECTED <= 99;\n        assert!(ok);\n        assert_eq!(crate::weight(4), EXPECTED);",
        ),
        (
            "static EXPECTED: AtomicU32 = AtomicU32::new(12);\n\n",
            "let _w = crate::weight(4);\n        let _v = AtomicU32::load(&EXPECTED, Ordering::SeqCst);\n        assert_eq!(crate::weight(4), EXPECTED.load(Ordering::SeqCst));",
        ),
        (
            "",
            "let _w = crate::weight(4);\n        let expected = 12;\n        let t = (expected, 1);\n        assert_eq!(crate::weight(4), t.0);",
        ),
        (
            "",
            "let e: u32 = 12;\n        let expected: u32 = e;\n        assert_eq!(crate::weight(4), expected);",
        ),
    ] {
        let lib = unit_tests(prelude, body);
        assert_eq!(path_admitted(&lib, "x * 3", None).len(), 1, "{body}");
    }
}

/// #8929: the `&mut` family spelled through grouping parentheses or a path,
/// `ref mut` bindings on a place, the `addr_of_mut!` macro and a `?` / `.await`
/// tail after a call group. Every write row is a mutable alias or write of the
/// expected value the scan must refuse; every read row is the sibling shape
/// that must stay a read so the fix does not become a blanket refusal.
#[test]
fn written_by_test_refuses_grouped_pathed_ref_mut_and_addr_of_mut_writes() {
    for write in [
        // `&mut (name)` / `&mut (name).f`.
        "let r = &mut (EXPECTED); *r = f(4);",
        "let r = &mut (EXPECTED).f; *r = f(4);",
        "let r = & mut ( EXPECTED );",
        "let r = &raw mut (EXPECTED);",
        // `ref mut` on a scrutinee place.
        "let ref mut r = EXPECTED; *r = f(4);",
        "let ref mut r: u32 = EXPECTED; *r = f(4);",
        "let (ref mut a, b) = EXPECTED;",
        "match EXPECTED { ref mut r => { *r = f(4); } }",
        "match EXPECTED.f { ref mut r => { *r = f(4); } }",
        "if let Some(ref mut x) = EXPECTED { *x = f(4); }",
        "while let Some(ref mut x) = EXPECTED.f {}",
        // `ref mut` on a path-qualified or grouped scrutinee.
        "let ref mut r = crate::EXPECTED; *r = f(4);",
        "let ref mut r = self::EXPECTED;",
        "let ref mut r = m::n::EXPECTED;",
        "let ref mut r = (crate::EXPECTED);",
        "let ref mut r = ((EXPECTED));",
        "if let Some(ref mut x) = crate::EXPECTED {}",
        "if let Some(ref mut x) = self::EXPECTED {}",
        "if let Some(ref mut x) = m::n::EXPECTED {}",
        "while let Some(ref mut x) = crate::EXPECTED.f {}",
        "while let Some(ref mut x) = m::n::EXPECTED {}",
        "match crate::EXPECTED { ref mut r => {} }",
        "match self::EXPECTED { ref mut r => {} }",
        "match m::n::EXPECTED { ref mut r => {} }",
        "match (crate::EXPECTED) { ref mut r => {} }",
        "match (EXPECTED) { ref mut r => {} }",
        // `addr_of_mut!` and its path-qualified spellings.
        "let p = addr_of_mut!(EXPECTED); *p = f(4);",
        "let p = ptr::addr_of_mut!(EXPECTED);",
        "let p = std::ptr::addr_of_mut!( EXPECTED );",
        "let p = addr_of_mut! (EXPECTED);",
        "let p = addr_of_mut!((EXPECTED).f);",
        "let p = addr_of_mut!(crate::EXPECTED); *p = f(4);",
        "let p = std::ptr::addr_of_mut!(m::EXPECTED);",
        "let p = addr_of_mut!( (EXPECTED) );",
        "let p = addr_of_mut!(self::EXPECTED);",
        // A path-qualified name behind `&mut`.
        "let r = &mut crate::EXPECTED; *r = f(4);",
        "let r = &mut self::EXPECTED;",
        "let r = &mut m::n::EXPECTED;",
        "let r = & mut ::EXPECTED;",
        "let r = &mut (crate::EXPECTED);",
        // A `?` / `.await` tail followed by a place write still refuses.
        "foo(EXPECTED)?.x = 1;",
        "foo(EXPECTED)?[0] = 1;",
        "(foo(EXPECTED)?).x = 1;",
        "foo(EXPECTED).await?.x = 1;",
        "*foo(EXPECTED)? = 1;",
    ] {
        assert!(super::written_by_test(write, "EXPECTED", false), "{write}");
    }
    for read in [
        "let r = &(EXPECTED);",
        "let r = &mut foo(EXPECTED);",
        "let r = &mut foo::bar(EXPECTED);",
        "let r = x & (EXPECTED);",
        "let r = &crate::EXPECTED;",
        "let r = crate::EXPECTED;",
        "let r = self::EXPECTED;",
        "match crate::EXPECTED { _ => {} }",
        "match m::n::EXPECTED { r => {} }",
        "if let Some(x) = crate::EXPECTED {}",
        "while let Some(x) = m::n::EXPECTED {}",
        "let r = EXPECTED;",
        "let mut r = EXPECTED;",
        "let ref r = EXPECTED;",
        "let ref mut r = other; let v = EXPECTED;",
        "match EXPECTED { x => {} }",
        "match EXPECTED { Some(ref x) => {} }",
        "match other { ref mut r => {} } let v = EXPECTED;",
        "if let Some(x) = EXPECTED {}",
        "let p = addr_of!(EXPECTED);",
        "let p = ptr::addr_of!(EXPECTED);",
        "let p = my_addr_of_mut!(EXPECTED);",
        // A `?` / `.await` after a call group only reads the argument.
        "foo(EXPECTED)?;",
        "let x = foo(EXPECTED)?;",
        "assert_eq!(f(4), parse(EXPECTED)?);",
        "foo(EXPECTED).await?;",
        "let x = foo(EXPECTED).await;",
        "bar(foo(EXPECTED)?);",
    ] {
        assert!(!super::written_by_test(read, "EXPECTED", false), "{read}");
    }
}

/// #8944: fail-open residuals of #8943. Raw identifiers, a `>` before `=`,
/// a deref-then-borrow, `ref mut` scrutinees in a tuple or block, and borrow
/// operands that are groups, tuples, blocks or qualified/turbofish paths are
/// all mutable aliases of the expected value. Each read row is the nearby
/// shape that must stay a read.
#[test]
fn written_by_test_refuses_raw_deref_tuple_and_generic_path_aliases() {
    for write in [
        // The `>` closing a type annotation sits directly before `=`.
        "let ref mut r: Vec<u8>= EXPECTED; *r = f(4);",
        "let ref mut r: Option<Vec<u8>>= EXPECTED;",
        // Raw identifiers.
        "let r = &mut r#EXPECTED; *r = f(4);",
        "let p = addr_of_mut!(r#EXPECTED); *p = f(4);",
        "let ref mut r = r#EXPECTED; *r = f(4);",
        "let r = &mut m::r#EXPECTED;",
        // Deref then borrow.
        "let r = &mut *EXPECTED; *r = f(4);",
        "let r = &mut *(EXPECTED);",
        "let r = &mut (*EXPECTED);",
        "let r = &mut **EXPECTED;",
        "let p = addr_of_mut!(*EXPECTED); *p = f(4);",
        "let ref mut r = *EXPECTED; *r = f(4);",
        // `ref mut` away from the first position of the scrutinee.
        "let (a, ref mut b) = (x, EXPECTED); *b = f(4);",
        "match (x, EXPECTED) { (_, ref mut r) => { *r = f(4); } }",
        "if let (_, Some(ref mut r)) = (x, EXPECTED) {}",
        "let ref mut r = {EXPECTED}; *r = f(4);",
        "let [a, ref mut b] = [x, EXPECTED];",
        // Borrow operands that wrap the name.
        "let r = &mut {EXPECTED};",
        "let r = &mut (x, EXPECTED); r.1 = f(4);",
        "let r = &mut [x, EXPECTED];",
        "let r = &mut <T>::EXPECTED; *r = f(4);",
        "let r = &mut <T as Tr>::EXPECTED;",
        "let r = &mut m::<T>::EXPECTED;",
        "let r = &mut m::n::<T>::EXPECTED;",
        "let p = addr_of_mut!(<T>::EXPECTED);",
        "let p = addr_of_mut!(m::<T>::EXPECTED);",
        "let ref mut r = <T>::EXPECTED;",
        "let ref mut r = m::<T>::EXPECTED;",
        // A `ref mut` range-pattern binding keeps its `=` inside the pattern.
        "if let ref mut a @ 0..=9 = EXPECTED {}",
        // A pattern that keeps its own bracketed `;` or follows an earlier
        // statement still binds the scrutinee.
        "let S { ref mut a } = EXPECTED;",
        "let ref mut r: [u8; 3] = EXPECTED;",
        "let other = 1; let ref mut r = EXPECTED;",
    ] {
        assert!(super::written_by_test(write, "EXPECTED", false), "{write}");
    }
    for read in [
        "let r: Vec<u8>= EXPECTED;",
        "let r: Vec<u8> = EXPECTED;",
        "let r = r#EXPECTED;",
        "let r = &r#EXPECTED;",
        "let p = addr_of!(r#EXPECTED);",
        "let r = &*EXPECTED;",
        "let r = x * EXPECTED;",
        "let r = &mut x * EXPECTED;",
        "let p = addr_of!(*EXPECTED);",
        "let (a, b) = (x, EXPECTED);",
        "let (ref mut a, b) = (EXPECTED_OTHER, y); let v = EXPECTED;",
        "match (x, EXPECTED) { (_, r) => {} }",
        "let ref mut r = other; let v = (x, EXPECTED);",
        "let ref mut r = other; v = EXPECTED;",
        "let r = {EXPECTED};",
        "let r = &{EXPECTED};",
        "let r = &(x, EXPECTED);",
        "let r = &mut foo(x, EXPECTED);",
        "let r = &mut foo(<T>::EXPECTED);",
        "let r = &<T>::EXPECTED;",
        "let r = m::<T>::EXPECTED;",
        "let p = addr_of!(m::<T>::EXPECTED);",
        "let r = x > ::EXPECTED;",
        "foo(&mut x, EXPECTED);",
        "foo(&mut x, (EXPECTED, 1));",
        "if x >= EXPECTED {}",
    ] {
        assert!(!super::written_by_test(read, "EXPECTED", false), "{read}");
    }
}

/// #8960: a brace-bearing element after the name inside a `match` scrutinee
/// group, raw-identifier path segments and a `match <T>::NAME` head are all
/// mutable aliases of the expected value; each read row is the nearby shape
/// that must stay a read.
#[test]
fn written_by_test_refuses_scrutinee_brace_raw_segment_and_generic_head_aliases() {
    for write in [
        "match (EXPECTED, S { a: 1 }) { (ref mut r, _) => { *r = f(4); } }",
        "match (EXPECTED, { 1 }) { (ref mut r, _) => {} }",
        "match (EXPECTED, || {}) { (ref mut r, _) => {} }",
        "match [EXPECTED, S { a: 1 }] { [ref mut r, _] => {} }",
        "match [EXPECTED; 3] { [ref mut r, ..] => {} }",
        "let p = addr_of_mut!(r#type::EXPECTED);",
        "let r = &mut r#m::EXPECTED;",
        "let r = &mut crate::r#m::EXPECTED;",
        "let r = &mut r#mut::EXPECTED;",
        "match <T>::EXPECTED { ref mut r => {} }",
        "match m::<T>::EXPECTED { ref mut r => {} }",
        "let r = &mut <fn() -> u8>::EXPECTED;",
        "let r = &mut <T as Tr<{ N > 1 }>>::EXPECTED;",
    ] {
        assert!(super::written_by_test(write, "EXPECTED", false), "{write}");
    }
    for read in [
        "match (EXPECTED, S { a: 1 }) { (r, _) => {} }",
        "match (EXPECTED, { 1 }) { (r, _) => {} }",
        "match [EXPECTED, S { a: 1 }] { [r, _] => {} }",
        "match [EXPECTED; 3] { [r, ..] => {} }",
        "match <T>::EXPECTED { r => {} }",
        "match EXPECTED { _ => {} } if c { let ref mut x = y; }",
        "let r = &r#m::EXPECTED;",
        "let p = addr_of!(r#type::EXPECTED);",
        "let r = &mut foo(r#m::EXPECTED);",
        "let r = &<fn() -> u8>::EXPECTED;",
        "let r = &<T as Tr<{ N > 1 }>>::EXPECTED;",
    ] {
        assert!(!super::written_by_test(read, "EXPECTED", false), "{read}");
    }
}

/// #7847: the lexical write guard on its own, including operator edges the
/// pipeline fixtures cannot reach.
#[test]
fn written_by_test_reads_operators_and_let_boundaries() {
    for read in [
        "x == EXPECTED",
        "EXPECTED == 1",
        "EXPECTED != 1",
        "EXPECTED >= 1",
        "EXPECTED <= 1",
        "EXPECTED => 1",
        "let e: EXPECTED = 1;",
        "assert_eq!(f(4), &EXPECTED);",
        "AtomicU32::load(&EXPECTED, o);",
        "let t = (EXPECTED, 1);",
        "EXPECTED.next_power_of_two();",
        "EXPECTED.next_multiple_of(4);",
        "if (EXPECTED, 1) == t {}",
        // #8795: any `let` annotation shape that names the type.
        "let e: crate::EXPECTED = 1;",
        "let e: &EXPECTED = &1;",
        "let mut EXPECTED = 1; let r = &EXPECTED;",
        "let e: &mut EXPECTED = g();",
        "let e: (EXPECTED, u8) = p;",
        "let e: Vec<EXPECTED> = p;",
        "let (a, e): (EXPECTED, u8) = p;",
        // #8795: the name indexes another place.
        "v[EXPECTED] = 1;",
        "v[i][EXPECTED] += 1;",
        "v [EXPECTED] = 1;",
        "v[EXPECTED][0] = 1;",
        "foo(EXPECTED);",
        "f(a)[EXPECTED] = 1;",
    ] {
        assert!(!super::written_by_test(read, "EXPECTED", false), "{read}");
    }
    for write in [
        "EXPECTED = 1;",
        "EXPECTED += 1;",
        "EXPECTED.0 -= 1;",
        "let a: u32 = 1; S { f: EXPECTED.swap(1) }",
        "AtomicU32::store(&EXPECTED, 1, o);",
        "g(&mut EXPECTED);",
        "EXPECTED.with(|c| c.set(1));",
        "EXPECTED.next();",
        "EXPECTED.v?.bump();",
        "EXPECTED.widen::<u8>();",
        "write(&EXPECTED, 1);",
        "write!(EXPECTED, \"{}\", 1);",
        "writeln!(EXPECTED, \"{}\", 1);",
        "store::<u32>(&EXPECTED, 1);",
        "make()(&EXPECTED, 1);",
        "EXPECTED.resize_with(3, f);",
        "EXPECTED.compare_and_swap(1, 2, o);",
        "EXPECTED.next_if_eq(&1);",
        "EXPECTED.peek_mut();",
        "EXPECTED.peek();",
        "EXPECTED.compare_exchange_weak(1, 2, o, o);",
        "EXPECTED.try_lock();",
        "EXPECTED.try_write();",
        "EXPECTED.as_mut_ptr();",
        "EXPECTED.next_back();",
        "(EXPECTED, _) = p;",
        "[EXPECTED, ..] = p;",
        "S { f: EXPECTED, .. } = p;",
        // #8795 negative controls: the annotation skip needs a real
        // annotation, and an index skip needs a real index.
        "let (a, EXPECTED) = p; EXPECTED = 1;",
        "let (a, EXPECTED): (u8, u8) = p; EXPECTED = 1;",
        "let a: u8 = 1; EXPECTED = 2;",
        "let a: u8; EXPECTED = 2;",
        "let S { f: EXPECTED } = p; EXPECTED = 1;",
        "let a = 1; Some(&EXPECTED);",
        "[EXPECTED, 1] = p;",
        "match [EXPECTED, o] = p { _ => {} }",
        "return [EXPECTED, o] = p;",
        "break [EXPECTED, o] = p;",
        "break 'a [EXPECTED, o] = p;",
        // Postfix keyspace: a place chain after a closing group, a `?`
        // before the bracket, and macro-built arrays all refuse.
        "(EXPECTED)[0] = 1;",
        "(EXPECTED).f = 1;",
        "foo()?[EXPECTED] = 1;",
        "EXPECTED?[0] = 1;",
        "vec![EXPECTED][0] = 1;",
        "m![EXPECTED][i] = 1;",
        "foo(EXPECTED).x = 1;",
        "let r = & mut EXPECTED; *r = 1;",
        "let r = &raw mut EXPECTED;",
        "let r = &\n mut EXPECTED;",
        "continue 'a [EXPECTED, o] = p;",
        "(EXPECTED, v[0]) = p;",
        "EXPECTED.replace(f(4));",
        "EXPECTED.take();",
        "EXPECTED.lock();",
        "EXPECTED.replace(1, 2);",
        "EXPECTED.take(2);",
    ] {
        assert!(super::written_by_test(write, "EXPECTED", false), "{write}");
    }
    // #8795: on a plain `let` value a shared borrow is a read; every
    // `replace`, `take`, `lock`, assignment and `&mut` stays a write (a
    // test-local type may define any method arity).
    for read in [
        "Some(&EXPECTED);",
        "Ok(&EXPECTED);",
        "helper(f(4), &EXPECTED);",
    ] {
        assert!(!super::written_by_test(read, "EXPECTED", true), "{read}");
    }
    for write in [
        "EXPECTED.replace(f(4));",
        "EXPECTED.replace(1, 2);",
        "EXPECTED.take();",
        "EXPECTED.take(2);",
        "g(&mut EXPECTED);",
        "EXPECTED = 1;",
        "EXPECTED.lock();",
    ] {
        assert!(super::written_by_test(write, "EXPECTED", true), "{write}");
    }
    // Which bindings are plain: literals and values built only of them.
    for init in [
        ": u32 = 12;",
        " = -1.5;",
        " = 1.5e-3f64;",
        " = 0xff_u8;",
        " = \"x\";",
        " = (1, \"a\");",
        " = [1, 2];",
        " = [12; 2];",
    ] {
        assert!(super::plain_let_values(&[init]), "{init}");
    }
    for init in [
        " = f(4);",
        " = Cell::new(0);",
        " = Mutex::new(0);",
        " = x;",
        " = (1, y);",
        " = Config { retries: n };",
        " = Config { retries: 1, ..Default::default() };",
        " = &mut v;",
        " = [1, f(2)];",
        " = String::from(\"x\");",
        " = String::new();",
        " = \"x\".to_string();",
        " = Some(3);",
        " = Config(1, 2);",
        " = [f(1); 2];",
        " = Config { retries: 3, name: \"a\" };",
        " = E { tag: 0 };",
        " = 1..n;",
        " = 0..10;",
        " = 1.max(f(4));",
        " = [1; n];",
        " = Config { name: String::new() };",
        ";",
    ] {
        assert!(!super::plain_let_values(&[init]), "{init}");
    }
    assert!(!super::plain_let_values(&[]));
    // Only a first-use top-level `let` makes a name plain.
    for (body, plain) in [
        ("{ let e = 1; f(&e); }", true),
        ("{ let mut e = 1; f(&e); }", true),
        ("{ f(&e); let e = 1; }", false),
        ("{ { let e = 1; } f(&e); }", false),
        ("{ if c { let e = 1; } }", false),
        ("{ f(); }", false),
    ] {
        assert_eq!(
            super::first_use_is_top_level_let(body, "e"),
            plain,
            "{body}"
        );
    }
    assert!(!super::plain_let_values(&[" = 1;", " = f();"]));
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
        "assert_eq!(crate::weight(4) + 1, 13);",
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

/// The same unit-test file with `weight` declaring `-> Wt`, an alias the
/// pin does not read as a primitive scalar, so the #6974 shared-handle
/// guards apply as they do to any non-scalar result.
fn non_scalar_unit_tests(prelude: &str, body: &str) -> String {
    let lib = unit_tests(&format!("pub type Wt = u32;\n{prelude}"), body)
        .replace("-> u32 {\n    x * 3", "-> Wt {\n    x * 3");
    assert!(lib.contains("-> Wt {"), "{lib}");
    lib
}

#[test]
fn a_result_bound_once_and_only_asserted_pins_like_the_call() {
    for body in [
        "let total = crate::weight(4);\n        assert_eq!(total, 12);",
        "let total: u32 = super::weight(4);\n        assert_eq!(12, total);",
        "let total = weight(4);\n        assert_eq!(total, 12);\n        assert_eq!(total, 12);",
    ] {
        let lib = non_scalar_unit_tests("", body);
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
        // Only the only-statement-on-its-line rule refuses this one: the
        // first assertion is the next statement, and a statement runs
        // before the second (#7061 round 3).
        "let total = crate::weight(4);\n        assert_eq!(total, touch_count()); touch_count(); assert_eq!(total, 12);",
    ] {
        let lib = non_scalar_unit_tests(LET_BOUND_PRELUDE, body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
        // A scalar owner drops only the next-statement guard (cell-0094):
        // every other guard still refuses, so the binding is never pinned.
        let lib = unit_tests(LET_BOUND_PRELUDE, body);
        let scalar = path_admitted(&lib, "x * 3", None);
        assert!(
            !scalar
                .iter()
                .any(|text| text.starts_with("assert_eq!(total, 12)")),
            "{body}: {scalar:?}"
        );
    }
    // Control: the same prelude leaves the plain form pinned, so the cases
    // above are refused by their own guards.
    let body = "let total = crate::weight(4);\n        assert_eq!(total, 12);";
    assert_eq!(
        path_admitted(
            &non_scalar_unit_tests(LET_BOUND_PRELUDE, body),
            "x * 3",
            None
        ),
        ["assert_eq!(total, 12);"]
    );
}

#[test]
fn a_scalar_result_read_by_other_assertions_still_pins() {
    // cell-0094: `weight` returns `u32`, so `total` is `Copy` with no
    // interior mutability. Another std assertion that reads it, or a
    // statement between the `let` and the pin, cannot change it.
    for (body, admitted) in [
        (
            "let total = crate::weight(4);\n        assert!(total > 3);\n        assert_eq!(total, 12);",
            vec!["assert_eq!(total, 12);"],
        ),
        (
            "let total = crate::weight(4);\n        assert_ne!(total, 7);\n        assert_eq!(12, total);",
            vec!["assert_eq!(12, total);"],
        ),
        (
            "let total = crate::weight(4);\n        assert_eq!(touch_count(), 1);\n        assert_eq!(total, 12);",
            vec!["assert_eq!(touch_count(), 1);", "assert_eq!(total, 12);"],
        ),
        (
            "let total = crate::weight(4);\n        assert_eq!(total, 12);\n        assert_eq!(total, 12);",
            vec!["assert_eq!(total, 12);", "assert_eq!(total, 12);"],
        ),
        (
            // A block between them that closes again keeps the depth.
            "let total = crate::weight(4);\n        {\n            assert!(total > 3);\n        }\n        assert_eq!(total, 12);",
            vec!["assert_eq!(total, 12);"],
        ),
    ] {
        let lib = unit_tests(LET_BOUND_PRELUDE, body);
        let mut got = path_admitted(&lib, "x * 3", None);
        got.retain(|text| text.contains("total"));
        let want: Vec<_> = admitted
            .into_iter()
            .filter(|text| text.contains("total"))
            .collect();
        assert_eq!(got, want, "{body}");
        // The non-scalar owner keeps the next-statement guard: only an
        // assertion right after the `let` with no other use pins.
        let lib = non_scalar_unit_tests(LET_BOUND_PRELUDE, body);
        let mut got = path_admitted(&lib, "x * 3", None);
        got.retain(|text| text.contains("total"));
        assert!(got.len() <= 1, "{body}: {got:?}");
        assert!(
            !body.contains("total > 3") || got.is_empty(),
            "{body}: {got:?}"
        );
    }
    for body in [
        // A use outside an assertion, or in a macro that is not a std
        // assertion, may rebind or shadow the name.
        "let total = crate::weight(4);\n        touch(&total);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        println!(\"{}\", total);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        {\n            const total: u32 = 12;\n            assert_eq!(total, 12);\n        }",
        // Still bound once, immutably, before every use.
        "let mut total = crate::weight(4);\n        assert!(total > 3);\n        assert_eq!(total, 12);",
        "assert!(total > 3);\n        let total = crate::weight(4);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        assert!(total > 3);\n        let total = 12;\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        assert!([1].iter().all(|total| *total > 0));\n        assert_eq!(total, 12);",
        // The pin compares the result with itself.
        "let total = crate::weight(4);\n        assert!(total > 3);\n        assert_eq!(total, total);",
        // The `let` sits in a block the assertion is outside of, so
        // `total` there names the module's `const` (cell-0094 review).
        "{\n            let total = crate::weight(4);\n            assert!(total > 3);\n        }\n        assert_eq!(total, 12);",
        "if false {\n            let total = crate::weight(4);\n        }\n        assert_eq!(total, 12);",
        "let _f = || {\n            let total = crate::weight(4);\n        };\n        assert_eq!(total, 12);",
        // A deeper block may import another `total` over the binding.
        "let total = crate::weight(4);\n        {\n            use super::*;\n            assert_eq!(total, 12);\n        }",
        "let total = crate::weight(4);\n        { use super::*;\n        assert_eq!(total, 12); }",
        "let total = crate::weight(4);\n        assert!(total > 3); { use super::*; assert_eq!(total, 12); }",
        // A path-qualified or crate-specific assertion macro.
        "let total = crate::weight(4);\n        std::assert!(total > 3);\n        assert_eq!(total, 12);",
        "let total = crate::weight(4);\n        prop_assert!(total > 3);\n        assert_eq!(total, 12);",
    ] {
        let lib = unit_tests(LET_BOUND_PRELUDE, body);
        let got = path_admitted(&lib, "x * 3", None);
        assert!(
            !got.iter().any(|text| text.contains("assert_eq!(total")),
            "{body}: {got:?}"
        );
    }
}

#[test]
fn a_shadowed_primitive_return_is_not_a_scalar() {
    // A same-named item or import makes `u32` name another type, which may
    // hold interior mutability, so the shared-handle guard comes back.
    let body = "let total = crate::weight(4);\n        assert!(total > 3);\n        assert_eq!(total, 12);";
    for prelude in [
        "#[allow(non_camel_case_types)]\npub struct u32(std::cell::Cell<u64>);\n",
        "#[allow(non_camel_case_types)]\npub type u32 = std::cell::Cell<u64>;\n",
        "use std::cell::Cell as u32;\n",
        // #7835 review: a raw identifier declares the same name.
        "#[allow(non_camel_case_types)]\npub struct r#u32(std::cell::Cell<u64>);\n",
        "#[allow(non_camel_case_types)]\npub type r#u32 = std::cell::Cell<u64>;\n",
        "use std::cell::Cell as r#u32;\n",
    ] {
        let lib = unit_tests(prelude, body);
        assert!(lib.starts_with(prelude), "{lib}");
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{prelude}");
    }
    // Matched controls: the same items under another name, and a cast to
    // the primitive, leave it a scalar.
    for prelude in [
        "#[allow(non_camel_case_types)]\npub struct gauge(std::cell::Cell<u64>);\n",
        "#[allow(non_camel_case_types)]\npub type gauge = std::cell::Cell<u64>;\n",
        "use std::cell::Cell as gauge;\n",
        "#[allow(non_camel_case_types)]\npub type r#gauge = std::cell::Cell<u64>;\n",
        "pub fn widen(x: u8) -> u64 {\n    x as u64 + 0 as u32 as u64\n}\n",
    ] {
        let lib = unit_tests(prelude, body);
        assert_eq!(
            path_admitted(&lib, "x * 3", None),
            ["assert_eq!(total, 12);"],
            "{prelude}"
        );
    }
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
        // `-> Wt`: a statement between a scalar's `let` and its assertion
        // cannot change it (cell-0094), so the guard is read off a
        // non-scalar owner.
        let lib = non_scalar_unit_tests("fn tick() {}\n", body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
        if !body.contains("tick();") {
            let lib = unit_tests("fn tick() {}\n", body);
            assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
        }
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
    // `tally` returns `u32`: a statement between the `let` and the pin
    // cannot change the copied result (cell-0094).
    assert_eq!(
        admitted("let n = c.tally();\n    drop(c);\n    assert_eq!(n, 1);"),
        ["assert_eq!(n, 1);"]
    );
    for body in [
        "let mut n = c.tally();\n    n += 0;\n    assert_eq!(n, 1);",
        "let n = c.tally();\n    touch(n);\n    assert_eq!(n, 1);",
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

mod expect_test_pins;
mod helper_pins;

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

#[test]
fn a_shadowed_err_refuses_the_err_return_guard_twin() {
    // #7130 review: a value binding of the name `Err` can route the
    // guard's `return Err(..)` to an `Ok`, so a changed owner no longer
    // fails the test and the guard pins nothing. Every guard twin in a
    // file that binds the name is refused, never re-read.
    let guard = "    if weight(4) != 12 {\n        return Err(\"mismatch\".to_string());\n    }\n    Ok(())";
    let test_fn = |header: &str| {
        format!(
            "use demo::weight;\n\n{header}#[test]\nfn weighs() -> Result<(), String> {{\n{guard}\n}}\n"
        )
    };
    let shadowed = [
        // The wrong implementation itself: a fn of the name returning Ok.
        test_fn("fn Err(_: &str) -> Result<(), String> { Ok(()) }\n\n"),
        // A const constructor and a pattern binding spell the name in the
        // value namespace too.
        test_fn("const Err: fn(&str) -> Result<(), String> = |_| Ok(());\n\n"),
        {
            let body =
                "    let Err = |_: &str| -> Result<(), String> { Ok(()) };\n".to_string() + guard;
            format!(
                "use demo::weight;\n\n#[test]\nfn weighs() -> Result<(), String> {{\n{body}\n}}\n"
            )
        },
        // An explicit import of the name, and a glob that reaches outside
        // the file's own scan.
        test_fn(
            "mod shadow {\n    pub fn Err(_: &str) -> Result<(), String> { Ok(()) }\n}\nuse shadow::Err;\n\n",
        ),
        test_fn("use std::io::prelude::*;\n\n"),
        // A file-root `super::*` reaches the parent file, and a longer
        // path may leave the file at an out-of-line module.
        test_fn("use super::*;\n\n"),
        {
            format!(
                "use demo::weight;\n\nmod wrapped {{\n    use super::sub::*;\n\n    #[test]\n    fn weighs() -> Result<(), String> {{\n{guard}\n    }}\n}}\n"
            )
        },
        // Bindings of an inline module do not leak outward, but the
        // refusal is deliberately file-wide: coarser and conservative.
        test_fn("mod shadow {\n    pub fn Err(_: &str) -> Result<(), String> { Ok(()) }\n}\n\n"),
    ];
    for tests in &shadowed {
        assert_not_pinned(tests, "if weight(4) != 12");
    }
    // Without a binding of the name the same guard still pins: the twin
    // needs the shadow question only when the file can shadow.
    assert!(
        weight_verdicts(
            &weight_test(" -> Result<(), String>", guard),
            "if weight(4) != 12"
        )
        .iter()
        .all(|(text, admitted)| *admitted && text.starts_with("if weight(4) != 12")),
        "the unshadowed guard must keep its credit"
    );
    // The idiomatic test-module glob stays within the file's own scan: it
    // re-imports only the definitions the whole-file check already
    // refuses, so it neither shadows nor withholds.
    let globbed = format!(
        "use demo::weight;\n\nmod wrapped {{\n    use super::*;\n\n    #[test]\n    fn weighs() -> Result<(), String> {{\n{guard}\n    }}\n}}\n"
    );
    let verdicts = weight_verdicts(&globbed, "if weight(4) != 12");
    assert!(
        verdicts
            .iter()
            .all(|(text, admitted)| *admitted && text.starts_with("if weight(4) != 12")),
        "the in-file test glob must keep its credit: {verdicts:?}"
    );
}

#[test]
fn a_literal_matches_pattern_pins_the_owner_value() {
    // RIPR-SPEC-0197 literal `matches!` pins: a literal-only `matches!` pattern pins the value
    // exactly as `assert_eq!` does; every pattern that admits more than one
    // value, or that the line cannot resolve, refuses.
    let lib = "pub fn check(x: u32) -> Result<u32, String> {\n    Ok(x + 0)\n}\n";
    let pinned = [
        "assert!(matches!(check(5), Ok(5)));",
        "assert!(matches!(check(5), Ok(x @ 5)));",
        "assert!(matches!(check(5), Ok(-5)));",
        "assert!(matches!(check(5), Ok(5,)));",
        "assert!(matches!(check(5), Ok( 5 ),));",
        "assert_eq!(check(5), Ok(5));",
    ];
    let refused = [
        "assert!(matches!(check(5), Ok(_)));",
        "assert!(matches!(check(5), Ok(1..=5)));",
        "assert!(matches!(check(5), Ok(5) | Ok(6)));",
        "assert!(matches!(check(5), Ok(x) if x == 5));",
        "assert!(matches!(check(5), Ok(FIVE)));",
        "assert!(matches!(check(5), Ok(x)));",
        "assert!(matches!(check(5), Ok(ref x @ 5)));",
        "assert!(!matches!(check(5), Ok(5)));",
        "assert!(matches!(check(5), Ok(5)) || true);",
        "assert!(matches!(other(check(5)), Ok(5)));",
        "assert!(matches!(check(5), Result::Ok(5)));",
        "assert!(matches!(check(5), Ok(5..)));",
        "assert!(matches!(check(5), Ok(-)));",
        // A rooted `matches!` can be rebound; another nested macro may exit.
        "assert!(std::matches!(check(5), Ok(5)));",
        "assert!(core::matches!(check(5), Ok(5)));",
        "assert!(matches!(check(dbg!(5)), Ok(5)));",
        "assert!(matches!(check(5), Ok(x @ _)));",
        "assert!(matches!(check(5), Ok(mut x @ 5)));",
        "assert!(matches!(check(5), Ok(X @ 5)));",
        "assert!(matches!(check(5), Ok((5, 6))));",
        "assert!(matches!(check(5), Ok([5])));",
        "assert!(matches!(check(5), Ok(&5)));",
        "assert!(matches!(check(5), Ok(b'5')));",
    ];
    for (assertion, expected) in pinned
        .iter()
        .map(|text| (text, true))
        .chain(refused.iter().map(|text| (text, false)))
    {
        let tests = format!("use demo::check;\n\n#[test]\nfn checks() {{\n    {assertion}\n}}\n");
        let index = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index, "check", "Ok(x + 0)");
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert_eq!(
            admitted(&index, &pin),
            vec![(assertion.to_string(), expected)],
            "{assertion}"
        );
    }
    // Other literal shapes normalize to the value `assert_eq!` would pin.
    for (lib, owner_line, assertion, expected) in [
        (
            "pub fn check(x: u32) -> Option<bool> {\n    Some(x > 0)\n}\n",
            "Some(x > 0)",
            "assert!(matches!(check(5), Some(true)));",
            true,
        ),
        (
            "pub fn check(x: u32) -> Result<Option<u32>, String> {\n    Ok(Some(x + 0))\n}\n",
            "Ok(Some(x + 0))",
            "assert!(matches!(check(5), Ok(Some(5))));",
            true,
        ),
    ] {
        let tests = format!("use demo::check;\n\n#[test]\nfn checks() {{\n    {assertion}\n}}\n");
        let index = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index, "check", owner_line);
        assert!(pin.is_some(), "{lib}");
        let Some(pin) = pin else { return };
        assert_eq!(
            admitted(&index, &pin),
            vec![(assertion.to_string(), expected)],
            "{assertion}"
        );
    }
    // A pattern resolves `Ok` (or `None`) in the test's scope: a binding of
    // that name there matches some other value, so the pin refuses (#7581
    // review).
    for (prelude, expected) in [
        ("", true),
        ("enum Fake { Ok(u32) }\nuse Fake::Ok;\n", false),
        (
            "#[allow(non_upper_case_globals)]\nconst Ok: u32 = 5;\n",
            false,
        ),
    ] {
        let tests = format!(
            "use demo::check;\n{prelude}\n#[test]\nfn checks() {{\n    assert!(matches!(check(5), Ok(5)));\n}}\n"
        );
        let index = index(&[(LIB, lib), (TESTS, &tests)]);
        let pin = establish(&index, "check", "Ok(x + 0)");
        assert!(pin.is_some());
        let Some(pin) = pin else { return };
        assert_eq!(
            admitted(&index, &pin),
            vec![("assert!(matches!(check(5), Ok(5)));".to_string(), expected)],
            "{prelude}"
        );
    }
    // A workspace `macro_rules! matches` rebinds the nested name.
    let tests = "use demo::check;\n\nmacro_rules! matches {\n    ($($t:tt)*) => { true };\n}\n\n#[test]\nfn checks() {\n    assert!(matches!(check(5), Ok(5)));\n}\n";
    let index = index(&[(LIB, lib), (TESTS, tests)]);
    let pin = establish(&index, "check", "Ok(x + 0)");
    assert!(pin.is_some());
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted(&index, &pin),
        vec![("assert!(matches!(check(5), Ok(5)));".to_string(), false)]
    );
}

// --- #6617: associated-function owners pinned through `T::name(..)` ---

/// An inherent associated `Gauge::reading` (the owner, first in the file)
/// beside a module-level `reading` of the same name.
const ASSOC_GAUGE_LIB: &str = "pub struct Gauge;\n\nimpl Gauge {\n    pub fn reading() -> u32 {\n        40 + 2\n    }\n}\n\npub struct Other;\n\npub fn reading() -> u32 {\n    0\n}\n";

fn gauge_test(prelude: &str, body: &str) -> String {
    format!("{prelude}\n\n#[test]\nfn reads() {{\n    {body}\n}}\n")
}

/// The assertions of the one test that the `Gauge::reading` pin admits, or
/// `None` when the pin is not established.
fn gauge_admitted(files: &[(&str, &str)]) -> Option<Vec<String>> {
    let index = index(files);
    let owner = owner(&index, "reading");
    assert_eq!(
        (owner.item.container.clone(), owner.item.has_self_param),
        (
            FunctionContainer::Inherent {
                self_ty: "Gauge".to_string()
            },
            false
        ),
        "the fixture's owner is the associated `Gauge::reading`"
    );
    let pin = establish(&index, "reading", "40 + 2")?;
    Some(admitted_texts(&index, &pin))
}

#[test]
fn associated_function_pins_through_a_directly_imported_owner_type() {
    for body in [
        "assert_eq!(Gauge::reading(), 42);",
        "assert_eq!(42, Gauge::reading());",
        "let value = Gauge::reading();\n    assert_eq!(value, 42);",
    ] {
        let tests = gauge_test("use demo::Gauge;", body);
        let admitted = gauge_admitted(&[(LIB, ASSOC_GAUGE_LIB), (TESTS, &tests)]);
        assert_eq!(admitted.map(|texts| texts.len()), Some(1), "{body}");
    }
}

#[test]
fn associated_function_pin_refuses_other_names_for_the_type() {
    for (prelude, body) in [
        // Aliased, glob, brace-list and missing imports.
        (
            "use demo::Gauge as Knob;",
            "assert_eq!(Knob::reading(), 42);",
        ),
        (
            "use demo::Other as Gauge;",
            "assert_eq!(Gauge::reading(), 42);",
        ),
        ("use demo::*;", "assert_eq!(Gauge::reading(), 42);"),
        ("use demo::{Gauge};", "assert_eq!(Gauge::reading(), 42);"),
        ("", "assert_eq!(Gauge::reading(), 42);"),
        // A crate that is not the owner's library (an unindexed dependency).
        ("use spare::Gauge;", "assert_eq!(Gauge::reading(), 42);"),
        // An import that does not reach the top-level test.
        (
            "mod inner {\n    use demo::Gauge;\n}",
            "assert_eq!(Gauge::reading(), 42);",
        ),
        // The test's file or body rebinds the name.
        (
            "use demo::Gauge;",
            "struct Gauge;\n    assert_eq!(Gauge::reading(), 42);",
        ),
        (
            "use demo::Gauge;",
            "use spare::Gauge;\n    assert_eq!(Gauge::reading(), 42);",
        ),
        (
            "use demo::Gauge;\nmod nested {\n    pub use spare::Gauge;\n}",
            "assert_eq!(Gauge::reading(), 42);",
        ),
        // Another type's associated function of the same name.
        ("use demo::Other;", "assert_eq!(Other::reading(), 42);"),
        ("use demo::Gauge;", "assert_eq!(Other::reading(), 42);"),
        // A bare call names the module-level `reading`, never the owner.
        ("use demo::reading;", "assert_eq!(reading(), 42);"),
        // The owner compared with itself, a method-call shape, an
        // inequality, and a call chained after the owner.
        (
            "use demo::Gauge;",
            "assert_eq!(Gauge::reading(), Gauge::reading());",
        ),
        (
            "use demo::Gauge;",
            "let gauge = Gauge;\n    assert_eq!(gauge.reading(), 42);",
        ),
        ("use demo::Gauge;", "assert_ne!(Gauge::reading(), 41);"),
        (
            "use demo::Gauge;",
            "assert_eq!(Gauge::reading().saturating_sub(1), 41);",
        ),
    ] {
        let tests = gauge_test(prelude, body);
        let admitted = gauge_admitted(&[(LIB, ASSOC_GAUGE_LIB), (TESTS, &tests)]);
        // A test-local `struct Gauge` is a second workspace declaration, so
        // that shape refuses already on the owner side; every other row is
        // refused by the test-side gate on an established pin.
        let expected = if body.starts_with("struct Gauge;") {
            None
        } else {
            Some(Vec::new())
        };
        assert_eq!(admitted, expected, "{tests}");
    }
}

#[test]
fn associated_function_pin_refuses_a_cfg_gated_owner() {
    // With the cfg off, `Gauge::reading()` reaches a trait default the
    // index never sees (#7724), so a gate on the owner, its `impl` or an
    // enclosing module refuses.
    let tests = gauge_test("use demo::Gauge;", "assert_eq!(Gauge::reading(), 42);");
    for lib in [
        "pub struct Gauge;\n\n#[cfg(feature = \"inherent\")]\nimpl Gauge {\n    pub fn reading() -> u32 {\n        40 + 2\n    }\n}\n",
        "pub struct Gauge;\n\nimpl Gauge {\n    #[cfg(feature = \"inherent\")]\n    pub fn reading() -> u32 {\n        40 + 2\n    }\n}\n",
        "pub struct Gauge;\n\nimpl Gauge {\n    #[cfg_attr(feature = \"inherent\", inline)]\n    pub fn reading() -> u32 {\n        40 + 2\n    }\n}\n",
    ] {
        let admitted = gauge_admitted(&[(LIB, lib), (TESTS, &tests)]);
        assert_eq!(admitted, Some(Vec::new()), "{lib}");
    }
    // Control: the same owner without a cfg pins.
    let lib = "pub struct Gauge;\n\nimpl Gauge {\n    pub fn reading() -> u32 {\n        40 + 2\n    }\n}\n";
    let admitted = gauge_admitted(&[(LIB, lib), (TESTS, &tests)]);
    assert_eq!(admitted.map(|texts| texts.len()), Some(1), "{lib}");
}

#[test]
fn associated_function_pin_refuses_a_longer_import_path() {
    // `demo::fs` re-exports another crate, whose own `Gauge` the test could
    // be importing; the pin admits only `use <krate>::T;`.
    let reexporting = format!("pub use spare as fs;\n{ASSOC_GAUGE_LIB}");
    for (lib, prelude) in [
        (reexporting.as_str(), "use demo::fs::Gauge;"),
        (ASSOC_GAUGE_LIB, "use demo::self::Gauge;"),
    ] {
        let tests = gauge_test(prelude, "assert_eq!(Gauge::reading(), 42);");
        let admitted = gauge_admitted(&[(LIB, lib), (TESTS, &tests)]);
        assert_eq!(admitted, Some(Vec::new()), "{lib}\n{tests}");
    }
    // Control: the same library admits the direct import.
    let tests = gauge_test("use demo::Gauge;", "assert_eq!(Gauge::reading(), 42);");
    let admitted = gauge_admitted(&[(LIB, &reexporting), (TESTS, &tests)]);
    assert_eq!(
        admitted.map(|texts| texts.len()),
        Some(1),
        "the direct import must still pin"
    );
}

#[test]
fn associated_function_pin_is_not_established_beside_a_competitor() {
    let tests = gauge_test("use demo::Gauge;", "assert_eq!(Gauge::reading(), 42);");
    for other in [
        // Another type's associated function or a trait item of the name.
        "pub struct Meter;\nimpl Meter {\n    pub fn reading() -> u32 {\n        1\n    }\n}\n",
        "pub trait Read {\n    fn reading() -> u32 {\n        1\n    }\n}\n",
        // A second `Gauge`, or a module, trait or alias of the name.
        "pub mod spare {\n    pub struct Gauge;\n}\n",
        "pub mod Gauge {\n    pub fn level() -> u32 {\n        1\n    }\n}\n",
        "pub trait Gauge {}\n",
        "pub type Gauge = u32;\n",
    ] {
        let admitted = gauge_admitted(&[
            (LIB, ASSOC_GAUGE_LIB),
            (TESTS, &tests),
            ("src/other.rs", other),
        ]);
        assert_eq!(admitted, None, "{other}");
    }
    // The library re-exports a foreign `Gauge` beside its own: the pin is
    // established but the import may name the foreign type.
    let admitted = gauge_admitted(&[
        (LIB, &format!("pub use external::Gauge;\n{ASSOC_GAUGE_LIB}")),
        (TESTS, &tests),
    ]);
    assert_eq!(admitted, Some(Vec::new()));
    // A generic self type is not a plain type name.
    let generic = "pub struct Gauge<T>(T);\n\nimpl<T> Gauge<T> {\n    pub fn reading() -> u32 {\n        40 + 2\n    }\n}\n";
    let index = index(&[(LIB, generic), (TESTS, &tests)]);
    assert!(establish(&index, "reading", "40 + 2").is_none());
}

/// The `authored-old-deps` corpus shape (#6617): `panel`'s integration test
/// imports `meter::Gauge`; `spare` holds a same-named free `reading`. The
/// manifests decide which crate names reach the owner's library.
#[test]
fn associated_function_pin_resolves_the_importing_crate_through_manifests() -> Result<(), String> {
    let panel_manifest = "[package]\nname = \"panel\"\nversion = \"0.1.0\"\n\n[dependencies]\nmeter = { path = \"../meter\" }\nspare = { path = \"../spare\" }\ngadget = \"1\"\n";
    let root = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-associated",
        &[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"meter\", \"spare\", \"panel\"]\n",
            ),
            (
                "meter/Cargo.toml",
                "[package]\nname = \"meter\"\nversion = \"0.1.0\"\n",
            ),
            (
                "spare/Cargo.toml",
                "[package]\nname = \"spare\"\nversion = \"0.1.0\"\n",
            ),
            ("panel/Cargo.toml", panel_manifest),
        ],
    )?;
    let meter = "pub struct Gauge;\n\nimpl Gauge {\n    pub fn reading() -> u32 {\n        40 + 2\n    }\n}\n";
    let spare = "pub fn reading() -> u32 {\n    0\n}\n";
    let admitted = |test_source: &str| {
        let mut index = RustIndex::default();
        for (path, text) in [
            ("meter/src/lib.rs", meter),
            ("spare/src/lib.rs", spare),
            ("panel/src/lib.rs", "//! Panel.\n"),
            ("panel/tests/direct.rs", test_source),
        ] {
            let facts = summarize_file(PathBuf::from(path), text.to_string());
            index.extend_functions(facts.functions.iter().cloned());
            index.extend_tests(facts.tests.iter().cloned());
            index.insert_file_only(PathBuf::from(path), facts);
        }
        index.member_crates = crate::analysis::facts::member_crates::MemberCrates::new(&root);
        let owner = index.functions().iter().find(|function| {
            function.name == "reading" && function.file == Path::new("meter/src/lib.rs")
        });
        let owner = owner?;
        let pin = OwnerReturnPin::establish(&return_probe(owner, "40 + 2"), owner, &index)?;
        let syntax = OwnerPinSyntax::default();
        let test = index.tests().at(0);
        Some(
            test.assertions
                .iter()
                .filter(|assertion| pin.admits(test, assertion, &index, &|_, _| false, &syntax))
                .count(),
        )
    };
    let direct = admitted(&gauge_test(
        "use meter::Gauge;",
        "assert_eq!(Gauge::reading(), 42);",
    ));
    let sibling = admitted(&gauge_test(
        "use spare::Gauge;",
        "assert_eq!(Gauge::reading(), 42);",
    ));
    let registry = admitted(&gauge_test(
        "use gadget::Gauge;",
        "assert_eq!(Gauge::reading(), 42);",
    ));
    let synthetic_name = admitted(&gauge_test(
        "use demo::Gauge;",
        "assert_eq!(Gauge::reading(), 42);",
    ));
    let aliased = admitted(&gauge_test(
        "use meter::Gauge as Knob;",
        "assert_eq!(Knob::reading(), 42);",
    ));
    let globbed = admitted(&gauge_test(
        "use meter::*;",
        "assert_eq!(Gauge::reading(), 42);",
    ));
    let bare = admitted(&gauge_test(
        "use spare::reading;",
        "assert_eq!(reading(), 42);",
    ));
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(direct, Some(1), "`use meter::Gauge;` must pin the owner");
    for (label, refused) in [
        ("sibling crate", sibling),
        ("registry dependency", registry),
        ("undeclared crate", synthetic_name),
        ("alias", aliased),
        ("glob", globbed),
        ("bare free call", bare),
    ] {
        assert_eq!(refused, Some(0), "{label}");
    }
    Ok(())
}

/// RIPR-SPEC-0265: an owner whose whole body is one closure in an iterator
/// adaptor over a slice parameter, with one same-file test.
fn adaptor_lib(tail: &str, test_items: &str) -> String {
    format!(
        "pub fn count_positive(xs: &[i32]) -> usize {{\n    {tail}\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{test_items}\n}}\n"
    )
}

const FILTER_TAIL: &str = "xs.iter().filter(|&&x| x > 0).count()";

fn adaptor_test(body: &str) -> String {
    format!("    #[test]\n    fn counts() {{\n{body}\n    }}")
}

/// RIPR-SPEC-0265 rule 2: the pinned call must run the closure at least
/// once on a literal sequence ripr reads.
#[test]
fn adaptor_tail_pins_only_a_non_empty_literal_sequence() {
    let lib = adaptor_lib(
        FILTER_TAIL,
        &adaptor_test(
            "        assert_eq!(count_positive(&[-1, 0, 3, 7]), 2);\n        assert_eq!(count_positive(&[]), 0);\n        assert_eq!(count_positive(&[1, 2][..]), 2);\n        let v = [1, 2];\n        assert_eq!(count_positive(&v), 2);\n        assert_eq!(2, count_positive(&[5, 6]));",
        ),
    );
    let index = index(&[(LIB, &lib)]);
    let pin = establish(&index, "count_positive", FILTER_TAIL);
    assert!(pin.is_some(), "the adaptor tail must establish a pin");
    let Some(pin) = pin else { return };
    assert_eq!(
        admitted_texts(&index, &pin),
        vec![
            "assert_eq!(count_positive(&[-1, 0, 3, 7]), 2);".to_string(),
            "assert_eq!(2, count_positive(&[5, 6]));".to_string(),
        ]
    );
}

/// RIPR-SPEC-0265 rule 2 through a constant-row table: some row must pass a
/// non-empty sequence.
#[test]
fn adaptor_tail_pins_a_table_with_a_non_empty_row() {
    for (rows, admitted) in [
        ("[(&[], 0), (&[1, -1], 1)]", true),
        ("[(&[], 0), (&[], 0)]", false),
        // One row ripr cannot read as a literal sequence refuses the pin,
        // even beside a non-empty literal row.
        ("[(&[1, -1], 1), (&[0; 2], 0)]", false),
        ("[(&[1, -1], 1), (&[3u32], 1)]", false),
    ] {
        let lib = adaptor_lib(
            FILTER_TAIL,
            &adaptor_test(&format!(
                "        let cases: [(&[i32], usize); 2] = {rows};\n        for (xs, want) in cases {{\n            assert_eq!(count_positive(xs), want);\n        }}"
            )),
        );
        let index = index(&[(LIB, &lib)]);
        let pin = establish(&index, "count_positive", FILTER_TAIL);
        assert!(pin.is_some(), "the adaptor tail must establish a pin");
        let Some(pin) = pin else { return };
        assert_eq!(!admitted_texts(&index, &pin).is_empty(), admitted, "{rows}");
    }
}

/// RIPR-SPEC-0265 rule 2 owner side: only the admitted tail, changed as a
/// whole, establishes the adaptor gate.
#[test]
fn adaptor_gate_refuses_tails_outside_the_rule() {
    let test = adaptor_test("        assert_eq!(count_positive(&[1]), 1);");
    for (tail, changed) in [
        ("xs.iter().filter(|&&x| x > 0 && x < 9).count()", None),
        (
            "xs.iter().filter(|&&x| [1].iter().any(|&y| y == x)).count()",
            None,
        ),
        (
            "xs.iter().filter(|&&x| if x > 0 { true } else { false }).count()",
            None,
        ),
        ("xs.iter().position(|&x| x > 0).unwrap_or(0)", None),
        ("xs.iter().fold(0, |n, &x| n + usize::from(x > 0))", None),
        ("xs.to_vec().iter().filter(|&&x| x > 0).count()", None),
        // Consumers that drop closure values: `map` + `count` ignores
        // them, `filter` + `sum` drops an additive identity unseen.
        ("xs.iter().map(|&x| x > 0).count()", None),
        ("xs.iter().filter(|&&x| x > 0).sum()", None),
        // The predicate alone is not the owner's return value.
        (FILTER_TAIL, Some("x > 0")),
    ] {
        let lib = adaptor_lib(tail, &test);
        let index = index(&[(LIB, &lib)]);
        assert!(
            establish(&index, "count_positive", changed.unwrap_or(tail)).is_none(),
            "{tail}"
        );
    }
}

/// RIPR-SPEC-0265 rule 2: a `collect` into `Vec` pins only while the
/// owner's file binds no other item to the name `Vec`.
#[test]
fn adaptor_gate_refuses_a_collect_into_a_rebound_vec() {
    let tail = "xs.iter().map(|&x| x > 0).collect()";
    let test = adaptor_test("        assert!(pos(&[0, 5]).len() > 0);");
    for (items, admitted) in [
        ("", true),
        ("type Vec<T> = std::collections::BTreeSet<T>;\n", false),
        ("use std::collections::BTreeSet as Vec;\n", false),
        ("use other::Vec;\n", false),
        ("pub struct Vec<T>(T);\n", false),
        ("pub enum Vec { A }\n", false),
    ] {
        let lib = format!(
            "pub fn pos(xs: &[i32]) -> Vec<bool> {{\n    {tail}\n}}\n{items}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{test}\n}}\n"
        );
        let index = index(&[(LIB, &lib)]);
        assert_eq!(
            establish(&index, "pos", tail).is_some(),
            admitted,
            "{items}"
        );
    }
}

#[test]
fn a_scalar_rendering_compared_with_exact_text_pins() {
    // trap-shine-debug-exact-control: the std `Debug` or `Display` text of
    // a primitive scalar names its value, so exact text pins it.
    for body in [
        "assert_eq!(format!(\"{:?}\", crate::weight(4)), \"12\");",
        "assert_eq!(\"12\", format!(\"{}\", crate::weight(4)));",
        "assert_eq!(format!(\"{}\", crate::weight(4),), \"12\");",
        "assert_eq!(format!(\"{:?}\", crate::weight(4)), \"12\", \"rendered\");",
        // RIPR-SPEC-0231 example 12: fixed text around the one placeholder
        // fixes where the rendering starts and ends.
        "assert_eq!(format!(\"w{}\", crate::weight(4)), \"w12\");",
        "assert_eq!(format!(\"{{\\\"w\\\":{}}}\", crate::weight(4)), \"{\\\"w\\\":12}\");",
    ] {
        let lib = unit_tests("", body);
        assert_eq!(path_admitted(&lib, "x * 3", None), [body], "{body}");
    }
    for body in [
        // Flags, width, padding and extra text or arguments.
        "assert_eq!(format!(\"{:x}\", crate::weight(4)), \"c\");",
        "assert_eq!(format!(\"{:#?}\", crate::weight(4)), \"12\");",
        "assert_eq!(format!(\"{:>3}\", crate::weight(4)), \" 12\");",
        "assert_eq!(format!(\"{}{}\", crate::weight(4), 1), \"121\");",
        // The expected text does not hold the template's text, or holds
        // nothing where the rendering goes.
        "assert_eq!(format!(\"w{}\", crate::weight(4)), \"v12\");",
        "assert_eq!(format!(\"w{}\", crate::weight(4)), \"w\");",
        // The other side is not plain literal text.
        "let want = String::from(\"12\");\n        assert_eq!(format!(\"{}\", crate::weight(4)), want);",
        "assert_eq!(format!(\"{}\", crate::weight(4)), format!(\"{}\", 12));",
        "assert_eq!(format!(\"{}\", crate::weight(4)), \"1\\x32\");",
        // A qualified or other nested macro, or a projection of the call.
        "assert_eq!(std::format!(\"{}\", crate::weight(4)), \"12\");",
        "assert_eq!(format!(\"{}\", crate::weight(dbg!(4))), \"12\");",
        "assert_eq!(format!(\"{}\", crate::weight(4) / 2), \"6\");",
        "assert_eq!(format!(\"{}\", crate::weight(4)).len(), 2);",
        "assert_eq!(format!(\"{}\", crate::weight(4)).len().to_string(), \"2\");",
        "assert_eq!(format!(\"{}\", crate::weight(4)).replace(\"6\", \"2\"), \"12\");",
        "assert_ne!(format!(\"{}\", crate::weight(4)), \"13\");",
        "assert!(!format!(\"{:?}\", crate::weight(4)).is_empty());",
    ] {
        let lib = unit_tests("", body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // #7905: every NaN payload renders as `NaN`, so that text pins no
    // single float value; finite float text still does.
    let float_tests = |body: &str| {
        let lib = unit_tests("", body).replace("-> u32 {\n    x * 3", "-> f64 {\n    x * 3");
        assert!(lib.contains("-> f64 {"), "{lib}");
        lib
    };
    for body in [
        "assert_eq!(format!(\"{:?}\", crate::weight(4)), \"NaN\");",
        "assert_eq!(format!(\"{}\", crate::weight(4)), \"NaN\");",
        "assert_eq!(\"NaN\", format!(\"{:?}\", crate::weight(4)));",
        "assert_eq!(\"NaN\", format!(\"{}\", crate::weight(4)));",
        "assert_eq!(format!(\"w={}\", crate::weight(4)), \"w=NaN\");",
    ] {
        assert!(
            path_admitted(&float_tests(body), "x * 3", None).is_empty(),
            "{body}"
        );
    }
    let body = "assert_eq!(format!(\"{:?}\", crate::weight(4)), \"12.0\");";
    assert_eq!(path_admitted(&float_tests(body), "x * 3", None), [body]);
    // A non-primitive return type may render many values the same way.
    let body = "assert_eq!(format!(\"{:?}\", crate::weight(4)), \"12\");";
    let lib = non_scalar_unit_tests("", body);
    assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{lib}");
}

/// RIPR-SPEC-0197 rule 3 short-circuit tails (#7969): a pure `&&`/`||`
/// chain pins only on a call that evaluates every operand.
fn short_circuit_lib(tail: &str, test_items: &str) -> String {
    format!(
        "pub fn both_zero(x: i32, y: i32) -> bool {{\n    {tail}\n}}\n\nfn check(x: i32, y: i32, want: bool) {{\n    assert_eq!(both_zero(x, y), want);\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{test_items}\n}}\n"
    )
}

#[test]
fn a_short_circuit_tail_pins_only_on_a_call_that_runs_every_operand() {
    const AND: &str = "x == 0 && 0 == y";
    const OR: &str = "x != 0 || 0 == y";
    for (tail, body, admitted) in [
        // `(0, 0)` runs both operands of the `&&`; `(1, 0)` stops at `x`.
        (AND, "        assert_eq!(both_zero(0, 0), true);", true),
        (AND, "        assert_eq!(both_zero(1, 0), false);", false),
        (AND, "        assert_eq!(false, both_zero(-1, 5));", false),
        // `||` continues on a false first operand.
        (OR, "        assert_eq!(both_zero(0, 4), false);", true),
        (OR, "        assert_eq!(both_zero(3, 0), true);", false),
        // An argument ripr cannot read never counts.
        (
            AND,
            "        let x = 0;\n        assert_eq!(both_zero(x, 0), true);",
            false,
        ),
        (AND, "        assert_eq!(both_zero(0i32, 0), true);", false),
        // An unread last argument fails closed too.
        (
            AND,
            "        let y = 0;\n        assert_eq!(both_zero(0, y), true);",
            false,
        ),
        // A table row that runs both operands.
        (
            AND,
            "        for (x, y, want) in [(1, 0, false), (0, 0, true)] {\n            assert_eq!(both_zero(x, y), want);\n        }",
            true,
        ),
        (
            AND,
            "        for (x, y, want) in [(1, 0, false), (2, 0, false)] {\n            assert_eq!(both_zero(x, y), want);\n        }",
            false,
        ),
    ] {
        let lib = short_circuit_lib(
            tail,
            &format!("    #[test]\n    fn pins() {{\n{body}\n    }}"),
        );
        let index = index(&[(LIB, &lib)]);
        let pin = establish(&index, "both_zero", tail);
        assert!(pin.is_some(), "the chain must establish a pin: {tail}");
        let Some(pin) = pin else { return };
        assert_eq!(!admitted_texts(&index, &pin).is_empty(), admitted, "{body}");
    }
}

#[test]
fn a_short_circuit_table_column_counts_only_inside_its_loop() {
    // Outside the loop, `x` and `y` are the module's constants `(1, 0)`,
    // which stop at `x == 0`; the loop's `(0, 0)` row is not that call.
    const AND: &str = "x == 0 && 0 == y";
    for (assertion, admitted) in [
        (
            "        assert_eq!(both_zero(x, y), want);\n        }",
            true,
        ),
        (
            "        }\n        assert_eq!(both_zero(x, y), false);",
            false,
        ),
    ] {
        let lib = short_circuit_lib(
            AND,
            &format!(
                "    #[allow(non_upper_case_globals)]\n    const x: i32 = 1;\n    #[allow(non_upper_case_globals)]\n    const y: i32 = 0;\n\n    #[test]\n    fn pins() {{\n        for (x, y, want) in [(0, 0, true)] {{\n            let _ = (x, y, want);\n    {assertion}\n    }}"
            ),
        );
        let index = index(&[(LIB, &lib)]);
        let pin = establish(&index, "both_zero", AND);
        assert!(pin.is_some(), "the chain must establish a pin");
        let Some(pin) = pin else { return };
        assert_eq!(
            !admitted_texts(&index, &pin).is_empty(),
            admitted,
            "{assertion}"
        );
    }
}

#[test]
fn a_short_circuit_tail_refuses_a_body_with_earlier_statements() {
    // `let x = x + 1;` rebinds the parameter the chain reads, so the call's
    // `0` is not the value `x == 0` compares. The same pin without the
    // statement is the control.
    for (statement, admitted) in [("", true), ("    let x = x + 1;\n", false)] {
        let lib = format!(
            "pub fn both_zero(x: i32, y: i32) -> bool {{\n{statement}    x == 0 && y == 1\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn pins() {{\n        assert_eq!(both_zero(0, 1), true);\n    }}\n}}\n"
        );
        let index = index(&[(LIB, lib.as_str())]);
        let owner = owner(&index, "both_zero");
        // The probe sits on the chain's line, after any statement.
        let mut probe = return_probe(owner, "x == 0 && y == 1");
        probe.location = SourceLocation::new(
            owner.file.clone(),
            owner.start_line + 1 + usize::from(!statement.is_empty()),
            1,
        );
        assert_eq!(
            OwnerReturnPin::establish(&probe, owner, &index).is_some(),
            admitted,
            "{statement:?}"
        );
    }
}

#[test]
fn a_short_circuit_tail_refuses_impure_or_mixed_chains() {
    for tail in [
        "x == 0 && check_y(y)",
        "x == 0 && y + 1 == 1",
        "x == 0 || y == 0 && x == y",
        "(x == 0) && y == 0",
        "x == 0 && y == LIMIT",
    ] {
        let lib = short_circuit_lib(
            tail,
            "    #[test]\n    fn pins() {\n        assert_eq!(both_zero(0, 0), true);\n    }",
        );
        let index = index(&[(LIB, &lib)]);
        assert!(establish(&index, "both_zero", tail).is_none(), "{tail}");
    }
}

#[test]
fn an_integer_projection_that_keeps_the_value_pins() {
    // trap-ceiling-wide-min-control / trap-muffle-times-one-control: a
    // result below a `.min` bound (above a `.max` bound) is the call's own
    // value, and `* 1` / `+ 0` keep every value.
    for body in [
        "assert_eq!(crate::weight(4).min(100), 12);",
        "assert_eq!(12, crate::weight(4).min(1_000));",
        "assert_eq!(crate::weight(4)\n            .min(100), 12);",
        "assert_eq!(crate::weight(4).max(3), 12);",
        "assert_eq!(crate::weight(4) * 1, 12);",
        "assert_eq!(1 * crate::weight(4), 12);",
        "assert_eq!(crate::weight(4) + 0, 12);",
        "assert_eq!(0 + crate::weight(4), 12);",
    ] {
        let lib = unit_tests("", body);
        assert_eq!(path_admitted(&lib, "x * 3", None), [body], "{body}");
    }
    for body in [
        // The bound clamps, or the projection loses the value.
        "assert_eq!(crate::weight(4).min(12), 12);",
        "assert_eq!(crate::weight(4).min(1), 1);",
        "assert_eq!(crate::weight(4).max(12), 12);",
        "assert_eq!(crate::weight(4).max(20), 20);",
        "assert_eq!(crate::weight(4) * 0, 0);",
        "assert_eq!(crate::weight(4) * 10, 120);",
        "assert_eq!(crate::weight(4) + 10, 22);",
        "assert_eq!(crate::weight(4) - 0, 12);",
        "assert_eq!(crate::weight(4) / 1, 12);",
        // A bound or expected value ripr does not read as a literal.
        "assert_eq!(crate::weight(4).min(LIMIT), 12);",
        "assert_eq!(crate::weight(4).min(100), TWELVE);",
        "assert_eq!(crate::weight(4).min(100u32), 12);",
        "assert_eq!(crate::weight(4).min(0x64), 12);",
        // A leading `_` names a const or binding, not a literal.
        "assert_eq!(crate::weight(4).min(_100), 12);",
        "assert_eq!(crate::weight(4).max(3), _12);",
        // Something else around the call.
        "assert_eq!(crate::weight(4).min(100).min(50), 12);",
        "assert_eq!(crate::weight(4) / 2 * 1, 6);",
        "assert_ne!(crate::weight(4).min(100), 13);",
    ] {
        let lib = unit_tests("", body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // Floats and other non-integer returns keep only the bare call.
    let body = "assert_eq!(crate::weight(4).min(100), 12);";
    let lib = non_scalar_unit_tests("", body);
    assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{lib}");
    // A float or bool owner is a scalar but not an integer.
    for returned in ["f64", "bool"] {
        let lib = unit_tests("", body).replace("-> u32", &format!("-> {returned}"));
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{lib}");
    }
    // #7932 review: `.min`/`.max` are the prelude `Ord`'s only while nothing
    // rebinds them. A local `trait Ord` shadows the prelude's, and an import
    // of `Ord`, a foreign glob, or a workspace `fn min` may supply another.
    let rebinding = [
        "trait Ord {\n    fn min(self, k: u32) -> u32;\n}\nimpl Ord for u32 {\n    fn min(self, _k: u32) -> u32 {\n        12\n    }\n}\n\n",
        "use other::Ord;\n\n",
        "use other::{\n    Ord,\n};\n\n",
        "pub struct Span;\nimpl Span {\n    pub fn min(&self) -> u32 {\n        0\n    }\n}\n\n",
    ];
    for prelude in rebinding {
        let lib = unit_tests(prelude, body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{lib}");
        // `* 1` keeps std's operator impl, which coherence fixes.
        let lib = unit_tests(prelude, "assert_eq!(crate::weight(4) * 1, 12);");
        assert_eq!(path_admitted(&lib, "x * 3", None).len(), 1, "{lib}");
    }
    // A foreign glob may import an `Ord`; it already refuses every pin.
    let lib = unit_tests("use other::*;\n\n", body);
    assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{lib}");
    let max = "assert_eq!(crate::weight(4).max(3), 12);";
    let lib = unit_tests("pub fn max(a: u32) -> u32 {\n    a\n}\n\n", max);
    assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{lib}");
    // A longer name, or the other method, rebinds nothing.
    for prelude in [
        "pub fn minimum(a: u32) -> u32 {\n    a\n}\n\n",
        "pub fn max(a: u32) -> u32 {\n    a\n}\n\n",
        "use std::collections::BTreeMap;\n\n",
    ] {
        let lib = unit_tests(prelude, body);
        assert_eq!(path_admitted(&lib, "x * 3", None), [body], "{lib}");
    }
}

#[test]
fn a_one_element_vec_indexed_at_zero_pins_the_owner() {
    // trap-pile-first-element-control: `vec![call][0]` is the call's value.
    for body in [
        "assert_eq!(vec![crate::weight(4)][0], 12);",
        "assert_eq!(12, vec![crate::weight(4)][0]);",
        "assert_eq!(vec![crate::weight(4),][0], 12);",
        "assert_eq!(vec![\n            crate::weight(4),\n        ][0], 12);",
        "assert_eq!(vec![crate::weight(4)][0], 12, \"first\");",
        // A `;` inside a string argument is not the repeat separator.
        "assert_eq!(vec![crate::weight(\"a;b\".len() as u32)][0], 9);",
        // A `vec!` in the expected value or message leaves the bare call.
        "assert_eq!(crate::weight(4), vec![12][0]);",
        "assert_eq!(crate::weight(4), 12, \"{:?}\", vec![1]);",
    ] {
        let lib = unit_tests("", body);
        assert_eq!(path_admitted(&lib, "x * 3", None), [body], "{body}");
    }
    for body in [
        // Another element, index, repeat form or projection.
        "assert_eq!(vec![crate::weight(4), 1][0], 12);",
        "assert_eq!(vec![1, crate::weight(4)][1], 12);",
        "assert_eq!(vec![crate::weight(4)][1], 12);",
        "assert_eq!(vec![crate::weight(4); 2][0], 12);",
        "assert_eq!(vec![crate::weight(4) ; 2][0], 12);",
        "assert_eq!(vec![crate::weight(4)][0] + 1, 13);",
        "assert_eq!(vec![crate::weight(4)].len(), 1);",
        "assert_eq!(vec![crate::weight(4)], vec![12]);",
        // A qualified `vec!`, another nested macro, or another assertion.
        "assert_eq!(std::vec![crate::weight(4)][0], 12);",
        "assert_eq!(vec![dbg!(crate::weight(4))][0], 12);",
        "assert_eq!(vec![format!(\"{}\", crate::weight(4))][0], \"12\");",
        "assert_ne!(vec![crate::weight(4)][0], 13);",
        "assert!(vec![crate::weight(4)][0] == 12);",
    ] {
        let lib = unit_tests("", body);
        assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{body}");
    }
    // A non-scalar owner keeps only the bare call.
    let body = "assert_eq!(vec![crate::weight(4)][0], 12);";
    let lib = non_scalar_unit_tests("", body);
    assert!(path_admitted(&lib, "x * 3", None).is_empty(), "{lib}");
}

#[test]
fn a_direct_call_name_is_one_bare_call_ending_the_subject() {
    for (subject, name) in [
        ("weight(4)", Some("weight")),
        (" weight ( 4 , [1, 2] ) ", Some("weight")),
        ("_w(f(1))", Some("_w")),
        ("weight(4).min(5)", None),
        ("weight(4) + 1", None),
        ("other::weight(4)", None),
        ("x.weight(4)", None),
        ("(weight(4))", None),
        ("weight(4)(5)", None),
        ("4weight(1)", None),
        ("weight", None),
        ("weight(4", None),
    ] {
        assert_eq!(direct_call_name(subject), name, "{subject}");
    }
}

#[test]
fn a_macro_body_that_binds_a_trusted_name_makes_it_ambiguous() {
    // #5991: a body is a token tree, so the `macro_rules!` and `use` items it
    // emits never parse as items. Each binding position in it counts.
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    let refusal = |other: &str| weight_refusal(tests, &[("src/other.rs", other)]);
    let refused = |other: &str| {
        matches!(
            refusal(other),
            Some(
                AssertionRefusal::MacroBinding { .. }
                    | AssertionRefusal::Syntax(AssertionContextRefusal::MacroBinding(_))
            )
        )
    };
    for other in [
        // A local trusted-named macro defining a name its caller passes.
        "macro_rules! vec { ($n:ident) => { macro_rules! $n { ($a:expr, $b:expr) => {}; } }; }\nvec!(assert_eq);\n",
        // The issue's own shape, and the same body under a trusted name.
        "macro_rules! m { () => { macro_rules! assert_eq { ($a:expr, $b:expr) => {{ let _ = (&$a, &$b); }}; } #[cfg(test)] mod tests; }; }\nm!();\n",
        "macro_rules! vec { () => { #[macro_export] macro_rules! assert_eq { ($a:expr, $b:expr) => {{ let _ = ($a, $b); }}; } }; }\nvec!();\n",
        // Nested one level deeper, as a macro 2.0 item, and by import.
        "macro_rules! m { () => { macro_rules! inner { () => { macro_rules! assert_eq { ($($t:tt)*) => {}; } }; } inner!(); }; }\nm!();\n",
        "macro_rules! m { () => { macro assert_eq($($t:tt)*) {} }; }\nm!();\n",
        "macro_rules! m { () => { use other::check as assert_eq; }; }\nm!();\n",
        "macro_rules! m { () => { use other::assert_eq; }; }\nm!();\n",
        "macro_rules! m { () => { use other::{assert_eq, Thing}; }; }\nm!();\n",
        "macro_rules! m { () => { use ::other::assert_eq; }; }\nm!();\n",
        // Words that may bring in any name.
        "macro_rules! m { () => { #[macro_use] extern crate other; }; }\nm!();\n",
        "macro_rules! m { () => { #![no_implicit_prelude] }; }\nm!();\n",
        "macro_rules! m { () => { use other::*; }; }\nm!();\n",
        "macro_rules! m { ($k:ident) => { use $k::*; }; }\nm!(other);\n",
        // A trusted-named macro whose body imports its caller's name, as a
        // whole tree.
        "macro_rules! vec { ($n:ident) => { use $n; }; }\nvec!(assert_eq);\n",
    ] {
        assert!(refused(other), "{other}");
    }
    // A matcher matches the invocation's tokens and emits nothing, so the
    // `use` in it binds nothing; a rename target or a path segment before
    // `::` is what the fragment binds instead.
    for other in [
        "macro_rules! m { (use other::assert_eq;) => {}; }\nm!();\n",
        "macro_rules! m { (use other::*) => {}; }\nm!();\n",
    ] {
        assert_eq!(refusal(other), None, "{other}");
    }
    // #5941, intentional over-refusal: a trusted-named macro with any binding
    // site is opaque, so `vec!(assert_eq)` counts as a mention of `assert_eq`
    // even though these bodies only rename or path-extend their argument. The
    // earlier "binds nothing" expectation for them is dropped on purpose.
    for other in [
        "macro_rules! vec { ($n:ident) => { use $n as check; }; }\nvec!(assert_eq);\n",
        "macro_rules! vec { ($n:ident) => { use $n::path; }; }\nvec!(assert_eq);\n",
    ] {
        assert!(refused(other), "{other}");
    }
    // A test-generating macro whose body only calls `assert_eq!`, renames it
    // away, or glob-imports this crate binds nothing.
    for other in [
        "macro_rules! m { ($n:ident) => { mod $n { use super::*; #[test] fn t() { assert_eq!(1, 1); } } }; }\nm!(one);\n",
        "macro_rules! m { () => { use other::assert_eq as _; use $crate::prelude::*; use self::x::*; }; }\nm!();\n",
        "macro_rules! m { () => { use other::assert_eq as check; }; }\nm!();\n",
    ] {
        assert_eq!(refusal(other), None, "{other}");
    }
}

#[test]
fn a_private_macro_bodys_any_binding_stays_in_its_own_target() {
    let tests = "use demo::weight;\n#[test]\nfn weighs() { assert_eq!(weight(4), 12); }\n";
    // A private macro's `macro_use`, `no_implicit_prelude` or foreign-glob
    // body binds only where the macro can expand, like a direct site in the
    // same file would; a bench cannot expand it in the test's crate.
    for other in [
        (
            "benches/b.rs",
            "macro_rules! m { () => { use other::*; }; }\nm!();\n",
        ),
        (
            "benches/b.rs",
            "macro_rules! m { () => { #[macro_use] extern crate other; }; }\nm!();\n",
        ),
        (
            "benches/b.rs",
            "macro_rules! m { () => { #![no_implicit_prelude] }; }\nm!();\n",
        ),
    ] {
        assert_eq!(weight_refusal(tests, &[other]), None, "{other:?}");
    }
    // Its exported twin reaches every test, and the test file's own private
    // macro keeps its binding.
    let exported = (
        "benches/b.rs",
        "#[macro_export]\nmacro_rules! m { () => { use other::*; }; }\nm!();\n",
    );
    assert!(matches!(
        weight_refusal(tests, &[exported]),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
    let own = format!("macro_rules! m {{ () => {{ use other::*; }}; }}\nm!();\n{tests}");
    assert!(matches!(
        weight_refusal(&own, &[]),
        Some(AssertionRefusal::Syntax(
            AssertionContextRefusal::MacroBinding(_)
        ))
    ));
}

#[test]
fn a_macro_body_site_names_the_macro_and_its_reach() {
    let trusted = &["assert", "assert_eq", "vec", "format", "dbg", "write"];
    let sites = |source: &str| {
        sites_in_widest_context(source, trusted)
            .into_iter()
            .map(|(name, site)| (name, site.kind, site.crate_local))
            .collect::<Vec<_>>()
    };
    let body = |name: &str| MacroBindingKind::MacroBody(name.to_string());
    // Only binding positions count: the `assert_eq!` call does not.
    assert_eq!(
        sites(
            "macro_rules! m { () => { macro_rules! assert { () => {} } use a::{b, vec}; use c::d as format; assert_eq!(1, 1); }; }\n"
        ),
        vec![
            ("assert".to_string(), body("m"), true),
            ("vec".to_string(), body("m"), true),
            ("format".to_string(), body("m"), true),
        ]
    );
    // An exported macro, or an export in its body, reaches other crates.
    assert_eq!(
        sites("#[macro_export]\nmacro_rules! e { () => { macro_rules! dbg { () => {} } }; }\n"),
        vec![("dbg".to_string(), body("e"), false)]
    );
    assert_eq!(
        sites("macro_rules! f { () => { #[macro_export] macro_rules! write { () => {} } }; }\n"),
        vec![("write".to_string(), body("f"), false)]
    );
    // A glob from a fragment root may import anything, and a private
    // macro's reach stops at its own crate, like a direct glob's would; the
    // exported twin reaches other crates.
    let any = sites("macro_rules! h { ($k:ident) => { use $k::*; }; }\n");
    assert_eq!(any.len(), trusted.len());
    assert!(any.iter().all(|(_, kind, crate_local)| {
        *kind
            == MacroBindingKind::BodyBindsAny {
                macro_name: "h".to_string(),
                binding: "use $k::*".to_string(),
            }
            && *crate_local
    }));
    assert!(
        sites("#[macro_export]\nmacro_rules! h { ($k:ident) => { use $k::*; }; }\n")
            .iter()
            .all(|(_, _, crate_local)| !*crate_local)
    );
    // A matcher matches the invocation and emits nothing.
    assert_eq!(
        sites("macro_rules! m { (use other::*) => {}; (use other::assert_eq;) => {}; }\n"),
        Vec::new()
    );
    // A local macro under a trusted name is never read at its calls, so a
    // name it takes from its caller may be any name; an untrusted one's
    // calls are read instead.
    let any = sites("macro_rules! vec { ($n:ident) => { macro_rules! $n { () => {} } }; }\n");
    assert_eq!(any.len(), trusted.len());
    assert!(any.iter().all(|(_, kind, _)| {
        *kind
            == MacroBindingKind::BodyBindsAny {
                macro_name: "vec".to_string(),
                binding: "macro_rules! $n".to_string(),
            }
    }));
    for source in [
        "macro_rules! vec { ($n:ident) => { macro $n() {} }; }\n",
        "macro_rules! vec { ($n:ident) => { use other::$n; }; }\n",
        "macro_rules! vec { ($n:ident) => { use $n; }; }\n",
    ] {
        assert_eq!(sites(source).len(), trusted.len(), "{source}");
    }
    assert_eq!(
        sites("macro_rules! define { ($n:ident) => { macro_rules! $n { () => {} } }; }\n"),
        Vec::new()
    );
    // This crate's globs bind nothing.
    assert_eq!(
        sites("macro_rules! g { () => { use crate::x::*; use $crate::y::*; use super::*; }; }\n"),
        Vec::new()
    );
}

/// #8388: a glob from the standard crates (`use core::simd::prelude::*`)
/// names only items the standard library defines, so a sibling file's glob
/// no longer withholds every plain `assert_eq!` in the crate. The shape is
/// rand's `utils.rs`: a cfg-gated std glob and an unrelated `macro_rules!`
/// beside a test module whose `assert_eq!` calls carry cfg attributes. Each
/// control keeps a real rebinding, or a glob that may carry one, ambiguous.
#[test]
fn a_standard_crate_glob_does_not_rebind_the_standard_assertion_macros() -> Result<(), String> {
    let manifest = |dependencies: &str| {
        format!("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dependencies}")
    };
    let plain = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-std-glob-plain",
        &[
            ("Cargo.toml", &manifest("")),
            ("src/simd.rs", ""),
            ("src/root.rs", ""),
            ("src/lib.rs", ""),
            ("tests/buf_tests.rs", ""),
        ],
    )?;
    let renamed = crate::analysis::facts::drop_in::temp_workspace(
        "owner-pin-std-glob-renamed",
        &[(
            "Cargo.toml",
            &manifest("core = { package = \"fake-core\", version = \"1\" }\n"),
        )],
    )?;
    let tests = "use demo::weight;\n\n#[cfg(test)]\nmod test {\n    use crate::weight;\n    #[cfg(feature = \"simd_support\")]\n    use std::simd::{Mask, Simd};\n\n    #[test]\n    fn weighs() {\n        assert_eq!(weight(4), 12);\n\n        #[cfg(feature = \"simd_support\")]\n        assert_eq!(weight(5), 15);\n    }\n}\n";
    let refusal_under = |root: &Path, sibling: &str| {
        let mut index = index(&[(LIB, WEIGHT_LIB), ("src/simd.rs", sibling), (TESTS, tests)]);
        index.drop_in_manifests = crate::analysis::facts::drop_in::DropInManifests::new(root);
        let test = index
            .tests()
            .iter()
            .find(|test| test.file == Path::new(TESTS))?;
        let assertion = test
            .assertions
            .iter()
            .find(|assertion| assertion.text.starts_with("assert_eq!(weight(4)"))?;
        OwnerPinSyntax::default().refusal(test, assertion, &index)
    };
    let refusal_beside = |sibling: &str| refusal_under(&plain, sibling);
    let rebound = |refusal: Option<AssertionRefusal>| {
        matches!(
            refusal,
            Some(AssertionRefusal::Syntax(
                AssertionContextRefusal::MacroBinding(_)
            ))
        )
    };
    // Fixture premise: a rebinding sibling does refuse this exact test.
    let shadowing = "macro_rules! assert_eq { ($($tokens:tt)*) => {} }\n";
    assert!(rebound(refusal_beside(shadowing)));
    // Credited: standard-crate globs, gated or not, beside unrelated macros.
    for sibling in [
        "#[cfg(feature = \"simd_support\")]\nuse core::simd::prelude::*;\nmacro_rules! wmul_impl { () => {}; }\n",
        "use std::io::prelude::*;\n",
        "use ::core::arch::x86::*;\n",
        "use core::{simd::*, mem};\n",
        "mod inner {\n    use std::collections::*;\n}\n",
    ] {
        assert_eq!(refusal_beside(sibling), None, "{sibling}");
    }
    // Still not credited: a real definition or import of the name, even
    // beside a standard glob.
    for sibling in [
        "use core::simd::prelude::*;\nmacro_rules! assert_eq { ($($tokens:tt)*) => {} }\n",
        "use std::io::prelude::*;\nuse other::assert_eq;\n",
    ] {
        assert!(rebound(refusal_beside(sibling)), "{sibling}");
    }
    // Still not credited: any glob that is not rooted at the standard
    // crates, and a standard root a local module or alias takes. (A
    // workspace package's own glob is scanned as workspace-owned code.)
    for sibling in [
        "use rayon::prelude::*;\n",
        "use stdx::prelude::*;\n",
        "use ::rayon::prelude::*;\n",
        "mod core {}\nuse core::simd::prelude::*;\n",
        "extern crate other as std;\nuse std::io::prelude::*;\n",
        "use other as core;\nuse core::simd::prelude::*;\n",
        // `alloc` has no manifest proof, so it stays a foreign glob.
        "use alloc::vec::*;\n",
    ] {
        assert!(rebound(refusal_beside(sibling)), "{sibling}");
    }
    // A Cargo rename of the root, and a crate-root alias in a file the glob
    // is not in, both rebind it.
    assert!(rebound(refusal_under(
        &renamed,
        "use core::simd::prelude::*;\n"
    )));
    let aliased_elsewhere = index(&[
        (LIB, &format!("extern crate other as std;\n{WEIGHT_LIB}")),
        ("src/simd.rs", "use std::io::prelude::*;\n"),
        (TESTS, tests),
    ]);
    let mut aliased_elsewhere = aliased_elsewhere;
    aliased_elsewhere.drop_in_manifests =
        crate::analysis::facts::drop_in::DropInManifests::new(&plain);
    let test = aliased_elsewhere
        .tests()
        .iter()
        .find(|test| test.file == Path::new(TESTS))
        .ok_or("the fixture test must be indexed")?;
    let assertion = test
        .assertions
        .iter()
        .find(|assertion| assertion.text.starts_with("assert_eq!(weight(4)"))
        .ok_or("the fixture assertion must be indexed")?;
    let refusal = OwnerPinSyntax::default().refusal(test, assertion, &aliased_elsewhere);
    assert!(rebound(refusal.clone()), "{refusal:?}");
    Ok(())
}

#[test]
fn scalar_names_rebound_sees_type_struct_and_import_rebinding() {
    assert_eq!(declared_type(": Foo<{ A == B }> = 1"), None);
    assert_eq!(declared_type(": u32 = 1"), Some(" u32 "));
    assert!(!scalar_names_rebound("u32", "static X: u32 = 1;"));
    for source in [
        "fn f(x: u64) -> u32 { x as u32 }",
        "fn f(n: u64) { let y = n as u32; }",
        "const M: u32 = std::u32::MAX;",
        "fn f<T>() { <u32 as Tr>::g(); }",
        "use std::u32::MAX;",
        "use m::Counter;\nfn f(n: u64) -> u32 { n as u32 }",
    ] {
        assert!(!scalar_names_rebound("u32", source), "{source}");
    }
    for source in [
        "use x as u32;",
        "use a::{b as u32};",
        "pub use x as u32;",
        "use m::{Counter, u32};",
        "use m::{u32};",
        "use m::*;",
        "extern crate x as u32;",
    ] {
        assert!(scalar_names_rebound("u32", source), "{source}");
    }
    assert!(!scalar_names_rebound(
        "&'static str",
        "const S: &str = \"type str\";"
    ));
    for source in [
        "type u32 = Counter;",
        "struct u32;",
        "use other::u32;",
        "use other::Counter as u32;",
    ] {
        assert!(scalar_names_rebound("u32", source), "{source}");
    }
}
