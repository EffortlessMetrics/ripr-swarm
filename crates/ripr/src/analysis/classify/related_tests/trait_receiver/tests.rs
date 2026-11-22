use super::super::owner_call_is_direct;
use crate::analysis::RustIndex;
use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::path::{Path, PathBuf};

/// `BytesMut` and `Limit` both implement `BufMut::advance_mut`, and each is
/// built by an associated function the text receiver rule does not know.
const BUF: &str = "pub trait BufMut {\n    fn advance_mut(&mut self, cnt: usize);\n}\n\
    pub struct BytesMut { len: usize, cap: usize }\n\
    pub struct Limit { left: usize }\n\
    impl BytesMut {\n\
    pub fn with_capacity(capacity: usize) -> BytesMut { BytesMut { len: 0, cap: capacity } }\n\
    pub fn try_with(capacity: usize) -> Option<BytesMut> { Some(BytesMut { len: 0, cap: capacity }) }\n\
    }\n\
    impl Limit {\n    pub fn of(left: usize) -> Limit { Limit { left } }\n}\n\
    impl BufMut for BytesMut {\n    fn advance_mut(&mut self, cnt: usize) { if cnt > self.cap - self.len { panic!() } self.len += cnt; }\n}\n\
    impl BufMut for Limit {\n    fn advance_mut(&mut self, cnt: usize) { self.left -= cnt; }\n}\n";

const BYTES_MUT_ADVANCE: &str = "impl BufMut for BytesMut::advance_mut";

/// A call on a `BytesMut` built by `with_capacity`.
const TYPED_CALL: &str = "let mut b = BytesMut::with_capacity(8); b.advance_mut(1);";

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
/// direct call of the one function whose symbol id ends in `owner_suffix`.
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

/// A receiver built by an indexed associated function that returns the
/// impl's own type runs that type's trait impl.
#[test]
fn a_receiver_built_by_an_indexed_constructor_reaches_the_trait_impl() -> Result<(), String> {
    for body in [
        TYPED_CALL,
        "BytesMut::with_capacity(8).advance_mut(1);",
        "let mut b = BytesMut::with_capacity(8); { b.advance_mut(1); }",
        "let mut b = BytesMut::with_capacity(8); (&mut b).advance_mut(1);",
    ] {
        assert!(direct(BUF, BYTES_MUT_ADVANCE, body)?, "{body}");
    }
    Ok(())
}

/// The same calls do not reach the sibling impl on `Limit`, and a `Limit`
/// receiver does not reach `BytesMut`'s.
#[test]
fn a_typed_receiver_reaches_only_its_own_types_impl() -> Result<(), String> {
    assert!(!direct(
        BUF,
        "impl BufMut for Limit::advance_mut",
        TYPED_CALL
    )?);
    let limit = "let mut l = Limit::of(8); l.advance_mut(1);";
    assert!(!direct(BUF, BYTES_MUT_ADVANCE, limit)?);
    assert!(direct(BUF, "impl BufMut for Limit::advance_mut", limit)?);
    Ok(())
}

/// A receiver whose type the index does not establish keeps the name-only
/// relation: an unknown function, a wrapped return, a later rebinding to
/// another type, and a binding from an expired block.
#[test]
fn a_receiver_the_index_cannot_type_stays_name_only() -> Result<(), String> {
    for body in [
        "let mut b = make(); b.advance_mut(1);",
        "let mut b = BytesMut::try_with(8).unwrap(); b.advance_mut(1);",
        "let mut b = BytesMut::try_with(8); b.advance_mut(1);",
        "let mut b = BytesMut::with_capacity(8); let mut b = Limit::of(8); b.advance_mut(1);",
        "{ let mut b = BytesMut::with_capacity(8); } b.advance_mut(1);",
        "let mut b = BytesMut::missing(8); b.advance_mut(1);",
    ] {
        assert!(!direct(BUF, BYTES_MUT_ADVANCE, body)?, "{body}");
    }
    Ok(())
}

