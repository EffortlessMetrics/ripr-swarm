//! Conservative authority for a source file owned only by a test module.
//!
//! This intentionally admits only a single, ordinary default-library package.
//! Every ambiguity is a refusal; callers must recheck at the edit boundary.

use std::collections::BTreeMap;
#[cfg(unix)]
use std::collections::BTreeSet;
use std::path::Path;
#[cfg(unix)]
use std::path::{Component, PathBuf};

#[cfg(unix)]
use ra_ap_syntax::{AstNode, ast};
use sha2::{Digest, Sha256};

use crate::analysis::facts::RustIndex;
#[cfg(unix)]
use crate::analysis::facts::{ModulePathTarget, SourceRoleProvenanceEdgeKind};
#[cfg(unix)]
use crate::analysis::syntax::{
    RustModuleTreeEdge, has_test_attribute, parse_clean_source_file, rust_module_tree_scan,
};

#[cfg(unix)]
const MAX_AUTHORITY_ENTRIES: usize = 4_096;
#[cfg(unix)]
const MAX_AUTHORITY_SOURCE_FILES: usize = 128;

/// Establish exclusive test ownership for a conventional sibling file.
/// This first slice deliberately refuses custom targets, dependencies, macros
/// outside test functions, symlinks, hardlinks, unindexed source, and any
/// unresolved module edge. These restrictions are proof requirements, not
/// classifications of those workspaces as unsafe.
pub(crate) fn exclusive_test_module(root: &Path, index: &RustIndex, target: &Path) -> bool {
    #[cfg(not(unix))]
    {
        let _ = (root, index, target);
        return false;
    }
    #[cfg(unix)]
    {
        if !super::is_source_test_candidate(&target.to_string_lossy())
            || !ordinary_manifest(root)
            || !no_ancestor_manifest(root)
            || !is_exact_git_root(root)
        {
            return false;
        }
        let Some(files) = complete_source_listing(root) else {
            return false;
        };
        let indexed: BTreeSet<_> = index.files().iter().map(|(path, _)| path.clone()).collect();
        if files != indexed || !files.contains(target) || !files.contains(Path::new("src/lib.rs")) {
            return false;
        }
        if !index.include_parents.is_empty() || !index.include_limitations.is_empty() {
            return false;
        }
        for (path, facts) in index.files().iter() {
            let Ok(current_source) = std::fs::read(root.join(path)) else {
                return false;
            };
            if facts.used_lexical_fallback || facts.source.as_bytes() != current_source {
                return false;
            }
            let scan = rust_module_tree_scan(&facts.source);
            if !safe_source(&facts.source)
                || !scan.complete
                || scan.edges.iter().any(|edge| match edge {
                    RustModuleTreeEdge::Include(_) => true,
                    RustModuleTreeEdge::Default { inline, .. } => !inline.is_empty(),
                    _ => false,
                })
            {
                return false;
            }
            if facts
                .module_declarations
                .iter()
                .any(|d| match &d.path_target {
                    ModulePathTarget::Default => false,
                    ModulePathTarget::Literal(value) => {
                        Path::new(value)
                            .components()
                            .any(|c| !matches!(c, Component::Normal(_)))
                            || value.contains('\\')
                    }
                    ModulePathTarget::Unknown => true,
                })
            {
                return false;
            }
        }
        let Some(parents) = index.execution_module_parents.as_ref() else {
            return false;
        };
        // Missing, ambiguous, or uncomposed declarations are not proof of
        // absence: a later file or alternate route could make them live.
        for (parent, facts) in index.files().iter() {
            for declaration in &facts.module_declarations {
                let name = format!("mod {};", declaration.name);
                let matches = parents
                    .values()
                    .flatten()
                    .filter(|edge| {
                        edge.parent == *parent
                            && edge.line == declaration.line
                            && edge.declaration == name
                            && edge.requires_test == declaration.requires_test
                    })
                    .count();
                if matches != 1 {
                    return false;
                }
            }
        }
        let mut path = target.to_path_buf();
        let mut visited = BTreeSet::new();
        let mut gated = false;
        while path != Path::new("src/lib.rs") {
            if !visited.insert(path.clone()) {
                return false;
            }
            let Some(facts) = index.files().get(&path) else {
                return false;
            };
            if facts.role_provenance.earliest_unresolved_reason.is_some()
                || facts.role_provenance.outside_module_tree
            {
                return false;
            }
            let Some([edge]) = parents.get(&path).map(Vec::as_slice) else {
                return false;
            };
            if edge.kind != SourceRoleProvenanceEdgeKind::Module {
                return false;
            }
            gated |= edge.requires_test;
            path = edge.parent.clone();
        }
        gated
    }
}

#[cfg(unix)]
fn is_exact_git_root(root: &Path) -> bool {
    let Ok(output) = crate::git::run_git_output_with_deadline_and_limit(
        root,
        &["rev-parse", "--show-toplevel"],
        std::time::Duration::from_secs(10),
        64 * 1024,
    ) else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let Ok(top) = std::str::from_utf8(&output.stdout) else {
        return false;
    };
    let Ok(top) = Path::new(top.trim()).canonicalize() else {
        return false;
    };
    root.canonicalize().is_ok_and(|root| root == top)
}

#[cfg(unix)]
fn no_ancestor_manifest(root: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    root.ancestors().skip(1).all(|ancestor| {
        match std::fs::symlink_metadata(ancestor.join("Cargo.toml")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
            _ => false,
        }
    })
}

