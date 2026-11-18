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
//! the owner's file. It also reads the gates that live outside a `mod`
//! declaration (#14952): a `cfg` on a `use` that re-exports the owner, a file
//! reached only through `include!`, and a `macro_rules!` that may emit a
//! module. The function's own inner attributes (`fn f() { #![cfg(..)] .. }`)
//! gate it too (#15003).

use super::{ChangedFile, reconstruct_old_lines_with_origins};
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasName, HasVisibility},
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
    /// on either side: on the function, an inner attribute of its body
    /// (#15003), an enclosing item or the owner file (an inner attribute);
    /// or, read from [`ModuleDeclarations`] on the new side, on any
    /// out-of-line `mod` declaration named like a component of the owner
    /// file's path, on an item enclosing it or on its declaring file; on a
    /// `use` that names the owner or a glob over a module named like a
    /// component of its path (#14952); or any `#[path]` declaration in the
    /// workspace (whose target the path cannot name). It is also set when
    /// the owner file's module chain is not established by plain `mod`
    /// declarations: no out-of-line declaration names its module, an
    /// `include!` resolves to its file, or a `macro_rules!` of its package
    /// (or an exported one) may emit an out-of-line module (#14952). Which
    /// `use` items, `include!` paths and macros count is narrowed in
    /// [`ModuleDeclarations`]. Identity holds regardless; a claim decides
    /// whether a gated function may be credited.
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
///   one, on any `#[path]` declaration, on a gated `use` naming the owner or
///   a module of its chain, on an `include!` naming its file, or on a
///   `macro_rules!` that may emit a module into its package, and read every
///   changed file.
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
    let name = unraw(name);
    let chain = module_chain_names(owner_file);
    if modules.chain_changed(owner_file, &chain, name) {
        return None;
    }
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
            || modules.chain_gated(owner_file, &chain, name)
            || !modules.declares(owner_file),
    })
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

/// Whether a `cfg` or `cfg_attr` attribute on the function, an inner
/// attribute of its body, an enclosing item or the file (an inner
/// attribute) may compile it out or change it.
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

/// `file`'s path components (split on either separator), with the index of
/// its anchor directory: the last `src`, or with none the last `tests`,
/// `examples` or `benches`. `None` when no such directory is on the path.
fn anchored_components(file: &Path) -> Option<(Vec<String>, usize)> {
    let components: Vec<String> = file
        .iter()
        .filter_map(|component| component.to_str())
        .flat_map(|component| component.split(['/', '\\']))
        .filter(|component| !component.is_empty())
        .map(str::to_string)
        .collect();
    let anchor = components
        .iter()
        .rposition(|component| component == "src")
        .or_else(|| {
            components.iter().rposition(|component| {
                matches!(component.as_str(), "tests" | "examples" | "benches")
            })
        })?;
    Some((components, anchor))
}

/// The module path of `file` inside its crate, read from its path under the
/// last `src` directory (or, with none, the last `tests`, `examples` or
/// `benches` directory): empty for a crate root (`src/lib.rs`,
/// `src/main.rs`, a file directly under `src/bin` or `tests`, or such a
/// directory's `main.rs`), otherwise each directory and the file stem, a
/// final `mod` dropped. `None` when no such directory is on the path.
fn module_path(file: &Path) -> Option<Vec<String>> {
    let (components, anchor) = anchored_components(file)?;
    let mut rest: Vec<String> = components
        .get(anchor + 1..)?
        .iter()
        .map(|component| {
            component
                .strip_suffix(".rs")
                .unwrap_or(component)
                .to_string()
        })
        .collect();
    let in_src = components
        .get(anchor)
        .is_some_and(|component| component == "src");
    // A target directory of its own: `src/bin`, or the anchor itself
    // outside `src`. `<dir>/x.rs` and `<dir>/x/main.rs` are roots;
    // `<dir>/x/a.rs` is module `a` of target `x`.
    let target_dir = !in_src || rest.first().is_some_and(|component| component == "bin");
    if in_src && target_dir {
        rest.drain(..1);
    }
    let root = match rest.as_slice() {
        [] => return None,
        [only] => target_dir || matches!(only.as_str(), "lib" | "main"),
        [_, second] => target_dir && second == "main",
        _ => false,
    };
    if root {
        return Some(Vec::new());
    }
    if target_dir {
        rest.drain(..1);
    }
    if rest.last().is_some_and(|component| component == "mod") {
        rest.pop();
    }
    Some(rest)
}

/// The package directory `file` belongs to: every path component before
/// its anchor directory (see [`module_path`]), joined with `/` (empty for a
/// package at the analysis root). Every target of one package shares it,
/// which over-approximates the crate a file belongs to. `None` when no
/// anchor directory is on the path.
fn package_scope(file: &Path) -> Option<String> {
    let (components, anchor) = anchored_components(file)?;
    Some(components.get(..anchor)?.join("/"))
}

/// `file` as `/`-separated components with `.` dropped and `..` applied.
/// `None` when a `..` climbs above the path's first component, or the path
/// is absolute (a root, a drive or a `\\` prefix).
fn normalized_path(file: &Path) -> Option<Vec<String>> {
    let text = file.to_str()?;
    if text.starts_with(['/', '\\']) || text.contains(':') {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for component in text.split(['/', '\\']) {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other.to_string()),
        }
    }
    Some(parts)
}

/// Whether one normalized path is a component suffix of the other, so a
/// path read relative to another root still matches.
fn same_file(a: &[String], b: &[String]) -> bool {
    !a.is_empty() && !b.is_empty() && (a.ends_with(b) || b.ends_with(a))
}

/// Where module-emitting macros may emit their modules (#14952 review R1):
/// the package that defines the macro, or, for an exported macro (a
/// `#[macro_export]` `macro_rules!`, a `macro` with a visibility) or one in
/// a file with no package anchor, any package in the workspace.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum MacroScope {
    Workspace,
    Package(String),
}

impl MacroScope {
    fn of(file: &Path, exported: bool) -> Self {
        match package_scope(file) {
            Some(package) if !exported => Self::Package(package),
            _ => Self::Workspace,
        }
    }
}

