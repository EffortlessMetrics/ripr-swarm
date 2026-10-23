//! Workspace facade-crate reach limitation for Rust `no_static_path` findings
//! (#9982).
//!
//! A facade crate re-exports an owner crate (`pub use inner::*;`) and carries
//! the tests, so a test calls the owner's code through `facade::Ext` and never
//! spells the owner or its crate. The related-test search keeps tests of
//! another package out unless they call the owner's name, so such a finding
//! reads as a definite `no_static_path` ("No static test path found") while a
//! candidate test path exists that ripr does not trace.
//!
//! ## Fail-closed design
//!
//! - The limitation is only a named non-claim; the classification stays
//!   `no_static_path` and the test is never added to `related_tests`, so
//!   nothing is promoted toward `exposed` and no repair signal is created.
//!   Naming the limit withholds the repair gap, the safe direction.
//! - The evidence is structural, not lexical: a complete path-dependency graph
//!   with a forward normal `[dependencies]` edge from the facade package to
//!   the owner's package; a crate-root, unconditional, non-macro `pub use` of
//!   the owner's crate in the facade's `src/lib.rs` that covers the owner's
//!   public path (`inner::*`, `inner`, or a path that is a prefix of the
//!   module chain plus the function or enclosing type); and an integration
//!   test of the facade package that names the facade crate and ties to the
//!   owner (a `facade::*` glob, or the owner's or its type's name).
//! - The owner must be an unrestricted `pub` free function at module depth
//!   zero, or a `pub` method of a root-level `pub` struct/enum/union, with
//!   every module on the way `pub mod`. Private, restricted, trait-method,
//!   nested, `src/main.rs` and `src/bin/*` owners never fire.
//! - A missing or dev-only edge, an unrelated package, a private, nested,
//!   cfg-gated or macro `use`, a list that does not name the owner, or a test
//!   that never names the facade leaves the finding exactly as it was.
//! - Re-exports are not resolved. No item is matched beyond the names above.
//! - The facade crate's name is read from its manifest (`[lib] name`, else
//!   `[package] name`, `-` folded to `_`); an unreadable manifest or a
//!   non-default `[lib] path` does not fire.

use super::related_tests::{
    DependencyEdgeContext, nearest_manifest_identity, normalize_path, strip_comments_and_strings,
};
use crate::analysis::facts::{FunctionContainer, FunctionSummary, RustIndex};
use crate::analysis::seam_cache::PathDependencySection;
use crate::analysis::workspace::PathDependencyGraphStatus;
use std::path::{Path, PathBuf};

/// The facade-crate test that may reach the owner through a re-export.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct FacadeReachWitness {
    pub test_name: String,
    pub test_file: PathBuf,
    pub test_line: usize,
    /// The facade crate's name as the test spells it (`facade`).
    pub facade_crate: String,
    /// The owner's crate as the facade declares and re-exports it (`inner`).
    pub owner_crate: String,
    /// Other facade-package integration tests that name the facade.
    pub other_test_count: usize,
}

/// The message emitted as a stop-reason when this limitation fires. A named
/// limitation, not a coverage claim.
pub(in crate::analysis) const RUST_FACADE_REACH_MESSAGE: &str = "ripr saw a workspace crate whose root `pub use` names the crate holding this change, with an integration test that names that crate. \
     ripr does not resolve the re-export or match the test to this item. \
     This is not a coverage assessment -- ripr cannot confirm or deny \
     that the change is observed.";

