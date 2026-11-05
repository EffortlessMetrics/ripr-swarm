use super::*;
use crate::analysis::facts::WorkspaceRootAuthority;
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

struct ParsedIndex {
    index: RustIndex,
    root: PathBuf,
}

impl std::ops::Deref for ParsedIndex {
    type Target = RustIndex;

    fn deref(&self) -> &Self::Target {
        &self.index
    }
}

impl Drop for ParsedIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn parsed_index(source: &str) -> Result<ParsedIndex, String> {
    let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), source)?;
    let mut index = RustIndex::default();
    index.extend_tests(facts.tests.iter().cloned());
    index.extend_functions(facts.functions.iter().cloned());
    index.insert_file_only(PathBuf::from("src/lib.rs"), facts);
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let mut claimed = None;
    for _ in 0..32 {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-12234-assertion-owner-{}-{stamp}-{id}",
            std::process::id()
        ));
        match std::fs::create_dir(&root) {
            Ok(()) => {
                claimed = Some(root);
                break;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    let root = claimed.ok_or_else(|| "could not claim owner fixture root".to_string())?;
    let mut fixture = ParsedIndex { index, root };
    std::fs::write(
        fixture.root.join("Cargo.toml"),
        "[package]\nname = \"assertion-owner-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::create_dir(fixture.root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(fixture.root.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    fixture.index.workspace_authority = Some(WorkspaceRootAuthority::from_sources(
        &fixture.root,
        fixture
            .index
            .files()
            .iter()
            .map(|(path, facts)| (path, facts.data().source.as_ref())),
    ));
    crate::analysis::facts::compose_fixture_source_roles(&mut fixture.index, &fixture.root);
    if super::super::owner_pin::target_root(
        Path::new("src/lib.rs"),
        &fixture.index,
        &Default::default(),
    ) != Some(PathBuf::from("src/lib.rs"))
    {
        return Err("owner fixture did not establish its actual crate root".to_string());
    }
    Ok(fixture)
}

fn pins_other_in_test(
    index: &RustIndex,
    owner: &FunctionSummary,
    assertion: &str,
) -> Result<bool, String> {
    let source = &index
        .files()
        .get(&owner.file)
        .ok_or_else(|| "missing owner source".to_string())?
        .source;
    let source = format!(
        "{source}\n#[cfg(test)] mod contract_tests {{\n\
         use super::*;\n#[test] fn assertion_contract() {{\n\
         let widget = Widget;\n{assertion};\n}}\n}}",
    );
    let index = parsed_index(&source)?;
    let test = index
        .tests()
        .iter()
        .find(|test| test.name == "assertion_contract")
        .ok_or_else(|| "missing parsed contract test".to_string())?;
    let [oracle] = test.assertions.as_slice() else {
        return Err(format!(
            "expected one parsed assertion, got {}",
            test.assertions.len()
        ));
    };
    Ok(TransitiveReachIndex::new(&index)
        .owner_reach(&owner.name)
        .assertion_pins_other_function(owner, test, oracle, &oracle.text))
}

fn source(free_body: &str, method_body: &str, extra: &str) -> String {
    format!(
        "pub fn consume(value: u32) -> u32 {{ {free_body} }}\n\
         pub struct Widget;\n\
         impl Widget {{ pub fn consume(&self, value: u32) -> u32 {{ {method_body} }} }}\n\
         pub fn bridge(value: u32) -> u32 {{ consume(value) }}\n\
         {extra}\n"
    )
}

fn owner(index: &RustIndex, free: bool) -> Result<&FunctionSummary, String> {
    let owners: Vec<_> = index
        .functions()
        .iter()
        .filter(|function| {
            function.name == "consume"
                && (function.item.container == FunctionContainer::Free) == free
        })
        .collect();
    match owners.as_slice() {
        [owner] => Ok(owner),
        _ => Err(format!(
            "expected one selected consume owner, got {}",
            owners.len()
        )),
    }
}

