//! `From::from` run by `.into()` (#15277).
//!
//! `let f: Fahrenheit = Celsius(100).into();` runs the `from` of
//! `impl From<Celsius> for Fahrenheit` without spelling `from`: the call is
//! the standard blanket `impl<T, U: From<T>> Into<U> for T`, whose `into` is
//! `U::from(self)`. The same test written `Fahrenheit::from(Celsius(100))` is
//! a direct owner call; this reading makes the `.into()` spelling read the
//! same way, and only where Rust provably dispatches it to the owner's impl.
//!
//! The reading follows the `str::parse` one ([`super::parse_dispatch`]) and
//! fails closed:
//!
//! - the owner is the `from` of a non-generic, top-level
//!   `impl From<S> for T` in its file, with plain type names `S` and `T`
//!   that the file declares once each as a struct or enum. `From` is the
//!   standard trait: no workspace file declares a `trait From` or imports a
//!   `From` by a non-standard path, and it is the only indexed `from` of a
//!   `From<S>` impl for `T`;
//! - the test sits in the owner's own file and no scope between the test and
//!   the file declares or renames another `S` or `T`;
//! - the call is a statement at the test's top level, written
//!   `let [mut] name: T = receiver.into();` with the bare target type as the
//!   annotation (the only target spelling read for now). An unannotated
//!   `.into()`, a function argument and `Into::<T>::into` relate nothing;
//! - the receiver's type is exactly `S`: a tuple or struct literal, a unit
//!   struct, an associated call whose declared return type is `S`, or a
//!   binding whose one `let` is built by one of those. A reference, a
//!   dereference or a binding written with `&` or `*` relates nothing,
//!   since `From<&S>` is another impl;
//! - nothing else can answer `.into()` on an `S`: no workspace file writes a
//!   `fn into` (an inherent `into` wins over the blanket `Into`, and a
//!   workspace `impl Into` must write one), and none opts out of the prelude
//!   (`no_implicit_prelude`);
//! - the workspace is closed: every source is indexed and parses, no
//!   `macro_rules!`, `extern crate`, `include!` or `#[path]`, every `use`
//!   and attribute path stays in the standard library or the workspace, and
//!   path dependencies close over the indexed packages with no proc macro
//!   (`element_field_pin::field_workspace_is_closed`). A proc macro or an
//!   outside crate could write an inherent `into` no indexed source shows.
//!
//! This only relates the test to the owner. Whether its assertion reveals a
//! wrong result stays with the ordinary oracle pipeline; nothing here raises
//! a class.

use super::equality_dispatch::{
    associated_fn_returns_self, count_items, impl_header, imports_other_type, operand_has_type,
    path_words, plain_type_name, std_trait_is_unshadowed, type_scope_is_clear, use_paths,
};
use super::implicit_dispatch::at_test_top_level;
use super::operator_dispatch::{file_is_clear, stringified_ranges};
use super::{
    FunctionSummary, RustIndex, TestSummary, ident_boundary, is_ident_byte,
    mask_comments_and_strings, matching_open,
};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasGenericParams, HasName},
};

/// The `.into()` reading of an owner.
pub(super) struct IntoOwner<'a> {
    /// The `From` impl's source type (`Celsius`).
    source_type: &'a str,
    /// The `From` impl's self type, the conversion target (`Fahrenheit`).
    target_type: &'a str,
    /// File-level and workspace refusals are the same for every test of the
    /// owner's file, so they are computed once per owner on first use.
    file_level_clear: std::cell::OnceCell<bool>,
}

/// The source type of a standard `From<S>` trait spelling, when `S` is a
/// plain type name (`From<Celsius>`, `std::convert::From<Celsius>`).
fn from_trait_source(trait_ty: &str) -> Option<&str> {
    let compact = trait_ty.trim();
    let base = compact
        .strip_prefix("std::convert::")
        .or_else(|| compact.strip_prefix("core::convert::"))
        .unwrap_or(compact);
    let source = base.strip_prefix("From<")?.strip_suffix('>')?.trim();
    plain_ident_type(source)
}

/// A single-segment type name with no generic arguments, lifetimes or
/// reference: the only spelling one `From` impl can be told apart by.
fn plain_ident_type(ty: &str) -> Option<&str> {
    (!ty.is_empty()
        && ty.bytes().all(is_ident_byte)
        && ty.starts_with(|c: char| c.is_ascii_uppercase()))
    .then_some(ty)
}