/// Whether some scope in `scopes` reaches `file`.
fn macro_scope_reaches(scopes: &BTreeSet<MacroScope>, file: &Path) -> bool {
    if scopes.contains(&MacroScope::Workspace) {
        return true;
    }
    match package_scope(file) {
        Some(package) => scopes.contains(&MacroScope::Package(package)),
        None => !scopes.is_empty(),
    }
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
/// to one refuses every owner.
///
/// Gates that live outside a `mod` declaration are read too (#14952):
///
/// - a `use` with a `cfg`/`cfg_attr` of its own, or a `pub` one under a
///   gated item or file, gates every owner it names (by its last path
///   segment or its rename) and, through a glob or `self`, every owner whose
///   path has a component named like one of its path's modules. Two kinds of
///   `use` are not gates. One rooted at a sysroot crate (`std`, `core`,
///   `alloc`, `proc_macro`, `test`, with or without a leading `::`) cannot
///   name a workspace module. A private `use super::name;` directly inside a
///   `#[cfg(test)]` module is not one when nothing else in that module can
///   bind `name` (no other `use` binding it, no glob but `super::*`, no item
///   or macro call naming it): either branch of its gate binds `name` to the
///   parent's item or to nothing. Every other root (`crate`, `self`,
///   `super`, and any other crate or module name) counts as crate-local,
///   since telling a third-party crate from a workspace package or a local
///   module would need the manifests;
/// - an owner file is gated unless an out-of-line declaration is named like
///   its module and every directory between it and its crate root is named
///   by some module (a file reached only through `include!`, however the
///   macro is spelled, has no declaration);
/// - an `include!` whose literal path resolves (against the including
///   file's directory) to the owner file gates that file; a literal that
///   cannot be resolved (absolute, or climbing above the including path)
///   gates its file stem by name;
/// - a `macro_rules!` (or `macro`) whose body may emit an out-of-line module
///   (`mod` followed by a name and no `{` body), an `include!` or a `#[path]`
///   (`path =`) gates every owner outside a crate root file of the package
///   that defines it, or of every package when it is exported.
///
/// A diff touching any of these, on either side, counts as a change to the
/// names, files or packages it gates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleDeclarations {
    /// Names of declarations with a `cfg`/`cfg_attr` on themselves, an
    /// enclosing item or their file; module names a gated `use` glob or
    /// `self` import covers; file stems an unresolvable `include!` names.
    gated: BTreeSet<String>,
    /// Item names a gated `use` imports or renames to.
    gated_names: BTreeSet<String>,
    /// Normalized paths of files an `include!` names.
    gated_files: BTreeSet<Vec<String>>,
    /// Names of every out-of-line `mod` declaration.
    declared: BTreeSet<String>,
    /// Names of every module, inline or out-of-line.
    modules: BTreeSet<String>,
    /// Where some `macro_rules!` or `macro` body may emit a module.
    macro_scopes: BTreeSet<MacroScope>,
    /// Some declaration carries `#[path]` (directly or through `cfg_attr`).
    retargeted: bool,
    /// Names of declarations the diff touched: an added or removed line on
    /// the declaration or its attributes, on the header of an inline module
    /// enclosing it, or on an inner attribute of its file.
    changed: BTreeSet<String>,
    /// Item names a gated `use` the diff touched imports or renames to.
    changed_names: BTreeSet<String>,
    /// Normalized paths of files a touched `include!` names.
    changed_files: BTreeSet<Vec<String>>,
    /// Where a touched macro that may emit a module could emit it.
    changed_macro_scopes: BTreeSet<MacroScope>,
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

    /// A cheap lexical superset of the files in which [`OutsideGates`] can
    /// find a gate, so a caller need not parse every workspace file: a
    /// `cfg` whose attribute runs into a `use` (or a comment) before the
    /// next `;` or `{`, a `cfg` anywhere beside a `pub` or `pub(..)` `use`,
    /// an `include!`, or a `macro_rules!`/`macro` definition beside a
    /// `mod`, `include` or `path` token. A file holding an out-of-line `mod`
    /// declaration is kept by the caller on its own.
    pub(crate) fn may_hold_outside_gate(source: &str) -> bool {
        let words_before = |offset: usize| source.get(..offset).unwrap_or_default().trim_end();
        let gated_use = source.contains("cfg")
            && (source.match_indices("cfg").any(|(start, _)| {
                let rest = source.get(start + 3..).unwrap_or_default();
                let window = rest.split([';', '{']).next().unwrap_or_default();
                window.contains("//") || window.contains("/*") || has_word(window, "use")
            }) || word_offsets(source, "use").any(|offset| {
                let before = words_before(offset);
                before.ends_with(')') || ends_with_word(before, "pub")
            }));
        let include = source.match_indices("include").any(|(start, word)| {
            source
                .get(start + word.len()..)
                .unwrap_or_default()
                .trim_start()
                .starts_with('!')
        });
        let macro_definition = (source.contains("macro_rules")
            || word_offsets(source, "macro").next().is_some())
            && (source.contains("mod") || source.contains("include") || source.contains("path"));
        gated_use || include || macro_definition
    }

    /// Read `declaring_files` (each workspace file that may declare a module
    /// or hold a gate, as its path and new-side text; a superset is fine)
    /// and `changed_files` (each changed file with its new-side text, `None`
    /// when it is not available). A non-Rust changed file is skipped.
    pub(crate) fn read<'a>(
        declaring_files: impl IntoIterator<Item = (&'a Path, &'a str)>,
        changed_files: impl IntoIterator<Item = (&'a ChangedFile, Option<&'a str>)>,
    ) -> Self {
        let mut reading = Self::default();
        for (file, source) in declaring_files {
            let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
                reading.unreadable = true;
                continue;
            };
            let root = parse.tree().syntax().clone();
            for module in root.descendants().filter_map(ast::Module::cast) {
                let Some(name) = module.name() else {
                    reading.unreadable = true;
                    continue;
                };
                let name = unraw(name.text()).to_string();
                reading.modules.insert(name.clone());
                if module.item_list().is_some() {
                    continue;
                }
                reading.declared.insert(name.clone());
                if module_retargets(&module) {
                    reading.retargeted = true;
                }
                if cfg_gated_node(module.syntax()) {
                    reading.gated.insert(name);
                }
            }
            let gates = OutsideGates::read(&root, file, |_| true);
            reading.macro_scopes.extend(gates.macro_scopes);
            reading.gated.extend(gates.modules);
            reading.gated_names.extend(gates.names);
            reading.gated_files.extend(gates.files);
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
            reading.note_touched(&changed.path, new_source, &added);
            if changed.removed_lines.is_empty() {
                continue;
            }
            let Some(old_source) = super::reconstruct_old_source(new_source, changed) else {
                reading.unreadable = true;
                continue;
            };
            let removed: BTreeSet<usize> =
                changed.removed_lines.iter().map(|line| line.line).collect();
            reading.note_touched(&changed.path, &old_source, &removed);
        }
        reading
    }

    /// Record every declaration of `source` (the text of `file`) that a line
    /// in `lines` touches.
    fn note_touched(&mut self, file: &Path, source: &str, lines: &BTreeSet<usize>) {
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
            // An out-of-line declaration itself, or every one an inline
            // module's header encloses.
            //
            // (Gates outside `mod` declarations are read below.)
            for declaration in std::iter::once(module.clone())
                .filter(|module| module.item_list().is_none())
                .chain(out_of_line_modules(module.syntax()))
            {
                if module_retargets(&declaration) {
                    self.changed_retarget = true;
                }
                match declaration.name() {
                    Some(name) => {
                        self.changed.insert(unraw(name.text()).to_string());
                    }
                    None => self.unreadable = true,
                }
            }
        }
        // A gated `use`, an `include!` or a module-emitting macro the diff
        // touched (or, under a touched file attribute, every one).
        let gates = OutsideGates::read(&root, file, |node| {
            let range = node.text_range();
            file_attrs_touched || touches(usize::from(range.start()), usize::from(range.end()))
        });
        self.changed.extend(gates.modules);
        self.changed_names.extend(gates.names);
        self.changed_files.extend(gates.files);
        self.changed_macro_scopes.extend(gates.macro_scopes);
    }

    /// Whether the diff may have changed which module `chain` (the names on
    /// `owner_file`'s path) names, or how its declarations, its file, or a
    /// `use` naming the function `name`, are gated.
    fn chain_changed(&self, owner_file: &Path, chain: &BTreeSet<String>, name: &str) -> bool {
        self.unreadable
            || self.changed_retarget
            || !self.changed.is_disjoint(chain)
            || self.changed_names.contains(name)
            || macro_scope_reaches(&self.changed_macro_scopes, owner_file)
            || included(&self.changed_files, owner_file)
    }

    /// Whether a declaration on `chain`, `owner_file` itself, or a `use`
    /// naming the function `name`, may be compiled out or retargeted.
    fn chain_gated(&self, owner_file: &Path, chain: &BTreeSet<String>, name: &str) -> bool {
        self.retargeted
            || !self.gated.is_disjoint(chain)
            || self.gated_names.contains(name)
            || included(&self.gated_files, owner_file)
    }

    /// Whether plain `mod` declarations establish `file`'s module chain: it
    /// is a crate root, or no macro of its package (or exported) may emit a
    /// module, an out-of-line declaration is named like its module, and
    /// some module is named like each directory above it. A file reached
    /// only through `include!` (or a macro) has no such declaration. Names
    /// approximate the chain, as in [`ModuleDeclarations::chain_gated`].
    fn declares(&self, file: &Path) -> bool {
        let Some(path) = module_path(file) else {
            return false;
        };
        let Some((leaf, directories)) = path.split_last() else {
            return true;
        };
        !macro_scope_reaches(&self.macro_scopes, file)
            && self.declared.contains(leaf)
            && directories
                .iter()
                .all(|directory| self.modules.contains(directory))
    }
}

