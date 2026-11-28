//! Cfg gates on items that enclose a documented function (#12453,
//! RIPR-SPEC-0279).
//!
//! rustdoc compiles an example only when the documented item is compiled, so
//! a gate on an enclosing `mod`, `impl`, `trait` or `extern` block, or on the
//! `mod name;` declaration that reaches the file, gates the example exactly
//! as a gate on the item does. Each attribute is judged by the same rule as
//! the item's own gates: only `#[cfg(doctest)]` and a predicate provably true
//! under the package's default features admit, `#[cfg_attr(..)]` refuses, and
//! anything that cannot be read refuses.

use super::{
    AttributeCfgFamily, attribute_cfg_family, attribute_gate_is_provably_true,
    attribute_is_cfg_doctest, file_has_inner_cfg_gate,
};
use crate::analysis::facts::RustIndex;
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::ast::HasName;
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, TextRange, TextSize, ast};
use std::collections::BTreeSet;
use std::path::Path;

/// Whether an enclosing gate, in `path` or on the way from the library root
/// to it, may leave the item on `item_line` uncompiled.
pub(super) fn enclosing_gate_refuses(
    index: &RustIndex,
    path: &Path,
    item_line: usize,
    defaults: Option<&BTreeSet<String>>,
) -> bool {
    let Some(file) = index.files.get(path) else {
        return true;
    };
    if line_enclosing_refuses(&file.data.source, item_line, defaults, Anchor::Function) {
        return true;
    }
    let provenance = &file.data.role_provenance;
    provenance.edges.iter().any(|edge| {
        let Some(parent) = index.files.get(&edge.parent) else {
            return true;
        };
        file_has_inner_cfg_gate(&parent.data.source, defaults)
            || declared_mod_name(&edge.declaration).is_none_or(|name| {
                line_enclosing_refuses(
                    &parent.data.source,
                    edge.line,
                    defaults,
                    Anchor::OutOfLineMod(name),
                )
            })
    })
}

/// What the recorded line names: the documented function, or the
/// out-of-line `mod name;` item of a module edge. The module is found by its
/// declared name, so another `mod` on the same line is never taken for it.
#[derive(Clone, Copy)]
enum Anchor<'a> {
    Function,
    OutOfLineMod(&'a str),
}

/// The module name in an edge's `mod name;` declaration text.
fn declared_mod_name(declaration: &str) -> Option<&str> {
    let mut words = declaration.split(|c: char| !(c.is_alphanumeric() || c == '_'));
    words.by_ref().find(|word| *word == "mod")?;
    words.find(|word| !word.is_empty())
}

/// Whether any gate on the item that starts on `line`, or on an item that
/// encloses it, refuses. An item that cannot be found refuses.
fn line_enclosing_refuses(
    source: &str,
    line: usize,
    defaults: Option<&BTreeSet<String>>,
    anchor: Anchor<'_>,
) -> bool {
    let Some(parse) = parse_clean_source_file(source) else {
        return true;
    };
    let root = parse.syntax_node();
    let Some(item) = item_on_line(&root, source, line, anchor) else {
        return true;
    };
    match anchor {
        // The item's own gates are read between the comment and the item.
        Anchor::Function => ancestors_refuse(&item, defaults, false),
        // A module edge's own `#[cfg]` is the gate being asked about.
        Anchor::OutOfLineMod(_) => ancestors_refuse(&item, defaults, true),
    }
}

/// The function whose head is on 1-based `line`, or the out-of-line module
/// of the declared name that overlaps it.
fn item_on_line(
    root: &SyntaxNode,
    source: &str,
    line: usize,
    anchor: Anchor<'_>,
) -> Option<SyntaxNode> {
    match anchor {
        Anchor::Function => {
            let start = line_code_offset(source, line)?;
            let token = root.token_at_offset(start).right_biased()?;
            token
                .parent_ancestors()
                .find(|node| ast::Fn::can_cast(node.kind()))
        }
        Anchor::OutOfLineMod(name) => {
            let span = line_range(source, line)?;
            root.descendants().find(|node| {
                ast::Module::cast(node.clone()).is_some_and(|module| {
                    module.item_list().is_none()
                        && module
                            .name()
                            .is_some_and(|declared| declared.text() == name)
                        && node.text_range().intersect(span).is_some()
                })
            })
        }
    }
}

/// The byte range of 1-based `line`, without its line break.
fn line_range(source: &str, line: usize) -> Option<TextRange> {
    let mut offset = 0usize;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            let end = offset + text.trim_end_matches(['\n', '\r']).len();
            return Some(TextRange::new(
                TextSize::try_from(offset).ok()?,
                TextSize::try_from(end).ok()?,
            ));
        }
        offset += text.len();
    }
    None
}

/// The offset of the first non-blank character on 1-based `line`.
fn line_code_offset(source: &str, line: usize) -> Option<TextSize> {
    let mut offset = 0usize;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            let at = offset + (text.len() - text.trim_start().len());
            return (at < offset + text.trim_end().len())
                .then(|| TextSize::try_from(at).ok())
                .flatten();
        }
        offset += text.len();
    }
    None
}

/// Whether a gate on `item` (when `include_self`) or on any enclosing `mod`,
/// `impl`, `trait` or `extern` block refuses.
fn ancestors_refuse(
    item: &SyntaxNode,
    defaults: Option<&BTreeSet<String>>,
    include_self: bool,
) -> bool {
    let skip = usize::from(!include_self);
    item.ancestors().skip(skip).any(|node| {
        let encloses = matches!(
            node.kind(),
            SyntaxKind::MODULE | SyntaxKind::IMPL | SyntaxKind::TRAIT | SyntaxKind::EXTERN_BLOCK
        );
        encloses && attributes_refuse(&node, defaults)
    })
}

/// Whether any attribute that gates `node` may leave it uncompiled: its outer
/// attributes, and the inner `#![cfg(..)]` attributes that rust-analyzer
/// places in the item list of an inline `mod`, `impl`, `trait` or `extern`
/// block rather than on the item itself.
fn attributes_refuse(node: &SyntaxNode, defaults: Option<&BTreeSet<String>>) -> bool {
    node.children()
        .flat_map(|child| {
            let list = matches!(
                child.kind(),
                SyntaxKind::ITEM_LIST | SyntaxKind::ASSOC_ITEM_LIST | SyntaxKind::EXTERN_ITEM_LIST
            );
            let inner: Vec<SyntaxNode> = if list {
                child.children().collect()
            } else {
                Vec::new()
            };
            std::iter::once(child).chain(inner)
        })
        .filter_map(ast::Attr::cast)
        .any(|attr| attribute_refuses(&attr.syntax().text().to_string(), defaults))
}

fn attribute_refuses(text: &str, defaults: Option<&BTreeSet<String>>) -> bool {
    let trimmed = text.trim();
    match attribute_cfg_family(trimmed) {
        AttributeCfgFamily::CfgAttr => true,
        AttributeCfgFamily::Cfg => {
            !attribute_is_cfg_doctest(trimmed)
                && !attribute_gate_is_provably_true(trimmed, defaults)
        }
        AttributeCfgFamily::Other => false,
    }
}