/// The source and target types of the `impl From<S> for T` whose `from` is
/// `owner`.
fn from_impl_types(owner: &FunctionSummary) -> Option<(&str, &str)> {
    (owner.name == "from").then_some(())?;
    let (trait_ty, self_ty) = impl_header(owner)?;
    let source = from_trait_source(trait_ty)?;
    let target = plain_ident_type(plain_type_name(self_ty)?)?;
    (source != target).then_some((source, target))
}

/// Whether `owner` is shaped like `from` of a plain `impl From<S> for T`. Id
/// text alone cannot tell the standard trait from a local one of the same
/// name, nor a generic impl from a plain one, so this is only the candidate
/// prefilter's superset test.
pub(super) fn has_into_shape(owner: &FunctionSummary) -> bool {
    from_impl_types(owner).is_some()
}

/// The `.into()` reading of `owner`, or `None` when it is not `from` of the
/// one plain impl of the standard `From<S>` for its type.
pub(super) fn into_owner<'a>(
    owner: &'a FunctionSummary,
    index: &RustIndex,
) -> Option<IntoOwner<'a>> {
    let (source_type, target_type) = from_impl_types(owner)?;
    if !std_trait_is_unshadowed(owner, index, "From") {
        return None;
    }
    let impls = index
        .functions()
        .iter()
        .filter(|function| from_impl_types(function) == Some((source_type, target_type)))
        .count();
    (impls == 1).then(|| IntoOwner {
        source_type,
        target_type,
        file_level_clear: std::cell::OnceCell::new(),
    })
}

/// Whether `test` converts a value of the owner's source type into its
/// target type through an annotated `.into()`.
pub(super) fn test_converts_into_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    into: &IntoOwner<'_>,
    test_source: Option<&str>,
    index: &RustIndex,
) -> bool {
    let Some(source) = test_source else {
        return false;
    };
    if test.file != owner.file {
        return false;
    }
    let masked = mask_comments_and_strings(&test.body);
    // Text inside a property-test macro is opaque to every other relation.
    let opaque: Vec<std::ops::Range<usize>> =
        crate::analysis::extract::property_macros::opaque_property_macros(&test.body)
            .into_iter()
            .map(|item| item.range)
            .collect();
    let stringified = stringified_ranges(&masked);
    let returns_source = |name: &str| associated_fn_returns_self(index, into.source_type, name);
    let reaching = annotated_into_sites(&masked, into.target_type)
        .into_iter()
        .filter(|site| {
            !opaque.iter().any(|range| range.contains(&site.let_at))
                && !stringified.iter().any(|range| range.contains(&site.let_at))
                && at_test_top_level(&masked, site.let_at)
        })
        .any(|site| {
            receiver_has_source_type(
                &masked,
                source,
                site.receiver,
                site.let_at,
                into.source_type,
                &returns_source,
            )
        });
    if !reaching
        || !type_scope_is_clear(test, owner, source, into.source_type, index)
        || !type_scope_is_clear(test, owner, source, into.target_type, index)
    {
        return false;
    }
    *into
        .file_level_clear
        .get_or_init(|| file_and_workspace_are_clear(source, owner, into, index))
}

/// One `let [mut] name: T = receiver.into();` statement.
#[derive(Debug, PartialEq, Eq)]
struct IntoSite<'a> {
    /// Offset of the statement's `let`.
    let_at: usize,
    /// The receiver expression, trimmed.
    receiver: &'a str,
}

/// Every `let [mut] name: T = receiver.into();` in `masked` whose
/// annotation is exactly `target`.
fn annotated_into_sites<'a>(masked: &'a str, target: &str) -> Vec<IntoSite<'a>> {
    let bytes = masked.as_bytes();
    let mut sites = Vec::new();
    for (at, _) in masked.match_indices("into") {
        let name_end = at + "into".len();
        if !ident_boundary(masked, at, name_end) {
            continue;
        }
        // A method call: a single `.` (not a `..` range) before the name.
        let before = masked[..at].trim_end();
        let Some(dot) = before.len().checked_sub(1) else {
            continue;
        };
        if bytes[dot] != b'.' || (dot > 0 && bytes[dot - 1] == b'.') {
            continue;
        }
        // `()` and the end of the statement: no turbofish, no argument and
        // no further method or operator on the converted value.
        let after = masked[name_end..].trim_start();
        let Some(after) = after.strip_prefix('(') else {
            continue;
        };
        let Some(after) = after.trim_start().strip_prefix(')') else {
            continue;
        };
        if !after.trim_start().starts_with(';') {
            continue;
        }
        let Some(receiver_start) = receiver_start(bytes, dot) else {
            continue;
        };
        let receiver = masked[receiver_start..dot].trim();
        if receiver.is_empty() {
            continue;
        }
        let Some(let_at) = annotated_let_before(masked, receiver_start, target) else {
            continue;
        };
        sites.push(IntoSite { let_at, receiver });
    }
    sites
}

