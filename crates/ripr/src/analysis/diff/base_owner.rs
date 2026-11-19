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
//!
//! An out-of-line module's identity and gates also live in another file:
//! the parent's `mod name;` declaration, its attributes (`#[path]`,
//! `#[cfg]`) and the items enclosing it. [`ModuleDeclarations`] reads every
//! such declaration once per run, so a diff that swaps `#[path]` targets or
//! a `cfg`-gated `mod real;` twin of a test-only `mod fake;` is seen from
//! the owner's file.

use super::{ChangedFile, reconstruct_old_lines_with_origins};
use ra_ap_syntax::{
    AstNode, AstToken, SyntaxKind, SyntaxNode,
    ast::{self, HasName},
};
use std::collections::BTreeSet;
use std::path::Path;

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
    /// A `cfg` or `cfg_attr` that may compile the function out or change it,
    /// on either side: on the function, an enclosing item or the owner file
    /// (an inner attribute); or, read from [`ModuleDeclarations`] on the new
    /// side, on any out-of-line `mod` declaration named like a component of
    /// the owner file's path, on an item enclosing it or on its declaring
    /// file; or any `#[path]` declaration in the workspace (whose target the
    /// path cannot name). Identity holds regardless; a claim decides whether
    /// a gated function may be credited.
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
///   attributes or signature;
/// - `modules` saw no added or removed line on an out-of-line `mod`
///   declaration (attributes included) named like a component of
///   `owner_file`'s path, on an inline module or file attribute enclosing
///   one, or on any `#[path]` declaration, and read every changed file.
///
/// Names compare with any `r#` prefix removed.
pub(crate) fn verified_base_owner(
    new_source: &str,
    owner_file: &Path,
    changed: &ChangedFile,
    modules: &ModuleDeclarations,
    name: &str,
    start_line: usize,
    end_line: usize,
) -> Option<BaseOwner> {
    let chain = module_chain_names(owner_file);
    if modules.chain_changed(owner_file, &chain) {
        return None;
    }
    let name = unraw(name);
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
        || old_fn
            .name()
            .is_none_or(|old_name| unraw(old_name.text()) != name)
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
        cfg_gated: cfg_gated(&new_fn)
            || cfg_gated(&old_fn)
            || modules.chain_gated(owner_file, &chain),
    })
}

/// #15104: whether a test may run some other function in place of the
/// new-side function `name` spanning `start_line..=end_line` of
/// `new_source`. This is the new-side half of [`verified_base_owner`]'s
/// gate decision, for a claim that reads only the new item (no base-side
/// body): `true` when a `cfg` or `cfg_attr` sits on the function, an
/// enclosing item or the file, when another function of that name shares
/// its attribute-free path (a `cfg` twin), when `modules` reports a gated
/// or changed declaration on the owner file's module chain, or when any of
/// that cannot be read (fail closed).
pub(crate) fn owner_may_be_swapped(
    new_source: &str,
    owner_file: &Path,
    modules: &ModuleDeclarations,
    name: &str,
    start_line: usize,
    end_line: usize,
) -> bool {
    let chain = module_chain_names(owner_file);
    if modules.chain_changed(owner_file, &chain) || modules.chain_gated(owner_file, &chain) {
        return true;
    }
    let name = unraw(name);
    let Some(parse) = crate::analysis::syntax::parse_clean_source_file(new_source) else {
        return true;
    };
    let root = parse.tree().syntax().clone();
    // Innermost last, as in `verified_base_owner`.
    let Some(function) = named_functions(&root, name).rfind(|function| {
        node_lines(new_source, function.syntax())
            .is_some_and(|(first, last)| first <= start_line && end_line <= last)
    }) else {
        return true;
    };
    let Some(bare) = bare_path(new_source, &function) else {
        return true;
    };
    cfg_gated(&function) || !unique_under(new_source, &root, name, &bare)
}

