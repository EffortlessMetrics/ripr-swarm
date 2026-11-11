//! Sibling impls of one generic trait on one type (#14074).
//!
//! `impl From<u32> for Cents` and `impl From<&str> for Cents` share a self
//! type and a method name, so a relation that resolves a call by self type
//! alone credits `Cents::from(2u32)` to both. The trait argument is what
//! selects the impl, and for these traits it is the type of the call's first
//! non-`self` argument.
//!
//! This reading only refuses, and only on positive evidence: every call of the
//! name in the test passes an argument whose type is written in the test
//! (a suffixed literal, a string literal, an annotated or literal-initialised
//! `let`, a struct, tuple-struct or variant literal) and that type is a
//! sibling's trait argument, not the owner's. An argument whose type would
//! need inference keeps the relation the caller already had, so an unreadable
//! call never turns into a new gap.

use super::{
    FunctionContainer, FunctionSummary, RustIndex, TestSummary, compact_impl_type_name,
    first_argument_has_type, first_argument_span, ident_boundary, is_ident_byte,
    mask_comments_and_strings, skip_ws_if_paren,
};
use crate::analysis::classify::transitive_reach::IMPLICIT_DISPATCH_METHODS;
use crate::analysis::syntax::fn_signature::{OwnerContainer, owner_signature_at_line};