/// Where the receiver that ends just before byte `dot` starts: an optional
/// path (`Celsius`, `Celsius::new`) followed by at most one balanced `(..)`
/// or `{..}` group. `None` when the bytes there are not such an expression.
fn receiver_start(bytes: &[u8], dot: usize) -> Option<usize> {
    let mut end = dot;
    while end > 0 && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    let last = *bytes.get(end.checked_sub(1)?)?;
    if matches!(last, b')' | b'}') {
        let open_byte = if last == b')' { b'(' } else { b'{' };
        end = matching_open(bytes, end - 1, open_byte, last)?;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
    }
    let mut start = end;
    while start > 0 && (is_ident_byte(bytes[start - 1]) || bytes[start - 1] == b':') {
        start -= 1;
    }
    (start < end).then_some(start)
}

/// The offset of `let` when the text before `receiver_start` is
/// `let [mut] name: target =` from the start of its statement.
fn annotated_let_before(masked: &str, receiver_start: usize, target: &str) -> Option<usize> {
    let head = masked[..receiver_start].trim_end();
    let head = head.strip_suffix('=')?;
    // `==`, `<=`, `+=` and the like are not the binding's `=`.
    if head
        .bytes()
        .last()
        .is_some_and(|byte| b"=<>!+-*/%&|^".contains(&byte))
    {
        return None;
    }
    let statement_start = head.rfind([';', '{', '}']).map_or(0, |at| at + 1);
    let statement = &head[statement_start..];
    let leading = statement.len() - statement.trim_start().len();
    let rest = statement.trim_start().strip_prefix("let")?;
    let rest = rest.strip_prefix(char::is_whitespace)?.trim_start();
    let rest = match rest.strip_prefix("mut") {
        Some(after) if after.starts_with(char::is_whitespace) => after.trim_start(),
        _ => rest,
    };
    let name_len = rest.bytes().take_while(|&byte| is_ident_byte(byte)).count();
    if name_len == 0 || rest.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let annotation = rest[name_len..].trim_start().strip_prefix(':')?;
    (annotation.trim() == target).then_some(statement_start + leading)
}

/// Whether `receiver` evaluates to a value of exactly `source_type`. `masked`
/// is the masked test body, `file_source` the unmasked file the test sits
/// in, and `let_at` the offset of the conversion's `let`.
fn receiver_has_source_type(
    masked: &str,
    file_source: &str,
    receiver: &str,
    let_at: usize,
    source_type: &str,
    returns_source: &dyn Fn(&str) -> bool,
) -> bool {
    // A reference or a dereference converts through another impl, and a
    // closure or a turbofish is not a value ripr reads.
    if receiver.starts_with(['&', '*']) || receiver.contains('|') || receiver.contains("::<") {
        return false;
    }
    let before = &masked[..let_at];
    if receiver == source_type {
        // Only a unit struct is its own value; a tuple struct's name is its
        // constructor function.
        return declares_unit_struct(&mask_comments_and_strings(file_source), source_type);
    }
    if receiver.bytes().all(is_ident_byte) {
        // A binding: its one `let` must spell no reference or dereference
        // and no block, so the head that types it is the whole value.
        let Some(statement) = binding_statement(before, receiver) else {
            return false;
        };
        if statement.contains(['&', '*', '{']) {
            return false;
        }
    }
    operand_has_type(receiver, source_type, before, returns_source)
}

/// The text of the one `let [mut] name` statement in `before`, from `let`
/// to its `;`; `None` when there is not exactly one.
fn binding_statement<'a>(before: &'a str, name: &str) -> Option<&'a str> {
    let mut found = None;
    let mut count = 0usize;
    for (at, _) in before.match_indices("let") {
        if !ident_boundary(before, at, at + 3) {
            continue;
        }
        let rest = before[at + 3..].trim_start();
        let rest = match rest.strip_prefix("mut") {
            Some(after) if after.starts_with(char::is_whitespace) => after.trim_start(),
            _ => rest,
        };
        if rest.starts_with(name) && !rest[name.len()..].bytes().next().is_some_and(is_ident_byte) {
            count += 1;
            let end = before[at..].find(';').map_or(before.len(), |end| at + end);
            found = Some(&before[at..end]);
        }
    }
    (count == 1).then_some(found).flatten()
}

