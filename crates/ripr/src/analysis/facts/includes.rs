use super::{ResolvedIncludeParent, RustIncludeLimitation, RustIndex};
use crate::analysis::syntax::rust_include_directives;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const MAX_INCLUDE_EDGES: usize = 512;
const MAX_INCLUDE_DEPTH: usize = 32;
const MAX_INCLUDED_FILE_BYTES: usize = 4 * 1024 * 1024;

pub(super) fn resolve_repository_local_includes(root: &Path, index: &mut RustIndex) {
    let mut limitations = Vec::new();
    let mut edges = Vec::new();
    let mut directive_count = 0;
    let mut edge_limit_exceeded = false;
    let canonical_root = std::fs::canonicalize(root).ok();

    let candidates: Vec<(&PathBuf, &str)> = index
        .files
        .iter()
        .filter(|(_, facts)| {
            !facts.used_lexical_fallback && might_contain_include_macro(&facts.source)
        })
        .map(|(parent, facts)| (parent, facts.source.as_ref()))
        .collect();
    // `collect` on a parallel iterator over a slice preserves input order, so
    // each precomputed entry still matches the BTreeMap file order the serial
    // loop used (#9214).
    let precomputed: Vec<(PathBuf, Result<Vec<_>, String>)> = candidates
        .par_iter()
        .map(|(parent, source)| {
            (
                (*parent).clone(),
                rust_include_directives(parent, source, MAX_INCLUDE_EDGES),
            )
        })
        .collect();

    'files: for (parent, directives) in precomputed {
        let directives = match directives {
            Ok(directives) => directives,
            Err(reason_code) => {
                limitations.push(limitation(&parent, 0, "include!", &reason_code));
                continue;
            }
        };
        for directive in directives {
            directive_count += 1;
            if !include_edge_count_within_limit(directive_count) {
                edge_limit_exceeded = true;
                break 'files;
            }
            if !directive.is_file_level {
                limitations.push(limitation(
                    &parent,
                    directive.line,
                    &directive.expression,
                    "rust_include_nested_module_context",
                ));
                continue;
            }
            let Some(literal) = directive.literal_path else {
                limitations.push(limitation(
                    &parent,
                    directive.line,
                    &directive.expression,
                    "rust_include_dynamic_expression",
                ));
                continue;
            };
            let Some(candidate) = repository_relative_target(&parent, &literal) else {
                limitations.push(limitation(
                    &parent,
                    directive.line,
                    &directive.expression,
                    "rust_include_path_outside_repository",
                ));
                continue;
            };
            let full = root.join(&candidate);
            let canonical_target = match std::fs::canonicalize(&full) {
                Ok(path) => path,
                Err(_) => {
                    limitations.push(limitation(
                        &parent,
                        directive.line,
                        &directive.expression,
                        "rust_include_target_missing",
                    ));
                    continue;
                }
            };
            if canonical_root
                .as_ref()
                .is_none_or(|root| !canonical_target.starts_with(root))
            {
                limitations.push(limitation(
                    &parent,
                    directive.line,
                    &directive.expression,
                    "rust_include_symlink_escape",
                ));
                continue;
            }
            let Some(target_facts) = index.files.get(&candidate) else {
                limitations.push(limitation(
                    &parent,
                    directive.line,
                    &directive.expression,
                    "rust_include_target_not_indexed",
                ));
                continue;
            };
            if !included_file_within_size_limit(&target_facts.source) {
                limitations.push(limitation(
                    &parent,
                    directive.line,
                    &directive.expression,
                    "rust_include_file_too_large",
                ));
                continue;
            }
            edges.push((
                candidate,
                parent.clone(),
                directive.line,
                directive.expression,
                directive.requires_test,
            ));
        }
    }

    edges.sort();
    if edge_limit_exceeded {
        limitations.push(limitation(
            Path::new("."),
            0,
            "include!",
            "rust_include_edge_limit_exceeded",
        ));
        index.include_parents.clear();
        // A fresh build starts with no targets; an index extended in place
        // would otherwise keep the set an earlier pass found.
        index.include_targets.clear();
        sort_limitations(&mut limitations);
        index.include_limitations = limitations;
        return;
    }

    let mut parents_by_child: BTreeMap<PathBuf, Vec<(PathBuf, usize, String, bool)>> =
        BTreeMap::new();
    // Preserve physical include discovery independently of whether the
    // contextual role map can choose one parent. A conflict must fail closed
    // for role grants, but it must not erase the fragment's source directory
    // when resolving its own default modules.
    index.include_targets = edges.iter().map(|(child, ..)| child.clone()).collect();
    for (child, parent, line, expression, requires_test) in edges {
        parents_by_child
            .entry(child)
            .or_default()
            .push((parent, line, expression, requires_test));
    }

    let mut parents = BTreeMap::new();
    for (child, owners) in parents_by_child {
        let distinct = owners
            .iter()
            .map(|(parent, _, _, _)| parent)
            .collect::<BTreeSet<_>>();
        // Ambiguous ownership, or one parent whose include invocations
        // disagree on the cfg-test requirement (#3533): neither resolves to
        // one context, so the child keeps its standalone roles.
        let requirements = owners
            .iter()
            .map(|(_, _, _, requires_test)| *requires_test)
            .collect::<BTreeSet<_>>();
        if distinct.len() != 1 {
            for (parent, line, expression, _) in owners {
                limitations.push(limitation(
                    &parent,
                    line,
                    &expression,
                    "rust_include_ambiguous_parent",
                ));
            }
            continue;
        }
        if requirements.len() != 1 {
            for (parent, line, expression, _) in owners {
                limitations.push(limitation(
                    &parent,
                    line,
                    &expression,
                    "rust_include_conflicting_cfg_requirement",
                ));
            }
            continue;
        }
        if let Some((parent, _, _, requires_test)) = owners.into_iter().next() {
            parents.insert(
                child,
                ResolvedIncludeParent {
                    parent,
                    requires_test,
                },
            );
        }
    }

    let unsafe_children = cycle_or_depth_limited_children(&parents);
    for child in &unsafe_children {
        let parent = match parents.get(child) {
            Some(edge) => edge.parent.as_path(),
            None => Path::new("."),
        };
        limitations.push(limitation(
            parent,
            0,
            &format!("include!({})", child.display()),
            "rust_include_cycle_or_depth_limit",
        ));
    }
    parents.retain(|child, _| !unsafe_children.contains(child));

    index.include_parents = parents;
    sort_limitations(&mut limitations);
    index.include_limitations = limitations;
    rebase_function_identities(index);
}

