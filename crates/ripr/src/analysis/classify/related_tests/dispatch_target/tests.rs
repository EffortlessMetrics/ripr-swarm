use super::super::owner_call_is_direct;
use crate::analysis::RustIndex;
use crate::analysis::facts::{FunctionContainer, FunctionFact};
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

/// Which owner a case means.
#[derive(Clone, Copy)]
enum Owner {
    /// The `Counter` trait declaration's default `advance`. Its symbol id
    /// carries no `impl` segment, so it is selected by its container.
    DefaultAdvance,
    /// The function whose symbol id ends in this suffix.
    IdSuffix(&'static str),
}

fn is_owner(function: &FunctionFact, owner: Owner) -> bool {
    match owner {
        Owner::DefaultAdvance => {
            function.name == "advance"
                && matches!(
                    &function.item.container,
                    FunctionContainer::Trait { trait_name } if trait_name == "Counter"
                )
        }
        Owner::IdSuffix(suffix) => function.id.0.ends_with(suffix),
    }
}

/// Whether `ripr check`'s owner-call relation credits the one test as a
/// direct call of `owner`.
fn direct(production: &str, owner: Owner, test_body: &str) -> Result<bool, String> {
    let index = parsed_index(production, test_body)?;
    let functions = index.functions();
    let mut owners = functions
        .iter()
        .filter(|function| is_owner(function, owner));
    let (Some(owner), None) = (owners.next(), owners.next()) else {
        return Err("exactly one owner matches".to_string());
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

/// The parsed index holds exactly the default and `Fast`'s override, so a
/// changed container or id fails loudly instead of every case below passing
/// on a missing owner.
#[test]
fn the_default_method_owner_is_the_trait_declaration() -> Result<(), String> {
    let index = parsed_index(COUNTER, "Unit.advance();")?;
    let functions = index.functions();
    let advances: Vec<&FunctionFact> = functions
        .iter()
        .filter(|function| function.name == "advance")
        .collect();
    assert_eq!(advances.len(), 2);
    assert_eq!(
        advances
            .iter()
            .filter(|function| is_owner(function, Owner::DefaultAdvance))
            .count(),
        1
    );
    assert!(
        advances
            .iter()
            .any(|function| function.id.0.ends_with("impl Counter for Fast::advance")),
        "{:?}",
        advances
            .iter()
            .map(|function| function.id.0.as_str())
            .collect::<Vec<_>>()
    );
    Ok(())
}

/// #15273: every call resolves to `Fast`, whose impl overrides the default,
/// so the default never runs.
#[test]
fn calls_on_an_overriding_type_do_not_reach_the_default() -> Result<(), String> {
    for body in [
        "assert_eq!(Fast.advance(), 8);",
        "assert_eq!(Counter::advance(&Fast), 8);",
        "assert_eq!(<Fast as Counter>::advance(&Fast), 8);",
        "assert_eq!(Fast.advance(), 8); assert_eq!(Counter::advance(&Fast), 8);",
    ] {
        assert!(!direct(COUNTER, Owner::DefaultAdvance, body)?, "{body}");
    }
    Ok(())
}

/// A call on a type that keeps the default, one ripr cannot type, a mix, or
/// a path value keeps the direct relation: the default may run.
#[test]
fn a_call_that_may_run_the_default_keeps_the_relation() -> Result<(), String> {
    for body in [
        "assert_eq!(Unit.advance(), 8);",
        "assert_eq!(Counter::advance(&Unit), 8);",
        "assert_eq!(Fast.advance(), 8); assert_eq!(Unit.advance(), 8);",
        "let c = make(); assert_eq!(c.advance(), 8);",
        "fn run<C: Counter>(c: &C) -> u32 { c.advance() } assert_eq!(run(&Unit), 8);",
        "assert_eq!(Fast.advance(), 8); let f = Counter::advance; assert_eq!(f(&Unit), 8);",
    ] {
        assert!(direct(COUNTER, Owner::DefaultAdvance, body)?, "{body}");
    }
    Ok(())
}

const TRAIT_AREA: Owner = Owner::IdSuffix("impl Shape for Square::area");

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
/// `<Square as Shape>`, `dyn Shape`), so it keeps the relation.
#[test]
fn a_trait_spelling_keeps_the_relation() -> Result<(), String> {
    for body in [
        "assert_eq!(Shape::area(&Square(3)), 9);",
        "assert_eq!(<Square as Shape>::area(&Square(3)), 9);",
        "let s: &dyn Shape = &Square(3); assert_eq!(s.area(), 9); assert_eq!(Square(3).area(), 9);",
    ] {
        assert!(direct(SHAPE, TRAIT_AREA, body)?, "{body}");
    }
    Ok(())
}

/// A twin with no `self`, or with another receiver kind, does not take the
/// method call first: method probing reaches a trait impl's `&self` step
/// before an inherent `&mut self`, and a by-value trait impl before an
/// inherent `&self`.
#[test]
fn a_twin_with_another_receiver_kind_keeps_the_relation() -> Result<(), String> {
    for twin in ["pub fn area()", "pub fn area(&mut self)"] {
        let production = SHAPE.replace("pub fn area(&self)", twin);
        assert!(
            direct(&production, TRAIT_AREA, "assert_eq!(Square(3).area(), 9);")?,
            "{twin}"
        );
    }
    let by_value = SHAPE
        .replace("fn area(&self) -> u32;", "fn area(self) -> u32;")
        .replace(
            "fn area(&self) -> u32 { self.0 * self.0 }",
            "fn area(self) -> u32 { self.0 * self.0 }",
        );
    assert!(direct(
        &by_value,
        TRAIT_AREA,
        "assert_eq!(Square(3).area(), 9);"
    )?);
    Ok(())
}

/// An override the test build may not compile, a namesake `Fast` in another
/// module, or an override of one instantiation of a generic trait does not
/// establish that `Fast.advance()` skips the default.
#[test]
fn an_override_that_may_not_apply_keeps_the_relation() -> Result<(), String> {
    let gated = COUNTER.replace(
        "    fn advance(&self) -> u32 { 8 }\n",
        "    #[cfg(any())]\n    fn advance(&self) -> u32 { 8 }\n",
    );
    let namesake = format!("{COUNTER}pub mod other {{\n    pub struct Fast;\n}}\n");
    for production in [gated, namesake] {
        assert!(
            direct(
                &production,
                Owner::DefaultAdvance,
                "assert_eq!(Fast.advance(), 8);"
            )?,
            "{production}"
        );
    }
    let generic = "pub trait Counter<T> {\n    fn step(&self) -> u32;\n    fn advance(&self) -> u32 { 4 * self.step() }\n}\n\
        pub struct Fast;\n\
        impl Counter<u8> for Fast {\n    fn step(&self) -> u32 { 2 }\n    fn advance(&self) -> u32 { 8 }\n}\n\
        impl Counter<u16> for Fast {\n    fn step(&self) -> u32 { 2 }\n}\n";
    assert!(direct(
        generic,
        Owner::DefaultAdvance,
        "assert_eq!(<Fast as Counter<u16>>::advance(&Fast), 8);"
    )?);
    Ok(())
}

/// A cfg-gated twin, a namesake `Square`, or a generic self type whose
/// inherent impl may carry bounds keeps the relation.
#[test]
fn a_twin_that_may_not_apply_keeps_the_relation() -> Result<(), String> {
    let gated = SHAPE.replace(
        "    pub fn area(&self) -> u32 { 9 }\n",
        "    #[cfg(any())]\n    pub fn area(&self) -> u32 { 9 }\n",
    );
    let namesake = format!("{SHAPE}pub mod other {{\n    pub struct Square;\n}}\n");
    for production in [gated, namesake] {
        assert!(
            direct(&production, TRAIT_AREA, "assert_eq!(Square(3).area(), 9);")?,
            "{production}"
        );
    }
    let generic = "pub trait Shape {\n    fn area(&self) -> u32;\n}\npub struct Square<T>(pub T);\n\
        impl<T> Shape for Square<T> {\n    fn area(&self) -> u32 { 4 }\n}\n\
        impl<T: Copy> Square<T> {\n    pub fn area(&self) -> u32 { 9 }\n}\n";
    assert!(direct(
        generic,
        Owner::IdSuffix("impl Shape for Square<T>::area"),
        "assert_eq!(Square(String::new()).area(), 4);"
    )?);
    Ok(())
}

/// A gate on the enclosing `impl`, a `mod` or the file may compile the
/// override or twin out of the test build, and a private twin is skipped by
/// callers outside its module, so each keeps the relation.
#[test]
fn an_enclosing_gate_or_private_twin_keeps_the_relation() -> Result<(), String> {
    let gated_impl = COUNTER.replace(
        "impl Counter for Fast {",
        "#[cfg(feature = \"fast\")]\nimpl Counter for Fast {",
    );
    let gated_mod = "pub trait Counter {\n    fn step(&self) -> u32;\n    fn advance(&self) -> u32 { 4 * self.step() }\n}\n\
        pub struct Fast;\n\
        impl Counter for Fast {\n    fn step(&self) -> u32 { 2 }\n}\n\
        #[cfg(feature = \"fast\")]\nmod fast {\n    impl super::Counter for super::Fast {\n        fn step(&self) -> u32 { 2 }\n        fn advance(&self) -> u32 { 8 }\n    }\n}\n"
        .to_string();
    for production in [gated_impl, gated_mod] {
        assert!(
            direct(
                &production,
                Owner::DefaultAdvance,
                "assert_eq!(Fast.advance(), 8);"
            )?,
            "{production}"
        );
    }
    let private_twin = SHAPE.replace(
        "    pub fn area(&self) -> u32 { 9 }\n",
        "    fn area(&self) -> u32 { 9 }\n",
    );
    let gated_twin_mod = SHAPE.replace(
        "impl Square {\n    pub fn area(&self) -> u32 { 9 }\n}\n",
        "#[cfg(feature = \"twin\")]\nmod twin {\n    impl super::Square {\n        pub fn area(&self) -> u32 { 9 }\n    }\n}\n",
    );
    for production in [private_twin, gated_twin_mod] {
        assert!(
            direct(&production, TRAIT_AREA, "assert_eq!(Square(3).area(), 9);")?,
            "{production}"
        );
    }
    Ok(())
}
