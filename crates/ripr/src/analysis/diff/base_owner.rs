//! The verified base-side counterpart of a changed function (#14852, #14815).
//!
//! A diff pairs lines, not items. When two modules or impls swap their
//! functions, or `cfg` twins trade attributes, an unchanged
//! `pub fn ready() -> bool {` line pairs with the *other* function's header,
//! and the old body read at the origin-mapped span belongs to a different
//! item. A claim that compares a function's old and new body must first know
//! that the base-side function is the same item. This module answers that
//! once, for every such claim; it returns `None` whenever identity is not
//! established.

use super::{ChangedFile, reconstruct_old_lines_with_origins};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasName},
};
use std::collections::BTreeSet;

/// A function whose base-side item is verified to be the same item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BaseOwner {
    /// The new-side function item's text, outer attributes and doc comments
    /// included, and its 1-based first line in the new file.
    pub(crate) new_text: String,
    pub(crate) new_first_line: usize,
    /// The base-side function item's text (same extent) and its 1-based
    /// first line in the reconstructed old file.
    pub(crate) old_text: String,
    pub(crate) old_first_line: usize,
    /// A `cfg` or `cfg_attr` on the function, an enclosing item or the file
    /// (an inner attribute), on either side. Identity holds regardless; a
    /// claim decides whether a gated function may be credited.
    pub(crate) cfg_gated: bool,
}

/// The base-side counterpart of the function `name` whose new-side summary
/// spans lines `start_line..=end_line` of `new_source`, read through
/// `changed`. `Some` only when:
///
/// - both the new file and the reconstructed old file parse cleanly;
/// - exactly one function named `name` encloses that span on the new side,
///   and no other function of that name shares its qualified path;
/// - the item's first and last lines are unchanged, and exactly one old
///   function spans the old lines between their counterparts;
/// - that old function has the same name and the same qualified path
///   (every enclosing module, impl, trait and function header compared as
///   text, generics, self type, trait and outer attributes included), and
///   is unique under that path in the old file;
/// - no added line falls on an enclosing item's header or on the function's
///   attributes or signature.
pub(crate) fn verified_base_owner(
    new_source: &str,
    changed: &ChangedFile,
    name: &str,
    start_line: usize,
    end_line: usize,
) -> Option<BaseOwner> {
    let new_parse = crate::analysis::syntax::parse_clean_source_file(new_source)?;
    let new_root = new_parse.tree().syntax().clone();
    let mut enclosing = named_functions(&new_root, name).filter(|function| {
        node_lines(new_source, function.syntax())
            .is_some_and(|(first, last)| first <= start_line && end_line <= last)
    });
    // Innermost last: a nested function of the same name is the owner.
    let new_fn = enclosing.next_back()?;
    let new_path = qualified_path(new_source, &new_fn)?;
    let bare = bare_path(new_source, &new_fn)?;
    if !unique_under(new_source, &new_root, name, &bare) {
        return None;
    }
    let (new_first, new_last) = node_lines(new_source, new_fn.syntax())?;

    // No added line on an enclosing header or on the function's attributes
    // and signature (everything before its body).
    let added: BTreeSet<usize> = changed.added_lines.iter().map(|line| line.line).collect();
    let signature_end = new_fn
        .body()
        .map(|body| line_of(new_source, usize::from(body.syntax().text_range().start())))
        .unwrap_or(Some(new_last))?;
    if (new_first..=signature_end).any(|line| added.contains(&line)) {
        return None;
    }
    for (first, last) in header_line_ranges(new_source, &new_fn)? {
        if (first..=last).any(|line| added.contains(&line)) {
            return None;
        }
    }

    let old_lines = reconstruct_old_lines_with_origins(new_source, changed)?;
    let old_first = old_lines
        .iter()
        .position(|(origin, _)| *origin == Some(new_first))?
        + 1;
    let old_last = old_lines
        .iter()
        .position(|(origin, _)| *origin == Some(new_last))?
        + 1;
    let old_source = old_lines
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let old_parse = crate::analysis::syntax::parse_clean_source_file(&old_source)?;
    let old_root = old_parse.tree().syntax().clone();
    let mut spanning = old_root
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            node_lines(&old_source, function.syntax()) == Some((old_first, old_last))
        });
    let old_fn = spanning.next()?;
    if spanning.next().is_some()
        || old_fn.name().is_none_or(|old_name| old_name.text() != name)
        || qualified_path(&old_source, &old_fn)? != new_path
        || !unique_under(&old_source, &old_root, name, &bare)
    {
        return None;
    }
    Some(BaseOwner {
        new_text: node_text(new_source, new_fn.syntax())?.to_string(),
        new_first_line: new_first,
        old_text: node_text(&old_source, old_fn.syntax())?.to_string(),
        old_first_line: old_first,
        cfg_gated: cfg_gated(&new_fn) || cfg_gated(&old_fn),
    })
}

