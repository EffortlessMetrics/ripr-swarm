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
//! the owner's file, as are gates outside a `mod` declaration (#14952) and
//! the function's own inner attributes (#15003).

use super::{ChangedFile, reconstruct_old_lines_with_origins};
use ra_ap_syntax::{
    AstNode, AstToken, SyntaxKind, SyntaxNode, SyntaxToken,
    ast::{self, HasName, HasVisibility},
};
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

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
    /// `use` that names the owner or whose path names a module named like a
    /// component of its path (#14952, #15035); or any `#[path]` declaration
    /// or `include!` of an unreadable path in the workspace (whose target
    /// the path cannot name). It is also set when
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
/// - `modules` saw no change to a declaration or gate of `owner_file`'s
///   chain (see [`ModuleDeclarations`]) and read every changed file.
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
    let new_parse = crate::analysis::syntax::parse_clean_source_file(new_source)?;
    let new_root = new_parse.tree().syntax().clone();
    let mut enclosing = named_functions(&new_root, name).filter(|function| {
        node_lines(new_source, function.syntax())
            .is_some_and(|(first, last)| first <= start_line && end_line <= last)
    });
    // Innermost last: a nested function of the same name is the owner.
    let new_fn = enclosing.next_back()?;
    let chain = owner_chain(owner_file, &new_fn);
    let names = owner_names(&new_fn, name);
    if modules.chain_changed(owner_file, &chain, &names) {
        return None;
    }
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
            || modules.chain_gated(owner_file, &chain, &names)
            || !modules.declares(owner_file),
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
    // The gates outside `mod` declarations (#14952) apply here as in
    // `verified_base_owner`; its declaration check
    // (`ModuleDeclarations::declares`) is not applied here (#15037).
    let chain = owner_chain(owner_file, &function);
    let names = owner_names(&function, name);
    if modules.chain_changed(owner_file, &chain, &names)
        || modules.chain_gated(owner_file, &chain, &names)
    {
        return true;
    }
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
/// `owner_file`'s [`module_chain_names`] plus each inline module enclosing
/// `function`, which a gated glob may name like a file module (#15035).
fn owner_chain(owner_file: &Path, function: &ast::Fn) -> BTreeSet<String> {
    let mut chain = module_chain_names(owner_file);
    chain.extend(
        function
            .syntax()
            .ancestors()
            .filter_map(ast::Module::cast)
            .filter_map(|module| module.name())
            .map(|found| unraw(found.text()).to_string()),
    );
    chain
}

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

/// The module path of `file` under its anchor directory (see
/// [`anchored_components`]): empty for a crate root, otherwise each
/// directory and the file stem, a final `mod` dropped; the crate-root
/// layout is in RIPR-SPEC-0001. `None` with no anchor directory.
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
    // A library's `src/bin.rs` (`mod bin;`) is a module, not the `src/bin`
    // directory (#15035 round-7 review).
    let target_dir =
        !in_src || (rest.len() > 1 && rest.first().is_some_and(|component| component == "bin"));
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
    if target_dir && !matches!(rest.as_slice(), [_, second] if second == "mod") {
        rest.drain(..1);
    }
    if rest.last().is_some_and(|component| component == "mod") {
        rest.pop();
    }
    Some(rest)
}

/// The file stem of `file` when it sits below two `src` directories (a
/// crate root of a package nested under another `src`).
fn nested_root_stem(file: &Path) -> Option<String> {
    let (components, anchor) = anchored_components(file)?;
    components
        .get(..anchor)?
        .iter()
        .any(|component| component == "src")
        .then(|| file.file_stem()?.to_str().map(str::to_string))
        .flatten()
}

/// The package directory of `file`: the path components before its anchor
/// directory, joined with `/` (shared by every target of the package).
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

/// Where a `concat` shadow in `file` reaches: its package, or every package
/// when `source` mentions `macro_export` at all.
fn concat_scope(file: &Path, source: &str) -> MacroScope {
    concat_package(file, source.contains("macro_export"))
}

/// [`MacroScope::of`], but every package for a path with more than one
/// `src` component: `src/a/src/imp.rs` may be a module of the root package
/// (`mod src;` in `src/a.rs`), which a root macro reaches (#15035 round-10
/// review). A genuinely nested package then over-gates.
fn concat_package(file: &Path, exported: bool) -> MacroScope {
    let nested = file
        .components()
        .filter(|component| component.as_os_str() == "src")
        .nth(1)
        .is_some();
    if nested {
        MacroScope::Workspace
    } else {
        MacroScope::of(file, exported)
    }
}

/// Whether some scope in `a` reaches some scope in `b`.
fn scopes_meet(a: &BTreeSet<MacroScope>, b: &BTreeSet<MacroScope>) -> bool {
    !a.is_empty()
        && !b.is_empty()
        && (a.contains(&MacroScope::Workspace)
            || b.contains(&MacroScope::Workspace)
            || !a.is_disjoint(b))
}

/// Whether one normalized path is a component suffix of the other, so a
/// path read relative to another root still matches.
fn same_file(a: &[String], b: &[String]) -> bool {
    !a.is_empty() && !b.is_empty() && (a.ends_with(b) || b.ends_with(a))
}

