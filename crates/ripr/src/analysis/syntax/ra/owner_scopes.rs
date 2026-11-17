//! Attribute-carrying scopes that enclose one function (#13910).
//!
//! cargo-mutants reads `#[mutants::skip]` on a function, on an enclosing
//! `impl` or `trait` block, on an enclosing module and as an inner
//! `#![mutants::skip]` at the top of a file, and mutates nothing inside a
//! skipped scope. This module only reports which scopes enclose a function
//! and the attribute metadata each one carries; the owner opt-out decides
//! whether any of it is a skip.
//!
//! Ancestry comes from the parsed syntax tree, never from text scanning. A
//! source that does not parse cleanly, and a function the tree does not name
//! exactly once, yields no scopes at all, so nothing is withheld (the
//! direction that must fail closed).

use super::{LineIndex, impl_owner_segment, parse_clean_source_file};
use ra_ap_syntax::{
    AstNode,
    ast::{self, HasAttrs, HasName},
};

/// The kind of scope an owner opt-out attribute was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EnclosingScopeKind {
    /// The function itself (an inner `#![..]` in its body) or a function
    /// whose body declares it.
    Fn,
    /// An `impl` block.
    Impl,
    /// A `trait` block.
    Trait,
    /// An inline `mod name { .. }`.
    Module,
    /// The whole file, through an inner `#![..]` attribute.
    File,
}

/// One scope enclosing a function, innermost first, with its attributes
/// rewritten as outer `#[<meta>]` text so the shared attribute recognizer
/// reads inner and outer forms alike.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EnclosingScope {
    pub(crate) kind: EnclosingScopeKind,
    /// The scope's name as written: an enclosing function's, trait's or
    /// module's name, the impl's `Type` or `Trait for Type`; empty for the
    /// function's own inner attributes and for the file.
    pub(crate) label: String,
    pub(crate) attrs: Vec<String>,
}

/// The scopes enclosing each `(name, fn_line)` owner of `source`, in input
/// order. `fn_line` is the 1-based line of the `fn` keyword, the line the
/// parser producer records as a function's `start_line`.
///
/// The function's own outer attributes are not repeated here (they are
/// already on its fact), but its inner attributes are, as an
/// [`EnclosingScopeKind::Fn`] scope. Returns `None` for every owner when the
/// source does not parse cleanly, and `None` for an owner the tree does not
/// name exactly once.
pub(crate) fn enclosing_scopes(
    source: &str,
    owners: &[(&str, usize)],
) -> Vec<Option<Vec<EnclosingScope>>> {
    let Some(parse) = parse_clean_source_file(source) else {
        return vec![None; owners.len()];
    };
    let tree = parse.tree();
    let lines = LineIndex::new(source);
    let functions: Vec<(String, usize, ast::Fn)> = tree
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter_map(|function| {
            let name = function.name()?.text().to_string();
            let start = function
                .fn_token()
                .map(|token| token.text_range().start())
                .unwrap_or_else(|| function.syntax().text_range().start());
            Some((name, lines.line(start), function))
        })
        .collect();
    owners
        .iter()
        .map(|(name, line)| {
            let mut matches = functions.iter().filter(|(candidate, candidate_line, _)| {
                candidate.as_str() == *name && candidate_line == line
            });
            let (_, _, function) = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            Some(scopes_of(function))
        })
        .collect()
}