/// Whether the index already carries include state from an earlier
/// resolution pass: resolved parents (whose function identities are rebased)
/// or physical include targets. The overflow path of the resolution pass
/// returns before replacing the targets, so a stale set would survive an
/// in-place extension that a fresh build never had.
pub(super) fn index_holds_include_state(index: &RustIndex) -> bool {
    !index.include_parents.is_empty() || !index.include_targets.is_empty()
}

/// Whether adding a file with this decoded `source` to an already-indexed
/// workspace can re-parent function identities through an `include!`
/// directive. Only an invocation that can become an edge counts: a
/// parser-backed file-level `include!` with a literal path to a repository
/// file the merged index holds (`kept`). A mention in a comment or a string
/// never enters the producer the resolution pass reads, and a nested,
/// dynamic, out-of-repository or unindexed-target invocation only records a
/// limitation, so none of them re-parents anything (the same-run reuse used
/// to fall back to a full rebuild for every added file whose bytes merely
/// contained `include!`). A file the parser refuses, or one over the
/// directive budget, reports `true`: its directives are unknown, so the
/// caller fails closed to a fresh build. The edge budget is shared by the
/// whole index and is spent on inert directives too, so once the cached
/// index already holds include state (`core_has_include_state`) any
/// directive in an added file could push the merged total over the budget,
/// which clears the edges without undoing the rebased identities: it counts.
pub(super) fn added_file_may_rebase_includes(
    path: &Path,
    source: &str,
    kept: &BTreeSet<&Path>,
    core_has_include_state: bool,
) -> bool {
    if !might_contain_include_macro(source) {
        return false;
    }
    let Ok(directives) = rust_include_directives(path, source, MAX_INCLUDE_EDGES) else {
        return true;
    };
    directives.len() > MAX_INCLUDE_EDGES
        || (core_has_include_state && !directives.is_empty())
        || directives.iter().any(|directive| {
            directive.is_file_level
                && directive
                    .literal_path
                    .as_deref()
                    .and_then(|literal| repository_relative_target(path, literal))
                    .is_some_and(|target| kept.contains(target.as_path()))
        })
}