#[test]
fn assertion_call_kind_separates_same_named_free_and_receiver_owners() -> Result<(), String> {
    let index = parsed_index(&source("value + 1", "value + 2", ""))?;
    for (free, assertion, other) in [
        (true, "assert_eq!(widget.consume(1), 3)", true),
        (false, "assert_eq!(consume(1), 2)", true),
        (true, "assert_eq!(consume(1), 2)", false),
        (true, "assert_eq!(0..consume(1), 0..2)", false),
        (true, "assert_eq!(0 .. /* start */ consume(1), 0..2)", false),
        (false, "assert_eq!(widget.consume(1), 3)", false),
        (true, "assert_eq!(crate::consume(1), 2)", false),
        (false, "assert_eq!(Widget::consume(&widget, 1), 3)", false),
        (false, "assert_eq!(<Widget>::consume(&widget, 1), 3)", false),
        (true, "assert_eq!(widget.consume(1), consume(1))", false),
        (false, "assert_eq!(widget.consume(1), consume(1))", false),
        (true, "assert_eq!(widget.consume::<u32>(1), 3)", true),
        (true, "assert_eq!(widget.consume(1), 3); // consume(1)", true),
    ] {
        assert_eq!(
            pins_other_in_test(&index, owner(&index, free)?, assertion)?,
            other,
            "free owner {free}: {assertion}"
        );
    }
    Ok(())
}

#[test]
fn opposite_kind_enum_constructor_admission_preserves_declared_context() -> Result<(), String> {
    for (attributes, extra, generic, unresolved) in [
        ("#[derive(Debug, PartialEq)]", "", false, false),
        ("", "", false, false),
        ("#[derive(Debug, PartialEq)]", "use foreign::Debug;", false, true),
        ("#[derive(Debug, PartialEq)]", "use foreign::*;", false, true),
        ("#[rewrite]", "", false, true),
        ("#[cfg(feature = \"optional\")]", "", false, true),
        ("#[derive(Debug, PartialEq)]", "declare!();", false, true),
        ("#[derive(Debug, PartialEq)]", "", true, true),
    ] {
        let (widget, implementation) = if generic {
            (
                "pub struct Widget<T>(std::marker::PhantomData<T>);",
                "impl<T> Widget<T>",
            )
        } else {
            ("pub struct Widget;", "impl Widget")
        };
        let source = format!(
            "{attributes}\n\
             pub enum Reduction {{ Over(u32), Empty }}\n\
             {extra}\n\
             pub fn consume(value: u32) -> Reduction {{ Reduction::Over(value) }}\n\
             {widget}\n\
             {implementation} {{\n\
                 pub fn consume(&self, value: u32) -> Reduction {{ Reduction::Over(value) }}\n\
             }}\n",
        );
        let index = parsed_index(&source)?;
        let reach_index = TransitiveReachIndex::new(&index);
        let reach = reach_index.owner_reach("consume");
        assert_eq!(
            reach.opposite_kind_function_may_run_owner(owner(&index, false)?),
            unresolved,
            "{attributes} / {extra} / generic {generic}"
        );
    }
    Ok(())
}

#[test]
fn same_named_opposite_kind_wrappers_keep_owner_uncertainty() -> Result<(), String> {
    for (free_body, method_body, free, assertion) in [
        (
            "value + 1",
            "consume(value)",
            true,
            "assert_eq!(widget.consume(1), 2)",
        ),
        (
            "value + 1",
            "bridge(value)",
            true,
            "assert_eq!(widget.consume(1), 2)",
        ),
        (
            "value + 1",
            "let run = || consume(value); run()",
            true,
            "assert_eq!(widget.consume(1), 2)",
        ),
        (
            "value + 1",
            "let run = consume; run(value)",
            true,
            "assert_eq!(widget.consume(1), 2)",
        ),
        (
            "value + 1",
            "let run = bridge; let forward = || run(value); forward()",
            true,
            "assert_eq!(widget.consume(1), 2)",
        ),
        (
            "value + 1",
            "opaque!(value)",
            true,
            "assert_eq!(widget.consume(1), 2)",
        ),
        (
            "Widget.consume(value)",
            "value + 2",
            false,
            "assert_eq!(consume(1), 3)",
        ),
    ] {
        let index = parsed_index(&source(free_body, method_body, ""))?;
        assert!(
            !pins_other_in_test(&index, owner(&index, free)?, assertion)?,
            "wrapper must retain uncertainty: {free_body} / {method_body}"
        );
    }
    Ok(())
}