fn named_functions<'a>(
    root: &'a SyntaxNode,
    name: &'a str,
) -> impl DoubleEndedIterator<Item = ast::Fn> + 'a {
    root.descendants()
        .filter_map(ast::Fn::cast)
        .filter(move |function| {
            function
                .name()
                .is_some_and(|found| unraw(found.text()) == name)
        })
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
    path.push(format!("fn {}", unraw(function.name()?.text())));
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
    path.push(format!("fn {}", unraw(function.name()?.text())));
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
    cfg_gated_node(function.syntax())
}

fn unraw(name: &str) -> &str {
    name.strip_prefix("r#").unwrap_or(name)
}

/// Every component of `file`'s path, extension removed: a superset of the
/// module names on its default declaration chain.
fn module_chain_names(file: &Path) -> BTreeSet<String> {
    file.iter()
        .filter_map(|component| component.to_str())
        .flat_map(|component| component.split(['/', '\\']))
        .map(|component| {
            component
                .strip_suffix(".rs")
                .unwrap_or(component)
                .to_string()
        })
        .filter(|component| !component.is_empty())
        .collect()
}

/// What the workspace's out-of-line `mod` declarations, and the diff's
/// changes to them, say about every owner's module chain. Read once per run
/// by [`ModuleDeclarations::read`]; a run that never read them uses
/// [`ModuleDeclarations::unread`], under which every owner refuses.
///
/// File-to-module mapping is approximated by name: a declaration counts for
/// an owner when its name equals a component of the owner file's path. That
/// over-approximates the declaration chain, so among `mod` declarations it
/// can only gate or refuse more owners, never fewer. A `#[path]` declaration
/// breaks the name mapping, so its presence gates every owner and a change
/// to one refuses every owner, except an owner in a default crate root
/// (`src/lib.rs`, `src/main.rs`) whose file name no `#[path]` literal names,
/// case-insensitively, while every `#[path]` target reads as a literal on an
/// out-of-line declaration: a `mod` declaration cannot move or compile out a
/// crate root, and the only file a `#[path]` can add as a module is the one
/// it names (#15104). Gates that live outside a `mod` declaration
/// (a `cfg` on `include!`, on a `pub use` re-export, or a `mod` emitted by
/// `macro_rules!`) are not read here; see #14952.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleDeclarations {
    /// Names of declarations with a `cfg`/`cfg_attr` on themselves, an
    /// enclosing item or their file.
    gated: BTreeSet<String>,
    /// Some declaration carries `#[path]` (directly or through `cfg_attr`).
    retargeted: bool,
    /// File names (last path component) that a `#[path]` literal on a
    /// read or changed declaration names.
    retarget_files: BTreeSet<String>,
    /// A `#[path]` declaration's target could not be read as a literal.
    retarget_opaque: bool,
    /// Names of declarations the diff touched: an added or removed line on
    /// the declaration or its attributes, on the header of an inline module
    /// enclosing it, or on an inner attribute of its file.
    changed: BTreeSet<String>,
    /// The diff touched a `#[path]` declaration.
    changed_retarget: bool,
    /// A declaring source or a changed Rust file could not be read.
    unreadable: bool,
}

impl ModuleDeclarations {
    /// A reading under which every owner's chain counts as changed.
    pub(crate) fn unread() -> Self {
        Self {
            unreadable: true,
            ..Self::default()
        }
    }

