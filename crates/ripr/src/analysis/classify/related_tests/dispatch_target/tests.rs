use super::super::owner_call_is_direct;
use crate::analysis::RustIndex;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::path::{Path, PathBuf};

/// `Unit` keeps the default `advance`; `Fast` overrides it.
const COUNTER: &str = "pub trait Counter {\n    fn step(&self) -> u32;\n    fn advance(&self) -> u32 { 4 * self.step() }\n}\n\
    pub struct Unit;\npub struct Fast;\n\
    impl Counter for Unit {\n    fn step(&self) -> u32 { 2 }\n}\n\
    impl Counter for Fast {\n    fn step(&self) -> u32 { 2 }\n    fn advance(&self) -> u32 { 8 }\n}\n";

/// `Square` implements `Shape::area` and also has an inherent `area(&self)`.
const SHAPE: &str = "pub trait Shape {\n    fn area(&self) -> u32;\n}\npub struct Square(pub u32);\n\
    impl Shape for Square {\n    fn area(&self) -> u32 { self.0 * self.0 }\n}\n\
    impl Square {\n    pub fn area(&self) -> u32 { 9 }\n}\n";

/// A parser-backed index of `production` followed by one test whose body
/// is `test_body`, so containers and symbol ids are the pipeline's own.
fn parsed_index(production: &str, test_body: &str) -> Result<RustIndex, String> {
    let source = format!(
        "{production}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn t() {{ {test_body} }}\n}}\n"
    );
    let path = Path::new("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(path, &source)?;
    let mut index = RustIndex::default();
    index.extend_tests(facts.tests.iter().cloned());
    index.extend_functions(facts.functions.iter().cloned());
    index.insert_file_only(PathBuf::from(path), facts);
    Ok(index)
}

/// Whether `ripr check`'s owner-call relation credits the one test as a
/// direct call of the owner whose symbol id ends in `owner_suffix`.
fn direct(production: &str, owner_suffix: &str, test_body: &str) -> Result<bool, String> {
    let index = parsed_index(production, test_body)?;
    let functions = index.functions();
    let mut owners = functions
        .iter()
        .filter(|function| function.id.0.ends_with(owner_suffix));
    let (Some(owner), None) = (owners.next(), owners.next()) else {
        return Err(format!("exactly one owner ends in `{owner_suffix}`"));
    };
    let tests = index.tests();
    let (Some(test), None) = (tests.iter().next(), tests.iter().nth(1)) else {
        return Err("exactly one test is indexed".to_string());
    };
    if !test.body.contains(owner.name.as_str()) {
        return Err(format!("the test body names `{}`", owner.name));
    }
    Ok(owner_call_is_direct(test, owner, &index))
}

const DEFAULT_ADVANCE: &str = "trait Counter::advance";

/// The parsed id of the default method, so a renamed id fails loudly instead
/// of every case below passing on a missing owner.
#[test]
fn the_default_method_owner_is_the_trait_declaration() -> Result<(), String> {
    let index = parsed_index(COUNTER, "Unit.advance();")?;
    let ids: Vec<String> = index
        .functions()
        .iter()
        .filter(|function| function.name == "advance")
        .map(|function| function.id.0.clone())
        .collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert!(
        ids.iter().any(|id| id.ends_with(DEFAULT_ADVANCE)),
        "{ids:?}"
    );
    Ok(())
}

/// #15273: every call resolves to `Fast`, whose impl overrides the default,
/// so the default never runs.
#[test]
fn calls_on_an_overriding_type_do_not_reach_the_default() -> Result<(), String> {
    for body in [
        "assert_eq!(Fast.advance(), 8);",
        "let f = Fast; assert_eq!(f.advance(), 8);",
        "assert_eq!(Counter::advance(&Fast), 8);",
        "assert_eq!(<Fast as Counter>::advance(&Fast), 8);",
        "assert_eq!(Fast.advance(), 8); assert_eq!(Counter::advance(&Fast), 8);",
    ] {
        assert!(!direct(COUNTER, DEFAULT_ADVANCE, body)?, "{body}");
    }
    Ok(())
}

/// A call on a type that keeps the default, one ripr cannot type, or a mix
/// keeps the direct relation: the default may run.
#[test]
fn a_call_that_may_run_the_default_keeps_the_relation() -> Result<(), String> {
    for body in [
        "assert_eq!(Unit.advance(), 8);",
        "assert_eq!(Counter::advance(&Unit), 8);",
        "assert_eq!(Fast.advance(), 8); assert_eq!(Unit.advance(), 8);",
        "let c = make(); assert_eq!(c.advance(), 8);",
        "fn run<C: Counter>(c: &C) -> u32 { c.advance() } assert_eq!(run(&Unit), 8);",
    ] {
        assert!(direct(COUNTER, DEFAULT_ADVANCE, body)?, "{body}");
    }
    Ok(())
}

const TRAIT_AREA: &str = "impl Shape for Square::area";

/// #15273: method-call syntax on `Square` runs the inherent `area` first.
#[test]
fn method_calls_taken_by_an_inherent_twin_do_not_reach_the_trait_impl() -> Result<(), String> {
    for body in [
        "assert_eq!(Square(3).area(), 9);",
        "let s = Square(3); assert_eq!(s.area(), 9);",
    ] {
        assert!(!direct(SHAPE, TRAIT_AREA, body)?, "{body}");
    }
    Ok(())
}

/// A test that names the trait may dispatch through it (`Shape::area`,
/// `<Square as Shape>`, `dyn Shape`), and an inherent twin with no `self`
/// cannot take a method call, so both keep the relation.
#[test]
fn a_trait_spelling_or_an_associated_twin_keeps_the_relation() -> Result<(), String> {
    for body in [
        "assert_eq!(Shape::area(&Square(3)), 9);",
        "assert_eq!(<Square as Shape>::area(&Square(3)), 9);",
        "let s: &dyn Shape = &Square(3); assert_eq!(s.area(), 9); assert_eq!(Square(3).area(), 9);",
    ] {
        assert!(direct(SHAPE, TRAIT_AREA, body)?, "{body}");
    }
    let associated = SHAPE.replace("pub fn area(&self)", "pub fn area()");
    assert!(direct(
        &associated,
        TRAIT_AREA,
        "assert_eq!(Square(3).area(), 9);"
    )?);
    Ok(())
}
