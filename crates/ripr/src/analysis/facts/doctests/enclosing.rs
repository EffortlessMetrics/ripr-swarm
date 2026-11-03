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
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, TextSize, ast};
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
            || line_enclosing_refuses(
                &parent.data.source,
                edge.line,
                defaults,
                Anchor::OutOfLineMod,
            )
    })
}

/// What the recorded line names: the documented function, or the
/// out-of-line `mod name;` item of a module edge.
#[derive(Clone, Copy)]
enum Anchor {
    Function,
    OutOfLineMod,
}

/// Whether any gate on the item that starts on `line`, or on an item that
/// encloses it, refuses. An item that cannot be found refuses.
fn line_enclosing_refuses(
    source: &str,
    line: usize,
    defaults: Option<&BTreeSet<String>>,
    anchor: Anchor,
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
        Anchor::OutOfLineMod => ancestors_refuse(&item, defaults, true),
    }
}

/// The function or out-of-line module whose head is on 1-based `line`.
fn item_on_line(
    root: &SyntaxNode,
    source: &str,
    line: usize,
    anchor: Anchor,
) -> Option<SyntaxNode> {
    let start = line_code_offset(source, line)?;
    let token = root.token_at_offset(start).right_biased()?;
    token.parent_ancestors().find(|node| match anchor {
        Anchor::Function => ast::Fn::can_cast(node.kind()),
        Anchor::OutOfLineMod => {
            ast::Module::cast(node.clone()).is_some_and(|module| module.item_list().is_none())
        }
    })
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

/// Whether any outer attribute of `node` may leave it uncompiled.
fn attributes_refuse(node: &SyntaxNode, defaults: Option<&BTreeSet<String>>) -> bool {
    node.children()
        .filter_map(ast::Attr::cast)
        .filter(|attr| attr.excl_token().is_none())
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
