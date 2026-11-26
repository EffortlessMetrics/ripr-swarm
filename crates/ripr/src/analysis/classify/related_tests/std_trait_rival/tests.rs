use super::super::dispatch_target::ReceiverKind;
use super::super::{FunctionSummary, TestSummary, owner_call_is_direct};
use super::{
    a_standard_trait_method_may_take_the_call, calls_through_the_trait_path,
    every_method_call_borrows_its_receiver,
};
use crate::analysis::RustIndex;
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::path::{Path, PathBuf};

/// `Foo` is an `Iterator`, so `IntoIterator::into_iter(self)` reaches it by
/// the standard blanket impl, and `Peeky::into_iter(&self)` is the only
/// indexed method of the name.
fn production(peeky_receiver: &str, name: &str) -> String {
    format!(
        "pub struct Foo(pub u32);\n\
         impl Foo {{\n    pub fn new(n: u32) -> Self {{ Foo(n) }}\n}}\n\
         impl Iterator for Foo {{\n    type Item = u32;\n    fn next(&mut self) -> Option<u32> {{ None }}\n}}\n\
         pub trait Peeky {{\n    fn {name}({peeky_receiver}) -> u32;\n}}\n\
         impl Peeky for Foo {{\n    fn {name}({peeky_receiver}) -> u32 {{ if self.0 > 3 {{ 1 }} else {{ 0 }} }}\n}}\n"
    )
}

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

/// Whether the owner-call relation credits the one test as a direct call of
/// the one function whose symbol id ends in `owner_suffix`.
fn direct(production: &str, owner_suffix: &str, test_body: &str) -> Result<bool, String> {
    judged(production, owner_suffix, test_body, owner_call_is_direct)
}

/// Whether this module's rule alone withholds the relation, whatever the
/// rest of the owner-call relation decides for the test.
fn withheld(production: &str, owner_suffix: &str, test_body: &str) -> Result<bool, String> {
    judged(production, owner_suffix, test_body, |test, owner, _| {
        a_standard_trait_method_may_take_the_call(test, owner, owner.name.as_str())
    })
}

/// `judge` applied to the one test and the one function whose symbol id ends
/// in `owner_suffix`.
fn judged<R>(
    production: &str,
    owner_suffix: &str,
    test_body: &str,
    judge: impl FnOnce(&TestSummary, &FunctionSummary, &RustIndex) -> R,
) -> Result<R, String> {
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
    Ok(judge(test, owner, &index))
}

/// The issue's example: the by-value `IntoIterator::into_iter` takes
/// `Foo::new(5).into_iter()` before the owner's `&self` step.
#[test]
fn a_standard_trait_method_of_the_name_withholds_the_relation() -> Result<(), String> {
    let owner = "impl Peeky for Foo::into_iter";
    let peeky = production("&self", "into_iter");
    assert!(!direct(&peeky, owner, "let _ = Foo::new(5).into_iter();")?);
    assert!(!direct(
        &peeky,
        owner,
        "let foo = Foo::new(5); let _ = foo.into_iter();"
    )?);
    assert!(!direct(
        &production("&mut self", "into_iter"),
        owner,
        "let _ = Foo::new(5).into_iter();"
    )?);
    Ok(())
}

/// The same impl under a name no standard trait has keeps the relation, and
/// so does an owner taking `self` by value, which the first probing step
/// tries.
#[test]
fn an_unlisted_name_or_a_by_value_owner_keeps_the_relation() -> Result<(), String> {
    assert!(direct(
        &production("&self", "peek_count"),
        "impl Peeky for Foo::peek_count",
        "let _ = Foo::new(5).peek_count();"
    )?);
    assert!(direct(
        &production("self", "into_iter"),
        "impl Peeky for Foo::into_iter",
        "let _ = Foo::new(5).into_iter();"
    )?);
    Ok(())
}

/// A standard method at the owner's own probing step would make the call
/// ambiguous, so it takes nothing: `ExactSizeIterator::len(&self)` leaves a
/// `&self` owner named `len`, and withholds a `&mut self` one, which it
/// precedes.
#[test]
fn only_an_earlier_probing_step_withholds_the_relation() -> Result<(), String> {
    let owner = "impl Peeky for Foo::len";
    let body = "let _ = Foo::new(5).len();";
    assert!(direct(&production("&self", "len"), owner, body)?);
    assert!(!direct(&production("&mut self", "len"), owner, body)?);
    Ok(())
}

/// The standard trait's own impl is not its own rival, and neither is
/// another standard trait sharing the name: `Iterator::eq` takes `self` by
/// value, but only for a type that is an `Iterator` too.
#[test]
fn the_standard_traits_own_impl_keeps_the_relation() -> Result<(), String> {
    assert!(direct(
        &production("&self", "peek_count"),
        "impl Iterator for Foo::next",
        "let _ = Foo::new(5).next();"
    )?);
    let equality = "pub struct Foo(pub u32);\n\
         impl Foo {\n    pub fn new(n: u32) -> Self { Foo(n) }\n}\n\
         impl PartialEq for Foo {\n    fn eq(&self, other: &Foo) -> bool { self.0 == other.0 }\n}\n";
    assert!(direct(
        equality,
        "impl PartialEq for Foo::eq",
        "assert!(Foo::new(1).eq(&Foo::new(1)));"
    )?);
    Ok(())
}