/// Whether some path in `files` names `owner_file`. An owner path that
/// cannot be normalized counts as named whenever any file is.
fn included(files: &BTreeSet<Vec<String>>, owner_file: &Path) -> bool {
    match normalized_path(owner_file) {
        Some(owner) => files.iter().any(|file| same_file(file, &owner)),
        None => !files.is_empty(),
    }
}

/// Whether `text` holds `word` delimited by non-identifier characters.
fn has_word(text: &str, word: &str) -> bool {
    word_offsets(text, word).next().is_some()
}

fn is_ident_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Byte offsets of every occurrence of `word` in `text` delimited by
/// non-identifier characters.
fn word_offsets<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    text.match_indices(word)
        .map(|(start, _)| start)
        .filter(move |start| {
            let before = text.get(..*start).and_then(|head| head.chars().next_back());
            let after = text
                .get(*start + word.len()..)
                .and_then(|tail| tail.chars().next());
            !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char)
        })
}

/// Whether `text` ends with `word` preceded by a non-identifier character.
fn ends_with_word(text: &str, word: &str) -> bool {
    text.strip_suffix(word)
        .is_some_and(|head| !head.chars().next_back().is_some_and(is_ident_char))
}

/// What gates outside `mod` declarations say, read from the nodes of one
/// file that `select` keeps (#14952).
#[derive(Default)]
struct OutsideGates {
    /// Module names a gated glob or `self` import covers, and file stems an
    /// unresolvable `include!` names.
    modules: BTreeSet<String>,
    /// Item names a gated `use` imports or renames to.
    names: BTreeSet<String>,
    /// Normalized paths of files an `include!` names.
    files: BTreeSet<Vec<String>>,
    /// Where a macro body that may emit a module could emit it.
    macro_scopes: BTreeSet<MacroScope>,
}

impl OutsideGates {
    fn read(root: &SyntaxNode, file: &Path, select: impl Fn(&SyntaxNode) -> bool) -> Self {
        let mut gates = Self::default();
        for node in root.descendants() {
            if let Some(item) = ast::Use::cast(node.clone()) {
                if select(&node) && use_gated(&item) {
                    gates.read_use(&item);
                }
            } else if let Some(call) = ast::MacroCall::cast(node.clone()) {
                if select(&node) && macro_named(&call, "include") {
                    gates.read_include(file, &call);
                }
            } else if let Some(rules) = ast::MacroRules::cast(node.clone()) {
                if select(&node) && rules.token_tree().is_none_or(|body| emits_module(&body)) {
                    gates
                        .macro_scopes
                        .insert(MacroScope::of(file, macro_exported(&rules)));
                }
            } else if let Some(definition) = ast::MacroDef::cast(node.clone())
                && select(&node)
                && definition.body().is_none_or(|body| emits_module(&body))
            {
                gates
                    .macro_scopes
                    .insert(MacroScope::of(file, definition.visibility().is_some()));
            }
        }
        gates
    }