#[test]
fn opposite_kind_indirect_function_values_keep_owner_uncertainty() -> Result<(), String> {
    for helper in [
        "pub fn forward(value: u32) -> u32 { let run = consume; run(value) }",
        "pub fn forward(value: u32) -> u32 { let run = callback; run(value) }\n\
         fn callback(value: u32) -> u32 { let run = consume; run(value) }",
        "pub fn forward(value: u32) -> u32 { opaque!(value) }",
        "pub fn forward(value: u32) -> u32 { external(value) }",
    ] {
        let index = parsed_index(&source("value + 1", "forward(value)", helper))?;
        assert!(
            !pins_other_in_test(
                &index,
                owner(&index, true)?,
                "assert_eq!(widget.consume(1), 2)",
            )?,
            "indirect wrapper must retain uncertainty: {helper}"
        );
    }
    Ok(())
}

#[test]
fn opposite_kind_unrelated_wrappers_keep_call_uncertainty() -> Result<(), String> {
    for helper in [
        "pub fn forward(value: u32) -> u32 { value + 2 }",
        "pub fn forward(value: u32) -> u32 { unrelated(value) }\n\
         fn unrelated(value: u32) -> u32 { value + 2 }",
        "pub fn forward(value: u32) -> u32 { nested(value) }\n\
         fn nested(value: u32) -> u32 { forward(value) }",
    ] {
        let index = parsed_index(&source("value + 1", "forward(value)", helper))?;
        assert!(
            !pins_other_in_test(
                &index,
                owner(&index, true)?,
                "assert_eq!(widget.consume(1), 3)",
            )?,
            "outgoing helper calls keep their binding uncertainty: {helper}"
        );
    }
    Ok(())
}

#[test]
fn opposite_kind_unicode_function_values_keep_owner_uncertainty() -> Result<(), String> {
    let index = parsed_index(
        "pub fn consumé(value: u32) -> u32 { value + 1 }\n\
         type Operation = fn(u32) -> u32;\n\
         pub fn forward() -> Operation { consumé }\n\
         pub struct Widget;\n\
         impl Widget { pub fn consumé(&self) -> Operation { forward() } }",
    )?;
    let owner = index
        .functions()
        .iter()
        .find(|function| {
            function.name == "consumé" && function.item.container == FunctionContainer::Free
        })
        .ok_or_else(|| "missing Unicode free owner".to_string())?;
    assert!(!pins_other_in_test(&index, owner, "assert_eq!(widget.consumé()(1), 2)")?);
    Ok(())
}

#[test]
fn opposite_kind_returned_function_items_keep_owner_uncertainty() -> Result<(), String> {
    // Retain the exact stimulus that exposed the earlier candidate: the
    // alias prevents a phantom `fn(...)` signature call from hiding the
    // missed returned-function item behind unrelated call uncertainty.
    for (owner_generic, helper, returned) in [
        ("", "", "consume"),
        ("<T>", "", "consume::<u32>"),
        ("<T>", "", "consume:: <u32>"),
        ("", "", "\u{200e}consume\u{200e}"),
        ("", "", "\u{2028}consume\u{2028}"),
        (
            "",
            "fn r#bridge(value: u32) -> u32 { consume(value) }",
            "r#bridge",
        ),
        (
            "",
            "fn r#bridge(value: u32) -> u32 { consume(value) }",
            "bridge",
        ),
        (
            "",
            "fn bridge(value: u32) -> u32 { consume(value) }",
            "r#bridge",
        ),
    ] {
        let source = format!(
            "pub fn consume{owner_generic}(value: u32) -> u32 {{ value + 1 }}\n\
             type Operation = fn(u32) -> u32;\n\
             {helper}\n\
             fn forward() -> Operation {{ {returned} }}\n\
             pub struct Widget;\n\
             impl Widget {{ pub fn consume(&self, value: u32) -> u32 {{ forward()(value) }} }}",
        );
        let index = parsed_index(&source)?;
        let forward = index
            .functions()
            .iter()
            .find(|function| function.name == "forward")
            .ok_or_else(|| "missing returned-function helper".to_string())?;
        assert!(
            calls_of(forward).next().is_none(),
            "the regression stimulus must not acquire a phantom signature call: {returned:?}"
        );
        assert!(
            !pins_other_in_test(
                &index,
                owner(&index, true)?,
                "assert_eq!(widget.consume(1), 2)",
            )?,
            "returned owner path must retain uncertainty: {returned:?} / {helper}"
        );
    }
    Ok(())
}