/// A call through the owner's trait path names the owner's trait, so this
/// rule does not withhold the relation. Whether the rest of the relation
/// credits a bare `Peeky::name(` path call is decided elsewhere and is
/// unchanged by #16040; the qualified `<Foo as ..>` form is credited.
#[test]
fn a_call_through_the_trait_path_keeps_the_relation() -> Result<(), String> {
    let peeky = production("&self", "into_iter");
    let owner = "impl Peeky for Foo::into_iter";
    assert!(withheld(&peeky, owner, "let _ = Foo::new(5).into_iter();")?);
    for body in [
        "let _ = Peeky::into_iter(&Foo::new(5));",
        "let _ = crate::Peeky :: into_iter (&Foo::new(5));",
        "let _ = <Foo as Peeky>::into_iter(&Foo::new(5));",
        "let _ = <Foo as crate::Peeky>::into_iter(&Foo::new(5));",
    ] {
        assert!(!withheld(&peeky, owner, body)?, "{body}");
    }
    assert!(direct(
        &peeky,
        owner,
        "assert_eq!(<Foo as crate::Peeky>::into_iter(&Foo::new(3)), 1);"
    )?);
    Ok(())
}

/// Only a path ending in the owner's trait counts, outside comments and
/// literals, and `Type<Trait>::name(` names no trait.
#[test]
fn only_the_owner_traits_path_counts() {
    for body in [
        "Peeky::into_iter(&foo)",
        "x = self::Peeky::into_iter(&foo)",
        "<Foo as Peeky>::into_iter(&foo)",
        "< Foo as r#Peeky > :: r#into_iter (&foo)",
        "<Foo as crate::Peeky>::into_iter(&foo)",
        "<Foo as crate::inner::Peeky<u8>>::into_iter(&foo)",
        "<Foo as Peeky<fn() -> u8>>::into_iter(&foo)",
        "Peeky::<u8>::into_iter(&foo)",
        "Peeky::into_iter::<u8>(&foo)",
        "<Foo as Peeky>::into_iter :: < Vec<u8> > (&foo)",
    ] {
        assert!(
            calls_through_the_trait_path(&mask_comments_and_strings(body), "into_iter", "Peeky"),
            "{body}"
        );
    }
    for body in [
        "foo.into_iter()",
        "HasPeeky::into_iter(&foo)",
        "Vec<Peeky>::into_iter(foo)",
        "Peeky::into_iter_all(&foo)",
        "// Peeky::into_iter(&foo)\nfoo.into_iter()",
        "let s = \"Peeky::into_iter(&foo)\"; foo.into_iter()",
        "Peeky::into_iter",
        "let f = Peeky::into_iter::<u8>;",
        "<Peeky as Other>::into_iter(&foo)",
        "<Foo as crate::Peeky::Inner>::into_iter(&foo)",
    ] {
        assert!(
            !calls_through_the_trait_path(&mask_comments_and_strings(body), "into_iter", "Peeky"),
            "{body}"
        );
    }
}

/// A receiver written as a borrow of the owner's kind starts probing at the
/// owner's step, so the owner takes the call.
#[test]
fn a_borrowed_receiver_keeps_the_relation() -> Result<(), String> {
    let owner = "impl Peeky for Foo::into_iter";
    let shared = production("&self", "into_iter");
    assert!(!withheld(
        &shared,
        owner,
        "let _ = (&Foo::new(5)).into_iter();"
    )?);
    assert!(direct(
        &shared,
        owner,
        "assert_eq!((&Foo::new(3)).into_iter(), 1);"
    )?);
    assert!(withheld(
        &shared,
        owner,
        "let foo = &Foo::new(5);\n    let _ = foo.into_iter();"
    )?);
    let exclusive = production("&mut self", "into_iter");
    assert!(!withheld(
        &exclusive,
        owner,
        "let mut foo = Foo::new(5);\n    let _ = (&mut foo).into_iter();"
    )?);
    assert!(withheld(
        &exclusive,
        owner,
        "let foo = Foo::new(5);\n    let _ = (&foo).into_iter();"
    )?);
    Ok(())
}

/// Every method call of the name needs a borrowed receiver of the owner's
/// kind; a call's parentheses are not a borrow.
#[test]
fn only_every_borrowed_receiver_counts() {
    let shared = ReceiverKind::Shared;
    let exclusive = ReceiverKind::Exclusive;
    for (body, kind) in [
        ("(&foo).into_iter()", shared),
        ("(&Foo::new::<u8>(&0)).into_iter()", shared),
        ("(&foo).into_iter::<u8>()", shared),
        ("( & foo ).into_iter(); (&bar).into_iter()", shared),
        ("(&mut foo).into_iter()", exclusive),
        ("(& mut foo) . r#into_iter ()", exclusive),
    ] {
        assert!(
            every_method_call_borrows_its_receiver(
                &mask_comments_and_strings(body),
                "into_iter",
                kind
            ),
            "{body}"
        );
    }
    for (body, kind) in [
        ("", shared),
        ("foo.into_iter()", shared),
        ("(&foo).into_iter(); foo.into_iter()", shared),
        ("(&mut foo).into_iter()", shared),
        ("(&&foo).into_iter()", shared),
        ("(&foo).into_iter()", exclusive),
        ("(&mutable).into_iter()", exclusive),
        ("wrap(&foo).into_iter()", shared),
        ("id!(&foo).into_iter()", shared),
        ("Foo::new::<u8>(&0).into_iter()", shared),
        ("items[0](&foo).into_iter()", shared),
        (
            "(&foo).into_iter(); Foo::new::<u8>(&0).into_iter::<u8>()",
            shared,
        ),
        ("(&foo).into_iter_all()", shared),
        ("Peeky::into_iter(&foo)", shared),
    ] {
        assert!(
            !every_method_call_borrows_its_receiver(
                &mask_comments_and_strings(body),
                "into_iter",
                kind
            ),
            "{body}"
        );
    }
}