fn scopes_of(function: &ast::Fn) -> Vec<EnclosingScope> {
    let mut scopes = Vec::new();
    let own_inner: Vec<String> = ast::attrs_including_inner(function)
        .filter(|attr| attr.kind().is_inner())
        .filter_map(|attr| outer_form(&attr))
        .collect();
    if !own_inner.is_empty() {
        scopes.push(EnclosingScope {
            kind: EnclosingScopeKind::Fn,
            label: String::new(),
            attrs: own_inner,
        });
    }
    for ancestor in function.syntax().ancestors().skip(1) {
        let scope = if let Some(item) = ast::Fn::cast(ancestor.clone()) {
            scope(EnclosingScopeKind::Fn, name_text(item.name()), &item)
        } else if let Some(item) = ast::Impl::cast(ancestor.clone()) {
            let segment = impl_owner_segment(&item);
            let label = segment
                .strip_prefix("impl ")
                .unwrap_or(&segment)
                .to_string();
            scope(EnclosingScopeKind::Impl, label, &item)
        } else if let Some(item) = ast::Trait::cast(ancestor.clone()) {
            scope(EnclosingScopeKind::Trait, name_text(item.name()), &item)
        } else if let Some(item) = ast::Module::cast(ancestor.clone()) {
            scope(EnclosingScopeKind::Module, name_text(item.name()), &item)
        } else if let Some(file) = ast::SourceFile::cast(ancestor.clone()) {
            scope(EnclosingScopeKind::File, String::new(), &file)
        } else {
            continue;
        };
        if !scope.attrs.is_empty() {
            scopes.push(scope);
        }
    }
    scopes
}

fn scope(kind: EnclosingScopeKind, label: String, item: &dyn HasAttrs) -> EnclosingScope {
    EnclosingScope {
        kind,
        label,
        attrs: ast::attrs_including_inner(item)
            .filter_map(|attr| outer_form(&attr))
            .collect(),
    }
}

fn name_text(name: Option<ast::Name>) -> String {
    name.map(|name| name.text().to_string()).unwrap_or_default()
}

/// `#[<meta>]` for an outer or inner attribute; `None` when the attribute
/// has no metadata node.
fn outer_form(attr: &ast::Attr) -> Option<String> {
    attr.meta()
        .map(|meta| format!("#[{}]", meta.syntax().text()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str, owner: (&str, usize)) -> Option<Vec<(EnclosingScopeKind, String)>> {
        enclosing_scopes(source, &[owner])
            .into_iter()
            .next()
            .flatten()
            .map(|scopes| {
                scopes
                    .into_iter()
                    .map(|scope| (scope.kind, scope.label))
                    .collect()
            })
    }

    #[test]
    fn reads_impl_trait_module_and_file_scopes_innermost_first() {
        let source = "#![no_std]\n#[cfg(any())]\nmod outer {\n    #[doc = \"x\"]\n    impl Cart {\n        fn limit(&self) {}\n    }\n    trait Shape {\n        #![mutants::skip]\n        fn area(&self) {}\n    }\n}\n";
        assert_eq!(
            kinds(source, ("limit", 6)),
            Some(vec![
                (EnclosingScopeKind::Impl, "Cart".to_string()),
                (EnclosingScopeKind::Module, "outer".to_string()),
                (EnclosingScopeKind::File, String::new()),
            ])
        );
        assert_eq!(
            kinds(source, ("area", 10)),
            Some(vec![
                (EnclosingScopeKind::Trait, "Shape".to_string()),
                (EnclosingScopeKind::Module, "outer".to_string()),
                (EnclosingScopeKind::File, String::new()),
            ])
        );
    }

    #[test]
    fn inner_attributes_are_rewritten_to_outer_form() {
        let source = "mod a {\n    #![cfg_attr(test, mutants::skip)]\n    fn f() {}\n}\n";
        let scopes = enclosing_scopes(source, &[("f", 3)]);
        let attrs: Vec<String> = scopes
            .into_iter()
            .flatten()
            .flatten()
            .flat_map(|scope| scope.attrs)
            .collect();
        assert_eq!(attrs, vec!["#[cfg_attr(test, mutants::skip)]".to_string()]);
    }

    #[test]
    fn unparsable_source_and_ambiguous_owner_yield_nothing() {
        assert_eq!(
            kinds("#[mutants::skip]\nimpl X {\n    fn f( {}\n}\n", ("f", 3)),
            None
        );
        // Two `f` on one line: the owner is not named exactly once.
        assert_eq!(
            kinds("mod a { fn f() {} } mod b { fn f() {} }\n", ("f", 1)),
            None
        );
        // A line with no such function.
        assert_eq!(kinds("fn f() {}\n", ("f", 2)), None);
    }
}