/// The best facade-crate test for `owner`, or `None` when the workspace
/// graph, the re-export or the test's use of the facade is not established.
pub(in crate::analysis) fn facade_reach_witness(
    index: &RustIndex,
    context: &DependencyEdgeContext<'_>,
    owner: &FunctionSummary,
) -> Option<FacadeReachWitness> {
    if context.adjacency.status() != PathDependencyGraphStatus::Complete {
        return None;
    }
    let owner_manifest = nearest_manifest_identity(context.manifest_dir_prefixes, &owner.file)?;
    let reach = owner_reach(index, &owner_manifest, owner)?;
    let dependents = context.adjacency.reverse_neighbors(&owner_manifest)?;
    let mut best: Option<FacadeReachWitness> = None;
    let mut count = 0usize;
    for dependent in dependents {
        let Some(directory) = dependent.strip_suffix("Cargo.toml") else {
            continue;
        };
        let Some(facade_crate) = manifest_lib_crate_name(context.root, directory) else {
            continue;
        };
        let Some(declarations) = context
            .adjacency
            .forward_dependency_declarations(dependent, &owner_manifest)
        else {
            continue;
        };
        // A re-export needs a normal dependency; a dev-dependency is not
        // visible to the library.
        let owner_crates: Vec<String> = declarations
            .iter()
            .filter(|(section, _)| *section == PathDependencySection::Dependencies)
            .map(|(_, name)| name.replace('-', "_"))
            .collect();
        let Some(owner_crate) = owner_crates
            .into_iter()
            .find(|name| lib_reexports_owner(index, directory, name, &reach.path))
        else {
            continue;
        };
        for test in index.tests().iter() {
            if !crate::analysis::rust_index::is_test_file(&test.file)
                || nearest_manifest_identity(context.manifest_dir_prefixes, &test.file).as_deref()
                    != Some(dependent.as_str())
                || !file_ties_to_owner(index, &test.file, &facade_crate, &reach.tie_names)
            {
                continue;
            }
            count += 1;
            let candidate = FacadeReachWitness {
                test_name: test.name.clone(),
                test_file: test.file.clone(),
                test_line: test.start_line,
                facade_crate: facade_crate.clone(),
                owner_crate: owner_crate.clone(),
                other_test_count: 0,
            };
            let key =
                |w: &FacadeReachWitness| (w.test_file.clone(), w.test_line, w.test_name.clone());
            if best
                .as_ref()
                .is_none_or(|current| key(&candidate) < key(current))
            {
                best = Some(candidate);
            }
        }
    }
    let mut witness = best?;
    witness.other_test_count = count.saturating_sub(1);
    Some(witness)
}

/// The library crate identifier of the package whose manifest sits in
/// `directory` (relative to `root`), read from the manifest: `[lib] name`,
/// else `[package] name`, `-` folded to `_`. `None` (the limitation does not
/// fire) when the manifest is unreadable, names no package, points `[lib]`
/// at a non-default path, or the name is a std crate that a path could mean
/// instead.
fn manifest_lib_crate_name(root: &Path, directory: &str) -> Option<String> {
    let text = std::fs::read_to_string(root.join(directory).join("Cargo.toml")).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let package_name = value
        .get("package")
        .and_then(toml::Value::as_table)?
        .get("name")
        .and_then(toml::Value::as_str)?;
    let lib = value.get("lib").and_then(toml::Value::as_table);
    if lib
        .and_then(|table| table.get("path"))
        .is_some_and(|path| path.as_str() != Some("src/lib.rs"))
    {
        return None;
    }
    let name = lib
        .and_then(|table| table.get("name"))
        .and_then(toml::Value::as_str)
        .unwrap_or(package_name)
        .replace('-', "_");
    let identifier = !name.is_empty()
        && !name.as_bytes()[0].is_ascii_digit()
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    (identifier
        && !matches!(
            name.as_str(),
            "std" | "core" | "alloc" | "proc_macro" | "test"
        ))
    .then_some(name)
}

/// How a facade can name an owner: its public module path inside the
/// owner's crate (`[sub, Type]` or `[sub, free_fn]`) and the names a facade
/// test must mention (when it does not glob the facade).
struct OwnerReach {
    path: Vec<String>,
    tie_names: Vec<String>,
}