fn might_contain_include_macro(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut offset = 0;
    while let Some(relative) = source[offset..].find("include") {
        let start = offset + relative;
        let end = start + "include".len();
        let has_identifier_prefix =
            start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
        let has_identifier_suffix =
            end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_');
        if !has_identifier_prefix && !has_identifier_suffix {
            let mut cursor = end;
            loop {
                while bytes
                    .get(cursor)
                    .is_some_and(|byte| byte.is_ascii_whitespace())
                {
                    cursor += 1;
                }
                if bytes.get(cursor..cursor + 2) == Some(b"//") {
                    cursor += 2;
                    while bytes.get(cursor).is_some_and(|byte| *byte != b'\n') {
                        cursor += 1;
                    }
                    continue;
                }
                if bytes.get(cursor..cursor + 2) == Some(b"/*") {
                    cursor += 2;
                    let mut depth = 1usize;
                    while depth > 0 {
                        match bytes.get(cursor..cursor + 2) {
                            Some(b"/*") => {
                                depth += 1;
                                cursor += 2;
                            }
                            Some(b"*/") => {
                                depth -= 1;
                                cursor += 2;
                            }
                            Some(_) => cursor += 1,
                            None => break,
                        }
                    }
                    if depth == 0 {
                        continue;
                    }
                }
                break;
            }
            if bytes.get(cursor) == Some(&b'!') {
                return true;
            }
        }
        offset = end;
    }
    false
}

fn sort_limitations(limitations: &mut Vec<RustIncludeLimitation>) {
    limitations.sort_by(|left, right| {
        left.parent
            .cmp(&right.parent)
            .then(left.line.cmp(&right.line))
            .then(left.reason_code.cmp(&right.reason_code))
            .then(left.expression.cmp(&right.expression))
    });
    limitations.dedup();
}