    /// Read `declaring_sources` (new-side text of every workspace file that
    /// may declare a module; a superset is fine) and `changed_files` (each
    /// changed file with its new-side text, `None` when it is not
    /// available). A non-Rust changed file is skipped.
    pub(crate) fn read<'a>(
        declaring_sources: impl IntoIterator<Item = &'a str>,
        changed_files: impl IntoIterator<Item = (&'a ChangedFile, Option<&'a str>)>,
    ) -> Self {
        let mut reading = Self::default();
        for source in declaring_sources {
            let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
                reading.unreadable = true;
                continue;
            };
            // An inline module's `#[path]` moves its children's directory;
            // which files that reaches is not read, so it gates every owner.
            if inline_retargets(parse.tree().syntax()) {
                reading.retargeted = true;
                reading.retarget_opaque = true;
            }
            for module in out_of_line_modules(parse.tree().syntax()) {
                let Some(name) = module.name() else {
                    reading.unreadable = true;
                    continue;
                };
                if module_retargets(&module) {
                    reading.retargeted = true;
                    reading.note_retarget_files(&module);
                }
                if cfg_gated_node(module.syntax()) {
                    reading.gated.insert(unraw(name.text()).to_string());
                }
            }
        }
        for (changed, new_source) in changed_files {
            if changed
                .path
                .extension()
                .is_none_or(|extension| extension != "rs")
            {
                continue;
            }
            if changed.added_lines.is_empty() && changed.removed_lines.is_empty() {
                continue;
            }
            let Some(new_source) = new_source else {
                reading.unreadable = true;
                continue;
            };
            let added: BTreeSet<usize> = changed.added_lines.iter().map(|line| line.line).collect();
            reading.note_touched(new_source, &added);
            if changed.removed_lines.is_empty() {
                continue;
            }
            let Some(old_source) = super::reconstruct_old_source(new_source, changed) else {
                reading.unreadable = true;
                continue;
            };
            let removed: BTreeSet<usize> =
                changed.removed_lines.iter().map(|line| line.line).collect();
            reading.note_touched(&old_source, &removed);
        }
        reading
    }

    /// Record every declaration of `source` that a line in `lines` touches.
    fn note_touched(&mut self, source: &str, lines: &BTreeSet<usize>) {
        if lines.is_empty() {
            return;
        }
        let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
            self.unreadable = true;
            return;
        };
        let root = parse.tree().syntax().clone();
        let touches = |start: usize, end: usize| -> bool {
            match (line_of(source, start), line_of(source, end)) {
                (Some(first), Some(last)) => lines.range(first..=last).next().is_some(),
                _ => true,
            }
        };
        // An inner attribute of the file (`#![cfg(..)]`) gates every
        // declaration in it.
        let file_attrs_touched = ast::SourceFile::cast(root.clone()).is_some_and(|file| {
            ast::HasAttrs::attrs(&file)
                .filter(|attr| attr.excl_token().is_some())
                .any(|attr| {
                    let range = attr.syntax().text_range();
                    touches(usize::from(range.start()), usize::from(range.end()))
                })
        });
        for module in root.descendants().filter_map(ast::Module::cast) {
            let start = usize::from(module.syntax().text_range().start());
            let end = module
                .item_list()
                .map_or(module.syntax().text_range().end(), |list| {
                    list.syntax().text_range().start()
                });
            if !file_attrs_touched && !touches(start, usize::from(end)) {
                continue;
            }
            if module.item_list().is_some() && module_retargets(&module) {
                self.changed_retarget = true;
                self.retarget_opaque = true;
            }
            // An out-of-line declaration itself, or every one an inline
            // module's header encloses.
            for declaration in std::iter::once(module.clone())
                .filter(|module| module.item_list().is_none())
                .chain(out_of_line_modules(module.syntax()))
            {
                if module_retargets(&declaration) {
                    self.changed_retarget = true;
                    self.note_retarget_files(&declaration);
                }
                match declaration.name() {
                    Some(name) => {
                        self.changed.insert(unraw(name.text()).to_string());
                    }
                    None => self.unreadable = true,
                }
            }
        }
    }

    /// Record the file name each `#[path]` literal on `module` names, or
    /// that one could not be read.
    fn note_retarget_files(&mut self, module: &ast::Module) {
        let mut named = false;
        for attr in ast::HasAttrs::attrs(module) {
            if !matches!(attr_name(&attr).as_deref(), Some("path" | "cfg_attr")) {
                continue;
            }
            for literal in attr
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
                .filter_map(ast::String::cast)
            {
                match literal.value().ok().as_deref().and_then(retarget_file_name) {
                    Some(file) => {
                        named = true;
                        self.retarget_files.insert(file.to_ascii_lowercase());
                    }
                    None => self.retarget_opaque = true,
                }
            }
        }
        if !named {
            self.retarget_opaque = true;
        }
    }

    /// Whether a `#[path]` declaration may move or duplicate `owner_file`.
    fn retarget_reaches(&self, owner_file: &Path) -> bool {
        self.retarget_opaque
            || !default_crate_root(owner_file)
            || owner_file
                .file_name()
                .and_then(|name| name.to_str())
                .is_none_or(|name| self.retarget_files.contains(&name.to_ascii_lowercase()))
    }

    /// Whether the diff may have changed which module `chain` names, or how
    /// its declarations are gated.
    fn chain_changed(&self, owner_file: &Path, chain: &BTreeSet<String>) -> bool {
        self.unreadable
            || (self.changed_retarget && self.retarget_reaches(owner_file))
            || !self.changed.is_disjoint(chain)
    }

    /// Whether a declaration on `chain` may be compiled out or retargeted.
    fn chain_gated(&self, owner_file: &Path, chain: &BTreeSet<String>) -> bool {
        (self.retargeted && self.retarget_reaches(owner_file)) || !self.gated.is_disjoint(chain)
    }
}