/// Whether the masked source declares `struct name;`.
fn declares_unit_struct(masked: &str, name: &str) -> bool {
    let needle = format!("struct {name}");
    masked.match_indices(&needle).any(|(at, _)| {
        ident_boundary(masked, at, at + needle.len())
            && masked[at + needle.len()..].trim_start().starts_with(';')
    })
}

/// File-level and workspace refusals of the reading.
fn file_and_workspace_are_clear(
    source: &str,
    owner: &FunctionSummary,
    into: &IntoOwner<'_>,
    index: &RustIndex,
) -> bool {
    let masked = mask_comments_and_strings(source);
    let declared_once = |name: &str| {
        count_items(&masked, &["struct", "enum", "union"], name) == 1
            && count_items(&masked, &["type"], name) == 0
    };
    // `file_is_clear` refuses a `macro_rules!` here or an impl-writing one
    // anywhere, and an import that may bring another source type in.
    declared_once(into.source_type)
        && declared_once(into.target_type)
        && file_is_clear(source, owner, into.source_type, index)
        && !imports_other_type(&masked, into.target_type, &owner.file)
        && one_plain_top_level_impl(source, into.source_type, into.target_type)
        && !workspace_may_answer_into(index)
        && crate::analysis::classify::element_field_pin::field_workspace_is_closed(index)
}

/// Whether the file holds exactly one `impl From<S> for T`, written at its
/// top level with no generic parameters or `where` clause and a `fn from`.
/// The index drops an impl's generic parameters from the owner's id, so
/// `impl<S: Temp> From<S> for T` must be told apart by its syntax.
fn one_plain_top_level_impl(source: &str, source_type: &str, target_type: &str) -> bool {
    let Some(parse) = parse_clean_source_file(source) else {
        return false;
    };
    let compact = |node: &ra_ap_syntax::SyntaxNode| -> String {
        node.text()
            .to_string()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    let matching: Vec<ast::Impl> = parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Impl::cast)
        .filter(|item| {
            item.trait_().is_some_and(|trait_ty| {
                from_trait_source(&compact(trait_ty.syntax())) == Some(source_type)
            }) && item
                .self_ty()
                .is_some_and(|self_ty| compact(self_ty.syntax()) == target_type)
        })
        .collect();
    let [item] = matching.as_slice() else {
        return false;
    };
    item.syntax()
        .parent()
        .is_some_and(|parent| ast::SourceFile::can_cast(parent.kind()))
        && item.generic_param_list().is_none()
        && item.where_clause().is_none()
        && item.assoc_item_list().is_some_and(|items| {
            items.assoc_items().any(|assoc| {
                matches!(&assoc, ast::AssocItem::Fn(function)
                    if function.name().is_some_and(|name| name.text() == "from"))
            })
        })
}

/// Whether any workspace file may give a value another `into` than the
/// blanket `Into`, or rename some other trait to `From`: a `fn into` (an
/// inherent method, a workspace trait's method, a direct `impl Into`), a
/// `no_implicit_prelude` opt-out, or a `use` outside the standard library
/// that names `From`.
fn workspace_may_answer_into(index: &RustIndex) -> bool {
    index.files().values().any(|facts| {
        let source = facts.data().source.as_ref();
        if !source.contains("into") && !source.contains("From") {
            return false;
        }
        let masked = mask_comments_and_strings(source);
        writes_fn_into(&masked)
            || masked.contains("no_implicit_prelude")
            || use_paths(&masked).into_iter().any(|path| {
                let words = path_words(path);
                !matches!(words.first(), Some(&("std" | "core" | "alloc")))
                    && words.contains(&"From")
            })
    })
}