/// With a receiver of the impl's type, method-call syntax still runs another
/// method when the type has one of that name, or when an impl the index does
/// not list could define one. Each addition to `BUF` withholds the relation
/// that `BUF` alone grants.
#[test]
fn another_method_of_the_name_on_the_type_withholds_the_relation() -> Result<(), String> {
    assert!(direct(BUF, BYTES_MUT_ADVANCE, TYPED_CALL)?);
    for (rival, addition) in [
        (
            "an inherent method taken at the by-value step",
            "impl BytesMut {\n    pub fn advance_mut(self, _cnt: usize) {}\n}\n",
        ),
        (
            "another trait's method on the type",
            "pub trait Other {\n    fn advance_mut(&mut self, cnt: usize);\n}\n\
             impl Other for BytesMut {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n",
        ),
        (
            "another trait's default body",
            "pub trait Other {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n\
             impl Other for BytesMut {}\n",
        ),
        (
            "a blanket impl",
            "pub trait Wide {\n    fn advance_mut(&mut self, cnt: usize);\n}\n\
             impl<T> Wide for T {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n",
        ),
        (
            "a macro that writes an impl for the type",
            "pub trait Other {\n    fn advance_mut(&mut self, cnt: usize);\n}\n\
             macro_rules! forward {\n    ($t:ty) => {\n        impl Other for $t {\n            fn advance_mut(&mut self, _cnt: usize) {}\n        }\n    };\n}\n\
             forward!(BytesMut);\n",
        ),
        (
            "an inherent method on an alias of the type",
            "pub type Buffer = BytesMut;\nimpl Buffer {\n    pub fn advance_mut(self, _cnt: usize) {}\n}\n",
        ),
        (
            "an inherent method on a renamed import of the type",
            "pub mod view {\n    use super::BytesMut as Buffer;\n    impl Buffer {\n        pub fn advance_mut(self, _cnt: usize) {}\n    }\n}\n",
        ),
        (
            "a default body of a second trait that shares the owner trait's name",
            "pub mod other {\n    pub trait BufMut: Sized {\n        fn advance_mut(self, _cnt: usize) {}\n    }\n    impl BufMut for super::BytesMut {}\n}\n",
        ),
        (
            "a macro call in the type's impl that writes the method",
            "macro_rules! by_value {\n    () => {\n        pub fn advance_mut(self, _cnt: usize) {}\n    };\n}\n\
             impl BytesMut {\n    by_value!();\n}\n",
        ),
        (
            "a second type of the same name",
            "pub mod other {\n    pub struct BytesMut;\n}\n",
        ),
    ] {
        let production = format!("{BUF}{addition}");
        assert!(
            !direct(&production, BYTES_MUT_ADVANCE, TYPED_CALL)?,
            "{rival}"
        );
    }
    Ok(())
}

/// An impl for a reference to the type, or one of two impls that share a
/// generic type's name, is not selected by a value the constructor returns.
#[test]
fn an_impl_the_type_name_does_not_select_stays_name_only() -> Result<(), String> {
    let by_reference = "pub trait BufMut {\n    fn advance_mut(&mut self, cnt: usize);\n}\n\
        pub struct BytesMut { len: usize }\npub struct Limit { left: usize }\n\
        impl BytesMut {\n    pub fn with_capacity(_capacity: usize) -> BytesMut { BytesMut { len: 0 } }\n}\n\
        impl BufMut for &BytesMut {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n\
        impl BufMut for Limit {\n    fn advance_mut(&mut self, cnt: usize) { self.left -= cnt; }\n}\n";
    assert!(!direct(
        by_reference,
        "impl BufMut for &BytesMut::advance_mut",
        TYPED_CALL
    )?);
    let instantiations = "pub trait BufMut {\n    fn advance_mut(&mut self, cnt: usize);\n}\n\
        pub struct Wrap<T>(pub T);\n\
        impl<T> Wrap<T> {\n    pub fn of(value: T) -> Wrap<T> { Wrap(value) }\n}\n\
        impl BufMut for Wrap<u8> {\n    fn advance_mut(&mut self, cnt: usize) { self.0 += cnt as u8; }\n}\n\
        impl BufMut for Wrap<u16> {\n    fn advance_mut(&mut self, cnt: usize) { self.0 += cnt as u16; }\n}\n";
    assert!(!direct(
        instantiations,
        "impl BufMut for Wrap<u8>::advance_mut",
        "let mut w = Wrap::of(1u8); w.advance_mut(1);"
    )?);
    Ok(())
}