/// Whether every call of `owner`'s name in `test` provably selects a sibling
/// impl of the same generic trait on the same type, so none of them can run
/// `owner`. `false` whenever any part of that is not established.
pub(super) fn every_call_selects_generic_sibling(
    test: &TestSummary,
    owner: &FunctionSummary,
    same_name_ids: &[&str],
    index: &RustIndex,
) -> bool {
    let FunctionContainer::TraitImpl {
        trait_path,
        self_ty,
    } = &owner.item.container
    else {
        return false;
    };
    let Some(owner_argument) = sole_trait_argument(trait_path) else {
        return false;
    };
    let Some(owner_head) = selecting_type_head(owner_argument) else {
        return false;
    };
    let (Some(trait_base), Some(self_base)) = (
        compact_impl_type_name(trait_path),
        compact_impl_type_name(self_ty),
    ) else {
        return false;
    };
    let sibling_heads = sibling_argument_heads(owner, same_name_ids, &trait_base, &self_base);
    // A sibling that reads as the owner's own argument (`From<Foo>` beside
    // `From<&Foo>`) is told apart only by a reference this reading drops.
    if sibling_heads.is_empty() || sibling_heads.contains(&owner_head) {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    if has_unnamed_route(owner.name.as_str(), &masked)
        || !every_site_passes_sibling_type(
            test,
            owner,
            &masked,
            &sibling_heads,
            owner_argument.starts_with('&'),
        )
    {
        return false;
    }
    // Last, because it parses the owner's file: the owner's first non-`self`
    // parameter must be the trait argument itself, in an impl and a function
    // that declare no type generics, so the argument's type is the selector.
    owner_first_parameter_head(owner, index).is_some_and(|head| head == owner_head)
}

/// The single generic argument of a trait path (`From<&str>` → `&str`).
/// `None` for a plain trait, two or more arguments, or an associated-type
/// binding.
fn sole_trait_argument(trait_path: &str) -> Option<&str> {
    let open = trait_path.find('<')?;
    let inner = trait_path[open + 1..].trim_end().strip_suffix('>')?;
    let mut depth = 0usize;
    for byte in inner.bytes() {
        match byte {
            b'<' | b'(' | b'[' => depth += 1,
            b'>' | b')' | b']' => depth = depth.checked_sub(1)?,
            b',' | b'=' if depth == 0 => return None,
            _ => {}
        }
    }
    Some(inner.trim())
}

/// The last path segment of a type written as a plain path, with one leading
/// reference (and its lifetime and `mut`) dropped: `&'a str` → `str`,
/// `units::Meters` → `Meters`. `None` for anything with generic arguments,
/// a tuple, a slice, a pointer, `dyn`/`impl`, or a second reference: those
/// have no single head a call-site argument could be compared with.
fn selecting_type_head(ty: &str) -> Option<String> {
    let mut ty = ty.trim();
    if let Some(rest) = ty.strip_prefix('&') {
        ty = rest.trim_start();
        if let Some(rest) = ty.strip_prefix('\'') {
            let lifetime_end = rest
                .bytes()
                .position(|byte| !is_ident_byte(byte))
                .unwrap_or(rest.len());
            ty = rest[lifetime_end..].trim_start();
        }
        if let Some(rest) = ty.strip_prefix("mut ") {
            ty = rest.trim_start();
        }
    }
    let head = ty.rsplit("::").next()?;
    let plain_path = ty
        .split("::")
        .all(|segment| !segment.is_empty() && segment.bytes().all(is_ident_byte));
    plain_path.then(|| head.to_string())
}

/// Trait-argument heads of the other impls of `trait_base` on `self_base`
/// in the owner's file that define the owner's method. An impl in another
/// file may be on another type of the same name, and one whose argument has
/// no plain head contributes nothing: no call-site type can be matched
/// against it.
fn sibling_argument_heads(
    owner: &FunctionSummary,
    same_name_ids: &[&str],
    trait_base: &str,
    self_base: &str,
) -> Vec<String> {
    let mut heads = Vec::new();
    let owner_file = owner.id.0.split("::impl ").next();
    for id in same_name_ids {
        if *id == owner.id.0 {
            continue;
        }
        let mut parts = id.split("::impl ");
        if parts.next() != owner_file {
            continue;
        }
        let Some((trait_ty, self_ty)) = parts
            .next()
            .and_then(|rest| rest.rsplit_once("::"))
            .and_then(|(impl_body, _)| impl_body.rsplit_once(" for "))
        else {
            continue;
        };
        if compact_impl_type_name(trait_ty).as_deref() != Some(trait_base)
            || compact_impl_type_name(self_ty).as_deref() != Some(self_base)
        {
            continue;
        }
        if let Some(head) = sole_trait_argument(trait_ty).and_then(selecting_type_head)
            && !heads.contains(&head)
        {
            heads.push(head);
        }
    }
    heads
}

/// Whether the test may run the owner without naming it. A method the
/// language or the standard library calls through syntax is left to the
/// readings that own that syntax; `from` and `try_from` are also reached by
/// `into`/`try_into`, and `from` by `?`.
fn has_unnamed_route(name: &str, masked: &str) -> bool {
    let names_word = |word: &str| {
        masked
            .match_indices(word)
            .any(|(at, matched)| ident_boundary(masked, at, at + matched.len()))
    };
    match name {
        "from" => names_word("into") || masked.contains('?'),
        "try_from" => names_word("try_into"),
        _ => IMPLICIT_DISPATCH_METHODS.contains(&name),
    }
}

/// Whether the test names the owner's method at least once and every mention
/// is a call, in the spelling whose first argument is the selecting one,
/// passing a value typed as one of `sibling_heads`. A method with `self` is
/// read at `value.name(arg)` only (`Trait::name(value, arg)` puts the
/// receiver first); an associated function at `Path::name(arg)` only. A
/// mention that is not a call (`map(Cents::from)`), a turbofish, or the other
/// spelling is not read, so the answer is `false`.
///
/// `owner_takes_reference`: the owner's parameter is a reference, which a
/// borrowed argument of another type can reach by deref coercion
/// (`&String` for `&str`), so a borrowed argument is not read either.
fn every_site_passes_sibling_type(
    test: &TestSummary,
    owner: &FunctionSummary,
    masked: &str,
    sibling_heads: &[String],
    owner_takes_reference: bool,
) -> bool {
    let name = owner.name.as_str();
    let bytes = masked.as_bytes();
    let mut sites = 0usize;
    for (at, matched) in masked.match_indices(name) {
        let after = at + matched.len();
        if !ident_boundary(masked, at, after) {
            continue;
        }
        let before = masked[..at].trim_end();
        // A nested `fn` of this name rebinds it for the rest of the body.
        if before.ends_with("fn") && ident_boundary(before, before.len() - 2, before.len()) {
            return false;
        }
        if !skip_ws_if_paren(masked, after) {
            return false;
        }
        let method_site = at > 0 && bytes[at - 1] == b'.';
        if method_site != owner.item.has_self_param {
            return false;
        }
        if owner_takes_reference
            && first_argument_span(masked, after)
                .and_then(|(start, end)| masked.get(start..end))
                .is_none_or(|argument| argument.trim_start().starts_with('&'))
        {
            return false;
        }
        let lets_before = masked[..at].rfind(';').map_or("", |end| &masked[..=end]);
        let passes_sibling = sibling_heads.iter().any(|head| {
            first_argument_has_type(masked, after, head, lets_before, masked, &test.body, at)
        }) || (sibling_heads.iter().any(|head| head == "str")
            && first_argument_is_string_literal(masked, &test.body, after));
        if !passes_sibling {
            return false;
        }
        sites += 1;
    }
    sites > 0
}

/// Whether the first argument of the call whose name ends at `after_name` is
/// one plain string literal (`"abc"`). The masked body blanks
/// the literal's contents, so anything left between its quotes other than
/// whitespace means the argument is a larger expression.
fn first_argument_is_string_literal(masked: &str, raw: &str, after_name: usize) -> bool {
    let Some((start, end)) = first_argument_span(masked, after_name) else {
        return false;
    };
    let (Some(raw_arg), Some(masked_arg)) = (raw.get(start..end), masked.get(start..end)) else {
        return false;
    };
    let lead = raw_arg.len() - raw_arg.trim_start().len();
    let trail = raw_arg.len() - raw_arg.trim_end().len();
    let raw_arg = raw_arg.trim();
    if raw_arg.len() < 2 || !raw_arg.starts_with('"') || !raw_arg.ends_with('"') {
        return false;
    }
    masked_arg
        .get(lead + 1..masked_arg.len() - trail - 1)
        .is_some_and(|inner| inner.chars().all(char::is_whitespace))
}

/// Head of the owner's first non-`self` parameter type, when the owner sits
/// in a trait impl that declares no type generics and is not itself generic.
/// The index's copy of the owner's file is parsed, so this is asked last.
fn owner_first_parameter_head(owner: &FunctionSummary, index: &RustIndex) -> Option<String> {
    let files = index.files();
    let source = files.get(&owner.file)?.data().source.as_ref();
    let signature = owner_signature_at_line(source, owner.start_line)?;
    if signature.name != owner.name
        || signature.has_type_generics
        || signature.receiver.is_some() != owner.item.has_self_param
        || !matches!(signature.container, OwnerContainer::TraitImpl { .. })
    {
        return None;
    }
    selecting_type_head(&signature.params.first()?.ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    use std::path::{Path, PathBuf};

    const CENTS: &str = "#[derive(Debug, PartialEq)]\npub struct Cents(pub u64);\npub struct Foo(pub u64);\n\
        impl From<u32> for Cents {\n    fn from(v: u32) -> Self { Cents(v as u64 * 100) }\n}\n\
        impl From<&str> for Cents {\n    fn from(v: &str) -> Self { Cents(v.len() as u64 + 3) }\n}\n";

    const SCALE: &str = "pub trait Scale<T> { fn scale(&self, by: T) -> u32; }\npub struct Bar(pub u32);\n\
        impl Scale<u8> for Bar {\n    fn scale(&self, by: u8) -> u32 { self.0 * by as u32 + 1 }\n}\n\
        impl Scale<f32> for Bar {\n    fn scale(&self, by: f32) -> u32 { (self.0 as f32 * by) as u32 + 2 }\n}\n";

    /// A parser-backed index of `production` followed by one test whose body
    /// is `test_body`, so the owner's container, signature and symbol id are
    /// the ones the real pipeline produces.
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

    /// Whether the full owner-call relation `ripr check` applies credits the
    /// fixture's one test as a direct call of the owner whose symbol id ends
    /// in `owner_suffix`.
    fn direct(production: &str, owner_suffix: &str, test_body: &str) -> Result<bool, String> {
        let index = parsed_index(production, test_body)?;
        let functions = index.functions();
        let mut owners = functions
            .iter()
            .filter(|function| function.id.0.ends_with(owner_suffix));
        let (Some(owner), None) = (owners.next(), owners.next()) else {
            return Err(format!("exactly one owner ends in `{owner_suffix}`"));
        };
        if !matches!(owner.item.container, FunctionContainer::TraitImpl { .. }) {
            return Err(format!("`{owner_suffix}` is a parsed trait-impl method"));
        }
        let tests = index.tests();
        let (Some(test), None) = (tests.iter().next(), tests.iter().nth(1)) else {
            return Err("exactly one test is indexed".to_string());
        };
        if !test.body.contains(owner.name.as_str()) {
            return Err(format!("the test body names `{}`", owner.name));
        }
        Ok(super::super::owner_call_is_direct(test, owner, &index))
    }

    /// #14074, the issue's repro in both directions: the only call passes a
    /// value written as the sibling impl's trait argument.
    #[test]
    fn a_call_typed_as_the_sibling_argument_is_not_a_direct_call_of_the_owner() -> Result<(), String>
    {
        for (owner, body) in [
            (
                "impl From<&str> for Cents::from",
                "assert_eq!(Cents::from(2u32), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "let n: u32 = 2; assert_eq!(Cents::from(n), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                "assert_eq!(Cents::from(2u32), Cents(200)); assert_eq!(Cents::from(1u32), Cents(100));",
            ),
            (
                "impl From<u32> for Cents::from",
                r#"assert_eq!(Cents::from("abc"), Cents(6));"#,
            ),
        ] {
            assert!(!direct(CENTS, owner, body)?, "{owner} <- {body}");
        }
        for (owner, body) in [
            (
                "impl Scale<u8> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2.0f32), 6);",
            ),
            (
                "impl Scale<f32> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2u8), 5);",
            ),
        ] {
            assert!(!direct(SCALE, owner, body)?, "{owner} <- {body}");
        }
        Ok(())
    }

    /// The same fixtures keep the direct call when a call passes the owner's
    /// own argument type, alone or beside a sibling call.
    #[test]
    fn a_call_typed_as_the_owner_argument_stays_a_direct_call() -> Result<(), String> {
        for (owner, body) in [
            (
                "impl From<u32> for Cents::from",
                "assert_eq!(Cents::from(2u32), Cents(200));",
            ),
            (
                "impl From<&str> for Cents::from",
                r#"assert_eq!(Cents::from("abc"), Cents(6));"#,
            ),
            (
                "impl From<u32> for Cents::from",
                r#"assert_eq!(Cents::from("abc"), Cents(6)); assert_eq!(Cents::from(2u32), Cents(200));"#,
            ),
            (
                "impl From<&str> for Cents::from",
                r#"assert_eq!(Cents::from(2u32), Cents(200)); assert_eq!(Cents::from("abc"), Cents(6));"#,
            ),
        ] {
            assert!(direct(CENTS, owner, body)?, "{owner} <- {body}");
        }
        for (owner, body) in [
            (
                "impl Scale<u8> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2u8), 5);",
            ),
            (
                "impl Scale<f32> for Bar::scale",
                "let b = Bar(2); assert_eq!(b.scale(2.0f32), 6);",
            ),
        ] {
            assert!(direct(SCALE, owner, body)?, "{owner} <- {body}");
        }
        Ok(())
    }

    /// Whether this reading alone refuses the fixture's one test for the
    /// owner whose symbol id ends in `owner_suffix`, given every indexed
    /// function of the owner's name.
    fn refused(production: &str, owner_suffix: &str, test_body: &str) -> Result<bool, String> {
        let index = parsed_index(production, test_body)?;
        let functions = index.functions();
        let owner = functions
            .iter()
            .find(|function| function.id.0.ends_with(owner_suffix))
            .ok_or_else(|| format!("owner `{owner_suffix}` indexed"))?;
        let ids: Vec<&str> = functions
            .iter()
            .filter(|function| function.name == owner.name)
            .map(|function| function.id.0.as_str())
            .collect();
        if ids.len() < 2 {
            return Err(format!("`{}` has a same-named sibling", owner.name));
        }
        let tests = index.tests();
        let test = tests.iter().next().ok_or("test indexed")?;
        Ok(every_call_selects_generic_sibling(
            test, owner, &ids, &index,
        ))
    }

    /// An argument whose type the test does not write and an unnamed route
    /// keep the relation the caller already had: nothing here turns an
    /// unreadable call into a gap.
    #[test]
    fn an_unreadable_call_keeps_the_direct_call() -> Result<(), String> {
        for body in [
            // Inferred: an unsuffixed literal and an untyped binding.
            "assert_eq!(Cents::from(2), Cents(200));",
            "let n = 2; assert_eq!(Cents::from(n), Cents(200));",
            // A larger expression that only starts or ends with a literal.
            r#"assert_eq!(Cents::from("abc".len() as u32), Cents(300));"#,
            r#"assert_eq!(Cents::from(if true { "a" } else { "b" }), Cents(4));"#,
            // `into` and `?` run `from` without naming it.
            r#"let c: Cents = 2u32.into(); assert_eq!(c, Cents(200)); assert_eq!(Cents::from("abc"), Cents(6));"#,
            r#"let r: Result<u32, ()> = (|| { let v = Ok::<u32, ()>(2)?; Ok(v) })(); assert_eq!(Cents::from("abc"), Cents(6)); assert!(r.is_ok());"#,
        ] {
            assert!(
                direct(CENTS, "impl From<u32> for Cents::from", body)?,
                "{body}"
            );
        }
        // `Trait::name(value, arg)` puts the receiver first, and an untyped
        // binding needs inference.
        for body in [
            "let b = Bar(2); assert_eq!(Scale::scale(&b, 2.0f32), 6);",
            "let b = Bar(2); let by = 2.0; assert_eq!(b.scale(by), 6);",
        ] {
            assert!(
                !refused(SCALE, "impl Scale<u8> for Bar::scale", body)?,
                "{body}"
            );
        }
        Ok(())
    }

    /// A mention that is not a call in the read spelling leaves the answer to
    /// the caller: a function reference, a method spelling of an associated
    /// function, a turbofish, a nested `fn` of the name, and no mention at
    /// all. The control is the same owner with a readable sibling call.
    #[test]
    fn a_mention_that_is_not_a_readable_call_is_not_refused() -> Result<(), String> {
        let owner = "impl From<u32> for Cents::from";
        for body in [
            r#"let all: Vec<Cents> = ["a"].into_iter().map(Cents::from).collect(); assert_eq!(all.len(), 1);"#,
            r#"let all: Vec<Cents> = ["a"].into_iter().map(Cents::from).collect(); assert_eq!(Cents::from("abc"), all[0]);"#,
            r#"assert_eq!(Cents(6), "abc".from("abc"));"#,
            r#"assert_eq!(<Cents as From<&str>>::from::<>("abc"), Cents(6));"#,
            r#"fn from(v: &str) -> Cents { Cents(v.len() as u64) } assert_eq!(from("abc"), Cents(3));"#,
            "assert_eq!(Cents(6), Cents(6));",
        ] {
            assert!(!refused(CENTS, owner, body)?, "{body}");
        }
        assert!(refused(
            CENTS,
            owner,
            r#"assert_eq!(Cents::from("abc"), Cents(6));"#
        )?);
        Ok(())
    }

    /// The reading needs the trait argument to be the first parameter's type
    /// in a non-generic impl and function, a sibling whose head differs from
    /// the owner's, and a method syntax does not run unnamed.
    #[test]
    fn a_shape_whose_argument_is_not_the_selector_keeps_the_direct_call() -> Result<(), String> {
        // The trait argument is the return type; the first parameter is `u32`
        // in both impls, so the argument selects nothing.
        let returns = "pub trait Convert<T> { fn convert(&self, scale: u32) -> T; }\npub struct Bar(pub u32);\n\
            impl Convert<u32> for Bar {\n    fn convert(&self, scale: u32) -> u32 { self.0 * scale }\n}\n\
            impl Convert<f64> for Bar {\n    fn convert(&self, scale: u32) -> f64 { (self.0 * scale) as f64 }\n}\n";
        assert!(direct(
            returns,
            "impl Convert<f64> for Bar::convert",
            "let b = Bar(2); let f: f64 = b.convert(2u32); assert_eq!(f, 4.0);",
        )?);
        // `From<Foo>` beside `From<&Foo>`: the dropped reference is the only
        // difference.
        let by_ref = "pub struct Foo(pub u64);\npub struct Cents(pub u64);\n\
            impl From<Foo> for Cents {\n    fn from(v: Foo) -> Self { Cents(v.0) }\n}\n\
            impl From<&Foo> for Cents {\n    fn from(v: &Foo) -> Self { Cents(v.0 + 1) }\n}\n";
        assert!(direct(
            by_ref,
            "impl From<&Foo> for Cents::from",
            "assert_eq!(Cents::from(Foo(1)).0, 1);",
        )?);
        // A generic owner impl: `T` is not a type a call-site value names.
        let generic = "pub trait Scale<T> { fn scale(&self, by: T) -> u32; }\npub struct Bar<K>(pub K);\n\
            impl<T: Copy> Scale<T> for Bar<u8> {\n    fn scale(&self, _by: T) -> u32 { 1 }\n}\n\
            impl Scale<f32> for Bar<u16> {\n    fn scale(&self, _by: f32) -> u32 { 2 }\n}\n";
        assert!(direct(
            generic,
            "impl Scale<T> for Bar<u8>::scale",
            "let b = Bar(2u8); assert_eq!(b.scale(2.0f32), 1);",
        )?);
        // A borrowed argument can reach a reference parameter by deref
        // coercion, so it does not name the sibling.
        let coerced = "pub struct Name(pub u64);\n\
            impl From<&str> for Name {\n    fn from(v: &str) -> Self { Name(v.len() as u64) }\n}\n\
            impl From<String> for Name {\n    fn from(v: String) -> Self { Name(v.len() as u64 + 1) }\n}\n";
        assert!(direct(
            coerced,
            "impl From<&str> for Name::from",
            "let s: String = String::new(); assert_eq!(Name::from(&s).0, 0);",
        )?);
        // An operator method is left to the operator reading.
        let add = "use std::ops::Add;\n#[derive(Debug, PartialEq, Clone, Copy)]\npub struct Meters(pub u32);\n\
            impl Add<u32> for Meters {\n    type Output = Meters;\n    fn add(self, rhs: u32) -> Meters { Meters(self.0 + rhs) }\n}\n\
            impl Add<Meters> for Meters {\n    type Output = Meters;\n    fn add(self, rhs: Meters) -> Meters { Meters(self.0 + rhs.0) }\n}\n";
        assert!(direct(
            add,
            "impl Add<u32> for Meters::add",
            "let m = Meters(1); assert_eq!(m.add(Meters(2)), Meters(3));",
        )?);
        Ok(())
    }

    /// A same-named impl on another type is not a sibling: this reading has
    /// nothing to say about it.
    #[test]
    fn an_impl_of_the_trait_on_another_type_is_not_a_sibling() -> Result<(), String> {
        let production = "pub struct Cents(pub u64);\npub struct Dollars(pub u64);\n\
            impl From<&str> for Cents {\n    fn from(v: &str) -> Self { Cents(v.len() as u64) }\n}\n\
            impl From<u32> for Dollars {\n    fn from(v: u32) -> Self { Dollars(v as u64) }\n}\n";
        let mut index = parsed_index(production, "assert_eq!(Dollars::from(2u32).0, 2);")?;
        // A second file's `Cents` is another type of the same name: its
        // `From<u32>` is not a sibling of this file's `From<&str>` either.
        let other = "pub struct Cents(pub u64);\n\
            impl From<u32> for Cents {\n    fn from(v: u32) -> Self { Cents(v as u64) }\n}\n";
        let other_path = Path::new("src/other.rs");
        let other_facts = RaRustSyntaxAdapter.summarize_file(other_path, other)?;
        index.extend_functions(other_facts.functions.iter().cloned());
        index.insert_file_only(PathBuf::from(other_path), other_facts);
        let functions = index.functions();
        let owner = functions
            .iter()
            .find(|function| function.id.0.ends_with("impl From<&str> for Cents::from"))
            .ok_or("owner indexed")?;
        let ids: Vec<&str> = functions
            .iter()
            .filter(|function| function.name == "from")
            .map(|function| function.id.0.as_str())
            .collect();
        assert_eq!(ids.len(), 3, "all three `from` impls are indexed");
        let tests = index.tests();
        let test = tests.iter().next().ok_or("test indexed")?;
        assert!(!every_call_selects_generic_sibling(
            test, owner, &ids, &index
        ));
        Ok(())
    }

    #[test]
    fn sole_trait_argument_reads_exactly_one_type_argument() {
        for (path, expected) in [
            ("From<u32>", Some("u32")),
            ("From< &'a str >", Some("&'a str")),
            ("convert::From<Vec<u8>>", Some("Vec<u8>")),
            ("Scale<(u8, u8)>", Some("(u8, u8)")),
            ("Render", None),
            ("Pair<u8, u16>", None),
            ("Iterator<Item = u8>", None),
            ("From<u32", None),
        ] {
            assert_eq!(sole_trait_argument(path), expected, "{path}");
        }
    }

    #[test]
    fn selecting_type_head_reads_only_a_plain_path_behind_one_reference() {
        for (ty, expected) in [
            ("u32", Some("u32")),
            ("&str", Some("str")),
            ("&'static str", Some("str")),
            ("&'a mut Meters", Some("Meters")),
            ("& mut Meters", Some("Meters")),
            ("units::Meters", Some("Meters")),
            ("Vec<u8>", None),
            ("&&str", None),
            ("(u8, u8)", None),
            ("[u8]", None),
            ("&[u8]", None),
            ("*const u8", None),
            ("dyn Shape", None),
            ("&dyn Shape", None),
            ("impl Shape", None),
            ("", None),
        ] {
            assert_eq!(selecting_type_head(ty).as_deref(), expected, "{ty}");
        }
    }

    #[test]
    fn unnamed_routes_are_read_per_method_name() {
        assert!(has_unnamed_route("from", "let c: Cents = 2u32.into();"));
        assert!(has_unnamed_route("from", "let c = parse(x)?;"));
        assert!(!has_unnamed_route("from", "let c = Cents::from(2u32);"));
        // `into` inside a longer identifier is not the conversion.
        assert!(!has_unnamed_route(
            "from",
            "let c = Cents::from(into_cents);"
        ));
        assert!(has_unnamed_route(
            "try_from",
            "let c: Cents = 2u32.try_into().unwrap();"
        ));
        assert!(!has_unnamed_route(
            "try_from",
            "let c = Cents::try_from(2u32);"
        ));
        // A method syntax runs unnamed is never read here.
        assert!(has_unnamed_route("add", "let m = a.add(b);"));
        assert!(!has_unnamed_route("scale", "let m = a.scale(2u8);"));
    }
}