#[cfg(unix)]
fn ordinary_manifest(root: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        return false;
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return false;
    };
    let Some(table) = value.as_table() else {
        return false;
    };
    if table.len() != 1 {
        return false;
    }
    let Some(package) = table.get("package").and_then(toml::Value::as_table) else {
        return false;
    };
    ["name", "version", "edition"]
        .iter()
        .all(|key| package.get(*key).and_then(toml::Value::as_str).is_some())
        && package.keys().all(|key| {
            matches!(
                key.as_str(),
                "name" | "version" | "edition" | "rust-version"
            )
        })
}

#[cfg(unix)]
fn complete_source_listing(root: &Path) -> Option<BTreeSet<PathBuf>> {
    use std::os::unix::fs::MetadataExt;
    let mut pending = vec![root.to_path_buf()];
    let mut files = BTreeSet::new();
    let mut seen = 0usize;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).ok()? {
            let entry = entry.ok()?;
            let path = entry.path();
            let relative = path.strip_prefix(root).ok()?;
            if relative == Path::new(".git") || relative == Path::new("target") {
                continue;
            }
            seen += 1;
            if seen > MAX_AUTHORITY_ENTRIES {
                return None;
            }
            let metadata = std::fs::symlink_metadata(&path).ok()?;
            if metadata.file_type().is_symlink() {
                return None;
            }
            if metadata.is_dir() {
                if relative == Path::new(".cargo") || relative == Path::new("src/bin") {
                    return None;
                }
                pending.push(path);
            } else if metadata.is_file() {
                if relative.file_name().is_some_and(|n| n == "Cargo.toml")
                    && relative != Path::new("Cargo.toml")
                {
                    return None;
                }
                if path.extension().is_some_and(|ext| ext == "rs") {
                    if !relative.starts_with("src")
                        || relative == Path::new("src/main.rs")
                        || metadata.nlink() != 1
                    {
                        return None;
                    }
                    files.insert(relative.to_path_buf());
                    if files.len() > MAX_AUTHORITY_SOURCE_FILES {
                        return None;
                    }
                }
            } else {
                return None;
            }
        }
    }
    Some(files)
}

#[cfg(unix)]
fn safe_source(source: &str) -> bool {
    if source.contains("macro_rules") || source.contains("macro ") {
        return false;
    }
    let Some(parse) = parse_clean_source_file(source) else {
        return false;
    };
    let tree = parse.tree();
    if tree
        .syntax()
        .descendants()
        .any(|node| ast::MacroRules::can_cast(node.kind()) || ast::MacroDef::can_cast(node.kind()))
    {
        return false;
    }
    for attr in tree.syntax().descendants().filter_map(ast::Attr::cast) {
        let text = attr.syntax().text().to_string();
        let compact: String = text.chars().filter(|ch| !ch.is_whitespace()).collect();
        let Some(rest) = compact
            .strip_prefix("#[")
            .or_else(|| compact.strip_prefix("#!["))
        else {
            return false;
        };
        let name = rest
            .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .next()
            .unwrap_or_default();
        if !matches!(
            name,
            "cfg"
                | "path"
                | "inline"
                | "test"
                | "allow"
                | "warn"
                | "deny"
                | "forbid"
                | "doc"
                | "must_use"
                | "deprecated"
                | "cold"
                | "track_caller"
                | "should_panic"
                | "ignore"
        ) {
            return false;
        }
    }
    for call in tree.syntax().descendants().filter_map(ast::MacroCall::cast) {
        if !call
            .syntax()
            .ancestors()
            .filter_map(ast::Fn::cast)
            .any(|function| has_test_attribute(&function))
        {
            return false;
        }
    }
    true
}

/// Rebuild the authority and return the exact source/configuration bytes it
/// depends on. The cage compares these digests with its captured state.
pub(crate) fn source_test_module_basis(
    root: &Path,
    target: &Path,
) -> Option<BTreeMap<String, String>> {
    let files = super::discover_rust_files(root).ok()?;
    let index = crate::analysis::facts::build_index(root, &files).ok()?;
    if !exclusive_test_module(root, &index, target) {
        return None;
    }
    let mut basis = BTreeMap::new();
    for (path, facts) in index.files().iter() {
        let bytes = std::fs::read(root.join(path)).ok()?;
        if bytes != facts.source.as_bytes() {
            return None;
        }
        basis.insert(
            path.to_str()?.replace('\\', "/"),
            format!("{:x}", Sha256::digest(&bytes)),
        );
    }
    let manifest = std::fs::read(root.join("Cargo.toml")).ok()?;
    if !ordinary_manifest(root) || std::fs::read(root.join("Cargo.toml")).ok()? != manifest {
        return None;
    }
    basis.insert(
        "Cargo.toml".to_string(),
        format!("{:x}", Sha256::digest(&manifest)),
    );
    Some(basis)
}

#[cfg(not(unix))]
fn ordinary_manifest(_root: &Path) -> bool {
    false
}

/// Classified caches do not yet bind the complete ownership graph. Recompute
/// in the narrow workspace shape where this authority can grant an edit.
pub(crate) fn source_test_cache_sensitive(root: &Path) -> bool {
    if !ordinary_manifest(root) {
        return false;
    }
    match super::discover_rust_files(root) {
        Ok(files) => files
            .iter()
            .any(|path| super::is_source_test_candidate(&path.to_string_lossy())),
        Err(_) => true,
    }
}