fn named_functions<'a>(
    root: &'a SyntaxNode,
    name: &'a str,
) -> impl DoubleEndedIterator<Item = ast::Fn> + 'a {
    root.descendants()
        .filter_map(ast::Fn::cast)
        .filter(move |function| function.name().is_some_and(|found| found.text() == name))
        .collect::<Vec<_>>()
        .into_iter()
}

/// Whether exactly one function named `name` has the attribute-free path
/// `bare`. Attributes are left out so `cfg` twins (`#[cfg(test)] fn f` and
/// `#[cfg(not(test))] fn f`, or twin modules) count as twins: a test may run
/// either one.
fn unique_under(source: &str, root: &SyntaxNode, name: &str, bare: &[String]) -> bool {
    named_functions(root, name)
        .filter(|function| bare_path(source, function).as_deref() == Some(bare))
        .count()
        == 1
}

fn node_text<'a>(source: &'a str, node: &SyntaxNode) -> Option<&'a str> {
    let range = node.text_range();
    source.get(usize::from(range.start())..usize::from(range.end()))
}

fn line_of(source: &str, offset: usize) -> Option<usize> {
    Some(1 + source.get(..offset)?.matches('\n').count())
}

/// 1-based first and last line of `node` in `source`.
fn node_lines(source: &str, node: &SyntaxNode) -> Option<(usize, usize)> {
    let range = node.text_range();
    Some((
        line_of(source, usize::from(range.start()))?,
        line_of(source, usize::from(range.end()))?,
    ))
}

fn is_list(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::ITEM_LIST
            | SyntaxKind::ASSOC_ITEM_LIST
            | SyntaxKind::EXTERN_ITEM_LIST
            | SyntaxKind::STMT_LIST
            | SyntaxKind::BLOCK_EXPR
    )
}

/// Every item enclosing `function`, innermost first, with the byte range of
/// its header: from the node start (outer attributes included) to its item
/// list or body.
fn enclosing_headers(function: &ast::Fn) -> Vec<(SyntaxNode, usize, usize)> {
    function
        .syntax()
        .ancestors()
        .skip(1)
        .take_while(|node| node.kind() != SyntaxKind::SOURCE_FILE)
        .filter(|node| !is_list(node.kind()))
        .map(|node| {
            let start = usize::from(node.text_range().start());
            let end = node
                .children()
                .find(|child| is_list(child.kind()))
                .map_or(node.text_range().end(), |child| child.text_range().start());
            (node, start, usize::from(end))
        })
        .collect()
}

