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
            "an alias written across lines, with a comment",
            "pub type /* same type */\nBuffer = BytesMut;\nimpl Buffer {\n    pub fn advance_mut(self, _cnt: usize) {}\n}\n",
        ),
        (
            "an alias whose name another module declares as a struct",
            "pub type Buffer = BytesMut;\nimpl Buffer {\n    pub fn advance_mut(self, _cnt: usize) {}\n}\n\
             mod unrelated {\n    pub struct Buffer;\n}\n",
        ),
        (
            "a renamed import whose name another module declares as a struct",
            "pub mod view {\n    use super::BytesMut as Buffer;\n    impl Buffer {\n        pub fn advance_mut(self, _cnt: usize) {}\n    }\n}\n\
             mod unrelated {\n    pub struct Buffer;\n}\n",
        ),
        (
            "a macro call in an impl on an alias another module declares as a struct",
            "macro_rules! by_value {\n    () => {\n        pub fn advance_mut(self, _cnt: usize) {}\n    };\n}\n\
             pub type Buffer = BytesMut;\nimpl Buffer {\n    by_value!();\n}\n\
             mod unrelated {\n    pub struct Buffer;\n}\n",
        ),
        (
            "a macro call in an impl on an alias of the type",
            "macro_rules! by_value {\n    () => {\n        pub fn advance_mut(self, _cnt: usize) {}\n    };\n}\n\
             pub type Buffer = BytesMut;\nimpl Buffer {\n    by_value!();\n}\n",
        ),
        (
            "a macro call in a trait body that writes a provided method",
            "macro_rules! provided {\n    () => {\n        fn advance_mut(self, _cnt: usize) {}\n    };\n}\n\
             pub trait Other: Sized {\n    provided!();\n}\nimpl Other for BytesMut {}\n",
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
        impl<T, U> BufMut for Chain<T, U> {\n    fn advance_mut(&mut self, cnt: usize) { let _ = (cnt, &self.a, &self.b); }\n}\n";
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
            "a trait-body macro of a namesake trait",
            "pub mod other {\n    pub trait BufMut {\n        forward!();\n    }\n}\n\
             macro_rules! forward {\n    () => {\n        fn advance_mut(self, _cnt: usize) {}\n    };\n}\n",
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
/// `fn count(&self)` does. The name alone withholds the relation, also for
/// an owner whose trait shares a standard trait's name; the same impl under
/// another name keeps it.
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
    for method in [
        "count",
        "into_iter",
        "into",
        "to_string",
        "last",
        "contains",
        "read_exact_at",
        "seek_read",
    ] {
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
    let local_into = "pub struct Shelf { left: usize }\n\
        impl Shelf {\n    pub fn stocked(left: usize) -> Shelf { Shelf { left } }\n}\n\
        pub trait Into {\n    fn into(&self) -> usize;\n}\n\
        impl Into for Shelf {\n    fn into(&self) -> usize { self.left }\n}\n";
    assert!(!direct(
        local_into,
        "impl Into for Shelf::into",
        "use std::convert::Into as StdInto; let _: Shelf = Shelf::stocked(5).into();"
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

/// A generic trait can be implemented for the type again under other
/// arguments (`impl Measure<u16> for Square {}` inherits the default), and
/// the call's argument types pick the impl, so the relation is withheld,
/// also when a defaulted parameter (`trait Measure<T = u8>`) lets the owner's
/// impl name the trait bare. The same impl of a trait without parameters
/// keeps it.
#[test]
fn a_generic_trait_withholds_the_typed_receiver() -> Result<(), String> {
    let plain = "pub trait Measure {\n    fn area(&self, x: u8) -> u32 { 0 }\n}\n\
        pub struct Square(pub u32);\n\
        impl Square {\n    pub fn with_side(side: u32) -> Square { Square(side) }\n}\n\
        impl Measure for Square {\n    fn area(&self, _x: u8) -> u32 { 9 }\n}\n";
    let generic = "pub trait Measure<T> {\n    fn area(&self, x: T) -> u32 { 0 }\n}\n\
        pub struct Square(pub u32);\n\
        impl Square {\n    pub fn with_side(side: u32) -> Square { Square(side) }\n}\n\
        impl Measure<u8> for Square {\n    fn area(&self, _x: u8) -> u32 { 9 }\n}\n\
        impl Measure<u16> for Square {}\n";
    let defaulted = "pub trait Measure<T = u8> {\n    fn area(&self, x: T) -> u32 { 0 }\n}\n\
        pub struct Square(pub u32);\n\
        impl Square {\n    pub fn with_side(side: u32) -> Square { Square(side) }\n}\n\
        impl Measure for Square {\n    fn area(&self, _x: u8) -> u32 { 9 }\n}\n\
        impl Measure<u16> for Square {}\n";
    let body = "let _ = Square::with_side(3).area(0);";
    assert!(direct(plain, "for Square::area", body)?);
    assert!(!direct(generic, "for Square::area", body)?);
    assert!(!direct(
        defaulted,
        "for Square::area",
        "let _ = Square::with_side(3).area(0u16);"
    )?);
    // A raw identifier declares the same trait the plain impl names.
    assert!(!direct(
        &defaulted.replacen("pub trait Measure", "pub trait r#Measure", 1),
        "for Square::area",
        "let _ = Square::with_side(3).area(0u16);"
    )?);
    Ok(())
}

/// An impl of the owner's trait under an alias may be for another
/// instantiation of the receiver's type, which the name-typed receiver cannot
/// tell apart, so the relation is withheld. The same impl under another
/// declared type's name leaves it.
#[test]
fn a_same_trait_impl_under_an_alias_withholds_the_relation() -> Result<(), String> {
    let other = "pub struct Other;\n\
        impl BufMut for Other {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n";
    let aliased = "pub struct Other;\npub type Tail = Other;\n\
        impl BufMut for Tail {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n";
    assert!(direct(
        &format!("{BUF}{other}"),
        BYTES_MUT_ADVANCE,
        TYPED_CALL
    )?);
    assert!(!direct(
        &format!("{BUF}{aliased}"),
        BYTES_MUT_ADVANCE,
        TYPED_CALL
    )?);
    Ok(())
}

/// A specialization of a generic type (`impl Shape for Square<u32>`) beside
/// a sibling impl that lists no method inherits the trait's default for
/// `Square<u8>`, which the name-typed receiver cannot tell apart, so the
/// relation is withheld. Without a default, every sibling impl lists the
/// method and the one specialization keeps it.
#[test]
fn a_specialization_beside_an_inheriting_sibling_is_withheld() -> Result<(), String> {
    let square = |declaration: &str, sibling: &str| {
        format!(
            "pub trait Shape {{\n    {declaration}\n}}\n\
             pub struct Square<T>(pub T);\n\
             impl<T> Square<T> {{\n    pub fn with_side(side: T) -> Square<T> {{ Square(side) }}\n}}\n\
             impl Shape for Square<u32> {{\n    fn area(&self) -> u32 {{ self.0 * self.0 }}\n}}\n\
             {sibling}"
        )
    };
    let owner = "impl Shape for Square<u32>::area";
    let body = "let _ = Square::with_side(3u32).area();";
    assert!(!direct(
        &square(
            "fn area(&self) -> u32 { 0 }",
            "impl Shape for Square<u8> {}\n"
        ),
        owner,
        body
    )?);
    assert!(direct(&square("fn area(&self) -> u32;", ""), owner, body)?);
    Ok(())
}

/// A type that dereferences to another type can run the target's method of
/// the name when the test does not have the owner's trait in scope, which
/// this does not read, so the relation is withheld.
#[test]
fn a_type_that_dereferences_withholds_the_relation() -> Result<(), String> {
    let production = format!(
        "{BUF}static EMPTY: Limit = Limit {{ left: 0 }};\n\
         impl std::ops::Deref for BytesMut {{\n    type Target = Limit;\n    fn deref(&self) -> &Limit {{ &EMPTY }}\n}}\n"
    );
    assert!(!direct(&production, BYTES_MUT_ADVANCE, TYPED_CALL)?);
    let through_an_alias = format!(
        "{BUF}static EMPTY: Limit = Limit {{ left: 0 }};\n\
         pub type View = BytesMut;\n\
         impl std::ops::Deref for View {{\n    type Target = Limit;\n    fn deref(&self) -> &Limit {{ &EMPTY }}\n}}\n"
    );
    assert!(!direct(&through_an_alias, BYTES_MUT_ADVANCE, TYPED_CALL)?);
    assert!(direct(BUF, BYTES_MUT_ADVANCE, TYPED_CALL)?);
    Ok(())
}

/// An owner taking `self` by value called on a receiver written as a borrow
/// starts probing at the borrow, where an impl for `&mut T` would take the
/// call first, so the relation is withheld, whatever whitespace or comment
/// sits between the tokens; the owned call keeps it.
#[test]
fn a_borrowed_receiver_of_a_by_value_owner_is_withheld() -> Result<(), String> {
    let production = "pub trait Shape {\n    fn area(self) -> u32;\n}\n\
        pub struct Square(pub u32);\n\
        impl Square {\n    pub fn with_side(side: u32) -> Square { Square(side) }\n}\n\
        impl Shape for Square {\n    fn area(self) -> u32 { self.0 * self.0 }\n}\n";
    let owner = "impl Shape for Square::area";
    assert!(!direct(
        production,
        owner,
        "let _ = (&mut Square::with_side(3)).area();"
    )?);
    assert!(!direct(
        production,
        owner,
        "let _ = (&mut Square::with_side(3))\n        . area ();"
    )?);
    assert!(!direct(
        production,
        owner,
        "let _ = (&mut Square::with_side(3)) /* the borrow */ .area();"
    )?);
    assert!(!direct(
        production,
        owner,
        "let _ = (&mut Square::with_side(3)). // the borrow\n        area();"
    )?);
    assert!(!direct(
        production,
        owner,
        "let _ = (( &mut Square::with_side(3) )).area();"
    )?);
    assert!(direct(
        production,
        owner,
        "let _ = Square::with_side(3).area();"
    )?);
    Ok(())
}

/// An impl covers every specialization of a generic type only when its
/// header lists distinct, unbounded parameters. `impl<T> BufMut for
/// Chain<T, T>` leaves `Chain<u8, u16>` to an impl that may list no method
/// and inherit the default, and a bound or `where` clause narrows the same
/// way, so each withholds the relation the unbounded header keeps.
#[test]
fn only_distinct_unbounded_parameters_cover_the_type() -> Result<(), String> {
    let chain = |header: &str| {
        format!(
            "pub trait BufMut {{\n    fn advance_mut(&mut self, _cnt: usize) {{}}\n}}\n\
             pub struct Chain<T, U> {{ a: T, b: U }}\n\
             impl<T, U> Chain<T, U> {{\n    pub fn pair(a: T, b: U) -> Chain<T, U> {{ Chain {{ a, b }} }}\n}}\n\
             {header} {{\n    fn advance_mut(&mut self, cnt: usize) {{ let _ = (cnt, &self.a, &self.b); }}\n}}\n"
        )
    };
    let body = "let mut c = Chain::pair(1u8, 2u16); c.advance_mut(1);";
    for (header, owner) in [
        (
            "impl<T> BufMut for Chain<T, T>",
            "impl BufMut for Chain<T, T>::advance_mut",
        ),
        (
            "impl<T: Copy, U> BufMut for Chain<T, U>",
            "impl BufMut for Chain<T, U>::advance_mut",
        ),
        (
            "impl<T, U> BufMut for Chain<T, U> where T: Copy",
            "impl BufMut for Chain<T, U>::advance_mut",
        ),
    ] {
        assert!(!direct(&chain(header), owner, body)?, "{header}");
    }
    assert!(direct(
        &chain("impl<T, U> BufMut for Chain<T, U>"),
        "impl BufMut for Chain<T, U>::advance_mut",
        body
    )?);
    // A bounded owner impl followed on the same line by an unbounded impl of
    // another trait: only the owner's own header is read.
    let same_line = "pub trait BufMut {\n    fn advance_mut(&mut self, _cnt: usize) {}\n}\n\
        pub trait Marker {}\n\
        pub struct Chain<T, U> { a: T, b: U }\n\
        impl<T, U> Chain<T, U> {\n    pub fn pair(a: T, b: U) -> Chain<T, U> { Chain { a, b } }\n}\n\
        impl<T: Copy, U> BufMut for Chain<T, U> { fn advance_mut(&mut self, cnt: usize) { let _ = (cnt, &self.a, &self.b); } } impl<T, U> Marker for Chain<T, U> {}\n";
    assert!(!direct(
        same_line,
        "impl BufMut for Chain<T, U>::advance_mut",
        body
    )?);
    Ok(())
}

/// A capitalised path segment on the impl's type counts as a variant
/// constructor for the typed trait receiver only when an `enum` of that name
/// declares the variant at variant position. `Tile::Make(3)` names no
/// variant of `Tile`, so it may be an associated function returning another
/// type, and the typed route withholds; a name used only as a field type
/// declares nothing. The guard is read directly: the older text receiver
/// route reads every capitalised segment as a variant on its own (#16040).
#[test]
fn a_variant_shaped_call_needs_a_declared_variant() -> Result<(), String> {
    let production = "pub trait Measure {\n    fn area(&self) -> u32;\n}\n\
        pub struct Make(pub u32);\n\
        pub enum Tile { Square(u32), Round(Make) }\n\
        impl Measure for Tile {\n    fn area(&self) -> u32 { match self { Tile::Square(s) => s * s, Tile::Round(m) => m.0 } }\n}\n";
    let provided = format!(
        "{production}pub struct Other(pub u32);\n\
         impl Measure for Other {{\n    fn area(&self) -> u32 {{ self.0 }}\n}}\n\
         pub trait Build {{\n    fn Make(n: u32) -> Other {{ Other(n) }}\n}}\n\
         impl Build for Tile {{}}\n"
    );
    let withheld = |production: &str, body: &str| -> Result<bool, String> {
        let index = parsed_index(production, body)?;
        let tests = index.tests();
        let Some(test) = tests.iter().next() else {
            return Err("the test is indexed".to_string());
        };
        Ok(super::a_variant_shaped_path_names_no_declared_variant(
            test, "Tile", &index,
        ))
    };
    assert!(!withheld(production, "let _ = Tile::Square(3).area();")?);
    assert!(!withheld(
        production,
        "let _ = Tile::Round(Make(3)).area();"
    )?);
    assert!(withheld(production, "let _ = Tile::Make(3).area();")?);
    assert!(withheld(&provided, "let _ = Tile::Make(3).area();")?);
    assert!(withheld(
        production,
        "let _ = Tile::Square(3).area(); let _ = Tile::Make(3).area();"
    )?);
    Ok(())
}

/// A changed-files index can leave out the module that declares a rival
/// method of the name, so the typed trait receiver reading credits a call
/// only over a complete index. The same index read as partial keeps the
/// name-only relation.
#[test]
fn a_partial_index_withholds_the_typed_trait_receiver() -> Result<(), String> {
    let index = parsed_index(BUF, TYPED_CALL)?;
    let functions = index.functions();
    let Some(owner) = functions
        .iter()
        .find(|function| function.id.0.ends_with(BYTES_MUT_ADVANCE))
    else {
        return Err("the owner is indexed".to_string());
    };
    let Some(test) = index.tests().iter().next() else {
        return Err("the test is indexed".to_string());
    };
    let same_name_ids: Vec<&str> = functions
        .iter()
        .filter(|function| function.name == owner.name)
        .map(|function| function.id.0.as_str())
        .collect();
    let source = index
        .files()
        .get(&test.file)
        .map(|facts| facts.data().source.clone());
    let relation = |workspace_complete| {
        super::super::owner_call_relation_reason_gated(
            test,
            Some(owner),
            &owner.name,
            &same_name_ids,
            source.as_deref(),
            &index,
            false,
            false,
            workspace_complete,
        )
    };
    assert_eq!(
        relation(true),
        super::super::RelationReason::DirectOwnerCall
    );
    assert_eq!(
        relation(false),
        super::super::RelationReason::WeakTokenSubstring
    );
    Ok(())
}

/// A macro whose expansion writes `impl Tile { .. }` for `type Tile =
/// BytesMut;` adds a method to `BytesMut` without naming it. When that
/// expansion can hold the owner's name, the relation is withheld; a macro
/// writing only another name leaves it.
#[test]
fn a_macro_writing_through_an_alias_withholds_the_relation() -> Result<(), String> {
    let production = |method: &str| {
        format!(
            "{BUF}pub type Tile = BytesMut;\n\
             macro_rules! add_method {{\n    () => {{\n        impl Tile {{\n            pub fn {method}(&mut self, _cnt: usize) {{}}\n        }}\n    }};\n}}\n\
             add_method!();\n"
        )
    };
    assert!(!direct(
        &production("advance_mut"),
        BYTES_MUT_ADVANCE,
        TYPED_CALL
    )?);
    assert!(direct(
        &production("reserve"),
        BYTES_MUT_ADVANCE,
        TYPED_CALL
    )?);
    Ok(())
}
