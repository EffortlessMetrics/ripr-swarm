use super::*;
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::path::Path;

fn parsed_index(source: &str) -> Result<RustIndex, String> {
    let facts = RaRustSyntaxAdapter.summarize_file(Path::new("src/lib.rs"), source)?;
    let mut index = RustIndex::default();
    index.extend_tests(facts.tests.iter().cloned());
    index.extend_functions(facts.functions.iter().cloned());
    index.insert_file_only(PathBuf::from("src/lib.rs"), facts);
    Ok(index)
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
    let reach_index = TransitiveReachIndex::new(&index);
    let reach = reach_index.owner_reach("consume");
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
            reach.assertion_pins_other_function(owner(&index, free)?, assertion),
            other,
            "free owner {free}: {assertion}"
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
        let reach_index = TransitiveReachIndex::new(&index);
        let reach = reach_index.owner_reach("consume");
        assert!(
            !reach.assertion_pins_other_function(owner(&index, free)?, assertion),
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
        let reach_index = TransitiveReachIndex::new(&index);
        assert!(
            !reach_index.owner_reach("consume").assertion_pins_other_function(
                owner(&index, true)?,
                "assert_eq!(widget.consume(1), 2)"
            ),
            "indirect wrapper must retain uncertainty: {helper}"
        );
    }
    Ok(())
}

#[test]
fn opposite_kind_wrapper_walk_keeps_unrelated_chains_and_cycles_separate() -> Result<(), String> {
    for helper in [
        "pub fn forward(value: u32) -> u32 { value + 2 }",
        "pub fn forward(value: u32) -> u32 { unrelated(value) }\n\
         fn unrelated(value: u32) -> u32 { value + 2 }",
        "pub fn forward(value: u32) -> u32 { nested(value) }\n\
         fn nested(value: u32) -> u32 { forward(value) }",
    ] {
        let index = parsed_index(&source("value + 1", "forward(value)", helper))?;
        let reach_index = TransitiveReachIndex::new(&index);
        assert!(
            reach_index.owner_reach("consume").assertion_pins_other_function(
                owner(&index, true)?,
                "assert_eq!(widget.consume(1), 3)"
            ),
            "indexed unrelated wrapper must not pin the owner: {helper}"
        );
    }
    Ok(())
}

#[test]
fn opposite_kind_wrapper_budget_retains_uncertainty() -> Result<(), String> {
    let mut helpers = String::new();
    for position in 0..65 {
        helpers.push_str(&format!(
            "pub fn link{position}(value: u32) -> u32 {{ link{}(value) }}\n",
            position + 1
        ));
    }
    helpers.push_str("pub fn link65(value: u32) -> u32 { value + 2 }\n");
    let index = parsed_index(&source("value + 1", "link0(value)", &helpers))?;
    let reach_index = TransitiveReachIndex::new(&index);
    assert!(
        !reach_index.owner_reach("consume").assertion_pins_other_function(
            owner(&index, true)?,
            "assert_eq!(widget.consume(1), 3)"
        ),
        "a bounded walk cannot claim the unexplored functions are unrelated"
    );
    Ok(())
}

#[test]
fn opposite_kind_unicode_function_values_keep_owner_uncertainty() -> Result<(), String> {
    let index = parsed_index(
        "pub fn consumé(value: u32) -> u32 { value + 1 }\n\
         pub fn forward() -> fn(u32) -> u32 { consumé }\n\
         pub struct Widget;\n\
         impl Widget { pub fn consumé(&self) -> fn(u32) -> u32 { forward() } }",
    )?;
    let owner = index
        .functions()
        .iter()
        .find(|function| {
            function.name == "consumé" && function.item.container == FunctionContainer::Free
        })
        .ok_or_else(|| "missing Unicode free owner".to_string())?;
    let reach_index = TransitiveReachIndex::new(&index);
    assert!(
        !reach_index.owner_reach("consumé").assertion_pins_other_function(
            owner,
            "assert_eq!(widget.consumé()(1), 2)"
        )
    );
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
    let reach_index = TransitiveReachIndex::new(&index);
    let reach = reach_index.owner_reach("consume");
    for assertion in [
        "assert_eq!(widget.consume(consume), 2)",
        "assert_eq!(widget.consume(bridge), 2)",
    ] {
        assert!(
            !reach.assertion_pins_other_function(owner(&index, true)?, assertion),
            "function value may reach owner: {assertion}"
        );
    }
    Ok(())
}

#[test]
fn assertion_call_kind_keeps_unresolved_and_implicit_inputs_uncertain() -> Result<(), String> {
    let unindexed = parsed_index("pub fn consume(value: u32) -> u32 { value + 1 }")?;
    let reach_index = TransitiveReachIndex::new(&unindexed);
    assert!(
        !reach_index.owner_reach("consume").assertion_pins_other_function(
            owner(&unindexed, true)?,
            "assert_eq!(external.consume(1), 2)"
        )
    );

    let ambiguous = parsed_index(&source(
        "value + 1",
        "value + 2",
        "struct Other; impl Other { fn consume(&self, value: u32) -> u32 { value + 3 } }",
    ))?;
    let reach_index = TransitiveReachIndex::new(&ambiguous);
    assert!(
        !reach_index.owner_reach("consume").assertion_pins_other_function(
            owner(&ambiguous, true)?,
            "assert_eq!(receiver.consume(1), 3)"
        )
    );

    let known = parsed_index(&source("value + 1", "value + 2", ""))?;
    let mut unknown = owner(&known, true)?.clone();
    unknown.item.container = FunctionContainer::Unknown;
    let reach_index = TransitiveReachIndex::new(&known);
    assert!(
        !reach_index.owner_reach("consume").assertion_pins_other_function(
            &unknown,
            "assert_eq!(widget.consume(1), 3)"
        )
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
    let reach_index = TransitiveReachIndex::new(&implicit);
    assert!(
        !reach_index.owner_reach("consume").assertion_pins_other_function(
            owner(&implicit, true)?,
            "assert_eq!(widget.consume(1), 3)"
        )
    );
    Ok(())
}