pub(crate) fn repository_relative_target(parent: &Path, literal: &Path) -> Option<PathBuf> {
    if literal.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    if let Some(parent_dir) = parent.parent() {
        normalized.push(parent_dir);
    }
    for component in literal.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => normalized.push(part),
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn included_file_within_size_limit(source: &str) -> bool {
    source.len() <= MAX_INCLUDED_FILE_BYTES
}

fn include_edge_count_within_limit(count: usize) -> bool {
    count <= MAX_INCLUDE_EDGES
}

fn cycle_or_depth_limited_children(
    parents: &BTreeMap<PathBuf, ResolvedIncludeParent>,
) -> BTreeSet<PathBuf> {
    let mut unsafe_children = BTreeSet::new();
    for child in parents.keys() {
        let mut seen = BTreeSet::new();
        let mut cursor = child;
        let mut cycle_detected = false;
        for _ in 0..=MAX_INCLUDE_DEPTH {
            if !seen.insert(cursor.clone()) {
                cycle_detected = true;
                break;
            }
            let Some(parent) = parents.get(cursor).map(|edge| &edge.parent) else {
                break;
            };
            cursor = parent;
        }
        if cycle_detected || seen.len() > MAX_INCLUDE_DEPTH {
            unsafe_children.extend(seen);
        }
    }
    unsafe_children
}

fn rebase_function_identities(index: &mut RustIndex) {
    let ids = index.live_function_ids();
    for id in ids {
        rebase_function_identity(&mut index.function_facts[id], &index.include_parents);
    }
}

fn rebase_function_identity(
    function: &mut super::FunctionFact,
    parents: &BTreeMap<PathBuf, ResolvedIncludeParent>,
) {
    let compilation_unit = compilation_unit_path_from_parents(parents, &function.file);
    if compilation_unit == function.file {
        return;
    }
    // Function identities are composed from the stable `/`-separated
    // path text (`stable_path_text`, #3469 family) while `file` keeps the
    // producing host's separators, so the prefix must be built from the
    // same stable form — on a backslash host the display form never
    // matches and the include rebase silently never fires (Windows-only
    // failure of `rust_include_compilation_unit`, #3631-adjacent).
    let source_prefix = format!("{}::", crate::analysis::stable_path_text(&function.file));
    if let Some(suffix) = function.id.0.strip_prefix(&source_prefix) {
        function.id.0 = format!(
            "{}::{suffix}",
            crate::analysis::stable_path_text(&compilation_unit)
        );
    }
}

pub(crate) fn compilation_unit_path_from_parents(
    parents: &BTreeMap<PathBuf, ResolvedIncludeParent>,
    file: &Path,
) -> PathBuf {
    let mut cursor = file.to_path_buf();
    for _ in 0..MAX_INCLUDE_DEPTH {
        let Some(parent) = parents.get(&cursor).map(|edge| edge.parent.clone()) else {
            break;
        };
        cursor = parent;
    }
    cursor
}

fn limitation(
    parent: &Path,
    line: usize,
    expression: &str,
    reason_code: &str,
) -> RustIncludeLimitation {
    RustIncludeLimitation {
        parent: parent.to_path_buf(),
        line,
        expression: expression.to_string(),
        reason_code: reason_code.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rebase prefix must be composed from the same stable identity
    /// text the symbol id uses, never from `file.display()`: identity is
    /// canonical `/`-separated text (#3469 family) while `file` keeps the
    /// producing host's separators, and a filename `%` makes the two
    /// forms diverge on every host — so this pins the contract on Linux
    /// CI too, where the Windows separator mismatch itself cannot run
    /// (#3638 review).
    #[test]
    fn rebase_prefix_uses_the_stable_identity_form_not_display() {
        let mut parents = BTreeMap::new();
        parents.insert(
            PathBuf::from("src/frag_50%.rs"),
            ResolvedIncludeParent {
                parent: PathBuf::from("src/lib.rs"),
                requires_test: false,
            },
        );
        let mut function = crate::analysis::rust_index::FunctionFact {
            id: crate::domain::SymbolId("src/frag_50%25.rs::impl W::f".to_string()),
            name: "f".to_string(),
            file: PathBuf::from("src/frag_50%.rs"),
            start_line: 1,
            end_line: 2,
            body: "fn f() {}".into(),
            calls: crate::analysis::facts::SharedFactSlice::empty(),
            returns: Vec::new(),
            literals: crate::analysis::facts::SharedFactSlice::empty(),
            source_role: crate::analysis::facts::FunctionSourceRole::Production,
            attrs: crate::analysis::facts::SharedFactSlice::empty(),
            impl_attrs: Vec::new(),
            nested_fn_names: crate::analysis::facts::SharedFactSlice::empty(),
            let_bindings: crate::analysis::facts::SharedFactSlice::empty(),
            item: Default::default(),
            impl_context: Default::default(),
        };
        super::rebase_function_identity(&mut function, &parents);
        assert_eq!(function.id.0, "src/lib.rs::impl W::f");
    }

    #[test]
    fn include_macro_prefilter_rejects_common_identifier_mentions() {
        assert!(!might_contain_include_macro("include_str!(\"data\")"));
        assert!(!might_contain_include_macro("let included = true;"));
        assert!(!might_contain_include_macro("preinclude!(\"fragment.rs\")"));
    }

    #[test]
    fn include_macro_prefilter_accepts_token_separating_trivia() {
        assert!(might_contain_include_macro("include!(\"fragment.rs\")"));
        assert!(might_contain_include_macro(
            "include \n ! (\"fragment.rs\")"
        ));
        assert!(might_contain_include_macro(
            "include /* outer /* nested */ comment */ ! (\"fragment.rs\")"
        ));
        assert!(might_contain_include_macro(
            "include // why this fragment\n ! (\"fragment.rs\")"
        ));
    }

    #[test]
    fn traversal_above_repository_root_is_rejected() {
        assert_eq!(
            repository_relative_target(Path::new("src/lib.rs"), Path::new("../shared.rs")),
            Some(PathBuf::from("shared.rs"))
        );
        assert_eq!(
            repository_relative_target(Path::new("src/lib.rs"), Path::new("../../escaped.rs")),
            None
        );
    }

    #[test]
    fn include_depth_over_bound_invalidates_the_chain() {
        let mut parents = BTreeMap::new();
        for depth in 0..=MAX_INCLUDE_DEPTH {
            parents.insert(
                PathBuf::from(format!("src/fragment-{depth}.rs")),
                ResolvedIncludeParent {
                    parent: if depth == MAX_INCLUDE_DEPTH {
                        PathBuf::from("src/lib.rs")
                    } else {
                        PathBuf::from(format!("src/fragment-{}.rs", depth + 1))
                    },
                    requires_test: false,
                },
            );
        }

        let limited = cycle_or_depth_limited_children(&parents);

        assert!(limited.contains(Path::new("src/fragment-0.rs")));
        assert!(limited.len() > MAX_INCLUDE_DEPTH);
    }

    #[test]
    fn include_resource_limits_are_inclusive_and_bounded() {
        assert!(include_edge_count_within_limit(MAX_INCLUDE_EDGES));
        assert!(!include_edge_count_within_limit(MAX_INCLUDE_EDGES + 1));

        let at_limit = "x".repeat(MAX_INCLUDED_FILE_BYTES);
        let over_limit = "x".repeat(MAX_INCLUDED_FILE_BYTES + 1);
        assert!(included_file_within_size_limit(&at_limit));
        assert!(!included_file_within_size_limit(&over_limit));
    }

    /// One parent whose include invocations disagree on the cfg-test
    /// requirement resolves to no single context: the child keeps its
    /// standalone roles and the conflict is named (#3533).
    #[test]
    fn conflicting_include_requirements_fail_closed() -> Result<(), String> {
        // Producer level: the invocation's own attributes classify through
        // the shared cfg authority.
        let source = "include!(\"fragment.rs\");\n\n#[cfg(test)]\ninclude!(\"fragment.rs\");\n";
        let directives =
            crate::analysis::syntax::rust_include_directives(Path::new("src/lib.rs"), source, 16)?;
        assert_eq!(directives.len(), 2);
        assert!(!directives[0].requires_test);
        assert!(directives[1].requires_test);
        Ok(())
    }

    use crate::analysis::facts::{FileFacts, RustIndex};
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    fn include_resolution_snapshot(index: &RustIndex) -> IncludeResolutionSnapshot {
        IncludeResolutionSnapshot {
            include_parents: index.include_parents.clone(),
            include_limitations: index.include_limitations.clone(),
            include_targets: index.include_targets.clone(),
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct IncludeResolutionSnapshot {
        include_parents: BTreeMap<PathBuf, ResolvedIncludeParent>,
        include_limitations: Vec<RustIncludeLimitation>,
        include_targets: BTreeSet<PathBuf>,
    }

    fn manual_index(sources: &[(&str, &str)]) -> RustIndex {
        let mut index = RustIndex::default();
        for (relative, source) in sources {
            let path = PathBuf::from(relative);
            index.insert_file(
                path.clone(),
                FileFacts {
                    path,
                    source: Arc::from(*source),
                    ..FileFacts::default()
                },
                true,
            );
        }
        index
    }

    fn write_fixture(root: &Path, sources: &[(&str, &str)]) -> std::io::Result<()> {
        for (relative, source) in sources {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, source)?;
        }
        Ok(())
    }

    /// Pre-#9214 serial loop: parse each candidate file only when reached and
    /// stop reading later files once the edge budget is exhausted.
    fn resolve_repository_local_includes_serial_reference(root: &Path, index: &mut RustIndex) {
        let mut limitations = Vec::new();
        let mut edges = Vec::new();
        let mut directive_count = 0;
        let mut edge_limit_exceeded = false;
        let canonical_root = std::fs::canonicalize(root).ok();

        'files: for (parent, facts) in &index.files {
            if facts.used_lexical_fallback || !might_contain_include_macro(&facts.source) {
                continue;
            }
            let directives = match rust_include_directives(parent, &facts.source, MAX_INCLUDE_EDGES)
            {
                Ok(directives) => directives,
                Err(reason_code) => {
                    limitations.push(limitation(parent, 0, "include!", &reason_code));
                    continue;
                }
            };
            for directive in directives {
                directive_count += 1;
                if !include_edge_count_within_limit(directive_count) {
                    edge_limit_exceeded = true;
                    break 'files;
                }
                if !directive.is_file_level {
                    limitations.push(limitation(
                        parent,
                        directive.line,
                        &directive.expression,
                        "rust_include_nested_module_context",
                    ));
                    continue;
                }
                let Some(literal) = directive.literal_path else {
                    limitations.push(limitation(
                        parent,
                        directive.line,
                        &directive.expression,
                        "rust_include_dynamic_expression",
                    ));
                    continue;
                };
                let Some(candidate) = repository_relative_target(parent, &literal) else {
                    limitations.push(limitation(
                        parent,
                        directive.line,
                        &directive.expression,
                        "rust_include_path_outside_repository",
                    ));
                    continue;
                };
                let full = root.join(&candidate);
                let canonical_target = match std::fs::canonicalize(&full) {
                    Ok(path) => path,
                    Err(_) => {
                        limitations.push(limitation(
                            parent,
                            directive.line,
                            &directive.expression,
                            "rust_include_target_missing",
                        ));
                        continue;
                    }
                };
                if canonical_root
                    .as_ref()
                    .is_none_or(|root| !canonical_target.starts_with(root))
                {
                    limitations.push(limitation(
                        parent,
                        directive.line,
                        &directive.expression,
                        "rust_include_symlink_escape",
                    ));
                    continue;
                }
                let Some(target_facts) = index.files.get(&candidate) else {
                    limitations.push(limitation(
                        parent,
                        directive.line,
                        &directive.expression,
                        "rust_include_target_not_indexed",
                    ));
                    continue;
                };
                if !included_file_within_size_limit(&target_facts.source) {
                    limitations.push(limitation(
                        parent,
                        directive.line,
                        &directive.expression,
                        "rust_include_file_too_large",
                    ));
                    continue;
                }
                edges.push((
                    candidate,
                    parent.clone(),
                    directive.line,
                    directive.expression,
                    directive.requires_test,
                ));
            }
        }

        edges.sort();
        if edge_limit_exceeded {
            limitations.push(limitation(
                Path::new("."),
                0,
                "include!",
                "rust_include_edge_limit_exceeded",
            ));
            index.include_parents.clear();
            sort_limitations(&mut limitations);
            index.include_limitations = limitations;
            return;
        }

        let mut parents_by_child: BTreeMap<PathBuf, Vec<(PathBuf, usize, String, bool)>> =
            BTreeMap::new();
        index.include_targets = edges.iter().map(|(child, ..)| child.clone()).collect();
        for (child, parent, line, expression, requires_test) in edges {
            parents_by_child.entry(child).or_default().push((
                parent,
                line,
                expression,
                requires_test,
            ));
        }

        let mut parents = BTreeMap::new();
        for (child, owners) in parents_by_child {
            let distinct = owners
                .iter()
                .map(|(parent, _, _, _)| parent)
                .collect::<BTreeSet<_>>();
            let requirements = owners
                .iter()
                .map(|(_, _, _, requires_test)| *requires_test)
                .collect::<BTreeSet<_>>();
            if distinct.len() != 1 {
                for (parent, line, expression, _) in owners {
                    limitations.push(limitation(
                        &parent,
                        line,
                        &expression,
                        "rust_include_ambiguous_parent",
                    ));
                }
                continue;
            }
            if requirements.len() != 1 {
                for (parent, line, expression, _) in owners {
                    limitations.push(limitation(
                        &parent,
                        line,
                        &expression,
                        "rust_include_conflicting_cfg_requirement",
                    ));
                }
                continue;
            }
            if let Some((parent, _, _, requires_test)) = owners.into_iter().next() {
                parents.insert(
                    child,
                    ResolvedIncludeParent {
                        parent,
                        requires_test,
                    },
                );
            }
        }

        let unsafe_children = cycle_or_depth_limited_children(&parents);
        for child in &unsafe_children {
            let parent = match parents.get(child) {
                Some(edge) => edge.parent.as_path(),
                None => Path::new("."),
            };
            limitations.push(limitation(
                parent,
                0,
                &format!("include!({})", child.display()),
                "rust_include_cycle_or_depth_limit",
            ));
        }
        parents.retain(|child, _| !unsafe_children.contains(child));

        index.include_parents = parents;
        sort_limitations(&mut limitations);
        index.include_limitations = limitations;
        rebase_function_identities(index);
    }

    #[test]
    fn parallel_include_resolution_matches_serial_reference()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "ripr-includes-parallel-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut files: Vec<(String, String)> = vec![
            ("src/frag_a.rs".into(), "pub fn a() {}\n".into()),
            ("src/frag_b.rs".into(), "pub fn b() {}\n".into()),
            ("src/frag_c.rs".into(), "pub fn c() {}\n".into()),
            (
                "src/lib.rs".into(),
                "include!(\"frag_a.rs\");\ninclude!(\"frag_b.rs\");\n".into(),
            ),
            ("src/other.rs".into(), "include!(\"frag_c.rs\");\n".into()),
            (
                "src/bad.rs".into(),
                "mod inner { include!(\"frag_a.rs\"); }\ninclude!(\"missing.rs\");\n".into(),
            ),
        ];
        let mut early = String::new();
        for _ in 0..MAX_INCLUDE_EDGES {
            early.push_str("include!(\"frag_a.rs\");\n");
        }
        files.push(("src/early.rs".into(), early));
        files.push(("src/late.rs".into(), "include!(\"frag_b.rs\");\n".into()));

        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, source)| (path.as_str(), source.as_str()))
            .collect();
        write_fixture(&root, &borrowed)?;

        let mut parallel = manual_index(&borrowed);
        resolve_repository_local_includes(&root, &mut parallel);
        let parallel_snapshot = include_resolution_snapshot(&parallel);

        let mut serial = manual_index(&borrowed);
        resolve_repository_local_includes_serial_reference(&root, &mut serial);
        let serial_snapshot = include_resolution_snapshot(&serial);

        assert_eq!(parallel_snapshot, serial_snapshot);
        assert!(
            parallel_snapshot
                .include_limitations
                .iter()
                .any(|limit| limit.reason_code == "rust_include_edge_limit_exceeded"),
            "{:?}",
            parallel_snapshot.include_limitations
        );
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// An index extended in place can carry the target set an earlier pass
    /// found; the budget-overflow path must leave none, as a fresh build does.
    #[test]
    fn edge_budget_overflow_leaves_no_stale_include_targets() -> Result<(), String> {
        let root = std::env::temp_dir().join(format!(
            "ripr-include-overflow-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut many = String::new();
        for _ in 0..=MAX_INCLUDE_EDGES {
            many.push_str("mod m { include!(\"frag.rs\"); }\n");
        }
        let sources = [
            ("src/lib.rs", many.as_str()),
            ("src/frag.rs", "pub fn f() {}\n"),
        ];
        write_fixture(&root, &sources).map_err(|error| error.to_string())?;
        let mut index = manual_index(&sources);
        index.include_targets.insert(PathBuf::from("src/frag.rs"));
        resolve_repository_local_includes(&root, &mut index);
        let _ = fs::remove_dir_all(&root);
        assert!(index.include_parents.is_empty());
        assert!(index.include_targets.is_empty());
        assert!(
            index
                .include_limitations
                .iter()
                .any(|limit| limit.reason_code == "rust_include_edge_limit_exceeded")
        );
        Ok(())
    }

    #[test]
    fn include_state_covers_targets_left_without_resolved_parents() {
        let mut index = RustIndex::default();
        assert!(!index_holds_include_state(&index));
        index.include_targets.insert(PathBuf::from("src/frag.rs"));
        assert!(
            index_holds_include_state(&index),
            "ambiguous parents leave targets but no include_parents"
        );
    }

    #[test]
    fn added_file_rebase_check_reads_parsed_directives_not_mentions() {
        let path = Path::new("src/added.rs");
        let kept: BTreeSet<&Path> = [Path::new("src/lib.rs"), Path::new("src/x.rs")].into();
        // A mention in a comment, a string, a similarly named macro or a
        // macro definition body is not an `include!` invocation.
        for mention in [
            "// include!(\"x.rs\") is documented here\npub fn a() {}\n",
            "pub fn a() -> &'static str { \"include!(\\\"x.rs\\\")\" }\n",
            "pub fn a() { let _ = include_str!(\"x.rs\"); }\n",
            "macro_rules! m { () => { include!(\"x.rs\") } }\npub fn a() {}\n",
        ] {
            assert!(
                !added_file_may_rebase_includes(path, mention, &kept, false),
                "mention must not count: {mention}"
            );
        }
        // A real invocation counts at file level, nested, attributed, with
        // token-separating trivia, and when the parser refuses the file.
        for directive in [
            "include!(\"x.rs\");\n",
            "#[cfg(test)]\ninclude!(\"x.rs\");\n",
            "include /* why */ ! (\"x.rs\");\n",
            "include!(\"x.rs\"); fn broken( {\n",
        ] {
            assert!(
                added_file_may_rebase_includes(path, directive, &kept, false),
                "directive must count: {directive}"
            );
        }
        // With rebased edges already in the core, even an inert directive
        // counts toward the shared edge budget.
        assert!(added_file_may_rebase_includes(
            path,
            "mod m { include!(\"x.rs\"); }\n",
            &kept,
            true
        ));
        assert!(!added_file_may_rebase_includes(
            path,
            "// include!(\"x.rs\")\n",
            &kept,
            true
        ));
        // An invocation that cannot become an edge only records a limitation.
        for inert in [
            "include!(\"missing.rs\");\n",
            "mod m { include!(\"x.rs\"); }\n",
            "include!(concat!(env!(\"OUT_DIR\"), \"/x.rs\"));\n",
            "include!(\"../../escaped.rs\");\n",
        ] {
            assert!(
                !added_file_may_rebase_includes(path, inert, &kept, false),
                "inert directive must not count: {inert}"
            );
        }
    }

    /// Reusing a same-run core for a workspace that adds a file which only
    /// mentions `include!` extends the index in place, and a real `include!`
    /// still takes the fresh build; both equal a fresh build over the target.
    #[test]
    fn added_file_mentioning_include_extends_in_place_and_real_include_rebuilds()
    -> Result<(), String> {
        use crate::analysis::facts::{
            build_analysis_index_from_loaded_files, index_build_probe, reuse_cached_index_in_run,
        };
        let root = std::env::temp_dir().join(format!(
            "ripr-include-rebase-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let plain_core = [
            ("Cargo.toml", "[package]\nname='demo'\nversion='0.0.0'\n"),
            ("src/lib.rs", "pub fn base() -> u8 { 1 }\n"),
            ("src/frag.rs", "pub fn fragment() -> u8 { 2 }\n"),
        ];
        // A core that already rebased `frag.rs` through a real edge.
        let edge_core = [
            ("Cargo.toml", "[package]\nname='demo'\nversion='0.0.0'\n"),
            (
                "src/lib.rs",
                "include!(\"frag.rs\");\npub fn base() -> u8 { 1 }\n",
            ),
            ("src/frag.rs", "pub fn fragment() -> u8 { 2 }\n"),
        ];
        let load = |names: &[(&str, &str)]| -> Vec<(PathBuf, Vec<u8>)> {
            names
                .iter()
                .filter(|(path, _)| path.ends_with(".rs"))
                .map(|(path, source)| (PathBuf::from(path), source.as_bytes().to_vec()))
                .collect()
        };
        let snapshot = |index: &RustIndex| -> Result<String, String> {
            Ok(format!(
                "{:?}|{}|{}",
                include_resolution_snapshot(index),
                serde_json::to_string(&index.functions().iter().collect::<Vec<_>>())
                    .map_err(|error| error.to_string())?,
                index
                    .files()
                    .keys()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ))
        };
        for (core_sources, added_source, expect_rebuild) in [
            (
                plain_core,
                "// include!(\"frag.rs\")\npub fn added() -> &'static str { \"include!\" }\n",
                0usize,
            ),
            // An include whose target the merged index does not hold records
            // a limitation only; it is not an edge.
            (
                plain_core,
                "include!(\"nope.rs\");\npub fn added() -> u8 { 3 }\n",
                0usize,
            ),
            (
                plain_core,
                "include!(\"frag.rs\");\npub fn added() -> u8 { 3 }\n",
                1usize,
            ),
            // The shared edge budget: with a rebased edge in the core, even an
            // inert directive or a mention-free file with one must rebuild,
            // while a plain mention still extends in place.
            (
                edge_core,
                "mod m { include!(\"nope.rs\"); }\npub fn added() -> u8 { 3 }\n",
                1usize,
            ),
            (
                edge_core,
                "// include!(\"nope.rs\")\npub fn added() -> u8 { 3 }\n",
                0usize,
            ),
        ] {
            write_fixture(&root, &core_sources).map_err(|error| error.to_string())?;
            let mut target_sources = core_sources.to_vec();
            target_sources.push(("src/added.rs", added_source));
            write_fixture(&root, &[("src/added.rs", added_source)])
                .map_err(|error| error.to_string())?;
            let core = build_analysis_index_from_loaded_files(&root, &load(&core_sources), &[])?;
            let target = load(&target_sources);
            let (reused, rebuilds) = index_build_probe::observe_rebase_fallbacks(|| {
                reuse_cached_index_in_run(core, &root, &target, &[])
            });
            let reused = reused?;
            assert_eq!(rebuilds, expect_rebuild, "{added_source}");
            let fresh = build_analysis_index_from_loaded_files(&root, &target, &[])?;
            assert_eq!(
                snapshot(&reused.index)?,
                snapshot(&fresh.index)?,
                "{added_source}"
            );
        }
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