/// Whether the masked source writes `fn into` or `fn r#into`.
fn writes_fn_into(masked: &str) -> bool {
    masked.match_indices("fn").any(|(at, _)| {
        if !ident_boundary(masked, at, at + 2) {
            return false;
        }
        let rest = masked[at + 2..].trim_start();
        let rest = rest.strip_prefix("r#").unwrap_or(rest);
        rest.strip_prefix("into")
            .is_some_and(|after| !after.bytes().next().is_some_and(is_ident_byte))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        IntoSite, annotated_into_sites, binding_statement, declares_unit_struct, from_trait_source,
        writes_fn_into,
    };
    use crate::analysis::extract::mask_comments_and_strings;

    fn sites(body: &str) -> Vec<(String, String)> {
        let masked = mask_comments_and_strings(body);
        annotated_into_sites(&masked, "Fahrenheit")
            .into_iter()
            .map(|IntoSite { let_at, receiver }| {
                (
                    masked.get(let_at..let_at + 3).unwrap_or("").to_string(),
                    receiver.to_string(),
                )
            })
            .collect()
    }

    fn one(receiver: &str) -> Vec<(String, String)> {
        vec![("let".to_string(), receiver.to_string())]
    }

    #[test]
    fn an_annotated_into_statement_is_a_site() {
        assert_eq!(
            sites("let f: Fahrenheit = Celsius(100).into();"),
            one("Celsius(100)")
        );
        assert_eq!(
            sites("let mut f : Fahrenheit = Celsius { deg: 100 } . into ( ) ;"),
            one("Celsius { deg: 100 }")
        );
        assert_eq!(
            sites("let c = Celsius(1);\nlet f: Fahrenheit = c.into();"),
            one("c")
        );
        assert_eq!(
            sites("let f: Fahrenheit = Celsius::new(1).into();"),
            one("Celsius::new(1)")
        );
    }

    #[test]
    fn other_targets_and_spellings_are_not_sites() {
        assert!(sites("let f: Kelvin = Celsius(100).into();").is_empty());
        assert!(sites("let f = Celsius(100).into();").is_empty());
        assert!(sites("let f: Option<Fahrenheit> = Celsius(100).into();").is_empty());
        assert!(sites("show(Celsius(100).into());").is_empty());
        assert!(sites("let f: Fahrenheit = Celsius(100).into::<Fahrenheit>();").is_empty());
        assert!(sites("let f: Fahrenheit = Celsius(100).into().clone();").is_empty());
        assert!(sites("let f: Fahrenheit = Celsius(100).into_other();").is_empty());
        assert!(sites("let f: Fahrenheit = Celsius(100).into(x);").is_empty());
        assert!(sites("// let f: Fahrenheit = Celsius(100).into();").is_empty());
        assert!(sites("let s = \"let f: Fahrenheit = Celsius(1).into();\";").is_empty());
        assert!(sites("let (f, g): Fahrenheit = Celsius(100).into();").is_empty());
        assert!(sites("if a == Celsius(100).into();").is_empty());
    }

    #[test]
    fn a_receiver_is_one_path_and_one_group() {
        // `a + Celsius(1)` ends at the group's path, so the `+` is between
        // the `=` and the receiver and no annotated `let` is found.
        assert!(sites("let f: Fahrenheit = a + Celsius(1).into();").is_empty());
        assert!(sites("let f: Fahrenheit = (&c).into();").is_empty());
        assert_eq!(sites("let f: Fahrenheit = &c.into();"), Vec::new());
    }

    #[test]
    fn only_a_standard_from_of_a_plain_type_is_read() {
        assert_eq!(from_trait_source("From<Celsius>"), Some("Celsius"));
        assert_eq!(
            from_trait_source("std::convert::From<Celsius>"),
            Some("Celsius")
        );
        assert_eq!(
            from_trait_source("core::convert::From<Celsius>"),
            Some("Celsius")
        );
        assert_eq!(from_trait_source("From<&Celsius>"), None);
        assert_eq!(from_trait_source("From<Vec<u8>>"), None);
        assert_eq!(from_trait_source("From<u8>"), None);
        assert_eq!(from_trait_source("TryFrom<Celsius>"), None);
        assert_eq!(from_trait_source("local::From<Celsius>"), None);
    }

    #[test]
    fn a_binding_has_one_let_and_a_unit_struct_is_declared_with_a_semicolon() {
        assert_eq!(
            binding_statement("let c = Celsius(1); ", "c"),
            Some("let c = Celsius(1)")
        );
        assert_eq!(
            binding_statement("let c = Celsius(1); let c = d; ", "c"),
            None
        );
        assert_eq!(binding_statement("let cc = Celsius(1); ", "c"), None);
        assert!(declares_unit_struct("pub struct Boiling;", "Boiling"));
        assert!(!declares_unit_struct("pub struct Boiling(u8);", "Boiling"));
        assert!(!declares_unit_struct("pub struct BoilingPoint;", "Boiling"));
    }

    #[test]
    fn any_written_fn_into_is_found() {
        assert!(writes_fn_into(
            "impl Celsius { pub fn into(self) -> F { F(0) } }"
        ));
        assert!(writes_fn_into("trait MyInto { fn r#into(&self); }"));
        assert!(!writes_fn_into("fn into_inner(self) {}"));
        assert!(!writes_fn_into("let f: F = c.into();"));
    }
}