/// Where a module-emitting macro may emit modules (#14952 review R1): its
/// package, or any package when it is exported or has no package anchor.
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
/// breaks the name mapping, so its presence gates, and a change to one
/// refuses, every owner it may reach: the file its literal names (resolved
/// against the declaring file's directory, case-insensitively), any file
/// under that file's directory except a default crate root (`src/lib.rs`,
/// `src/main.rs`), and any file whose path spells the declaration's name.
/// A target that does not read as a literal on a top-level out-of-line
/// declaration (an inline module's `#[path]`, a `#[path]` declaration inside
/// an inline module, a non-literal value) reaches every owner
/// (#15104).
///
/// Gates outside `mod` declarations follow RIPR-SPEC-0001, "Base-owner cfg
/// gates" (#14952, #15035); a diff touching one, on either side, counts as
/// a change to what it gates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleDeclarations {
    /// Names of declarations with a `cfg`/`cfg_attr` on themselves, an
    /// enclosing item or their file; module names on a gated `use` path;
    /// file stems an unresolvable `include!` names.
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
    /// Some declaration carries `#[path]` (directly or through `cfg_attr`),
    /// or some `include!` names a file that cannot be read from its tokens.
    retargeted: bool,
    /// Module and item names a gated `use` rooted at a sysroot crate would
    /// gate, which count only when [`Self::sysroot_shadowed`].
    sysroot_gated: BTreeSet<String>,
    /// Some `extern crate .. as` renames a crate to a sysroot crate's name.
    sysroot_shadowed: bool,
    /// Files a `#[path]` literal on a read or changed declaration names,
    /// resolved against its declaring file's directory and normalized
    /// lexically (lowercase, so a case-insensitive file system cannot hide a
    /// match).
    retarget_targets: BTreeSet<PathBuf>,
    /// Names of `#[path]` declarations: a file whose path spells one may be
    /// the module that declaration replaced.
    retarget_names: BTreeSet<String>,
    /// A `#[path]` declaration's target could not be read as a literal.
    retarget_opaque: bool,
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
    /// The diff touched a `#[path]` declaration or an `include!` whose file
    /// cannot be read from its tokens.
    changed_retarget: bool,
    /// Module and item names a touched gated `use` rooted at a sysroot crate
    /// would gate, which count only when [`Self::sysroot_shadowed`].
    changed_sysroot: BTreeSet<String>,
    /// The diff touched an `extern crate .. as` naming a sysroot crate.
    changed_sysroot_shadow: bool,
    /// Where a read file defines or imports a macro named `concat`, which
    /// shadows the built-in wherever it is in scope (#15035 round-7 review):
    /// its package, or any package when the file exports macros.
    concat_shadowed: BTreeSet<MacroScope>,
    /// Where the diff touched such a definition.
    changed_concat_shadow: BTreeSet<MacroScope>,
    /// The packages of read `include!`s that trusted a bare `concat!`.
    bare_concat_include: BTreeSet<MacroScope>,
    /// The packages of such `include!`s the diff touched.
    changed_bare_concat_include: BTreeSet<MacroScope>,
    /// Some read `include!` trusted the literals of a `core::`/`std::`
    /// qualified `concat!`, which a crate renamed to a sysroot name may
    /// supply instead.
    qualified_concat_include: bool,
    /// The diff touched such an `include!`.
    changed_qualified_concat_include: bool,
    /// Some read file binds `core` or `std` itself (a `use` or `mod`).
    binds_sysroot: bool,
    /// The diff touched such a binding.
    changed_binds_sysroot: bool,
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
    /// find a gate, so a caller need not parse every workspace file: with
    /// string literals and comments blanked (so a `;` in
    /// `#[cfg(feature = "x;y")]` or a comment cannot end the attribute,
    /// #15035 review F4), a `cfg` whose attribute runs into a `use` before
    /// the next `;` or `{`, or a `cfg` anywhere beside a `pub` or `pub(..)`
    /// `use`; an `include!` (read on the blanked code too); an `extern` (a
    /// crate rename); or a
    /// `macro_rules!`/`macro` definition beside a `mod`, `include` or `path`
    /// token, or beside a `concat` word (a macro that may shadow
    /// `concat!`). A file holding an out-of-line `mod` declaration is kept by the
    /// caller on its own.
    pub(crate) fn may_hold_outside_gate(source: &str) -> bool {
        let code = (source.contains("cfg") || source.contains("include"))
            .then(|| blank_literals_and_comments(source));
        let code = code.as_deref().unwrap_or_default();
        let gated_use = source.contains("cfg") && {
            let words_before = |offset: usize| code.get(..offset).unwrap_or_default().trim_end();
            code.split([';', '{']).any(cfg_runs_into_use)
                || word_offsets(code, "use").any(|offset| {
                    let before = words_before(offset);
                    // After any attribute: a `{` inside one (`#[doc =
                    // stringify!({})]`) splits the `cfg` from its `use`.
                    before.ends_with([')', ']']) || ends_with_word(before, "pub")
                })
        };
        // On the raw text and on the blanked code, so a comment between
        // `include` and its `!` (`include /* c */ !("real.rs")`, #15035
        // round-6 review) does not hide the call.
        let include = [source, code].into_iter().any(|text| {
            text.match_indices("include").any(|(start, word)| {
                text.get(start + word.len()..)
                    .unwrap_or_default()
                    .trim_start()
                    .starts_with('!')
            })
        });
        let macro_definition = (source.contains("macro_rules")
            || word_offsets(source, "macro").next().is_some())
            && (source.contains("mod") || source.contains("include") || source.contains("path"));
        // A macro or import named `concat` shadows the built-in an
        // `include!` path may be spelled with (#15035 round-7 review); a
        // `#[macro_use] extern crate` is admitted by its `extern`.
        let concat_definition = has_word(source, "concat");
        // `core` or `std` bound by an item (`as core;`, `mod std`), not a
        // `core::` path.
        let sysroot_binding = ["core", "std"].into_iter().any(|name| {
            word_offsets(source, name).any(|offset| {
                !source
                    .get(offset + name.len()..)
                    .unwrap_or_default()
                    .trim_start()
                    .starts_with("::")
            })
        });
        gated_use
            || include
            || macro_definition
            || concat_definition
            || sysroot_binding
            || has_word(source, "extern")
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
            // An inline module's `#[path]` moves its children's directory;
            // which files that reaches is not read, so it gates every owner.
            if inline_retargets(&root) {
                reading.retargeted = true;
                reading.retarget_opaque = true;
            }
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
                    reading.note_retarget_targets(file, &module);
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
            reading.sysroot_gated.extend(gates.sysroot);
            // An `include!` naming a file its tokens do not spell may
            // compile any owner's file, so it reaches every owner.
            reading.retargeted |= gates.opaque_include;
            reading.retarget_opaque |= gates.opaque_include;
            reading.sysroot_shadowed |= gates.shadows_sysroot;
            if gates.defines_concat {
                reading.concat_shadowed.insert(concat_scope(file, source));
            }
            if gates.bare_concat {
                reading
                    .bare_concat_include
                    .insert(concat_package(file, false));
            }
            reading.qualified_concat_include |= gates.qualified_concat;
            reading.binds_sysroot |= gates.binds_sysroot;
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
        // With a user macro named `concat` anywhere, a bare `concat!` in an
        // `include!` may expand to any path, so such an `include!` reaches
        // every owner, as an opaque one does.
        // A shadow reaches only an `include!` of its own package, unless
        // its file exports macros (#15035 round-9: a `#[macro_use] extern
        // crate` in one member does not rebind `concat!` in another).
        // A `#[path]` may compile a shadow's file into another package, so
        // with any retarget every shadow reaches every package.
        let mut shadowed: BTreeSet<MacroScope> = reading
            .concat_shadowed
            .union(&reading.changed_concat_shadow)
            .cloned()
            .collect();
        if (reading.retargeted || reading.changed_retarget) && !shadowed.is_empty() {
            shadowed.insert(MacroScope::Workspace);
            if !reading.changed_concat_shadow.is_empty() {
                reading.changed_concat_shadow.insert(MacroScope::Workspace);
            }
        }
        if scopes_meet(&shadowed, &reading.bare_concat_include) {
            reading.retargeted = true;
            reading.retarget_opaque = true;
        }
        if scopes_meet(&shadowed, &reading.changed_bare_concat_include)
            || scopes_meet(&reading.changed_concat_shadow, &reading.bare_concat_include)
        {
            reading.changed_retarget = true;
            reading.retarget_opaque = true;
        }
        // A crate renamed to `core` or `std` may supply the qualified
        // `concat!` such an `include!` names.
        let renamed = reading.sysroot_shadowed
            || reading.changed_sysroot_shadow
            || reading.binds_sysroot
            || reading.changed_binds_sysroot;
        if renamed && reading.qualified_concat_include {
            reading.retargeted = true;
            reading.retarget_opaque = true;
        }
        if (renamed && reading.changed_qualified_concat_include)
            || ((reading.changed_sysroot_shadow || reading.changed_binds_sysroot)
                && reading.qualified_concat_include)
        {
            reading.changed_retarget = true;
            reading.retarget_opaque = true;
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
            if module.item_list().is_some() && module_retargets(&module) {
                self.changed_retarget = true;
                self.retarget_opaque = true;
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
                    self.note_retarget_targets(file, &declaration);
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
        // touched, or one under an enclosing item whose attribute (outer, or
        // inner on its body) the diff touched: removing `#[cfg(..)]` from a
        // `mod api { pub use .. }` changes its unchanged re-export's gate
        // (#15035 round-7 review). Under a touched file attribute, every one.
        let gates = OutsideGates::read(&root, file, |node| {
            let range = node.text_range();
            file_attrs_touched
                || touches(usize::from(range.start()), usize::from(range.end()))
                || node.ancestors().skip(1).any(|ancestor| {
                    ancestor
                        .children()
                        .filter(|child| child.kind() == SyntaxKind::ATTR)
                        .any(|attr| {
                            let range = attr.text_range();
                            touches(usize::from(range.start()), usize::from(range.end()))
                        })
                })
        });
        self.changed.extend(gates.modules);
        self.changed_names.extend(gates.names);
        self.changed_files.extend(gates.files);
        self.changed_macro_scopes.extend(gates.macro_scopes);
        self.changed_sysroot.extend(gates.sysroot);
        self.changed_retarget |= gates.opaque_include;
        self.retarget_opaque |= gates.opaque_include;
        self.changed_sysroot_shadow |= gates.shadows_sysroot;
        if gates.defines_concat {
            self.changed_concat_shadow
                .insert(concat_scope(file, source));
        }
        if gates.bare_concat {
            self.changed_bare_concat_include
                .insert(concat_package(file, false));
        }
        self.changed_qualified_concat_include |= gates.qualified_concat;
        self.changed_binds_sysroot |= gates.binds_sysroot;
    }

    /// Record the file each `#[path]` literal on `module` (declared in
    /// `declaring_file`) names, or that one could not be read.
    fn note_retarget_targets(&mut self, declaring_file: &Path, module: &ast::Module) {
        match module.name() {
            Some(name) => {
                self.retarget_names.insert(unraw(name.text()).to_string());
            }
            None => self.retarget_opaque = true,
        }
        // Inside an inline `mod outer { .. }` the target resolves under the
        // inline modules' directories (and a non-mod-rs file's stem), which
        // is not read here.
        let nested = module
            .syntax()
            .ancestors()
            .skip(1)
            .filter_map(ast::Module::cast)
            .any(|ancestor| ancestor.item_list().is_some());
        if nested {
            self.retarget_opaque = true;
            return;
        }
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
                match literal
                    .value()
                    .ok()
                    .and_then(|target| retarget_target(declaring_file, &target))
                {
                    Some(target) => {
                        named = true;
                        self.retarget_targets.insert(target);
                    }
                    None => self.retarget_opaque = true,
                }
            }
        }
        if !named {
            self.retarget_opaque = true;
        }
    }

    /// Whether a `#[path]` declaration may move, duplicate or replace
    /// `owner_file` (whose module chain names are `chain`): its target is the
    /// owner's file, the owner's file sits under the target's directory,
    /// where the target's own `mod` declarations resolve (a default crate
    /// root is never such a child module), or the owner's path spells the
    /// declaration's name, whose default file it replaced.
    fn retarget_reaches(&self, owner_file: &Path, chain: &BTreeSet<String>) -> bool {
        if self.retarget_opaque || !self.retarget_names.is_disjoint(chain) {
            return true;
        }
        let Some(owner) = normalized(owner_file) else {
            return true;
        };
        let root = default_crate_root(owner_file);
        self.retarget_targets.iter().any(|target| {
            target.is_absolute() != owner.is_absolute()
                || *target == owner
                || (!root && target.parent().is_some_and(|dir| owner.starts_with(dir)))
        })
    }

    /// Whether the diff may have changed which module `chain` (the names on
    /// `owner_file`'s path) names, or how its declarations, its file, or a
    /// `use` naming one of the owner's `names` (see [`owner_names`]), are gated.
    fn chain_changed(
        &self,
        owner_file: &Path,
        chain: &BTreeSet<String>,
        names: &BTreeSet<String>,
    ) -> bool {
        self.unreadable
            || (self.changed_retarget && self.retarget_reaches(owner_file, chain))
            || (self.changed_sysroot_shadow
                && (touches_chain(&self.sysroot_gated, chain, names)
                    || touches_chain(&self.changed_sysroot, chain, names)))
            || !self.changed.is_disjoint(chain)
            || !self.changed_names.is_disjoint(names)
            || self.sysroot_shadowed && touches_chain(&self.changed_sysroot, chain, names)
            || (macro_scope_reaches(&self.changed_macro_scopes, owner_file)
                && !self.crate_root(owner_file))
            || included(&self.changed_files, owner_file)
    }

    /// Whether a declaration on `chain`, `owner_file` itself, or a `use`
    /// naming one of the owner's `names`, may be compiled out or retargeted.
    fn chain_gated(
        &self,
        owner_file: &Path,
        chain: &BTreeSet<String>,
        names: &BTreeSet<String>,
    ) -> bool {
        (self.retargeted && self.retarget_reaches(owner_file, chain))
            || !self.gated.is_disjoint(chain)
            || !self.gated_names.is_disjoint(names)
            || self.sysroot_shadowed && touches_chain(&self.sysroot_gated, chain, names)
            || included(&self.gated_files, owner_file)
    }

    /// Whether `file` is read as a crate root: [`module_path`] says so, and
    /// unless some module named `src` is declared, a root below another
    /// `src` directory (`src/tools/foo/src/lib.rs`, a workspace member) is
    /// one too; with one, it may be that module's child and is read as a
    /// module named by its stem (#15035 round-8 review).
    fn crate_root(&self, file: &Path) -> bool {
        module_path(file).is_some_and(|path| path.is_empty())
            && (nested_root_stem(file).is_none() || !self.modules.contains("src"))
    }

    /// Whether plain `mod` declarations establish `file`'s module chain: it
    /// is a crate root, or no macro of its package (or exported) may emit a
    /// module, an out-of-line declaration is named like its module, and
    /// some module is named like each directory above it. A file reached
    /// only through `include!` (or a macro) has no such declaration. Names
    /// approximate the chain, as in [`ModuleDeclarations::chain_gated`].
    fn declares(&self, file: &Path) -> bool {
        let Some(mut path) = module_path(file) else {
            return false;
        };
        if path.is_empty() && !self.crate_root(file) {
            path = nested_root_stem(file).into_iter().collect();
        }
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

/// Whether `gated` holds one of the owner's `names` or a component of
/// `chain`.
fn touches_chain(
    gated: &BTreeSet<String>,
    chain: &BTreeSet<String>,
    names: &BTreeSet<String>,
) -> bool {
    !gated.is_disjoint(names) || !gated.is_disjoint(chain)
}

/// The names a `use` can swap the owner through: `name`, the names in each
/// enclosing `impl`'s self type and trait, and each enclosing `trait` and
/// inline module's name (#15035).
fn owner_names(function: &ast::Fn, name: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::from([name.to_string()]);
    for ancestor in function.syntax().ancestors().skip(1) {
        if let Some(block) = ast::Impl::cast(ancestor.clone()) {
            let types = block
                .self_ty()
                .map(|ty| ty.syntax().clone())
                .into_iter()
                .chain(block.trait_().map(|ty| ty.syntax().clone()));
            names.extend(types.flat_map(|ty| {
                ty.descendants()
                    .filter_map(ast::NameRef::cast)
                    .map(|found| unraw(found.text()).to_string())
                    .collect::<Vec<_>>()
            }));
        } else if let Some(definition) = ast::Trait::cast(ancestor.clone())
            && let Some(found) = definition.name()
        {
            names.insert(unraw(found.text()).to_string());
        } else if let Some(module) = ast::Module::cast(ancestor)
            && let Some(found) = module.name()
        {
            // An enclosing inline module, which a gated `use real_impl as
            // imp;` may alias like a file module (#15035 round-6 review).
            names.insert(unraw(found.text()).to_string());
        }
    }
    names
}

/// Whether some path in `files` names `owner_file`. An owner path that
/// cannot be normalized counts as named whenever any file is.
fn included(files: &BTreeSet<Vec<String>>, owner_file: &Path) -> bool {
    // Case-insensitively, as `#[path]` targets: a case-insensitive file
    // system may resolve another spelling to the owner file.
    let lower =
        |path: &[String]| -> Vec<String> { path.iter().map(|part| case_fold(part)).collect() };
    match normalized_path(owner_file) {
        Some(owner) => files
            .iter()
            .any(|file| same_file(&lower(file), &lower(&owner))),
        None => !files.is_empty(),
    }
}

/// `source` with the text of every comment and of every string, raw string
/// and character literal replaced by spaces (newlines kept), so a lexical
/// scan sees only code. An unterminated comment or literal blanks the rest
/// of the source.
fn blank_literals_and_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let at = |index: usize| bytes.get(index).copied();
    let mut code = bytes.to_vec();
    let mut blank = |from: usize, to: usize| {
        for byte in code.iter_mut().take(to).skip(from) {
            if *byte != b'\n' {
                *byte = b' ';
            }
        }
    };
    let ident_before = |index: usize| {
        index
            .checked_sub(1)
            .and_then(at)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    };
    let mut index = 0;
    while let Some(byte) = at(index) {
        let next = at(index + 1);
        let end = match (byte, next) {
            (b'/', Some(b'/')) => (index..bytes.len())
                .find(|&at_index| at(at_index) == Some(b'\n'))
                .unwrap_or(bytes.len()),
            (b'/', Some(b'*')) => {
                let mut depth = 0_usize;
                let mut cursor = index;
                loop {
                    match (at(cursor), at(cursor + 1)) {
                        (None, _) => break bytes.len(),
                        (Some(b'/'), Some(b'*')) => {
                            depth += 1;
                            cursor += 2;
                        }
                        (Some(b'*'), Some(b'/')) => {
                            depth = depth.saturating_sub(1);
                            cursor += 2;
                            if depth == 0 {
                                break cursor;
                            }
                        }
                        _ => cursor += 1,
                    }
                }
            }
            (b'"', _) => {
                let mut cursor = index + 1;
                loop {
                    match at(cursor) {
                        None => break bytes.len(),
                        Some(b'\\') => cursor += 2,
                        Some(b'"') => break cursor + 1,
                        Some(_) => cursor += 1,
                    }
                }
            }
            // `r"..."`, or the byte and C raw strings `br"..."` and
            // `cr"..."` (#15035 round-5 review).
            (b'r', Some(b'#' | b'"'))
                if !ident_before(index)
                    || (matches!(at(index.wrapping_sub(1)), Some(b'b' | b'c'))
                        && !ident_before(index.wrapping_sub(1))) =>
            {
                let hashes = (index + 1..bytes.len())
                    .take_while(|&cursor| at(cursor) == Some(b'#'))
                    .count();
                let open = index + 1 + hashes;
                if at(open) != Some(b'"') {
                    // A raw identifier (`r#name`).
                    index += 1;
                    continue;
                }
                let closing: Vec<u8> = std::iter::once(b'"')
                    .chain(std::iter::repeat_n(b'#', hashes))
                    .collect();
                bytes
                    .get(open + 1..)
                    .and_then(|rest| {
                        rest.windows(closing.len())
                            .position(|window| window == closing.as_slice())
                    })
                    .map_or(bytes.len(), |found| open + 1 + found + closing.len())
            }
            (b'\'', Some(b'\\')) => {
                match (index + 2..(index + 14).min(bytes.len()))
                    .find(|&cursor| cursor > index + 2 && at(cursor) == Some(b'\''))
                {
                    Some(close) => close + 1,
                    None => {
                        index += 1;
                        continue;
                    }
                }
            }
            (b'\'', Some(_)) => {
                let width = source
                    .get(index + 1..)
                    .and_then(|rest| rest.chars().next())
                    .map_or(1, char::len_utf8);
                if at(index + 1 + width) == Some(b'\'') {
                    index + 2 + width
                } else {
                    // A lifetime or label.
                    index += 1;
                    continue;
                }
            }
            _ => {
                index += 1;
                continue;
            }
        };
        blank(index, end);
        index = end.max(index + 1);
    }
    String::from_utf8(code).unwrap_or_else(|_| source.to_string())
}