/// The whitespace-collapsed header of every enclosing item plus the
/// function's own name: module chain, impl self type and trait (with
/// generics and outer attributes), trait and outer function headers.
fn qualified_path(source: &str, function: &ast::Fn) -> Option<Vec<String>> {
    let mut path = Vec::new();
    for (node, start, end) in enclosing_headers(function) {
        let header = source.get(start..end)?;
        path.push(format!(
            "{:?} {}",
            node.kind(),
            header.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }
    path.push(format!("fn {}", function.name()?.text()));
    // The function's own outer attributes are part of its identity too.
    let attrs: Vec<String> = ast::HasAttrs::attrs(function)
        .map(|attr| {
            attr.syntax()
                .text()
                .to_string()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    path.push(attrs.join(" "));
    Some(path)
}

/// [`qualified_path`] without any outer attribute or comment: each header
/// read from its first token after them.
fn bare_path(source: &str, function: &ast::Fn) -> Option<Vec<String>> {
    let mut path = Vec::new();
    for (node, _, end) in enclosing_headers(function) {
        let start = usize::from(bare_start(&node)?);
        let header = source.get(start..end.max(start))?;
        path.push(format!(
            "{:?} {}",
            node.kind(),
            header.split_whitespace().collect::<Vec<_>>().join(" ")
        ));
    }
    path.push(format!("fn {}", function.name()?.text()));
    Some(path)
}

/// The offset of the first child of `node` that is not an attribute,
/// comment or whitespace.
fn bare_start(node: &SyntaxNode) -> Option<ra_ap_syntax::TextSize> {
    node.children_with_tokens()
        .find(|child| {
            !matches!(
                child.kind(),
                SyntaxKind::ATTR | SyntaxKind::COMMENT | SyntaxKind::WHITESPACE
            )
        })
        .map(|child| child.text_range().start())
}

/// 1-based line ranges of every enclosing item's header in `source`.
fn header_line_ranges(source: &str, function: &ast::Fn) -> Option<Vec<(usize, usize)>> {
    enclosing_headers(function)
        .into_iter()
        .map(|(_, start, end)| Some((line_of(source, start)?, line_of(source, end)?)))
        .collect()
}

/// Whether a `cfg` or `cfg_attr` attribute on the function, an enclosing
/// item or the file (an inner attribute) may compile it out or change it.
fn cfg_gated(function: &ast::Fn) -> bool {
    function.syntax().ancestors().any(|node| {
        ast::AnyHasAttrs::cast(node).is_some_and(|owner| {
            ast::HasAttrs::attrs(&owner)
                .any(|attr| matches!(attr.simple_name().as_deref(), Some("cfg" | "cfg_attr")))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One-hunk unified diff of `old` → `new` (common prefix and suffix kept
    /// as context, the middle replaced), parsed.
    fn changed(old: &str, new: &str) -> Result<ChangedFile, String> {
        let old_lines: Vec<&str> = old.lines().collect();
        let new_lines: Vec<&str> = new.lines().collect();
        let prefix = old_lines
            .iter()
            .zip(&new_lines)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = old_lines[prefix..]
            .iter()
            .rev()
            .zip(new_lines[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let mut diff = format!(
            "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{},{} +{},{} @@\n",
            prefix + 1,
            old_lines.len() - prefix,
            prefix + 1,
            new_lines.len() - prefix
        );
        for line in &old_lines[prefix..old_lines.len() - suffix] {
            diff.push_str(&format!("-{line}\n"));
        }
        for line in &new_lines[prefix..new_lines.len() - suffix] {
            diff.push_str(&format!("+{line}\n"));
        }
        for line in &new_lines[new_lines.len() - suffix..] {
            diff.push_str(&format!(" {line}\n"));
        }
        crate::analysis::diff::parse_unified_diff(&diff)
            .into_iter()
            .next()
            .ok_or_else(|| "fixture premise: diff did not parse".to_string())
    }

    /// The owner span of the function `name` whose body holds `line_text`.
    fn verify(
        old: &str,
        new: &str,
        name: &str,
        line_text: &str,
    ) -> Result<Option<BaseOwner>, String> {
        let line = new
            .lines()
            .position(|line| line.trim() == line_text)
            .ok_or("fixture premise: line missing")?
            + 1;
        Ok(verified_base_owner(
            new,
            &changed(old, new)?,
            name,
            line,
            line,
        ))
    }

    #[test]
    fn an_in_place_change_keeps_its_base_owner() -> Result<(), String> {
        let old = "pub mod a {\n    pub fn ready() -> bool {\n        true\n    }\n}\n";
        let new = "pub mod a {\n    pub fn ready() -> bool {\n        false\n    }\n}\n";
        let base = verify(old, new, "ready", "false")?.ok_or("in-place change refused")?;
        assert_eq!(
            base.old_text,
            "pub fn ready() -> bool {\n        true\n    }"
        );
        assert_eq!(
            base.new_text,
            "pub fn ready() -> bool {\n        false\n    }"
        );
        assert_eq!((base.old_first_line, base.new_first_line), (2, 2));
        assert!(!base.cfg_gated);
        Ok(())
    }

    /// Review F2 / swap_mods and swap_mods_bare: the unchanged header line
    /// pairs with the other module's function.
    #[test]
    fn swapped_modules_or_impls_are_not_the_same_owner() -> Result<(), String> {
        let module = |name: &str, value: &str| {
            format!(
                "pub mod {name} {{\n    pub fn ready(x: i32) -> i32 {{\n        {value}\n    }}\n}}\n"
            )
        };
        let old = format!("{}\n{}", module("a", "-x"), module("b", "x"));
        let new = format!("{}\n{}", module("b", "x"), module("a", "-x"));
        assert_eq!(
            new.lines().nth(1),
            Some("    pub fn ready(x: i32) -> i32 {"),
            "premise: the header line is unchanged"
        );
        for tail in ["x", "-x"] {
            assert_eq!(verify(&old, &new, "ready", tail)?, None, "swap_mods {tail}");
        }
        let imp = |name: &str, value: &str| {
            format!("impl {name} {{\n    pub fn ready() -> bool {{\n        {value}\n    }}\n}}\n")
        };
        let types = "pub struct A;\npub struct B;\n";
        let old = format!("{types}{}\n{}", imp("A", "true"), imp("B", "false"));
        let new = format!("{types}{}\n{}", imp("B", "false"), imp("A", "true"));
        for tail in ["false", "true"] {
            assert_eq!(
                verify(&old, &new, "ready", tail)?,
                None,
                "swap_impls {tail}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_trait_impl_is_not_an_inherent_impl() -> Result<(), String> {
        let old =
            "pub struct A;\nimpl A {\n    pub fn ready(&self) -> bool {\n        true\n    }\n}\n";
        let new = "pub struct A;\nimpl Flag for A {\n    fn ready(&self) -> bool {\n        false\n    }\n}\n";
        assert_eq!(verify(old, new, "ready", "false")?, None);
        Ok(())
    }

    /// cfg_swap, cfg_dup2, cfg_never2: attribute swaps and same-named twins.
    #[test]
    fn cfg_twins_and_attribute_changes_are_refused() -> Result<(), String> {
        let twin = |gate_a: &str, a: &str, gate_b: &str, b: &str| {
            format!(
                "#[cfg({gate_a})]\npub fn ready() -> bool {{\n    {a}\n}}\n\n#[cfg({gate_b})]\npub fn ready() -> bool {{\n    {b}\n}}\n"
            )
        };
        let old = twin("any()", "false", "not(any())", "true");
        let new = twin("not(any())", "true", "any()", "false");
        assert_eq!(verify(&old, &new, "ready", "false")?, None, "cfg_swap");
        let old = twin("not(test)", "true", "test", "false");
        let new = twin("not(test)", "false", "test", "false");
        let line = 3;
        assert_eq!(
            verified_base_owner(&new, &changed(&old, &new)?, "ready", line, line),
            None,
            "cfg_dup2"
        );
        // cfg-twin modules: the function is unique only by its module's
        // attribute.
        let twin_mods = |a: &str| {
            format!(
                "#[cfg(test)]\nmod m {{\n    pub fn ready() -> bool {{\n        {a}\n    }}\n}}\n#[cfg(not(test))]\nmod m {{\n    pub fn ready() -> bool {{\n        true\n    }}\n}}\n"
            )
        };
        let (old, new) = (twin_mods("true"), twin_mods("false"));
        assert_eq!(
            verify(&old, &new, "ready", "false")?,
            None,
            "cfg twin modules"
        );
        // An attribute line changed with the body.
        let old = "#[inline]\npub fn ready() -> bool {\n    true\n}\n";
        let new = "#[inline(never)]\npub fn ready() -> bool {\n    false\n}\n";
        assert_eq!(verify(old, new, "ready", "false")?, None);
        // A signature change.
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready(_x: u8) -> bool {\n    false\n}\n";
        assert_eq!(verify(old, new, "ready", "false")?, None);
        Ok(())
    }

    #[test]
    fn a_gated_owner_keeps_identity_and_reports_the_gate() -> Result<(), String> {
        let old = "#[cfg(unix)]\npub fn ready() -> bool {\n    true\n}\n";
        let new = "#[cfg(unix)]\npub fn ready() -> bool {\n    false\n}\n";
        let base = verify(old, new, "ready", "false")?.ok_or("gated owner refused")?;
        assert!(base.cfg_gated);
        Ok(())
    }

    #[test]
    fn an_enclosing_header_change_is_refused() -> Result<(), String> {
        // Same name, but the module was renamed in the same diff.
        let old = "pub mod a {\n    pub fn ready() -> bool {\n        true\n    }\n}\n";
        let new = "pub mod b {\n    pub fn ready() -> bool {\n        false\n    }\n}\n";
        assert_eq!(verify(old, new, "ready", "false")?, None);
        Ok(())
    }

    #[test]
    fn an_unparsable_side_is_refused() -> Result<(), String> {
        let old = "pub fn ready() -> bool {\n    true\n}\nfn broken( {\n";
        let new = "pub fn ready() -> bool {\n    false\n}\nfn broken( {\n";
        assert_eq!(verify(old, new, "ready", "false")?, None);
        Ok(())
    }
}