#[test]
fn opposite_kind_callback_values_keep_owner_uncertainty() -> Result<(), String> {
    let index = parsed_index(
        "pub fn consume(value: u32) -> u32 { value + 1 }\n\
         pub fn bridge(value: u32) -> u32 { consume(value) }\n\
         pub struct Widget;\n\
         impl Widget {\n\
             pub fn consume(&self, run: fn(u32) -> u32) -> u32 { run(1) }\n\
         }",
    )?;
    for assertion in [
        "assert_eq!(widget.consume(consume), 2)",
        "assert_eq!(widget.consume(bridge), 2)",
    ] {
        assert!(
            !pins_other_in_test(&index, owner(&index, true)?, assertion)?,
            "function value may reach owner: {assertion}"
        );
    }
    Ok(())
}

#[test]
fn assertion_call_kind_keeps_unresolved_and_implicit_inputs_uncertain() -> Result<(), String> {
    let unindexed = parsed_index("pub fn consume(value: u32) -> u32 { value + 1 }")?;
    assert!(
        !pins_other_in_test(
            &unindexed,
            owner(&unindexed, true)?,
            "assert_eq!(external.consume(1), 2)",
        )?
    );

    let ambiguous = parsed_index(&source(
        "value + 1",
        "value + 2",
        "struct Other; impl Other { fn consume(&self, value: u32) -> u32 { value + 3 } }",
    ))?;
    assert!(
        !pins_other_in_test(
            &ambiguous,
            owner(&ambiguous, true)?,
            "assert_eq!(receiver.consume(1), 3)",
        )?
    );

    let known = parsed_index(&source("value + 1", "value + 2", ""))?;
    let mut unknown = owner(&known, true)?.clone();
    unknown.item.container = FunctionContainer::Unknown;
    assert!(
        !pins_other_in_test(
            &known,
            &unknown,
            "assert_eq!(widget.consume(1), 3)",
        )?
    );

    let implicit = parsed_index(&source(
        "value + 1",
        "value + 2",
        "impl std::fmt::Display for Widget {\n\
             fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n\
                 let value = consume(1);\n\
                 write!(f, \"{}\", value)\n\
             }\n\
         }",
    ))?;
    assert!(
        !pins_other_in_test(
            &implicit,
            owner(&implicit, true)?,
            "assert_eq!(widget.consume(1), 3)",
        )?
    );
    Ok(())
}

#[test]
fn assertion_target_identity_is_local_to_each_oracle() -> Result<(), String> {
    for (setup, assertions, expected) in [
        (
            "let _ = next();",
            "assert_eq!(widget.next(), Reduction::Over(15));",
            vec![true],
        ),
        (
            "let _ = widget.next(); let mut external = std::iter::once_with(next);",
            "assert_eq!(external.next(), Some(Reduction::Over(16)));",
            vec![false],
        ),
        (
            "let mut external = std::iter::once_with(next);",
            "assert_eq!(widget.next(), Reduction::Over(15));\n\
             assert_eq!(external.next(), Some(Reduction::Over(16)));",
            vec![true, false],
        ),
        (
            "let mut external = std::iter::once_with(next);",
            "assert_eq!((widget.next(), external.next()),\n\
             (Reduction::Over(15), Some(Reduction::Over(16))));",
            vec![false],
        ),
    ] {
        let source = format!(
            "#[derive(Debug, PartialEq)]\n\
             pub enum Reduction {{ Over(u32), Empty }}\n\
             pub fn next() -> Reduction {{ Reduction::Over(16) }}\n\
             pub struct Widget;\n\
             impl Widget {{ pub fn next(&mut self) -> Reduction {{ Reduction::Over(15) }} }}\n\
             #[cfg(test)] mod tests {{\nuse super::*;\n\
             #[test] fn contract() {{\nlet mut widget = Widget;\n\
             {setup}\n{assertions}\n}}\n}}",
        );
        let index = parsed_index(&source)?;
        let owner = index
            .functions()
            .iter()
            .find(|function| {
                function.name == "next" && function.item.container == FunctionContainer::Free
            })
            .ok_or_else(|| "missing free next owner".to_string())?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "contract")
            .ok_or_else(|| "missing parsed contract".to_string())?;
        assert_eq!(
            test.assertions.len(),
            expected.len(),
            "nonempty exact oracle stimulus"
        );
        let reach_index = TransitiveReachIndex::new(&index);
        let reach = reach_index.owner_reach(&owner.name);
        for (assertion, expected) in test.assertions.iter().zip(expected) {
            assert_eq!(
                reach.assertion_pins_other_function(owner, test, assertion, &assertion.text),
                expected,
                "{setup} / {}",
                assertion.text
            );
        }
    }
    Ok(())
}