/// Every out-of-line `mod name;` declaration under `node`.
fn out_of_line_modules(node: &SyntaxNode) -> Vec<ast::Module> {
    node.descendants()
        .filter_map(ast::Module::cast)
        .filter(|module| module.item_list().is_none())
        .collect()
}

/// A `#[path]` attribute, directly or inside a `cfg_attr`.
fn module_retargets(module: &ast::Module) -> bool {
    ast::HasAttrs::attrs(module).any(|attr| match attr_name(&attr).as_deref() {
        Some("path") => true,
        Some("cfg_attr") => attr.syntax().text().to_string().contains("path"),
        _ => false,
    })
}

/// The last component of a `#[path]` target, `None` when it is empty.
fn retarget_file_name(target: &str) -> Option<String> {
    target
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// Whether `file` is `src/lib.rs` or `src/main.rs` of some package.
fn default_crate_root(file: &Path) -> bool {
    let name = file.file_name().and_then(|name| name.to_str());
    let parent = file
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str());
    // A nested `src` (`src/a/src/lib.rs`) may be a module file.
    let src_components = file
        .components()
        .filter(|component| component.as_os_str() == "src")
        .count();
    matches!(name, Some("lib.rs" | "main.rs")) && parent == Some("src") && src_components == 1
}

/// Whether an inline `mod name { .. }` under `node` carries `#[path]`.
fn inline_retargets(node: &SyntaxNode) -> bool {
    node.descendants()
        .filter_map(ast::Module::cast)
        .any(|module| module.item_list().is_some() && module_retargets(&module))
}

/// An attribute's name: the last segment of its path with any `r#` removed,
/// so `#[r#cfg(..)]` reads as `cfg`. `None` when it has no readable path.
fn attr_name(attr: &ast::Attr) -> Option<String> {
    if let Some(name) = attr.simple_name() {
        return Some(unraw(&name).to_string());
    }
    let name = attr.path()?.segment()?.name_ref()?;
    Some(unraw(name.text()).to_string())
}