/// `None` unless the owner is provably public from the crate root: an
/// unrestricted `pub` free function at module depth zero, or an unrestricted
/// `pub` method of a `pub` struct/enum/union declared at the root of a file,
/// with every module on the way declared `pub mod` at its parent's root.
/// Owners in `src/main.rs` or `src/bin/*`, trait methods, private,
/// restricted, nested or unlocatable owners never fire.
fn owner_reach(
    index: &RustIndex,
    owner_manifest: &str,
    owner: &FunctionSummary,
) -> Option<OwnerReach> {
    let manifest_dir = owner_manifest.strip_suffix("Cargo.toml")?;
    let file = normalize_path(&owner.file);
    let in_src = file.strip_prefix(manifest_dir)?.strip_prefix("src/")?;
    if !is_library_source(in_src) {
        return None;
    }
    let facts = index.files().get(&owner.file)?;
    let stripped = strip_comments_and_strings(&facts.data().source);
    let declaration = stripped
        .lines()
        .skip(owner.start_line.saturating_sub(1))
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("#["))?;
    let rest = declaration.strip_prefix("pub")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    match &owner.item.container {
        FunctionContainer::Free => {
            if brace_depth_before_line(&stripped, owner.start_line) != 0 {
                return None;
            }
            let mut path = module_chain(index, manifest_dir, in_src)?;
            path.push(owner.name.clone());
            Some(OwnerReach {
                path,
                tie_names: vec![owner.name.clone()],
            })
        }
        FunctionContainer::Inherent { self_ty } => {
            let type_file = find_root_pub_type(index, manifest_dir, self_ty)?;
            let mut path = module_chain(index, manifest_dir, &type_file)?;
            path.push(self_ty.clone());
            Some(OwnerReach {
                path,
                tie_names: vec![owner.name.clone(), self_ty.clone()],
            })
        }
        _ => None,
    }
}

fn is_library_source(in_src: &str) -> bool {
    in_src != "main.rs" && !in_src.starts_with("bin/")
}