/// Whether some `cfg` in `segment` (blanked code holding no `;` or `{`) is
/// followed in it by the word `use`, in one linear pass (#15035).
fn cfg_runs_into_use(segment: &str) -> bool {
    let Some(first) = segment.find("cfg") else {
        return false;
    };
    segment.match_indices("use").any(|(start, _)| {
        let after = segment
            .get(start + 3..)
            .and_then(|tail| tail.chars().next());
        let head = segment.get(..start).unwrap_or_default();
        start >= first + 3
            && !after.is_some_and(is_ident_char)
            && (head.ends_with("cfg") || !head.chars().next_back().is_some_and(is_ident_char))
    })
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
    /// Module names on a gated `use` path, and file stems an unresolvable
    /// `include!` names.
    modules: BTreeSet<String>,
    /// Item names a gated `use` imports or renames to.
    names: BTreeSet<String>,
    /// Normalized paths of files an `include!` names.
    files: BTreeSet<Vec<String>>,
    /// Where a macro body that may emit a module could emit it.
    macro_scopes: BTreeSet<MacroScope>,
    /// Module and item names a gated `use` rooted at a sysroot crate brings
    /// in, gates only when some crate is renamed to a sysroot name.
    sysroot: BTreeSet<String>,
    /// Some `include!` names a file its tokens do not spell.
    opaque_include: bool,
    /// Some `extern crate .. as` (or macro tokens spelling one) renames a
    /// crate to a sysroot crate's name (#15035 review F3).
    shadows_sysroot: bool,
    /// Some `include!` path was read from a bare `concat!` call.
    bare_concat: bool,
    /// Some `include!` path was read from a sysroot-qualified `concat!`.
    qualified_concat: bool,
    /// Some item binds `core` or `std` (see [`binds_sysroot_name`]).
    binds_sysroot: bool,
    /// Some `macro_rules! concat` or `macro concat` (an item, or macro
    /// tokens spelling one) defines a macro that shadows `concat!`.
    defines_concat: bool,
}