/// [`cfg_gated`] for any node: a `cfg`/`cfg_attr` on it, an enclosing item
/// or its file.
fn cfg_gated_node(node: &SyntaxNode) -> bool {
    node.ancestors().any(|node| {
        ast::AnyHasAttrs::cast(node).is_some_and(|owner| {
            ast::HasAttrs::attrs(&owner)
                .any(|attr| matches!(attr_name(&attr).as_deref(), Some("cfg" | "cfg_attr")))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One-hunk unified diff of `old` → `new` (common prefix and suffix kept
    /// as context, the middle replaced), parsed.
    fn changed(old: &str, new: &str) -> Result<ChangedFile, String> {
        changed_at("src/lib.rs", old, new)
    }

    fn changed_at(path: &str, old: &str, new: &str) -> Result<ChangedFile, String> {
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
            "--- a/{path}\n+++ b/{path}\n@@ -{},{} +{},{} @@\n",
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
        let changed = changed(old, new)?;
        let modules = ModuleDeclarations::read([new], [(&changed, Some(new))]);
        Ok(verified_base_owner(
            new,
            Path::new("src/lib.rs"),
            &changed,
            &modules,
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

    /// #15104: the new-side gate decision for a claim that reads no
    /// base-side body. Twins, gates on the item or an enclosing module, a
    /// gated module declaration on the owner's chain and an unparseable file
    /// may swap the owner; an ungated unique owner, a same-named function on
    /// another path and an unrelated gated module do not.
    #[test]
    fn owner_swap_reads_new_side_gates_and_twins() -> Result<(), String> {
        let lib = Path::new("src/lib.rs");
        let unchanged = |source: &str| -> Result<ModuleDeclarations, String> {
            let changed_file = changed(source, source)?;
            Ok(ModuleDeclarations::read(
                [source],
                [(&changed_file, Some(source))],
            ))
        };
        let swapped = |source: &str, file: &Path, line: usize| -> Result<bool, String> {
            Ok(owner_may_be_swapped(
                source,
                file,
                &unchanged(source)?,
                "ready",
                line,
                line,
            ))
        };
        let plain = "pub fn ready() -> i32 {\n    2\n}\n";
        assert!(!swapped(plain, lib, 2)?, "plain");
        let elsewhere = "pub fn ready() -> i32 {\n    2\n}\npub mod other {\n    pub fn ready() -> i32 {\n        5\n    }\n}\n";
        assert!(!swapped(elsewhere, lib, 2)?, "same name on another path");
        let unrelated = "pub fn ready() -> i32 {\n    2\n}\n#[cfg(test)]\nmod fake {\n    pub fn stub() -> i32 {\n        2\n    }\n}\n";
        assert!(!swapped(unrelated, lib, 2)?, "unrelated cfg(test) module");
        let twins = "#[cfg(not(test))]\npub fn ready() -> i32 {\n    2\n}\n#[cfg(test)]\npub fn ready() -> i32 {\n    2\n}\n";
        assert!(swapped(twins, lib, 3)?, "function-level twin");
        let bare_twins = "pub fn ready() -> i32 {\n    2\n}\npub fn ready() -> i32 {\n    2\n}\n";
        assert!(swapped(bare_twins, lib, 2)?, "same-path twin");
        let gated_mod =
            "#[cfg(not(test))]\nmod m {\n    pub fn ready() -> i32 {\n        2\n    }\n}\n";
        assert!(swapped(gated_mod, lib, 3)?, "gated enclosing module");
        let file_gate = "#![cfg(unix)]\npub fn ready() -> i32 {\n    2\n}\n";
        assert!(swapped(file_gate, lib, 3)?, "file inner attribute");
        assert!(swapped("pub fn ready( {\n", lib, 1)?, "unparseable");
        assert!(swapped(plain, lib, 9)?, "no function encloses the span");
        // The owner's file is declared by a gated `mod real;` elsewhere.
        let parent = "#[cfg(not(test))]\nmod real;\n";
        let changed_file = changed(plain, plain)?;
        let modules = ModuleDeclarations::read([parent], [(&changed_file, Some(plain))]);
        assert!(
            owner_may_be_swapped(plain, Path::new("src/real.rs"), &modules, "ready", 2, 2),
            "gated module declaration on the chain"
        );
        assert!(
            owner_may_be_swapped(plain, lib, &ModuleDeclarations::unread(), "ready", 2, 2),
            "unread declarations"
        );
        Ok(())
    }

    /// #15104: a `#[path]` declaration gates only the owners it may move: a
    /// default crate root stays ungated unless a `#[path]` literal names its
    /// file or a target cannot be read.
    #[test]
    fn path_declarations_gate_only_the_files_they_may_move() -> Result<(), String> {
        let plain = "pub fn ready() -> i32 {\n    2\n}\n";
        let swapped = |declaring: &str, file: &str, touched: bool| -> Result<bool, String> {
            let source = format!("{plain}{declaring}");
            let changed_file = if touched {
                changed(plain, &source)?
            } else {
                changed(&source, &source)?
            };
            let modules = ModuleDeclarations::read(
                [source.as_str()],
                [(&changed_file, Some(source.as_str()))],
            );
            Ok(owner_may_be_swapped(
                &source,
                Path::new(file),
                &modules,
                "ready",
                2,
                2,
            ))
        };
        let test_support = "#[cfg(test)]\n#[path = \"test_support.rs\"]\nmod tests;\n";
        for touched in [false, true] {
            assert!(
                !swapped(test_support, "src/lib.rs", touched)?,
                "crate root beside a #[path] test module (touched {touched})"
            );
            assert!(
                !swapped(test_support, "crates/a/src/main.rs", touched)?,
                "binary root (touched {touched})"
            );
            assert!(
                swapped(test_support, "src/inner.rs", touched)?,
                "a non-root file may be the target (touched {touched})"
            );
            assert!(
                swapped(test_support, "src/nested/lib.rs", touched)?,
                "lib.rs outside src/ is not a default root (touched {touched})"
            );
            assert!(
                swapped(
                    "#[cfg(test)]\n#[path = \"../src/lib.rs\"]\nmod again;\n",
                    "src/lib.rs",
                    touched
                )?,
                "a #[path] naming the root's file (touched {touched})"
            );
            assert!(
                swapped(
                    "#[cfg_attr(test, path = \"lib.rs\")]\nmod again;\n",
                    "src/lib.rs",
                    touched
                )?,
                "a cfg_attr path naming the root's file (touched {touched})"
            );
            assert!(
                swapped(
                    "#[cfg_attr(test, path)]\nmod again;\n",
                    "src/lib.rs",
                    touched
                )?,
                "an unreadable target (touched {touched})"
            );
            assert!(
                swapped(
                    "#[cfg(test)]\n#[path = \"..\\\\src\\\\LIB.rs\"]\nmod again;\n",
                    "src/lib.rs",
                    touched
                )?,
                "a backslash, differently cased literal (touched {touched})"
            );
            assert!(
                swapped(
                    "#[cfg(test)]\n#[path = \"main.rs\"]\nmod again;\n",
                    "src/main.rs",
                    touched
                )?,
                "a literal naming the binary root (touched {touched})"
            );
            assert!(
                swapped(test_support, "src/a/src/lib.rs", touched)?,
                "a nested src/ may hold a module file (touched {touched})"
            );
            assert!(
                swapped(
                    "#[cfg(test)]\n#[path = \"other\"]\nmod a {\n    mod inner;\n}\n",
                    "src/lib.rs",
                    touched
                )?,
                "an inline module's #[path] (touched {touched})"
            );
        }
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
        let changed_file = changed(&old, &new)?;
        let modules =
            ModuleDeclarations::read([new.as_str()], [(&changed_file, Some(new.as_str()))]);
        assert_eq!(
            verified_base_owner(
                &new,
                Path::new("src/lib.rs"),
                &changed_file,
                &modules,
                "ready",
                3,
                3
            ),
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

    /// `owner` and `others` as `(path, old, new)`; the owner function `name`
    /// is the one whose body holds `line_text`. Every new side is a
    /// declaring source; every file whose sides differ is changed.
    fn verify_in(
        owner: (&str, &str, &str),
        others: &[(&str, &str, &str)],
        name: &str,
        line_text: &str,
    ) -> Result<Option<BaseOwner>, String> {
        let (path, old, new) = owner;
        let line = new
            .lines()
            .position(|line| line.trim() == line_text)
            .ok_or("fixture premise: line missing")?
            + 1;
        let owner_changed = changed_at(path, old, new)?;
        let mut changed_files = vec![(owner_changed.clone(), new)];
        for (other, other_old, other_new) in others {
            if other_old != other_new {
                changed_files.push((changed_at(other, other_old, other_new)?, *other_new));
            }
        }
        let sources = std::iter::once(new).chain(others.iter().map(|(_, _, new)| *new));
        let modules = ModuleDeclarations::read(
            sources,
            changed_files
                .iter()
                .map(|(file, source)| (file, Some(*source))),
        );
        Ok(verified_base_owner(
            new,
            Path::new(path),
            &owner_changed,
            &modules,
            name,
            line,
            line,
        ))
    }

    const REAL_OLD: &str = "pub fn ready() -> bool {\n    true\n}\n";
    const REAL_NEW: &str = "pub fn ready() -> bool {\n    false\n}\n";

    #[test]
    fn an_ungated_out_of_line_module_keeps_identity() -> Result<(), String> {
        let lib = "mod real;\npub use real::ready;\n";
        let base = verify_in(
            ("src/real.rs", REAL_OLD, REAL_NEW),
            &[("src/lib.rs", lib, lib)],
            "ready",
            "false",
        )?
        .ok_or("ungated out-of-line module refused")?;
        assert!(!base.cfg_gated);
        // A changed declaration of another module does not touch this chain.
        let new_lib = "mod real;\nmod other;\npub use real::ready;\n";
        let base = verify_in(
            ("src/real.rs", REAL_OLD, REAL_NEW),
            &[("src/lib.rs", lib, new_lib)],
            "ready",
            "false",
        )?
        .ok_or("an unrelated declaration change refused")?;
        assert!(!base.cfg_gated);
        Ok(())
    }

    /// Review N1 / cfg_mod_twin and cfg_mod_alias: the gate is on the
    /// parent's `mod real;`, invisible from `real.rs`.
    #[test]
    fn a_gate_on_the_parent_mod_declaration_is_reported() -> Result<(), String> {
        let twin = "#[cfg(not(test))]\nmod real;\n#[cfg(not(test))]\npub use real::ready;\n\n#[cfg(test)]\nmod fake;\n#[cfg(test)]\npub use fake::ready;\n";
        let fake = "pub fn ready() -> bool {\n    false\n}\n";
        let base = verify_in(
            ("src/real.rs", REAL_OLD, REAL_NEW),
            &[("src/lib.rs", twin, twin), ("src/fake.rs", fake, fake)],
            "ready",
            "false",
        )?
        .ok_or("identity refused")?;
        assert!(base.cfg_gated, "cfg_mod_twin");
        let alias = twin.replace("pub use fake::ready;", "pub use fake::stub as ready;");
        let stub = "pub fn stub() -> bool {\n    false\n}\n";
        let base = verify_in(
            ("src/real.rs", REAL_OLD, REAL_NEW),
            &[("src/lib.rs", &alias, &alias), ("src/fake.rs", stub, stub)],
            "ready",
            "false",
        )?
        .ok_or("identity refused")?;
        assert!(base.cfg_gated, "cfg_mod_alias");
        // A gate on an inline module enclosing the declaration, deeper in
        // the chain, or on the declaring file.
        for lib in [
            "#[cfg(not(test))]\nmod outer {\n    pub mod real;\n}\n",
            "#![cfg(not(test))]\nmod real;\n",
            "#[cfg_attr(test, path = \"fake.rs\")]\nmod real;\n",
            // Raw attribute names are the same attributes (#14953 review).
            "#[r#cfg(not(test))]\nmod real;\n",
            "#[r#cfg_attr(test, path = \"fake.rs\")]\nmod real;\n",
            "#[r#path = \"real.rs\"]\nmod real;\n",
            "#[r#cfg(not(test))]\nmod outer {\n    pub mod real;\n}\n",
        ] {
            let base = verify_in(
                ("src/real.rs", REAL_OLD, REAL_NEW),
                &[("src/lib.rs", lib, lib)],
                "ready",
                "false",
            )?
            .ok_or("identity refused")?;
            assert!(base.cfg_gated, "{lib}");
        }
        Ok(())
    }

    /// Review N1 / path_swap and path_swap_bare: the parent swaps `#[path]`
    /// targets while the files swap tails.
    #[test]
    fn a_changed_path_declaration_refuses_every_owner() -> Result<(), String> {
        let old_lib = "#[path = \"x.rs\"]\npub mod a;\n#[path = \"y.rs\"]\npub mod b;\n";
        let new_lib = "#[path = \"y.rs\"]\npub mod a;\n#[path = \"x.rs\"]\npub mod b;\n";
        let (x_old, x_new) = (REAL_OLD, REAL_NEW);
        let (y_old, y_new) = (REAL_NEW, REAL_OLD);
        let others = [("src/lib.rs", old_lib, new_lib), ("src/y.rs", y_old, y_new)];
        assert_eq!(
            verify_in(("src/x.rs", x_old, x_new), &others, "ready", "false")?,
            None
        );
        // An unchanged `#[path]` declaration only gates: the name mapping
        // cannot say which file it loads.
        let others = [("src/lib.rs", new_lib, new_lib)];
        let base = verify_in(("src/x.rs", x_old, x_new), &others, "ready", "false")?
            .ok_or("unchanged path declaration refused")?;
        assert!(base.cfg_gated);
        Ok(())
    }

    #[test]
    fn a_changed_declaration_on_the_chain_refuses() -> Result<(), String> {
        for (old_lib, new_lib) in [
            // An attribute added to the owner's declaration.
            ("mod real;\n", "#[cfg(not(test))]\nmod real;\n"),
            // A gate removed from it.
            ("#[cfg(not(test))]\nmod real;\n", "mod real;\n"),
            // An enclosing inline module's header.
            (
                "mod outer {\n    mod real;\n}\n",
                "#[cfg(test)]\nmod outer {\n    mod real;\n}\n",
            ),
            // An inner attribute of the declaring file.
            ("mod real;\n", "#![cfg(unix)]\nmod real;\n"),
        ] {
            assert_eq!(
                verify_in(
                    ("src/real.rs", REAL_OLD, REAL_NEW),
                    &[("src/lib.rs", old_lib, new_lib)],
                    "ready",
                    "false"
                )?,
                None,
                "{old_lib} -> {new_lib}"
            );
        }
        // A changed file whose new side is not available.
        let owner = changed_at("src/real.rs", REAL_OLD, REAL_NEW)?;
        let lib = changed_at("src/lib.rs", "mod real;\n", "mod real;\nfn f() {}\n")?;
        let modules =
            ModuleDeclarations::read([REAL_NEW], [(&owner, Some(REAL_NEW)), (&lib, None)]);
        assert_eq!(
            verified_base_owner(
                REAL_NEW,
                Path::new("src/real.rs"),
                &owner,
                &modules,
                "ready",
                2,
                2
            ),
            None
        );
        assert_eq!(
            verified_base_owner(
                REAL_NEW,
                Path::new("src/real.rs"),
                &owner,
                &ModuleDeclarations::unread(),
                "ready",
                2,
                2
            ),
            None
        );
        Ok(())
    }

    /// Review N2: `r#ready` and `ready` name one function.
    #[test]
    fn a_raw_identifier_twin_is_a_twin() -> Result<(), String> {
        let twin = |a: &str| {
            format!(
                "#[cfg(test)]\npub fn r#ready() -> bool {{\n    true\n}}\n\n#[cfg(not(test))]\npub fn ready() -> bool {{\n    {a}\n}}\n"
            )
        };
        let (old, new) = (twin("true"), twin("false"));
        assert_eq!(verify(&old, &new, "ready", "false")?, None);
        // A raw name alone still verifies.
        let old = "pub fn r#ready() -> bool {\n    true\n}\n";
        let new = "pub fn r#ready() -> bool {\n    false\n}\n";
        assert!(verify(old, new, "ready", "false")?.is_some());
        assert!(verify(old, new, "r#ready", "false")?.is_some());
        Ok(())
    }
}