/// The brace depth at the start of 1-based `line` of comment-stripped text.
fn brace_depth_before_line(stripped: &str, line: usize) -> usize {
    let mut depth = 0usize;
    for text in stripped.lines().take(line.saturating_sub(1)) {
        for ch in text.chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    depth
}

/// The module names between the crate root and a file under `src/`
/// (`a/b.rs` and `a/b/mod.rs` are `[a, b]`; `lib.rs` is empty), `None`
/// unless each is declared `pub mod` at the root of its parent file. A parent
/// the index does not hold is unprovable.
fn module_chain(index: &RustIndex, manifest_dir: &str, in_src: &str) -> Option<Vec<String>> {
    let stem = in_src.strip_suffix(".rs")?;
    let mut segments: Vec<&str> = stem.split('/').collect();
    if segments.last() == Some(&"mod") {
        segments.pop();
    }
    if segments.as_slice() == ["lib"] {
        return Some(Vec::new());
    }
    for (at, segment) in segments.iter().enumerate() {
        let parent_stem = if at == 0 {
            "lib".to_string()
        } else {
            segments[..at].join("/")
        };
        let candidates = [
            format!("{manifest_dir}src/{parent_stem}.rs"),
            format!("{manifest_dir}src/{parent_stem}/mod.rs"),
        ];
        let declared = index
            .files()
            .iter()
            .filter(|(path, _)| candidates.contains(&normalize_path(path)))
            .any(|(_, facts)| {
                let stripped = strip_comments_and_strings(&facts.data().source);
                root_statements(&stripped).is_some_and(|statements| {
                    statements
                        .iter()
                        .any(|statement| declares_pub_item(statement, "mod", segment))
                })
            });
        if !declared {
            return None;
        }
    }
    Some(segments.iter().map(|segment| segment.to_string()).collect())
}

/// The `src/`-relative file of the single root-level `pub struct|enum|union
/// <name>` in the owner's package; `None` when absent or ambiguous.
fn find_root_pub_type(index: &RustIndex, manifest_dir: &str, name: &str) -> Option<String> {
    let mut found: Vec<String> = Vec::new();
    for (path, facts) in index.files().iter() {
        let normalized = normalize_path(path);
        let Some(rel) = normalized
            .strip_prefix(manifest_dir)
            .and_then(|rest| rest.strip_prefix("src/"))
        else {
            continue;
        };
        if !is_library_source(rel) {
            continue;
        }
        let stripped = strip_comments_and_strings(&facts.data().source);
        let Some(statements) = root_statements(&stripped) else {
            continue;
        };
        let declared = statements.iter().any(|statement| {
            ["struct", "enum", "union"]
                .iter()
                .any(|keyword| declares_pub_item(statement, keyword, name))
        });
        if declared {
            found.push(rel.to_string());
        }
    }
    (found.len() == 1).then(|| found.remove(0))
}

/// Whether the statement is `pub <keyword> <name>` (any attribute except
/// `cfg`/`cfg_attr`), ending the name at a non-identifier byte.
fn declares_pub_item(statement: &str, keyword: &str, name: &str) -> bool {
    let Some(rest) = strip_attributes(statement, false) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix("pub") else {
        return false;
    };
    if !rest.starts_with(char::is_whitespace) {
        return false;
    }
    let Some(rest) = rest.trim_start().strip_prefix(keyword) else {
        return false;
    };
    if !rest.starts_with(char::is_whitespace) {
        return false;
    }
    rest.trim_start().strip_prefix(name).is_some_and(|after| {
        !after
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// Whether `<directory>src/lib.rs` has a crate-root, unconditional,
/// non-macro `pub use` of `crate_name` that covers `owner_path`.
fn lib_reexports_owner(
    index: &RustIndex,
    directory: &str,
    crate_name: &str,
    owner_path: &[String],
) -> bool {
    let lib = format!("{directory}src/lib.rs");
    index
        .files()
        .iter()
        .filter(|(path, _)| normalize_path(path) == lib)
        .any(|(_, facts)| {
            let stripped = strip_comments_and_strings(&facts.data().source);
            root_pub_use_trees(&stripped)
                .iter()
                .any(|tree| use_tree_covers(tree, crate_name, owner_path))
        })
}

/// The use trees (`inner::*`) of every crate-root `pub use` item with no
/// attribute other than a harmless one. Items nested in a module, function,
/// impl or macro body sit at brace depth above zero and are never returned;
/// a `#[cfg]`/`#[cfg_attr]` or any unknown attribute drops the item.
/// Malformed braces yield nothing.
fn root_pub_use_trees(stripped: &str) -> Vec<String> {
    root_statements(stripped)
        .unwrap_or_default()
        .iter()
        .filter_map(|statement| pub_use_tree(statement))
        .collect()
}

/// The brace-depth-zero statements of comment-stripped source. A `pub use`
/// statement ends only at `;` (its braces belong to the use tree); any other
/// ends at `;` or at the `}` that returns to depth zero. `None` when braces
/// are unbalanced.
fn root_statements(stripped: &str) -> Option<Vec<String>> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    for ch in stripped.chars() {
        match ch {
            '{' => {
                depth += 1;
                current.push(ch);
            }
            '}' => {
                depth = depth.checked_sub(1)?;
                current.push(ch);
                if depth == 0 && !is_pub_use_statement(&current) {
                    statements.push(std::mem::take(&mut current));
                }
            }
            ';' if depth == 0 => {
                current.push(ch);
                statements.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    (depth == 0).then_some(statements)
}

/// Whether the unterminated statement so far is a `pub use` item, whose
/// braces belong to its use tree and end only at `;`.
fn is_pub_use_statement(statement: &str) -> bool {
    pub_use_tree(&format!("{statement};")).is_some()
}

/// The statement without its leading attributes. `strict` accepts only
/// `doc`, `allow`, `expect`, `deprecated` and `must_use`; otherwise anything
/// but `cfg`/`cfg_attr`. `None` for a rejected or malformed attribute.
fn strip_attributes(statement: &str, strict: bool) -> Option<&str> {
    let mut rest = statement.trim_start();
    while rest.starts_with('#') {
        let after = rest[1..].trim_start();
        let (inner, after) = match after.strip_prefix('!') {
            Some(after) => (true, after.trim_start()),
            None => (false, after),
        };
        let body = after.strip_prefix('[')?;
        let mut bracket = 1usize;
        let mut close = None;
        for (at, ch) in body.char_indices() {
            match ch {
                '[' => bracket += 1,
                ']' => {
                    bracket -= 1;
                    if bracket == 0 {
                        close = Some(at);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        if !inner {
            let name: String = body
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let allowed = if strict {
                matches!(
                    name.as_str(),
                    "doc" | "allow" | "expect" | "deprecated" | "must_use"
                )
            } else {
                !matches!(name.as_str(), "cfg" | "cfg_attr")
            };
            if !allowed {
                return None;
            }
        }
        rest = body[close + 1..].trim_start();
    }
    Some(rest)
}

fn pub_use_tree(statement: &str) -> Option<String> {
    let rest = strip_attributes(statement, true)?;
    let rest = rest.strip_prefix("pub")?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start().strip_prefix("use")?;
    if !rest.starts_with(|c: char| c.is_whitespace() || c == '{' || c == ':') {
        return None;
    }
    let tree = rest.trim().strip_suffix(';')?.trim();
    // Only use-tree text: anything else (a stray `;`, `(`, `#`, `=`, `[`)
    // means the statement splitter swallowed more than one item.
    tree.chars()
        .all(|c| {
            c.is_ascii_alphanumeric()
                || c.is_whitespace()
                || matches!(c, '_' | ':' | '{' | '}' | ',' | '*')
        })
        .then(|| tree.to_string())
}

/// Whether the use tree re-exports `crate_name` in a way that covers
/// `owner_path` (the owner's module chain then its item or type): the whole
/// crate (`inner`, `inner as x`, `inner::*`, `inner::{self}`), or a path
/// that is a prefix of the owner's path, or a glob or `self` under such a
/// prefix. A path that leaves the owner's (`inner::sub::Other`) does not
/// cover it. Nothing is resolved.
fn use_tree_covers(tree: &str, crate_name: &str, owner_path: &[String]) -> bool {
    let tree = tree.trim().trim_start_matches("::").trim_start();
    if let Some(list) = braced_list(tree) {
        return split_list(list)
            .iter()
            .any(|entry| use_tree_covers(entry, crate_name, owner_path));
    }
    let Some(rest) = tree.strip_prefix(crate_name) else {
        return false;
    };
    if rest
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return false;
    }
    let rest = rest.trim_start();
    if rest.is_empty() || rest.starts_with("as ") || rest.starts_with("as\t") {
        return true;
    }
    rest.strip_prefix("::")
        .is_some_and(|after| path_covers(after, owner_path))
}

fn path_covers(path: &str, owner: &[String]) -> bool {
    let path = path.trim();
    if path == "*" || path == "self" || path.starts_with("self ") {
        return true;
    }
    if let Some(list) = braced_list(path) {
        return split_list(list)
            .iter()
            .any(|entry| path_covers(entry, owner));
    }
    let (segment, rest) = match path.split_once("::") {
        Some((segment, rest)) => (segment, Some(rest)),
        None => (path, None),
    };
    let name: String = segment
        .trim()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() || owner.first() != Some(&name) {
        return false;
    }
    match rest {
        None => true,
        Some(rest) => path_covers(rest, &owner[1..]),
    }
}

/// The inside of a `{ ... }` whose closing brace ends the text.
fn braced_list(text: &str) -> Option<&str> {
    let inner = text.trim().strip_prefix('{')?.strip_suffix('}')?;
    let mut depth = 0usize;
    for ch in inner.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.checked_sub(1)?,
            _ => {}
        }
    }
    (depth == 0).then_some(inner)
}

fn split_list(list: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (at, ch) in list.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                entries.push(&list[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    entries.push(&list[start..]);
    entries
}

/// Whether the file spells `root::` as a path root (`use facade::Ext;`,
/// `facade::io::copy(..)`), outside comments and strings.
fn file_names_path_root(index: &RustIndex, file: &std::path::Path, root: &str) -> bool {
    let Some(facts) = index.files().get(file) else {
        return false;
    };
    let stripped = strip_comments_and_strings(&facts.data().source);
    stripped.match_indices(root).any(|(at, _)| {
        let end = at + root.len();
        let before_ok = at == 0
            || !stripped
                .as_bytes()
                .get(at - 1)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b':');
        before_ok && stripped[end..].starts_with("::")
    })
}

/// Whether a facade test ties to the owner: it names the facade crate as a
/// path root and either globs the facade (`use facade::*;`,
/// `use facade::{.., *}`) or mentions one of `tie_names` as a whole word.
/// Naming only an unrelated facade item (`facade::VERSION`) does not tie.
fn file_ties_to_owner(
    index: &RustIndex,
    file: &std::path::Path,
    root: &str,
    tie_names: &[String],
) -> bool {
    if !file_names_path_root(index, file, root) {
        return false;
    }
    let Some(facts) = index.files().get(file) else {
        return false;
    };
    let stripped = strip_comments_and_strings(&facts.data().source);
    globs_root(&stripped, root) || tie_names.iter().any(|name| contains_word(&stripped, name))
}

fn globs_root(stripped: &str, root: &str) -> bool {
    stripped.match_indices(root).any(|(at, _)| {
        let before_ok = at == 0
            || !stripped
                .as_bytes()
                .get(at - 1)
                .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b':');
        let Some(after) = stripped[at + root.len()..].strip_prefix("::") else {
            return false;
        };
        let after = after.trim_start();
        before_ok
            && (after.starts_with('*')
                || braced_prefix(after)
                    .is_some_and(|list| split_list(list).iter().any(|e| e.trim() == "*")))
    })
}

/// The inside of a leading balanced `{ ... }`, wherever the text goes on.
fn braced_prefix(text: &str) -> Option<&str> {
    let body = text.strip_prefix('{')?;
    let mut depth = 1usize;
    for (at, ch) in body.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&body[..at]);
                }
            }
            _ => {}
        }
    }
    None
}

fn contains_word(text: &str, word: &str) -> bool {
    let ident = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_';
    !word.is_empty()
        && text.match_indices(word).any(|(at, _)| {
            let before = at.checked_sub(1).and_then(|i| text.as_bytes().get(i));
            let after = text.as_bytes().get(at + word.len());
            !before.is_some_and(ident) && !after.is_some_and(ident)
        })
}

pub(in crate::analysis) fn facade_reach_witness_pointer(witness: &FacadeReachWitness) -> String {
    let others = match witness.other_test_count {
        0 => String::new(),
        1 => " (and 1 other test)".to_string(),
        n => format!(" (and {n} other tests)"),
    };
    format!(
        "{}`{}` ({}:{}) names workspace crate `{}`, whose root `pub use` names `{}`, the crate holding this change{}. \
         ripr does not resolve whether this test reaches this item; inspect it to judge whether this change is observed.",
        crate::domain::TRANSITIVE_REACH_WITNESS_PREFIX,
        witness.test_name,
        witness.test_file.display().to_string().replace('\\', "/"),
        witness.test_line,
        witness.facade_crate,
        witness.owner_crate,
        others
    )
}

pub(in crate::analysis) fn facade_reach_limitation_detail_lines(
    witness: &FacadeReachWitness,
    owner_name: &str,
) -> [String; 4] {
    [
        format!(
            "{}test `{}` ({}:{}) -> crate `{}`",
            crate::domain::LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX,
            witness.test_name,
            witness.test_file.display().to_string().replace('\\', "/"),
            witness.test_line,
            witness.facade_crate
        ),
        format!(
            "{}crate `{}` root `pub use` from `{}` -> owner `{}` through a re-export ripr does not resolve",
            crate::domain::LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX,
            witness.facade_crate,
            witness.owner_crate,
            owner_name
        ),
        format!(
            "{}analysis/rust-public-api-transitive-reach",
            crate::domain::LIMITATION_ANALYZER_ROUTE_PREFIX
        ),
        format!(
            "{}named limitation only; ripr cannot confirm or deny that this path observes the change",
            crate::domain::LIMITATION_NON_CLAIM_PREFIX
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn covers(source: &str, owner: &[&str]) -> bool {
        root_pub_use_trees(source)
            .iter()
            .any(|tree| use_tree_covers(tree, "inner", &path(owner)))
    }

    #[test]
    fn whole_crate_reexports_cover_any_owner() {
        for source in [
            "pub use inner::*;",
            "pub use inner as other;",
            "pub use ::inner::*;",
            "mod x {}\npub use inner;\n",
            "pub use inner::{self};",
            "pub use {inner::*, other::A};",
        ] {
            assert!(covers(source, &["sub", "f"]), "{source}");
        }
    }

    #[test]
    fn paths_must_be_a_prefix_of_the_owners_path() {
        assert!(covers("pub use inner::{Ext, f};", &["f"]));
        assert!(covers("pub use inner::f;", &["f"]));
        assert!(covers("pub use inner::sub::*;", &["sub", "f"]));
        assert!(covers("pub use inner::sub::f;", &["sub", "f"]));
        assert!(covers("pub use inner::sub;", &["sub", "f"]));
        assert!(covers("pub use inner::sub::{x, *};", &["sub", "f"]));
        assert!(covers("pub use inner::sub::{self};", &["sub", "f"]));
        assert!(covers("pub use inner::sub::Ty as T;", &["sub", "Ty"]));
        assert!(!covers("pub use inner::{Other};", &["f"]));
        assert!(!covers("pub use inner::Other;", &["f"]));
        assert!(!covers("pub use inner::sub::Other;", &["sub", "f"]));
        assert!(!covers("pub use inner::sub::*;", &["f"]));
        assert!(!covers("pub use inner::other::*;", &["sub", "f"]));
        assert!(!covers("pub use inner::{Other, nested::{g}};", &["f"]));
    }

    #[test]
    fn only_crate_root_unconditional_public_uses_count() {
        for source in [
            "pub(crate) use inner::*;",
            "use inner::*;",
            "pub use inner_extra::*;",
            "pub use other::inner::*;",
            "#[cfg(feature = \"x\")]\npub use inner::*;",
            "#[cfg_attr(unix, allow(unused))]\npub use inner::*;",
            "mod x { pub use inner::*; }",
            "fn f() { pub use inner::*; }",
            "macro_rules! m { () => { pub use inner::*; } }",
            "m! { pub use inner::*; }",
            "impl X { }\n}\npub use inner::*;",
            "mod x { \npub use inner::*;",
        ] {
            assert!(!covers(source, &["f"]), "{source}");
        }
        // Text around a valid `pub use` is never part of its tree.
        for source in [
            "pub use other::X; const A: [u8; 2] = [1, 2];\n",
            "pub use inner::{Other};\n#[cfg(feature = \"x\")]\nfn f() {}\n",
            "pub use other::X\n#[cfg(feature = \"x\")]\npub use inner::*;\n",
            "pub use other::X; // pub use inner::*;\n",
        ] {
            assert!(!covers(source, &["f"]), "{source}");
        }
        assert!(covers("#[doc(hidden)]\npub use inner::*;", &["f"]));
        assert!(covers("mod x { }\nfn f() {}\npub use inner::*;", &["f"]));
    }

    #[test]
    fn facade_tests_tie_by_owner_name_type_or_glob() {
        assert!(globs_root("use facade::*;", "facade"));
        assert!(globs_root("use facade::{Ext, *};", "facade"));
        assert!(!globs_root("use facade::{Ext};", "facade"));
        assert!(!globs_root("use other_facade::*;", "facade"));
        assert!(contains_word("facade::bump_counter(1)", "bump_counter"));
        assert!(!contains_word("facade::bump_counter_x(1)", "bump_counter"));
        assert!(!contains_word("facade::VERSION", "bump_counter"));
    }

    #[test]
    fn pub_items_need_pub_attribute_free_declarations() {
        assert!(declares_pub_item(
            "#[derive(Debug)]\npub struct T {}",
            "struct",
            "T"
        ));
        assert!(!declares_pub_item(
            "#[cfg(x)]\npub struct T {}",
            "struct",
            "T"
        ));
        assert!(!declares_pub_item("pub(crate) struct T {}", "struct", "T"));
        assert!(!declares_pub_item("pub struct Tx {}", "struct", "T"));
        assert!(declares_pub_item("pub mod a;", "mod", "a"));
        assert!(!declares_pub_item("mod a;", "mod", "a"));
    }
}