impl OutsideGates {
    fn read(root: &SyntaxNode, file: &Path, select: impl Fn(&SyntaxNode) -> bool) -> Self {
        let mut gates = Self::default();
        for node in root.descendants() {
            if let Some(item) = ast::Use::cast(node.clone()) {
                if use_gated(&item) && select(&node) {
                    gates.read_use(&item);
                }
            } else if let Some(call) = ast::MacroCall::cast(node.clone()) {
                if macro_named(&call, "include") && select(&node) {
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
        gates.shadows_sysroot = renames_to_sysroot(root, &select);
        gates.defines_concat = defines_concat_macro(root, &select);
        gates.binds_sysroot = binds_sysroot_name(root, &select);
        gates
    }

    /// Every name a gated `use` brings in: each leaf's last segment and
    /// rename. As module names, the ones through which it may alias or swap
    /// a module (#15035 review F1, narrowed in round 5): its last segment
    /// (`use a::imp;` may import the module `imp`), its rename and the
    /// renamed segment (`use real as imp;`), and for a glob or `self` leaf
    /// every segment of its path. An intermediate segment of any other leaf
    /// (`use crate::real::Helper;`) only names the module the leaf lives in,
    /// which it cannot swap. A redundant `super::name` import of a test
    /// module brings in none, and a leaf rooted at a sysroot crate brings its
    /// names in only as [`Self::sysroot`] (see [`ModuleDeclarations`]).
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
            if redundant_test_import(item, &leaf, &path) {
                continue;
            }
            let rename = leaf
                .rename()
                .and_then(|rename| rename.name())
                .map(|rename| unraw(rename.text()).to_string());
            let own_last = leaf
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .map(|name| unraw(name.text()).to_string())
                .filter(|last| leaf.star_token().is_none() && last != "self");
            let names: Vec<String> = rename.into_iter().chain(own_last.clone()).collect();
            // A glob or `self` leaf imports from (or is) every module on its
            // path; any other leaf may be a module only by its last segment
            // or rename.
            let modules = match own_last {
                Some(_) => names.clone(),
                None => names.iter().cloned().chain(path.iter().cloned()).collect(),
            };
            if sysroot_rooted(item, &path) {
                self.sysroot.extend(modules);
            } else {
                self.names.extend(names);
                self.modules.extend(modules);
            }
        }
    }

    /// The file a literal `include!("..")` names, resolved against the
    /// directory of `file`, or its stem when it cannot be resolved. For an
    /// argument that is not one plain string literal
    /// (`concat!(env!("CARGO_MANIFEST_DIR"), "/src/real.rs")`, #15035 review
    /// F5), the stem of the path its trailing string literals spell when
    /// that names a `.rs` file (see [`trailing_literal_path`]), and otherwise
    /// [`Self::opaque_include`].
    fn read_include(&mut self, file: &Path, call: &ast::MacroCall) {
        let Some(tree) = call.token_tree() else {
            self.opaque_include = true;
            return;
        };
        let mut arguments: Vec<_> = tree
            .syntax()
            .children_with_tokens()
            .filter(|element| !element.kind().is_trivia())
            .collect();
        // The delimiters, then a trailing comma.
        arguments.pop();
        if !arguments.is_empty() {
            arguments.remove(0);
        }
        if arguments
            .last()
            .is_some_and(|last| last.kind() == SyntaxKind::COMMA)
        {
            arguments.pop();
        }
        let literal = match arguments.as_slice() {
            [only] => only
                .as_token()
                .filter(|token| token.kind() == SyntaxKind::STRING)
                .and_then(|token| string_literal_value(token.text()).map(str::to_string)),
            _ => None,
        };
        let Some(text) = literal else {
            let path = trailing_literal_path(&arguments);
            self.bare_concat |= path.as_ref().is_some_and(|(_, bare)| *bare);
            self.qualified_concat |= path.as_ref().is_some_and(|(_, bare)| !*bare);
            let stem = path.and_then(|(text, _)| {
                let text = text.as_str();
                text.strip_suffix(".rs")?;
                Path::new(text)
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .filter(|stem| !stem.is_empty() && stem.chars().all(is_ident_char))
                    .map(str::to_string)
            });
            match stem {
                Some(stem) => {
                    self.modules.insert(stem);
                }
                None => self.opaque_include = true,
            }
            return;
        };
        let text = text.as_str();
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

/// The path the trailing string literals of the one `concat!` call in an
/// `include!` (`arguments`, delimiters removed) spell, joined, with whether
/// that `concat!` was bare rather than `core::`/`std::` qualified. `None`
/// for any other argument, or a run the spec does not trust (#15035).
fn trailing_literal_path(arguments: &[ra_ap_syntax::SyntaxElement]) -> Option<(String, bool)> {
    let (body, call) = arguments.split_last()?;
    let (bang, path) = call.split_last()?;
    let (name, qualifier) = path.split_last()?;
    let qualifier: String = qualifier
        .iter()
        .map(|element| match element {
            ra_ap_syntax::NodeOrToken::Token(token) => token.text().to_string(),
            ra_ap_syntax::NodeOrToken::Node(node) => node.text().to_string(),
        })
        .collect();
    let concat_call = body.kind() == SyntaxKind::TOKEN_TREE
        && bang.kind() == SyntaxKind::BANG
        && name
            .as_token()
            .is_some_and(|token| token.text() == "concat")
        && matches!(
            qualifier.as_str(),
            "" | "::core::" | "core::" | "::std::" | "std::"
        );
    if !concat_call {
        return None;
    }
    let bare = qualifier.is_empty();
    let elements: Vec<_> = body
        .as_node()?
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .collect();
    // The delimiters, then trailing commas.
    let mut rest = elements.get(1..elements.len().checked_sub(1)?)?;
    while let Some((last, before)) = rest.split_last() {
        if last.kind() != SyntaxKind::COMMA {
            break;
        }
        rest = before;
    }
    let mut parts = Vec::new();
    while let Some((last, before)) = rest.split_last() {
        let Some(token) = last
            .as_token()
            .filter(|token| token.kind() == SyntaxKind::STRING)
        else {
            break;
        };
        parts.push(string_literal_value(token.text())?.to_string());
        rest = before;
        match rest.split_last() {
            Some((comma, before)) if comma.kind() == SyntaxKind::COMMA => rest = before,
            _ => break,
        }
    }
    if parts.is_empty() {
        return None;
    }
    parts.reverse();
    let joined = parts.concat();
    (rest.is_empty() || joined.contains('/')).then_some((joined, bare))
}

/// Whether something `select` keeps may bind a macro named `concat` and
/// shadow the built-in (#15035): a definition, a rename `as concat`, a
/// `use` leaf `concat`, or any `#[macro_use] extern crate`.
fn defines_concat_macro(root: &SyntaxNode, select: &impl Fn(&SyntaxNode) -> bool) -> bool {
    let imported = root.descendants().any(|node| {
        if ast::Use::can_cast(node.kind()) {
            select(&node) && use_leaf_binds(&node, &["concat"])
        } else if let Some(item) = ast::ExternCrate::cast(node.clone()) {
            select(&node)
                && ast::HasAttrs::attrs(&item).any(|attr| {
                    attr.syntax()
                        .descendants_with_tokens()
                        .filter_map(|element| element.into_token())
                        .any(|token| token.text() == "macro_use")
                })
        } else {
            false
        }
    });
    imported || defines_concat_by_tokens(root, select)
}

/// Whether a leaf of the `use` item `node` binds one of `names`, by its
/// rename (`as name`) or else its last segment.
fn use_leaf_binds(node: &SyntaxNode, names: &[&str]) -> bool {
    node.descendants()
        .filter_map(ast::UseTree::cast)
        .filter(|tree| tree.use_tree_list().is_none() && tree.star_token().is_none())
        .any(|leaf| {
            if let Some(rename) = leaf.rename() {
                return rename
                    .name()
                    .is_some_and(|name| names.contains(&unraw(name.text())));
            }
            let mut path = use_leaf_path(&leaf);
            if path.last().is_some_and(|segment| segment == "self") {
                path.pop();
            }
            path.last()
                .is_some_and(|segment| names.contains(&segment.as_str()))
        })
}

/// Whether something `select` keeps binds `core` or `std` in a module
/// (#15035 round-8 review): a `use` leaf or rename (`use crate::paths as
/// core;`; inside macro tokens, any `as core`) or a `mod core`/`mod std`,
/// which a qualified `core::concat!` may then name. A cast `x as core`
/// outside macro tokens is no binding.
fn binds_sysroot_name(root: &SyntaxNode, select: &impl Fn(&SyntaxNode) -> bool) -> bool {
    const NAMES: [&str; 2] = ["core", "std"];
    let items = root.descendants().any(|node| {
        select(&node)
            && (ast::Use::can_cast(node.kind()) && use_leaf_binds(&node, &NAMES)
                || ast::Module::cast(node.clone())
                    .and_then(|module| module.name())
                    .is_some_and(|name| NAMES.contains(&unraw(name.text()))))
    });
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    items
        || tokens.windows(2).any(|pair| {
            matches!(pair, [keyword, name] if macro_token_as(keyword)
                && NAMES.contains(&unraw(name.text()))
                && name.parent().is_none_or(|parent| select(&parent)))
        })
}

/// The token half of [`defines_concat_macro`]: a `concat` after `macro`,
/// `macro_rules !` or (inside macro tokens) `as`.
fn defines_concat_by_tokens(root: &SyntaxNode, select: &impl Fn(&SyntaxNode) -> bool) -> bool {
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens.iter().enumerate().any(|(index, token)| {
        if unraw(token.text()) != "concat" {
            return false;
        }
        let before = |back: usize| {
            index
                .checked_sub(back)
                .and_then(|at| tokens.get(at))
                .map(|token| token.text())
        };
        let defined = before(1) == Some("macro")
            || index
                .checked_sub(1)
                .is_some_and(|at| macro_token_as(&tokens[at]))
            || (before(1) == Some("!") && before(2) == Some("macro_rules"));
        defined && token.parent().is_none_or(|parent| select(&parent))
    })
}

/// Whether `token` is an `as` inside macro tokens, where a rename is not
/// parsed as a `use` item.
fn macro_token_as(token: &SyntaxToken) -> bool {
    token.text() == "as"
        && token
            .parent_ancestors()
            .any(|node| node.kind() == SyntaxKind::TOKEN_TREE)
}

/// The value of a plain or raw string literal token with no escapes.
fn string_literal_value(text: &str) -> Option<&str> {
    if let Some(raw) = text.strip_prefix('r') {
        let body = raw.trim_start_matches('#');
        let hashes = "#".repeat(raw.len() - body.len());
        return body
            .strip_prefix('"')?
            .strip_suffix(hashes.as_str())?
            .strip_suffix('"');
    }
    text.strip_prefix('"')?
        .strip_suffix('"')
        .filter(|value| !value.contains('\\'))
}

/// Whether some `extern crate` (an item, or tokens of a macro body or call
/// spelling one) that `select` keeps renames a crate to a sysroot crate's
/// name, or to a macro fragment (`as $name`) that may be one.
fn renames_to_sysroot(root: &SyntaxNode, select: &impl Fn(&SyntaxNode) -> bool) -> bool {
    let tokens: Vec<_> = root
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens.iter().enumerate().any(|(index, token)| {
        if token.text() != "extern"
            || tokens
                .get(index + 1)
                .is_none_or(|next| next.text() != "crate")
        {
            return false;
        }
        let item: Vec<&str> = tokens
            .get(index + 2..)
            .unwrap_or_default()
            .iter()
            .map(|token| token.text())
            .take_while(|text| *text != ";")
            .collect();
        let renamed = item.windows(2).any(|pair| {
            matches!(pair, [keyword, name] if *keyword == "as" && (*name == "$" || is_sysroot(unraw(name))))
        });
        renamed && token.parent().is_none_or(|parent| select(&parent))
    })
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
/// nothing else can bind `name` (see [`scope_may_bind`]), so its gate cannot
/// swap the item a test reaches (#14952 review R2).
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
        && !scope_may_bind(items.syntax(), item.syntax(), name, true, |glob| {
            glob == ["super"]
        })
}

/// Whether the `use` leaf path `path` of `item` is rooted at a sysroot
/// crate that no item of the module holding `item` may shadow (#14952
/// review R3); a `use` in a block is never excluded.
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
    !scope_may_bind(&scope, item.syntax(), root, false, |glob| {
        glob.first().is_some_and(|root| is_sysroot(root))
    })
}

fn is_sysroot(root: &str) -> bool {
    matches!(root, "std" | "core" | "alloc" | "proc_macro" | "test")
}

/// Whether some item of `scope` other than `except` may bind `name`: a
/// `use` leaf binding it, a glob `harmless_glob` rejects, an item named
/// `name`, or a macro call (any one when `macro_calls_bind`, #15035 F2).
fn scope_may_bind(
    scope: &SyntaxNode,
    except: &SyntaxNode,
    name: &str,
    macro_calls_bind: bool,
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
            return macro_calls_bind
                || node.descendants_with_tokens().any(|element| {
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
/// (#14952 review R1): a `mod` without a `{` body, `include !` or `path =`.
fn emits_module(body: &ast::TokenTree) -> bool {
    // A `macro_rules!` arm's matcher is not emitted: read only the token
    // trees after each top-level `=>` (#15035 round-8 review), or the whole
    // body when it has none (a `macro` with its arguments outside).
    let children: Vec<_> = body
        .syntax()
        .children_with_tokens()
        .filter(|element| !element.kind().is_trivia())
        .collect();
    let text = |index: Option<usize>| {
        index
            .and_then(|index| {
                children
                    .get(index)?
                    .as_token()
                    .map(|token| token.text().to_string())
            })
            .unwrap_or_default()
    };
    let transcribers: Vec<SyntaxNode> = children
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            let back = |by: usize| text(index.checked_sub(by));
            back(1) == "=>" || (back(1) == ">" && back(2) == "=")
        })
        .filter_map(|(_, child)| child.as_node().cloned())
        .collect();
    if transcribers.is_empty() {
        return tokens_emit_module(body.syntax());
    }
    transcribers.iter().any(tokens_emit_module)
}

fn tokens_emit_module(tree: &SyntaxNode) -> bool {
    let tokens: Vec<String> = tree
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
/// Whether `source` may hold a `mod` declaration: the keyword `mod` as a
/// whole word, anywhere (comments and strings included), so `mod real;`,
/// `mod\nreal;` and `mod/* gap */real;` all count. A superset for [`ModuleDeclarations::read`]'s declaring
/// sources, so a file the index could not parse still reaches it and reads
/// as unreadable, whatever its comments or line breaks (#15104).
pub(crate) fn may_declare_module(source: &str) -> bool {
    source.match_indices("mod").any(|(at, _)| {
        let before = source.get(..at).and_then(|text| text.chars().next_back());
        let after = source.get(at + 3..).and_then(|text| text.chars().next());
        let word_edge = |c: char| !(c.is_alphanumeric() || c == '_');
        before.is_none_or(word_edge) && after.is_some_and(word_edge)
    })
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

/// The file a `#[path]` literal on an out-of-line declaration in
/// `declaring_file` names: relative to the declaring file's directory,
/// with a backslash read as a separator. `None` when it names no file.
fn retarget_target(declaring_file: &Path, target: &str) -> Option<PathBuf> {
    let target = target.replace('\\', "/");
    // A rooted or drive-qualified literal is compared against no relative
    // owner path on every platform, so it reads as opaque.
    if target.is_empty() || target.ends_with('/') || target.starts_with('/') || target.contains(':')
    {
        return None;
    }
    let dir = declaring_file.parent().unwrap_or_else(|| Path::new(""));
    normalized(&dir.join(target))
}

/// `path` with `.` dropped, `..` folded into its parent where one exists
/// and every component lowercased; `None` when it is not valid UTF-8.
fn normalized(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(out.components().next_back(), Some(Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            Component::Normal(name) => out.push(case_fold(name.to_str()?)),
            Component::RootDir | Component::Prefix(_) => out.push(component.as_os_str()),
        }
    }
    Some(out)
}

/// `name` folded through its uppercase form and then lowercased, so the
/// spellings a case-insensitive file system's uppercase table equates (such
/// as `ſ`/`s`, `ς`/`σ` and the Kelvin sign/`k`) fold alike. Folding only adds
/// matches, which only adds gates.
fn case_fold(name: &str) -> String {
    name.to_uppercase().to_lowercase()
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

/// [`cfg_gated`] for any node: a `cfg`/`cfg_attr` on it, an inner attribute
/// of its own body or item list (#15003), an enclosing item or its file.
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
                [(lib, source)],
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
        let modules = ModuleDeclarations::read([(lib, parent)], [(&changed_file, Some(plain))]);
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

    /// #15104: the lexical prefilter keeps every file that may declare a
    /// module, whatever its comments or line breaks.
    #[test]
    fn may_declare_module_keeps_any_mod_keyword() {
        assert!(may_declare_module("mod real;"));
        assert!(may_declare_module("mod real; // comment"));
        assert!(may_declare_module("#[cfg(x)]\nmod\n    real;"));
        assert!(may_declare_module("pub(crate) mod a {}"));
        assert!(may_declare_module("mod/* gap */real;"));
        assert!(!may_declare_module("fn model() {}"));
        assert!(!may_declare_module("let xmod = 1;"));
        assert!(!may_declare_module("let mod_count = 1;"));
        assert!(!may_declare_module("fn f() -> u8 { 7 }"));
    }

    /// #15104: a `#[path]` declaration gates only the owners it may move:
    /// its named file, files under that file's directory (never a default
    /// crate root), files whose path spells its name, or every owner when a
    /// target cannot be read.
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
                [(Path::new("src/lib.rs"), source.as_str())],
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
            let mount = "#[path = \"../../b/src/noop.rs\"]\nmod noop;\n";
            assert!(
                !swapped(mount, "src/shared.rs", touched)?,
                "a target in another package leaves this one's files alone (touched {touched})"
            );
            assert!(
                swapped(mount, "src/noop.rs", touched)?,
                "the default file the declaration replaced (touched {touched})"
            );
            assert!(
                swapped(mount, "../b/src/inner/x.rs", touched)?,
                "a file under the target's directory (touched {touched})"
            );
            assert!(
                swapped("#[path = \"sub/g.rs\"]\nmod g;\n", "src/sub/f.rs", touched)?,
                "a child the target may declare (touched {touched})"
            );
            assert!(
                swapped(
                    "#[cfg(not(test))]\nmod outer {\n    #[path = \"sub/real.rs\"]\n    mod x;\n}\n",
                    "src/outer/sub/real.rs",
                    touched
                )?,
                "a #[path] nested in an inline module (touched {touched})"
            );
            assert!(
                !swapped("#[path = \"sub/g.rs\"]\nmod g;\n", "other/f.rs", touched)?,
                "a file outside the target's directory (touched {touched})"
            );
            // The drive letter is joined at run time so the source holds no
            // literal local path.
            let drive = format!("{}:/abs/x.rs", 'C');
            for rooted in ["/abs/x.rs", drive.as_str(), "", "sub/"] {
                assert!(
                    swapped(
                        &format!("#[path = \"{rooted}\"]\nmod x;\n"),
                        "src/shared.rs",
                        touched
                    )?,
                    "an opaque literal {rooted:?} (touched {touched})"
                );
            }
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
        assert_eq!(path("src/tools/foo/src/lib.rs"), owned(&[]));
        // A root below another `src` is a crate root unless a module named
        // `src` is declared; then it must be declared itself.
        let read = |files: &[(&str, &str)]| {
            ModuleDeclarations::read(
                files.iter().map(|(file, text)| (Path::new(*file), *text)),
                std::iter::empty(),
            )
        };
        let nested = Path::new("src/a/src/lib.rs");
        assert!(read(&[("src/lib.rs", "pub fn f() {}\n")]).declares(nested));
        let with_src = read(&[("src/lib.rs", "mod a;\n"), ("src/a.rs", "mod src;\n")]);
        assert!(!with_src.declares(nested));
        assert!(!with_src.crate_root(nested));
        let declared = read(&[("src/a.rs", "mod src;\n"), ("src/a/src.rs", "mod lib;\n")]);
        assert!(declared.declares(nested));
        assert_eq!(path("src/a/b/mod.rs"), owned(&["a", "b"]));
        assert_eq!(path("src/bin/tool.rs"), owned(&[]));
        assert_eq!(path("src/bin/tool/main.rs"), owned(&[]));
        assert_eq!(path("src/bin/tool/cli.rs"), owned(&["cli"]));
        assert_eq!(path("tests/it.rs"), owned(&[]));
        assert_eq!(path("tests/it/common.rs"), owned(&["common"]));
        assert_eq!(path("tests/common/mod.rs"), owned(&["common"]));
        assert_eq!(path("src/bin/helper/mod.rs"), owned(&["helper"]));
        assert_eq!(path("tests/it/sub/mod.rs"), owned(&["sub"]));
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
            "(mod real;) => { fn helper() {} };",
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
            "#[cfg(not(feature = \"x;y\"))]\nuse crate::real::ready;\n",
            "#[cfg(feature = \"a{b\")] /* ; */\nuse crate::fake::stub as ready;\n",
            "#[cfg(feature = r#\"x;\"y\"#)]\nuse crate::real::ready;\n",
            "#[cfg(unix)]\nconst SEMI: char = ';';\n#[cfg(unix)]\nuse crate::real::ready;\n",
            "extern crate self as std;\n",
            "cfg_if::cfg_if! {\n    if #[cfg(unix)] {\n        extern crate core as std;\n    }\n}\n",
            "mod imp {\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/real.rs\"));\n}\n",
            "mod imp {\n    include!(env!(\"GENERATED\"));\n}\n",
            "#[cfg(unix)]\ninclude /* c */ !(\"real.rs\");\n",
            "macro_rules! concat {\n    ($n:literal) => {\n        \"\"\n    };\n}\n",
            "use crate::src_path as concat;\n",
            "pub(crate) use crate::paths::{concat};\n",
            "#[macro_use]\nextern crate paths;\n",
            "#[cfg(not(test))]\n#[doc = stringify!({})]\nuse crate::real::ready;\n",
            "use crate::paths as core;\n",
            "const C: &core::ffi::CStr = cr\"\\\";\n#[cfg(unix)]\nuse crate::real::ready;\nconst D: &str = \"\";\n",
            "const C: &core::ffi::CStr = cr#\"say \"hi\"\"#;\n#[cfg(unix)]\nuse crate::real::ready;\nconst D: &str = \"\";\n",
        ] {
            let parse = crate::analysis::syntax::parse_clean_source_file(source)
                .ok_or("fixture premise: source did not parse")?;
            let gates =
                OutsideGates::read(parse.tree().syntax(), Path::new("src/lib.rs"), |_| true);
            assert!(
                !gates.names.is_empty()
                    || !gates.modules.is_empty()
                    || !gates.files.is_empty()
                    || !gates.macro_scopes.is_empty()
                    || gates.opaque_include
                    || gates.shadows_sysroot
                    || gates.defines_concat
                    || gates.binds_sysroot,
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
            "#[cfg(feature = \"use\")]\npub fn f() {}\n",
            "// cfg: use the fast path\npub fn f() -> char {\n    '\"'\n}\n",
        ] {
            assert!(
                !ModuleDeclarations::may_hold_outside_gate(source),
                "{source}"
            );
        }
        Ok(())
    }
}