/// A generic type with one impl of the trait is selected by its name, and an
/// overridden default of the owner's own trait does not compete with it.
#[test]
fn a_generic_type_with_one_impl_reaches_it() -> Result<(), String> {
    let chain = "pub trait BufMut {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n\
        pub struct Limit { left: usize }\n\
        impl Limit {\n    pub fn of(left: usize) -> Limit { Limit { left } }\n}\n\
        impl BufMut for Limit {\n    fn advance_mut(&mut self, cnt: usize) { self.left -= cnt; }\n}\n\
        pub struct Chain<T, U> { a: T, b: U }\n\
        impl<T, U> Chain<T, U> {\n    pub fn pair(a: T, b: U) -> Chain<T, U> { Chain { a, b } }\n}\n\
        impl<T: BufMut, U: BufMut> BufMut for Chain<T, U> {\n    fn advance_mut(&mut self, cnt: usize) { self.a.advance_mut(cnt); self.b.advance_mut(cnt); }\n}\n";
    let owner = "impl BufMut for Chain<T, U>::advance_mut";
    assert!(direct(
        chain,
        owner,
        "let mut c = Chain::pair(Limit::of(1), Limit::of(2)); c.advance_mut(1);"
    )?);
    assert!(!direct(
        chain,
        owner,
        "let mut l = Limit::of(1); l.advance_mut(1);"
    )?);
    Ok(())
}

/// A workspace macro that names the type but cannot write a method of the
/// owner's name leaves the relation: its definition, the arguments of each
/// invocation and the macros it invokes are all readable and none holds the
/// name. A blanket impl of the owner's own trait written by such a macro
/// leaves it too.
#[test]
fn a_macro_that_cannot_write_the_method_leaves_the_relation() -> Result<(), String> {
    for (case, addition) in [
        (
            "an impl of another trait with other methods",
            "pub trait Named {\n    fn name(&self) -> u32;\n}\n\
             macro_rules! named {\n    ($t:ty) => {\n        impl Named for $t {\n            fn name(&self) -> u32 { assert!(true); 1 }\n        }\n    };\n}\n\
             named!(BytesMut);\n",
        ),
        (
            "a macro that forwards to a readable macro",
            "pub trait Named {\n    fn name(&self) -> u32;\n}\n\
             macro_rules! named {\n    ($t:ty) => {\n        impl Named for $t {\n            fn name(&self) -> u32 { 1 }\n        }\n    };\n}\n\
             macro_rules! each {\n    ($($t:ty),*) => { $(named!($t);)* };\n}\n\
             each!(BytesMut, Limit);\n",
        ),
        (
            "a macro-written blanket impl of the owner's trait",
            "macro_rules! forward {\n    () => {\n        fn advance_mut(&mut self, cnt: usize) { (**self).advance_mut(cnt) }\n    };\n}\n\
             impl<T: BufMut> BufMut for &mut T {\n    forward!();\n}\n",
        ),
        (
            "a macro-written blanket impl of another trait with other methods",
            "pub trait Named {\n    fn name(&self) -> u32;\n}\n\
             macro_rules! forward {\n    () => {\n        fn name(&self) -> u32 { 1 }\n    };\n}\n\
             impl<T: BufMut> Named for T {\n    forward!();\n}\n",
        ),
    ] {
        let production = format!("{BUF}{addition}");
        assert!(
            direct(&production, BYTES_MUT_ADVANCE, TYPED_CALL)?,
            "{case}"
        );
    }
    Ok(())
}