    /// Every name a gated `use` brings in: each leaf's last segment and
    /// rename, and for a glob or `self` leaf every segment of its path. A
    /// leaf rooted at a sysroot crate, or a redundant `super::name` import
    /// of a test module, brings in none (see [`ModuleDeclarations`]).
    fn read_use(&mut self, item: &ast::Use) {
        let Some(tree) = item.use_tree() else {
            return;
        };
        for leaf in std::iter::once(tree.clone())
            .chain(
                tree.syntax()
                    .descendants()
                    .skip(1)
                    .filter_map(ast::UseTree::cast),
            )
            .filter(|tree| tree.use_tree_list().is_none())
        {
            let path = use_leaf_path(&leaf);
            if sysroot_rooted(item, &path) || redundant_test_import(item, &leaf, &path) {
                continue;
            }
            if let Some(rename) = leaf.rename().and_then(|rename| rename.name()) {
                self.names.insert(unraw(rename.text()).to_string());
            }
            let own_last = leaf
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .map(|name| unraw(name.text()).to_string());
            match own_last {
                Some(last) if leaf.star_token().is_none() && last != "self" => {
                    self.names.insert(last);
                }
                _ => self.modules.extend(path),
            }
        }
    }

    /// The file a literal `include!("..")` names, resolved against the
    /// directory of `file`, or its stem when it cannot be resolved. A path
    /// it cannot read (`concat!(env!("OUT_DIR"), ..)`) names a generated
    /// file, not a workspace source.
    fn read_include(&mut self, file: &Path, call: &ast::MacroCall) {
        let Some(literal) = call.token_tree().and_then(|tree| {
            tree.syntax()
                .children_with_tokens()
                .filter_map(|element| element.into_token())
                .find(|token| token.kind() == SyntaxKind::STRING)
        }) else {
            return;
        };
        let text = literal.text().trim_matches('"');
        let resolved = file
            .parent()
            .map(|directory| directory.join(text))
            .and_then(|path| normalized_path(&path));
        match resolved {
            Some(path) if !path.is_empty() => {
                self.files.insert(path);
            }
            _ => {
                if let Some(stem) = Path::new(text).file_stem().and_then(|stem| stem.to_str()) {
                    self.modules.insert(stem.to_string());
                }
            }
        }
    }
}

/// Every segment of the path a `use` leaf imports, outermost first: its
/// enclosing trees' prefixes, then its own path, `r#` removed.
fn use_leaf_path(leaf: &ast::UseTree) -> Vec<String> {
    let mut trees: Vec<ast::UseTree> = leaf
        .syntax()
        .ancestors()
        .filter_map(ast::UseTree::cast)
        .collect();
    trees.reverse();
    trees
        .iter()
        .filter_map(|tree| tree.path())
        .flat_map(|path| {
            path.segments()
                .filter_map(|segment| segment.name_ref())
                .map(|name| unraw(name.text()).to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether the `use` leaf `leaf` of `item` (full path `path`) is a private
/// `use super::name;` directly inside a `#[cfg(test)]` module in which
/// nothing else can bind `name` (#14952 review R2). Its own gate then only
/// decides whether `name` names the parent's item or nothing, so it cannot
/// swap the item a test reaches. Anything else in the module that may bind
/// `name` (see [`scope_may_bind`]; only a `super::*` glob is harmless) keeps
/// the gate.
fn redundant_test_import(item: &ast::Use, leaf: &ast::UseTree, path: &[String]) -> bool {
    let [root, name] = path else {
        return false;
    };
    if root != "super"
        || name == "self"
        || item.visibility().is_some()
        || leaf.rename().is_some()
        || leaf.star_token().is_some()
    {
        return false;
    }
    let Some(module) = item
        .syntax()
        .parent()
        .filter(|parent| parent.kind() == SyntaxKind::ITEM_LIST)
        .and_then(|list| list.parent())
        .and_then(ast::Module::cast)
    else {
        return false;
    };
    let under_test = module
        .syntax()
        .ancestors()
        .filter_map(ast::Module::cast)
        .any(|module| ast::HasAttrs::attrs(&module).any(|attr| is_cfg_test(&attr)));
    let Some(items) = module.item_list() else {
        return false;
    };
    under_test
        && !scope_may_bind(items.syntax(), item.syntax(), name, |glob| {
            glob == ["super"]
        })
}

/// Whether the `use` leaf path `path` of `item` is rooted at a sysroot
/// crate (`std`, `core`, `alloc`, `proc_macro`, `test`) that no item of the
/// module scope holding `item` may shadow (#14952 review R3). A bare first
/// segment of a `use` path names a crate or an item of the current module
/// (both at once is ambiguous), so with no such item it names the crate,
/// which cannot hold a workspace function. A `use` in a block, which also
/// sees its enclosing module's items, is never excluded.
fn sysroot_rooted(item: &ast::Use, path: &[String]) -> bool {
    let Some(root) = path.first() else {
        return false;
    };
    if !is_sysroot(root) {
        return false;
    }
    let Some(scope) = item.syntax().parent().filter(|parent| {
        matches!(
            parent.kind(),
            SyntaxKind::ITEM_LIST | SyntaxKind::SOURCE_FILE
        )
    }) else {
        return false;
    };
    !scope_may_bind(&scope, item.syntax(), root, |glob| {
        glob.first().is_some_and(|root| is_sysroot(root))
    })
}

fn is_sysroot(root: &str) -> bool {
    matches!(root, "std" | "core" | "alloc" | "proc_macro" | "test")
}

/// Whether some item of `scope` other than `except` may bind `name`: a
/// `use` leaf binding it (by its last segment or rename), a glob that
/// `harmless_glob` does not accept, an item or `extern crate` named `name`,
/// or a macro call whose tokens mention it.
fn scope_may_bind(
    scope: &SyntaxNode,
    except: &SyntaxNode,
    name: &str,
    harmless_glob: impl Fn(&[String]) -> bool,
) -> bool {
    scope.children().any(|node| {
        if &node == except {
            return false;
        }
        if ast::Use::can_cast(node.kind()) {
            return node
                .descendants()
                .filter_map(ast::UseTree::cast)
                .filter(|tree| tree.use_tree_list().is_none())
                .any(|tree| {
                    let full = use_leaf_path(&tree);
                    if tree.star_token().is_some() {
                        return !harmless_glob(&full);
                    }
                    let bound = tree
                        .rename()
                        .and_then(|rename| rename.name())
                        .map(|rename| unraw(rename.text()).to_string())
                        .or_else(|| full.last().cloned());
                    bound.as_deref() == Some(name)
                });
        }
        if ast::MacroCall::can_cast(node.kind()) {
            return node.descendants_with_tokens().any(|element| {
                element
                    .into_token()
                    .is_some_and(|token| unraw(token.text()) == name)
            });
        }
        if let Some(extern_crate) = ast::ExternCrate::cast(node.clone()) {
            let bound = extern_crate
                .rename()
                .and_then(|rename| rename.name())
                .map(|rename| unraw(rename.text()).to_string())
                .or_else(|| {
                    extern_crate
                        .name_ref()
                        .map(|name| unraw(name.text()).to_string())
                });
            return bound.as_deref() == Some(name);
        }
        node.children()
            .filter_map(ast::Name::cast)
            .any(|found| unraw(found.text()) == name)
    })
}

/// Whether `attr` is the plain `#[cfg(test)]` gate, read through the
/// cfg-predicate authority. Any other spelling (a raw `r#cfg`, a
/// `cfg_attr`, a compound predicate) keeps the import a gate.
fn is_cfg_test(attr: &ast::Attr) -> bool {
    crate::analysis::cfg_predicates::attribute_is_plain_test_gate(&attr.syntax().text().to_string())
}

/// Whether a `macro_rules!` is exported beyond its crate: a
/// `#[macro_export]`, directly or inside a `cfg_attr`.
fn macro_exported(rules: &ast::MacroRules) -> bool {
    ast::HasAttrs::attrs(rules).any(|attr| match attr_name(&attr).as_deref() {
        Some("macro_export") => true,
        Some("cfg_attr") => attr.syntax().text().to_string().contains("macro_export"),
        _ => false,
    })
}

/// Whether a `use` may be compiled out or swapped: a `cfg`/`cfg_attr` of its
/// own, or, for a `pub` (re-exporting) one, on an enclosing item or its
/// file. A private import gated only through its enclosing module (the test
/// module's `use super::f`) is that module's own scope, not a re-export.
fn use_gated(item: &ast::Use) -> bool {
    ast::HasAttrs::attrs(item).any(|attr| is_cfg_attr(&attr))
        || item.visibility().is_some() && cfg_gated_node(item.syntax())
}

/// Whether `call` invokes a macro whose last path segment is `name`.
fn macro_named(call: &ast::MacroCall, name: &str) -> bool {
    call.path()
        .and_then(|path| path.segment())
        .and_then(|segment| segment.name_ref())
        .is_some_and(|found| unraw(found.text()) == name)
}

/// Whether a macro body may emit an out-of-line module or retarget a file
/// (#14952 review R1): a `mod` followed by a name (`$m`, `$m:ident` or a
/// plain one) and anything but a `{` body, or by no name at all
/// (`mod $($tt)*`); an `include` followed by `!`; or a `path` followed by
/// `=` (`#[path = ..]`, `cfg_attr(.., path = ..)`). A macro emitting only
/// inline `mod name { .. }` modules does not.
fn emits_module(body: &ast::TokenTree) -> bool {
    let tokens: Vec<String> = body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .map(|token| unraw(token.text()).to_string())
        .collect();
    tokens.iter().enumerate().any(|(index, token)| {
        let rest = tokens.get(index + 1..).unwrap_or_default();
        match token.as_str() {
            "mod" => declares_out_of_line(rest),
            "include" => rest.first().is_some_and(|next| next == "!"),
            "path" => rest.first().is_some_and(|next| next == "="),
            _ => false,
        }
    })
}

/// Whether the tokens after a `mod` keyword may make it an out-of-line
/// declaration: the name (with a `$` and a `:fragment` it may carry) is
/// not followed by a `{` body.
fn declares_out_of_line(rest: &[String]) -> bool {
    let rest = match rest.first() {
        Some(dollar) if dollar == "$" => rest.get(1..).unwrap_or_default(),
        _ => rest,
    };
    let Some((name, rest)) = rest.split_first() else {
        return true;
    };
    if !name.chars().all(is_ident_char) {
        return true;
    }
    let rest = match rest.first() {
        Some(colon) if colon == ":" => rest.get(2..).unwrap_or_default(),
        _ => rest,
    };
    rest.first().is_none_or(|next| next != "{")
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

/// An attribute's name: the last segment of its path with any `r#` removed,
/// so `#[r#cfg(..)]` reads as `cfg`. `None` when it has no readable path.
fn attr_name(attr: &ast::Attr) -> Option<String> {
    if let Some(name) = attr.simple_name() {
        return Some(unraw(&name).to_string());
    }
    let name = attr.path()?.segment()?.name_ref()?;
    Some(unraw(name.text()).to_string())
}

/// [`cfg_gated`] for any node: a `cfg`/`cfg_attr` on it, an inner attribute
/// of its own body or item list (`fn f() { #![cfg(..)] .. }` or
/// `mod m { #![cfg(..)] .. }`, #15003), an enclosing item or its file.
/// The enclosing items' bodies and lists are ancestors, so their inner
/// attributes are read by the walk; the node's own are descendants.
fn cfg_gated_node(node: &SyntaxNode) -> bool {
    node.ancestors()
        .chain(own_inner_attribute_holders(node))
        .any(|node| has_cfg(&node))
}

/// The nodes holding `node`'s own inner attributes: its item list, or a
/// function's body block and that block's statement list.
fn own_inner_attribute_holders(node: &SyntaxNode) -> impl Iterator<Item = SyntaxNode> {
    node.children()
        .filter(|child| is_list(child.kind()))
        .flat_map(|list| {
            let statements = list
                .children()
                .filter(|child| child.kind() == SyntaxKind::STMT_LIST)
                .collect::<Vec<_>>();
            std::iter::once(list).chain(statements)
        })
        .collect::<Vec<_>>()
        .into_iter()
}

/// Whether `node` carries a `cfg`/`cfg_attr` attribute, outer or inner.
fn has_cfg(node: &SyntaxNode) -> bool {
    ast::AnyHasAttrs::cast(node.clone())
        .is_some_and(|owner| ast::HasAttrs::attrs(&owner).any(|attr| is_cfg_attr(&attr)))
}

fn is_cfg_attr(attr: &ast::Attr) -> bool {
    matches!(attr_name(attr).as_deref(), Some("cfg" | "cfg_attr"))
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
        let modules =
            ModuleDeclarations::read([(Path::new("src/lib.rs"), new)], [(&changed, Some(new))]);
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
        let modules = ModuleDeclarations::read(
            [(Path::new("src/lib.rs"), new.as_str())],
            [(&changed_file, Some(new.as_str()))],
        );
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
        let sources = std::iter::once((Path::new(path), new)).chain(
            others
                .iter()
                .map(|(other, _, new)| (Path::new(*other), *new)),
        );
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
        let modules = ModuleDeclarations::read(
            [(Path::new("src/real.rs"), REAL_NEW)],
            [(&owner, Some(REAL_NEW)), (&lib, None)],
        );
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

    /// #15003: a function-body inner attribute applies to the function, but
    /// it is a descendant of the function node, not an ancestor.
    #[test]
    fn a_body_inner_cfg_gates_the_function() -> Result<(), String> {
        let body = |attribute: &str, tail: &str| {
            format!("pub fn ready() -> bool {{\n    {attribute}\n    {tail}\n}}\n")
        };
        for attribute in [
            "#![cfg(any())]",
            "#![cfg_attr(test, cfg(any()))]",
            "#![r#cfg(not(test))]",
        ] {
            let (old, new) = (body(attribute, "true"), body(attribute, "false"));
            let base = verify(&old, &new, "ready", "false")?.ok_or("identity refused")?;
            assert!(base.cfg_gated, "{attribute}");
        }
        // Inside an impl and a module too.
        let nested = |tail: &str| {
            format!(
                "pub struct A;\nimpl A {{\n    pub fn ready() -> bool {{\n        #![cfg(any())]\n        {tail}\n    }}\n}}\n"
            )
        };
        let base = verify(&nested("true"), &nested("false"), "ready", "false")?
            .ok_or("identity refused")?;
        assert!(base.cfg_gated, "impl method");
        // The gate appears only on the new side (the attribute line is in the
        // body, which identity does not compare).
        let old = "pub fn ready() -> bool {\n    true\n}\n";
        let new = "pub fn ready() -> bool {\n    #![cfg(any())]\n    false\n}\n";
        let base = verify(old, new, "ready", "false")?.ok_or("identity refused")?;
        assert!(base.cfg_gated, "new-side body gate");
        // Controls: a non-cfg inner attribute, and a `cfg` on a statement
        // (which gates the statement, not the function).
        for attribute in ["#![allow(unused)]", "#[cfg(any())]\n    let _x = 1;"] {
            let (old, new) = (body(attribute, "true"), body(attribute, "false"));
            let base = verify(&old, &new, "ready", "false")?.ok_or("identity refused")?;
            assert!(!base.cfg_gated, "{attribute}");
        }
        // A nested function's own body attribute does not gate its parent.
        let parent = |tail: &str| {
            format!(
                "pub fn ready() -> bool {{\n    fn inner() {{\n        #![cfg(any())]\n    }}\n    {tail}\n}}\n"
            )
        };
        let base = verify(&parent("true"), &parent("false"), "ready", "false")?
            .ok_or("identity refused")?;
        assert!(!base.cfg_gated, "nested function's body attribute");
        Ok(())
    }

    /// #14952: the four shapes whose gate lives outside a `mod` declaration
    /// (inc_cfg, inc_mod, cfg_use, macro_mod_decl), plus spelling variants.
    #[test]
    fn gates_outside_mod_declarations_are_reported() -> Result<(), String> {
        let stub = "pub fn stub() -> bool {\n    false\n}\n";
        for (owner, others) in [
            // inc_cfg: reached only through a gated `include!`.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "#[cfg(not(test))]\ninclude!(\"real.rs\");\n#[cfg(test)]\nmod fake;\n#[cfg(test)]\npub use fake::stub as ready;\n",
                )],
            ),
            // The same through a renamed macro: no declaration names it.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "use core::include as inc;\n#[cfg(not(test))]\ninc!(\"real.rs\");\n",
                )],
            ),
            // An `include!` of a file whose stem a declaration also names.
            (
                "src/real.rs",
                vec![
                    (
                        "src/lib.rs",
                        "mod a;\n#[cfg(not(test))]\nmod shim {\n    include!(\"real.rs\");\n}\n",
                    ),
                    ("src/a.rs", "mod real;\n"),
                ],
            ),
            // inc_mod: an inline gated module including the owner's file.
            (
                "src/real_impl.rs",
                vec![(
                    "src/lib.rs",
                    "#[cfg(not(test))]\nmod real {\n    include!(\"real_impl.rs\");\n}\n",
                )],
            ),
            // cfg_use: only the re-export is gated.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(not(test))]\npub use real::ready;\n#[cfg(test)]\nmod fake;\n#[cfg(test)]\npub use fake::stub as ready;\n",
                )],
            ),
            // A gated rename to the owner's name alone.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\npub use real::ready;\nmod fake;\n#[cfg(test)]\npub use fake::stub as ready;\n",
                )],
            ),
            // Grouped, glob and `self` re-exports; `cfg_attr`; raw names.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(not(test))]\npub use real::{other, ready};\n",
                )],
            ),
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(not(test))]\npub use real::*;\n",
                )],
            ),
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(not(test))]\npub use self::real::{self};\n",
                )],
            ),
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg_attr(test, cfg(any()))]\npub use real::r#ready;\n",
                )],
            ),
            // A re-export inside a gated module.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(not(test))]\nmod shim {\n    pub use crate::real::ready;\n}\n",
                )],
            ),
            // macro_mod_decl: a macro emits the gated declaration.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "macro_rules! declare {\n    ($m:ident) => {\n        #[cfg(not(test))]\n        pub mod $m;\n    };\n}\ndeclare!(real);\n#[cfg(test)]\npub mod real {\n    pub fn ready() -> bool {\n        false\n    }\n}\n",
                )],
            ),
            // A macro emitting a module elsewhere, beside a plain declaration.
            (
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\nmacro_rules! declare {\n    ($m:ident) => { mod $m; };\n}\n",
                )],
            ),
            // No declaration at all, and no anchor directory.
            ("src/real.rs", vec![("src/lib.rs", "pub fn other() {}\n")]),
            ("real.rs", vec![("lib.rs", "mod real;\n")]),
            // A directory no module names.
            ("src/a/real.rs", vec![("src/lib.rs", "mod real;\n")]),
        ] {
            let mut files: Vec<(&str, &str, &str)> = others
                .iter()
                .map(|(path, text)| (*path, *text, *text))
                .collect();
            files.push(("src/fake.rs", stub, stub));
            let base = verify_in((owner, REAL_OLD, REAL_NEW), &files, "ready", "false")?
                .ok_or("identity refused")?;
            assert!(base.cfg_gated, "{owner}: {others:?}");
        }
        Ok(())
    }

    /// #14952 controls (pos_plain, pos_unrelated_mod, pos_unrelated_gated):
    /// unrelated gates and the test module's own imports keep credit.
    #[test]
    fn unrelated_gates_outside_mod_declarations_keep_credit() -> Result<(), String> {
        for (owner, lib) in [
            ("src/real.rs", "mod real;\npub use real::ready;\n"),
            ("src/real.rs", "mod real;\n#[cfg(test)]\nmod other;\n"),
            (
                "src/real.rs",
                "mod real;\nmod other;\n#[cfg(not(test))]\npub use other::{g, h as k};\n",
            ),
            (
                "src/real.rs",
                "mod real;\n#[cfg(test)]\nmod tests {\n    use super::real::ready;\n    use super::*;\n}\n",
            ),
            (
                "src/real.rs",
                "mod real;\ninclude!(concat!(env!(\"OUT_DIR\"), \"/gen.rs\"));\ninclude!(\"other.rs\");\n",
            ),
            (
                "src/real.rs",
                "mod real;\nmacro_rules! m {\n    () => { fn x() {} };\n}\n",
            ),
            ("src/a/real.rs", "mod a {\n    pub mod real;\n}\n"),
            ("src/a/real/mod.rs", "mod a;\n"),
            ("crates/x/src/bin/tool/real.rs", "mod real;\n"),
            // Crate roots need no declaration.
            (
                "src/main.rs",
                "macro_rules! m {\n    () => { mod x; };\n}\n",
            ),
            ("src/bin/tool.rs", ""),
            ("tests/it.rs", ""),
            ("examples/demo/main.rs", ""),
        ] {
            let mut others = vec![("src/lib.rs", lib, lib)];
            if owner == "src/a/real/mod.rs" {
                others.push(("src/a.rs", "pub mod real;\n", "pub mod real;\n"));
            }
            let base = verify_in((owner, REAL_OLD, REAL_NEW), &others, "ready", "false")?
                .ok_or("identity refused")?;
            assert!(!base.cfg_gated, "{owner}: {lib}");
        }
        Ok(())
    }

    /// #14952: a diff that adds or removes a gate outside a declaration
    /// refuses, as a changed declaration does.
    #[test]
    fn a_changed_gate_outside_mod_declarations_refuses() -> Result<(), String> {
        for (old_lib, new_lib) in [
            (
                "mod real;\npub use real::ready;\n",
                "mod real;\n#[cfg(not(test))]\npub use real::ready;\n",
            ),
            (
                "mod real;\n#[cfg(not(test))]\npub use real::*;\n",
                "mod real;\npub use real::*;\n",
            ),
            (
                "mod real;\n",
                "mod real;\n#[cfg(test)]\nmod shim {\n    include!(\"real.rs\");\n}\n",
            ),
            (
                "mod real;\nmacro_rules! m {\n    () => { mod x; };\n}\n",
                "mod real;\n",
            ),
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
        // An ungated import added beside the change keeps identity.
        let base = verify_in(
            ("src/real.rs", REAL_OLD, REAL_NEW),
            &[(
                "src/lib.rs",
                "mod real;\n",
                "mod real;\npub use real::ready;\n",
            )],
            "ready",
            "false",
        )?
        .ok_or("an ungated re-export refused")?;
        assert!(!base.cfg_gated);
        Ok(())
    }

    #[test]
    fn module_paths_follow_cargo_layout() {
        let path = |file: &str| module_path(Path::new(file));
        let owned = |parts: &[&str]| Some(parts.iter().map(|part| (*part).to_string()).collect());
        assert_eq!(path("src/lib.rs"), owned(&[]));
        assert_eq!(path("crates/x/src/main.rs"), owned(&[]));
        assert_eq!(path("work/src/proj/src/real.rs"), owned(&["real"]));
        assert_eq!(path("src/a/b/mod.rs"), owned(&["a", "b"]));
        assert_eq!(path("src/bin/tool.rs"), owned(&[]));
        assert_eq!(path("src/bin/tool/main.rs"), owned(&[]));
        assert_eq!(path("src/bin/tool/cli.rs"), owned(&["cli"]));
        assert_eq!(path("tests/it.rs"), owned(&[]));
        assert_eq!(path("tests/it/common.rs"), owned(&["common"]));
        assert_eq!(path("benches/b/main.rs"), owned(&[]));
        assert_eq!(path("build.rs"), None);
        assert_eq!(path("src"), None);
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

    /// #14952 review R1: only a body that may emit an out-of-line module, an
    /// `include!` or a `#[path]` emits a module.
    #[test]
    fn macro_bodies_emit_modules_only_out_of_line() -> Result<(), String> {
        let emits = |body: &str| -> Result<bool, String> {
            let source = format!("macro_rules! m {{\n    {body}\n}}\n");
            let parse = crate::analysis::syntax::parse_clean_source_file(&source)
                .ok_or("fixture premise: macro did not parse")?;
            let rules = parse
                .tree()
                .syntax()
                .descendants()
                .find_map(ast::MacroRules::cast)
                .ok_or("fixture premise: no macro_rules")?;
            let tree = rules.token_tree().ok_or("fixture premise: no body")?;
            Ok(emits_module(&tree))
        };
        for body in [
            "($m:ident) => { mod $m; };",
            "($m:ident) => { pub(crate) mod $m; };",
            "($m:ident) => { #[cfg(test)] mod $m; };",
            "() => { mod real; };",
            "($($t:tt)*) => { mod $($t)* };",
            "($f:literal) => { include!($f); };",
            "($p:literal, $m:ident) => { #[path = $p] mod $m {} };",
            "($p:literal) => { #[cfg_attr(test, path = $p)] mod real {} };",
        ] {
            assert!(emits(body)?, "{body}");
        }
        for body in [
            "($m:ident) => { mod $m { pub fn f() {} } };",
            "() => { mod inner { fn x() {} } };",
            "() => { fn x() {} };",
            "($p:path) => { $p() };",
            "() => { include_str!(\"x.txt\") };",
        ] {
            assert!(!emits(body)?, "{body}");
        }
        Ok(())
    }

    #[test]
    fn package_scopes_and_normalized_paths_follow_the_layout() {
        let scope = |file: &str| package_scope(Path::new(file));
        assert_eq!(scope("src/lib.rs").as_deref(), Some(""));
        assert_eq!(scope("tests/it.rs").as_deref(), Some(""));
        assert_eq!(scope("crates/x/src/a/b.rs").as_deref(), Some("crates/x"));
        assert_eq!(scope("crates/x/tests/it.rs").as_deref(), Some("crates/x"));
        assert_eq!(scope("build.rs"), None);
        let normal = |file: &str| normalized_path(Path::new(file));
        let owned = |parts: &[&str]| Some(parts.iter().map(|part| (*part).to_string()).collect());
        assert_eq!(normal("src/../data/real.rs"), owned(&["data", "real.rs"]));
        assert_eq!(normal("./src/./real.rs"), owned(&["src", "real.rs"]));
        assert_eq!(normal("src\\real.rs"), owned(&["src", "real.rs"]));
        assert_eq!(normal("../real.rs"), None);
        assert_eq!(normal("/abs/real.rs"), None);
        assert_eq!(normal("drive:real.rs"), None);
    }

    /// #14952 review R1-R3 at the identity layer: the narrowed shapes keep
    /// credit, and the crate-local shapes beside them stay gated.
    #[test]
    fn narrowed_outside_gates_keep_credit_and_local_ones_gate() -> Result<(), String> {
        let gated = |owner: &str, others: &[(&str, &str)]| -> Result<bool, String> {
            let files: Vec<(&str, &str, &str)> = others
                .iter()
                .map(|(path, text)| (*path, *text, *text))
                .collect();
            Ok(
                verify_in((owner, REAL_OLD, REAL_NEW), &files, "ready", "false")?
                    .ok_or("identity refused")?
                    .cfg_gated,
            )
        };
        let declare = "macro_rules! declare {\n    ($m:ident) => { mod $m; };\n}\n";
        let exported =
            "#[macro_export]\nmacro_rules! declare {\n    ($m:ident) => { mod $m; };\n}\n";
        let tests_module = |extra: &str| {
            format!(
                "mod real;\npub use real::ready;\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[cfg(unix)]\n    use super::ready;\n    {extra}\n}}\n"
            )
        };
        let plain = tests_module("");
        let own_package = format!("mod real;\n{declare}");
        let beside_fn = tests_module("fn ready() -> bool { true }");
        let beside_macro = tests_module("make!(ready);");
        let renamed = plain.replace("use super::ready;", "use super::fake::ready;");
        for (case, owner, others, expected) in [
            // R1: inline-only macro; another package's macro, unless exported.
            (
                "inline macro",
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\nmacro_rules! w {\n    ($m:ident) => { mod $m { fn f() {} } };\n}\n",
                )],
                false,
            ),
            (
                "other package",
                "src/real.rs",
                vec![("src/lib.rs", "mod real;\n"), ("other/src/lib.rs", declare)],
                false,
            ),
            (
                "other package, exported",
                "src/real.rs",
                vec![
                    ("src/lib.rs", "mod real;\n"),
                    ("other/src/lib.rs", exported),
                ],
                true,
            ),
            (
                "other package, its own owner",
                "other/src/real.rs",
                vec![("other/src/lib.rs", own_package.as_str())],
                true,
            ),
            // R2: a redundant `super::ready` import in a test module.
            (
                "test import",
                "src/real.rs",
                vec![("src/lib.rs", plain.as_str())],
                false,
            ),
            (
                "test import beside a local fn",
                "src/real.rs",
                vec![("src/lib.rs", beside_fn.as_str())],
                true,
            ),
            (
                "test import beside a macro call naming it",
                "src/real.rs",
                vec![("src/lib.rs", beside_macro.as_str())],
                true,
            ),
            (
                "test import renamed",
                "src/real.rs",
                vec![("src/lib.rs", renamed.as_str())],
                true,
            ),
            // R3: sysroot roots, unless a module item shadows the root.
            (
                "std rename",
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(unix)]\nuse core::convert::identity as ready;\n",
                )],
                false,
            ),
            (
                "shadowed core",
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\nmod core;\n#[cfg(unix)]\nuse core::stub as ready;\n",
                )],
                true,
            ),
            (
                "std rename in a block",
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\nfn f() {\n    #[cfg(unix)]\n    use std::fmt::Write as ready;\n}\n",
                )],
                true,
            ),
            (
                "include of another file with the owner's stem",
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(unix)]\nmod data {\n    include!(\"../data/real.rs\");\n}\n",
                )],
                false,
            ),
            (
                "unresolvable include falls back to the stem",
                "src/real.rs",
                vec![(
                    "src/lib.rs",
                    "mod real;\n#[cfg(unix)]\nmod data {\n    include!(\"../../real.rs\");\n}\n",
                )],
                true,
            ),
        ] {
            assert_eq!(gated(owner, &others)?, expected, "{case}");
        }
        Ok(())
    }

    /// The caller's lexical prefilter keeps every file in which the scanner
    /// finds a gate outside `mod` declarations, and drops the common shapes
    /// that hold none.
    #[test]
    fn the_outside_gate_prefilter_is_a_superset_of_the_scanner() -> Result<(), String> {
        for source in [
            "mod real;\n#[cfg(not(test))]\npub use real::ready;\n",
            "#[cfg(unix)]\n/// docs; with a semicolon\nuse fake::stub as ready;\n",
            "#[cfg_attr(test, cfg(any()))]\npub use real::r#ready;\n",
            "#[cfg(not(test))]\nmod shim {\n    pub use crate::real::ready;\n}\n",
            "#![cfg(unix)]\npub(crate) use crate::real::ready;\n",
            "#[cfg(not(test))]\nmod shim {\n    include ! (\"real.rs\");\n}\n",
            "macro_rules! declare {\n    ($m:ident) => { mod $m; };\n}\n",
            "macro_rules! inc {\n    ($f:literal) => { include!($f); };\n}\n",
        ] {
            let parse = crate::analysis::syntax::parse_clean_source_file(source)
                .ok_or("fixture premise: source did not parse")?;
            let gates =
                OutsideGates::read(parse.tree().syntax(), Path::new("src/lib.rs"), |_| true);
            assert!(
                !gates.names.is_empty()
                    || !gates.modules.is_empty()
                    || !gates.files.is_empty()
                    || !gates.macro_scopes.is_empty(),
                "fixture premise: no gate in {source}"
            );
            assert!(
                ModuleDeclarations::may_hold_outside_gate(source),
                "{source}"
            );
        }
        for source in [
            "pub fn other() {}\n",
            "#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() {}\n}\n",
            "#[derive(Debug)]\npub struct S;\n// includes nothing\n",
        ] {
            assert!(
                !ModuleDeclarations::may_hold_outside_gate(source),
                "{source}"
            );
        }
        Ok(())
    }
}