/// A macro that names the type withholds the relation whenever its
/// expansion could hold the method's name: from an invocation's arguments,
/// through a metavariable function name, through a macro it invokes, or
/// because its definition is not in the workspace. A macro-written blanket
/// impl of another trait withholds it the same way.
#[test]
fn a_macro_that_may_write_the_method_withholds_the_relation() -> Result<(), String> {
    const OTHER: &str = "pub trait Other {\n    fn advance_mut(&mut self, cnt: usize);\n}\n";
    for (case, addition) in [
        (
            "the method arrives in the invocation's arguments",
            "macro_rules! with_items {\n    ($t:ty, $($body:tt)*) => {\n        impl Other for $t { $($body)* }\n    };\n}\n\
             with_items!(BytesMut, fn advance_mut(&mut self, _cnt: usize) {});\n",
        ),
        (
            "the function is named by a metavariable",
            "macro_rules! method {\n    ($t:ty, $name:ident) => {\n        impl Other for $t {\n            fn $name(&mut self, _cnt: usize) {}\n        }\n    };\n}\n\
             method!(BytesMut, advance);\n",
        ),
        (
            "a macro it invokes writes the method",
            "macro_rules! inner {\n    ($t:ty) => {\n        impl Other for $t {\n            fn advance_mut(&mut self, _cnt: usize) {}\n        }\n    };\n}\n\
             macro_rules! outer {\n    ($t:ty) => { inner!($t); };\n}\n\
             outer!(BytesMut);\n",
        ),
        (
            "a macro it invokes is not defined in the workspace",
            "macro_rules! pasted {\n    ($t:ty) => { paste::paste! { impl Other for $t {} } };\n}\n\
             pasted!(BytesMut);\n",
        ),
        (
            "the macro is not defined in the workspace",
            "foreign::implement!(Other for BytesMut);\n",
        ),
        (
            "a macro-written blanket impl of another trait",
            "macro_rules! forward {\n    () => {\n        fn advance_mut(&mut self, _cnt: usize) {}\n    };\n}\n\
             impl<T: BufMut> Other for T {\n    forward!();\n}\n",
        ),
    ] {
        let production = format!("{BUF}{OTHER}{addition}");
        assert!(
            !direct(&production, BYTES_MUT_ADVANCE, TYPED_CALL)?,
            "{case}"
        );
    }
    Ok(())
}

/// A standard trait's provided method is written nowhere in the workspace,
/// and `Iterator::count(self)` takes an owned `Shelf` before a workspace
/// `fn count(&self)` does. The name alone withholds the relation; the same
/// impl under another name, and the standard trait's own impl, keep it.
#[test]
fn a_standard_trait_method_of_the_name_withholds_the_relation() -> Result<(), String> {
    let shelf = |method: &str| {
        format!(
            "pub struct Shelf {{ left: usize }}\n\
             impl Shelf {{\n    pub fn stocked(left: usize) -> Shelf {{ Shelf {{ left }} }}\n}}\n\
             impl Iterator for Shelf {{\n    type Item = usize;\n    fn next(&mut self) -> Option<usize> {{ if self.left > 0 {{ self.left -= 1; Some(self.left) }} else {{ None }} }}\n}}\n\
             pub trait Stock {{\n    fn {method}(&self) -> usize;\n}}\n\
             impl Stock for Shelf {{\n    fn {method}(&self) -> usize {{ if self.left > 3 {{ 1 }} else {{ 0 }} }}\n}}\n"
        )
    };
    for method in ["count", "into_iter", "into", "to_string", "last"] {
        let body = format!("let _ = Shelf::stocked(5).{method}();");
        assert!(
            !direct(
                &shelf(method),
                &format!("impl Stock for Shelf::{method}"),
                &body
            )?,
            "{method}"
        );
    }
    assert!(direct(
        &shelf("on_hand"),
        "impl Stock for Shelf::on_hand",
        "let _ = Shelf::stocked(5).on_hand();"
    )?);
    assert!(direct(
        &shelf("on_hand"),
        "impl Iterator for Shelf::next",
        "let _ = Shelf::stocked(5).next();"
    )?);
    Ok(())
}

/// A rival under another type's name, and a macro call in the type's impl
/// that cannot write the method, leave the relation.
#[test]
fn a_rival_on_another_type_and_an_unrelated_impl_macro_leave_the_relation() -> Result<(), String> {
    for addition in [
        "pub struct Other;\nimpl Other {\n    pub fn advance_mut(self, _cnt: usize) {}\n}\n",
        "macro_rules! getter {\n    () => {\n        pub fn len(&self) -> usize { self.len }\n    };\n}\n\
         impl BytesMut {\n    getter!();\n}\n",
    ] {
        let production = format!("{BUF}{addition}");
        assert!(
            direct(&production, BYTES_MUT_ADVANCE, TYPED_CALL)?,
            "{addition}"
        );
    }
    Ok(())
}
