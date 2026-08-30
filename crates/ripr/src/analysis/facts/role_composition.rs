//! Contextual source-role composition across include and module edges (#3533).
//!
//! Per-file parsing classifies each function from its own file's syntax only.
//! Rust compilation crosses those boundaries: an out-of-line
//! `#[cfg(test)] mod tests;` makes every function in the child file
//! test-only, and a file-level `include!` pastes a fragment into the
//! including file's context. Before this pass, roles never crossed those
//! edges — ripr's own out-of-line test-module helpers classified as
//! `Production` and re-entered the production seam inventory.
//!
//! This pass runs **after** the style normalizer (which recomputes roles from
//! same-file text and would stomp any earlier composition) and composes roles
//! per occurrence under one closed rule:
//!
//! - **Evidence roles union; production requires both sides production.** A
//!   function already carrying an evidence role keeps it. A `Production`
//!   function inside a context that structurally requires a test build is
//!   granted `CfgTestModule`.
//! - **Composition only mints `CfgTestModule`** (evidence-only). It never
//!   creates or removes executable `TestFact`s — that stays behind the
//!   #3499/#3532 executable-test authority.
//! - **Unknown fails closed.** Ambiguous or cyclic module ownership,
//!   conflicting module/include contexts, dynamic `#[path]` targets, and
//!   unresolved targets grant nothing and name the earliest unresolved edge
//!   on the file's `SourceRoleProvenance`.
//! - **Dual contextual ownership** (one physical file both declared as a
//!   module and included as a fragment) composes only when both contexts
//!   agree; disagreement fails closed. The two-occurrence identity the issue
//!   prefers is not built here; the composed single-identity result stays
//!   conservative in the meantime.

use super::FunctionSourceRole;
use super::compilation_unit_path_from_parents;
use super::model::{ModuleDeclarationFact, ModulePathTarget, RustIndex, SourceRoleProvenance};
use super::{SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// Chain-walk bound, mirroring the include pass's depth limit. Module and
/// include chains past this bound are exotic input that fails closed instead
/// of risking unbounded recursion.
const MAX_CONTEXT_CHAIN_DEPTH: usize = 32;

/// Limitation reason: two distinct parent declarations resolve to the same
/// child file (or a default-resolution declaration matches both candidate
/// layouts), so no single contextual role can be composed.
pub(super) const REASON_MODULE_AMBIGUOUS_PARENT: &str = "rust_module_ambiguous_parent";
/// Limitation reason: the module/include context chain cycled or exceeded the
/// depth bound, so the context could not be resolved.
pub(super) const REASON_MODULE_CYCLE_OR_DEPTH_LIMIT: &str = "rust_module_cycle_or_depth_limit";
/// Limitation reason: the file is both a module child and an include fragment
/// and the two contexts disagree on test requirement.
pub(super) const REASON_MODULE_CONTEXT_CONFLICT: &str = "rust_module_context_conflict";

/// One resolved module-declaration edge: parent file declares an out-of-line
/// module whose child file exists in the index.
#[derive(Clone)]
struct ModuleEdge {
    parent: PathBuf,
    declaration: String,
    line: usize,
    requires_test: bool,
}

/// The effective test requirement of a file's composition context.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Context {
    /// Every recorded context edge resolved and none requires a test build.
    Production,
    /// At least one recorded context edge structurally requires a test build.
    RequiresTest,
    /// A context edge could not be resolved; the named reason is the earliest
    /// unresolved edge in the chain.
    Unresolved(&'static str),
}

impl Context {
    /// `Some(true)` when the context structurally requires a test build,
    /// `Some(false)` when it provably does not, `None` when unresolved.
    fn requires_test(&self) -> Option<bool> {
        match self {
            Context::RequiresTest => Some(true),
            Context::Production => Some(false),
            Context::Unresolved(_) => None,
        }
    }

    fn unresolved_reason(&self) -> Option<String> {
        match self {
            Context::Unresolved(reason) => Some((*reason).to_string()),
            _ => None,
        }
    }
}

/// Composes contextual source roles across the include and module edges of a
/// fully parsed, normalized index. Deterministic: all intermediate maps are
/// ordered, so grants and provenance are byte-identical across runs.
pub(super) fn compose_index_source_roles(index: &mut RustIndex) {
    let module_edges = resolved_module_edges(index);
    // Phase 1 (immutable): resolve every file's context and provenance.
    // The resolver borrows `include_parents`, so all mutations wait for it
    // to go out of scope.
    let resolutions: Vec<(PathBuf, Context, SourceRoleProvenance)> = {
        let mut resolver = ContextResolver {
            module_edges,
            include_parents: &index.include_parents,
            memo: BTreeMap::new(),
        };
        index
            .files
            .keys()
            .cloned()
            .map(|file| {
                let (context, provenance) = resolver.context_with_provenance(&file);
                (file, context, provenance)
            })
            .collect()
    };

    // Phase 2 (mutable): record provenance and grant evidence roles.
    let mut granted: BTreeSet<PathBuf> = BTreeSet::new();
    for (file, context, provenance) in resolutions {
        if context == Context::RequiresTest {
            granted.insert(file.clone());
        }
        let has_provenance =
            !provenance.edges.is_empty() || provenance.earliest_unresolved_reason.is_some();
        if has_provenance && let Some(facts) = index.files.get_mut(&file) {
            facts.role_provenance = provenance;
        }
    }
    if granted.is_empty() {
        return;
    }
    apply_cfg_test_module_grants(index, &granted);
}

/// Grants the evidence-only `CfgTestModule` role to `Production` functions of
/// the given files, in both the per-file facts and the flat function list.
/// Existing evidence roles (executable tests, promoted expansions, same-file
/// cfg-test helpers) are never demoted: evidence roles union.
fn apply_cfg_test_module_grants(index: &mut RustIndex, granted: &BTreeSet<PathBuf>) {
    for file in granted {
        let Some(facts) = index.files.get_mut(file) else {
            continue;
        };
        for function in &mut facts.functions {
            if function.source_role == FunctionSourceRole::Production {
                function.source_role = FunctionSourceRole::CfgTestModule;
            }
        }
    }
    for function in &mut index.functions {
        if granted.contains(&function.file)
            && function.source_role == FunctionSourceRole::Production
        {
            function.source_role = FunctionSourceRole::CfgTestModule;
        }
    }
}

/// Resolution outcome for one declaration's target path.
enum DeclarationTargets {
    /// Exactly one indexed child file.
    Exact(PathBuf),
    /// More than one indexed file matches the declaration (two claiming
    /// parents, or both default layouts present — the ambiguous E0761 shape).
    Ambiguous(Vec<PathBuf>),
    /// No indexed child (missing target, dynamic `#[path]`, or repository
    /// escape): no edge, fail closed.
    Unresolvable,
}

/// Resolves every indexed out-of-line module declaration to its child file,
/// keeping exactly one edge per child. Ambiguous resolution yields no edge:
/// the child keeps its standalone roles and records the ambiguity on its
/// provenance.
fn resolved_module_edges(index: &RustIndex) -> BTreeMap<PathBuf, Result<ModuleEdge, &'static str>> {
    let mut candidates: BTreeMap<PathBuf, BTreeSet<(PathBuf, usize, String, bool)>> =
        BTreeMap::new();
    let mut ambiguous: BTreeSet<PathBuf> = BTreeSet::new();

    for (file, facts) in &index.files {
        if facts.used_lexical_fallback || facts.module_declarations.is_empty() {
            continue;
        }
        // Module declarations inside an include fragment are pasted into the
        // compilation unit, so resolution anchors at the unit's location.
        let anchor = compilation_unit_path_from_parents(&index.include_parents, file);
        for declaration in &facts.module_declarations {
            match declaration_targets(index, &anchor, declaration) {
                DeclarationTargets::Unresolvable => {
                    // Dynamic or out-of-repository `#[path]` targets grant
                    // nothing; there is no child identity to attach
                    // provenance to.
                }
                DeclarationTargets::Ambiguous(children) => {
                    ambiguous.extend(children);
                }
                DeclarationTargets::Exact(child) => {
                    candidates.entry(child).or_default().insert((
                        file.clone(),
                        declaration.line,
                        format!("mod {};", declaration.name),
                        declaration.requires_test,
                    ));
                }
            }
        }
    }

    let mut edges = BTreeMap::new();
    for child in ambiguous {
        edges.insert(child, Err(REASON_MODULE_AMBIGUOUS_PARENT));
    }
    for (child, owners) in candidates {
        if owners.len() > 1 {
            edges.insert(child, Err(REASON_MODULE_AMBIGUOUS_PARENT));
            continue;
        }
        if let Some((parent, line, declaration, requires_test)) = owners.into_iter().next() {
            edges.insert(
                child,
                Ok(ModuleEdge {
                    parent,
                    declaration,
                    line,
                    requires_test,
                }),
            );
        }
    }
    edges
}

/// Resolves one declaration against the index. An exact string-literal
/// `#[path]` resolves relative to the declaring file's containing directory
/// (the Rust reference rule for non-inline `#[path]` targets, which differs
/// from the default stem-directory rule). Default resolution prefers
/// `<module-dir>/<name>.rs` over `<module-dir>/<name>/mod.rs`.
fn declaration_targets(
    index: &RustIndex,
    anchor: &Path,
    declaration: &ModuleDeclarationFact,
) -> DeclarationTargets {
    match &declaration.path_target {
        ModulePathTarget::Unknown => DeclarationTargets::Unresolvable,
        ModulePathTarget::Literal(literal) => {
            match resolve_relative(&directory_of(anchor), literal) {
                Some(child) if index.files.contains_key(&child) => DeclarationTargets::Exact(child),
                _ => DeclarationTargets::Unresolvable,
            }
        }
        ModulePathTarget::Default => {
            let indexed: Vec<PathBuf> = default_candidates(anchor, &declaration.name)
                .into_iter()
                .filter(|candidate| index.files.contains_key(candidate))
                .collect();
            match indexed.as_slice() {
                [] => DeclarationTargets::Unresolvable,
                [single] => DeclarationTargets::Exact(single.clone()),
                // Both default layouts indexed at once: ambiguous.
                [..] => DeclarationTargets::Ambiguous(indexed.clone()),
            }
        }
    }
}

/// The default-resolution candidate layouts for `mod <name>;` declared in
/// `anchor`: `<module-dir>/<name>.rs` and `<module-dir>/<name>/mod.rs`.
fn default_candidates(anchor: &Path, name: &str) -> Vec<PathBuf> {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let module_dir = module_directory(anchor);
    vec![
        module_dir.join(format!("{name}.rs")),
        module_dir.join(name).join("mod.rs"),
    ]
}

/// The module directory of a file: its containing directory for `mod.rs`,
/// `lib.rs`, and `main.rs`, otherwise the directory named after the file stem
/// (`test_styles.rs` resolves child modules under `test_styles/`).
fn module_directory(file: &Path) -> PathBuf {
    let file_name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let directory = directory_of(file);
    match file_name {
        "mod.rs" | "lib.rs" | "main.rs" => directory,
        _ => match file.file_stem() {
            Some(stem) => directory.join(stem),
            None => directory,
        },
    }
}

fn directory_of(file: &Path) -> PathBuf {
    file.parent().map(Path::to_path_buf).unwrap_or_default()
}

/// Resolves a `#[path]` literal against a base directory, rejecting absolute
/// targets and repository escapes the same way the include pass does.
fn resolve_relative(base: &Path, literal: &str) -> Option<PathBuf> {
    if Path::new(literal).is_absolute() {
        return None;
    }
    let mut normalized = base.to_path_buf();
    for component in Path::new(literal).components() {
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

/// Resolves each file's effective context with memoization, cycle guarding,
/// and a depth bound, producing the provenance chain alongside the verdict.
struct ContextResolver<'index> {
    module_edges: BTreeMap<PathBuf, Result<ModuleEdge, &'static str>>,
    include_parents: &'index BTreeMap<PathBuf, PathBuf>,
    memo: BTreeMap<PathBuf, (Context, SourceRoleProvenance)>,
}

impl ContextResolver<'_> {
    fn context_with_provenance(&mut self, file: &Path) -> (Context, SourceRoleProvenance) {
        let mut visiting = BTreeSet::new();
        self.resolve(file, &mut visiting, 0)
    }

    fn resolve(
        &mut self,
        file: &Path,
        visiting: &mut BTreeSet<PathBuf>,
        depth: usize,
    ) -> (Context, SourceRoleProvenance) {
        if let Some(cached) = self.memo.get(file) {
            return cached.clone();
        }
        if depth > MAX_CONTEXT_CHAIN_DEPTH || !visiting.insert(file.to_path_buf()) {
            return (
                Context::Unresolved(REASON_MODULE_CYCLE_OR_DEPTH_LIMIT),
                SourceRoleProvenance {
                    edges: Vec::new(),
                    earliest_unresolved_reason: Some(
                        REASON_MODULE_CYCLE_OR_DEPTH_LIMIT.to_string(),
                    ),
                },
            );
        }

        let module_edge = self.module_edges.get(file).cloned();
        let include_parent = self.include_parents.get(file).cloned();

        let result = match (module_edge, include_parent) {
            (Some(Err(reason)), _) => {
                let context = Context::Unresolved(reason);
                let provenance = SourceRoleProvenance {
                    edges: Vec::new(),
                    earliest_unresolved_reason: context.unresolved_reason(),
                };
                (context, provenance)
            }
            (Some(Ok(edge)), None) => {
                let (parent_context, mut provenance) =
                    self.resolve(&edge.parent, visiting, depth + 1);
                let context = match (edge.requires_test, &parent_context) {
                    (true, _) => Context::RequiresTest,
                    (false, Context::RequiresTest) => Context::RequiresTest,
                    (false, Context::Production) => Context::Production,
                    (false, Context::Unresolved(reason)) => Context::Unresolved(reason),
                };
                provenance.edges.push(SourceRoleProvenanceEdge {
                    kind: SourceRoleProvenanceEdgeKind::Module,
                    parent: edge.parent,
                    child: file.to_path_buf(),
                    declaration: edge.declaration,
                    line: edge.line,
                    requires_test: edge.requires_test,
                });
                if provenance.earliest_unresolved_reason.is_none() {
                    provenance.earliest_unresolved_reason = context.unresolved_reason();
                }
                (context, provenance)
            }
            (None, Some(parent)) => {
                let (parent_context, mut provenance) = self.resolve(&parent, visiting, depth + 1);
                let requires_test = parent_context.requires_test().unwrap_or(false);
                provenance.edges.push(SourceRoleProvenanceEdge {
                    kind: SourceRoleProvenanceEdgeKind::Include,
                    parent,
                    child: file.to_path_buf(),
                    declaration: "include!".to_string(),
                    // The include pass keeps only the parent map, not the
                    // directive line; the edge is exactly identified by the
                    // parent/child pair.
                    line: 0,
                    requires_test,
                });
                if let Some(reason) = parent_context.unresolved_reason()
                    && provenance.earliest_unresolved_reason.is_none()
                {
                    provenance.earliest_unresolved_reason = Some(reason);
                }
                (parent_context, provenance)
            }
            (Some(Ok(edge)), Some(parent)) => {
                // Dual contextual ownership: compose only when both sides
                // agree on the verdict, naming the conflict otherwise (law 10).
                let (module_context, module_provenance) =
                    self.resolve(&edge.parent, visiting, depth + 1);
                let (include_context, include_provenance) =
                    self.resolve(&parent, visiting, depth + 1);
                let module_verdict = if edge.requires_test {
                    Some(true)
                } else {
                    module_context.requires_test()
                };
                let include_verdict = include_context.requires_test();
                let mut provenance = module_provenance;
                let mut edges = include_provenance.edges;
                edges.push(SourceRoleProvenanceEdge {
                    kind: SourceRoleProvenanceEdgeKind::Include,
                    parent,
                    child: file.to_path_buf(),
                    declaration: "include!".to_string(),
                    line: 0,
                    requires_test: include_verdict.unwrap_or(false),
                });
                provenance.edges.append(&mut edges);
                match (module_verdict, include_verdict) {
                    (Some(module_test), Some(include_test)) if module_test == include_test => {
                        provenance.edges.push(SourceRoleProvenanceEdge {
                            kind: SourceRoleProvenanceEdgeKind::Module,
                            parent: edge.parent,
                            child: file.to_path_buf(),
                            declaration: edge.declaration,
                            line: edge.line,
                            requires_test: edge.requires_test,
                        });
                        let context = if module_test {
                            Context::RequiresTest
                        } else {
                            Context::Production
                        };
                        if provenance.earliest_unresolved_reason.is_none() {
                            provenance.earliest_unresolved_reason = context.unresolved_reason();
                        }
                        (context, provenance)
                    }
                    (Some(_), Some(_)) => {
                        provenance.earliest_unresolved_reason =
                            Some(REASON_MODULE_CONTEXT_CONFLICT.to_string());
                        (
                            Context::Unresolved(REASON_MODULE_CONTEXT_CONFLICT),
                            provenance,
                        )
                    }
                    (None, _) | (_, None) => {
                        let reason = module_context
                            .unresolved_reason()
                            .or_else(|| include_context.unresolved_reason())
                            .unwrap_or_else(|| REASON_MODULE_CONTEXT_CONFLICT.to_string());
                        provenance.earliest_unresolved_reason = Some(reason);
                        (
                            Context::Unresolved(REASON_MODULE_CONTEXT_CONFLICT),
                            provenance,
                        )
                    }
                }
            }
            (None, None) => (Context::Production, SourceRoleProvenance::default()),
        };

        visiting.remove(file);
        self.memo.insert(file.to_path_buf(), result.clone());
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::build_index_from_loaded_files_with_cache;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("ripr-role-composition-{name}-{stamp}"));
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }

    fn write_manifest(root: &Path) -> Result<(), String> {
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='role-composition'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())
    }

    fn write(root: &Path, relative: &str, source: &str) -> Result<PathBuf, String> {
        let full = root.join(relative);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&full, source).map_err(|error| error.to_string())?;
        Ok(PathBuf::from(relative))
    }

    fn role_of(index: &RustIndex, file: &str, name: &str) -> Result<FunctionSourceRole, String> {
        index
            .functions
            .iter()
            .find(|function| function.file == Path::new(file) && function.name == name)
            .map(|function| function.source_role)
            .ok_or_else(|| format!("missing function fact for {file}::{name}"))
    }

    fn file_role_of(
        index: &RustIndex,
        file: &str,
        name: &str,
    ) -> Result<FunctionSourceRole, String> {
        index
            .files
            .get(Path::new(file))
            .and_then(|facts| {
                facts
                    .functions
                    .iter()
                    .find(|function| function.name == name)
                    .map(|function| function.source_role)
            })
            .ok_or_else(|| format!("missing per-file fact for {file}::{name}"))
    }

    fn test_names(index: &RustIndex) -> Vec<String> {
        index.tests.iter().map(|test| test.name.clone()).collect()
    }

    /// The live-defect shape: ripr's own `#[cfg(test)] mod tests;` modules.
    /// The normalizer runs before composition and its same-file walk cannot
    /// see the out-of-line gate, so without the composition pass this helper
    /// demotes to `Production` — this test doubles as the
    /// normalizer-does-not-stomp pin (reordering the passes in `facts::mod`
    /// fails it).
    #[test]
    fn out_of_line_cfg_test_module_composes_evidence_roles() -> Result<(), String> {
        let root = temp_dir("out-of-line")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "pub fn production_control() -> i32 { 1 }\n\n#[cfg(test)]\nmod tests;\n",
            )?,
            write(
                &root,
                "src/tests.rs",
                "pub fn unattributed_helper() -> i32 { 2 }\n\n#[test]\nfn real_test() { assert_eq!(unattributed_helper(), 2); }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/lib.rs", "production_control")?,
            FunctionSourceRole::Production
        );
        assert_eq!(
            role_of(&index, "src/tests.rs", "unattributed_helper")?,
            FunctionSourceRole::CfgTestModule
        );
        assert_eq!(
            role_of(&index, "src/tests.rs", "real_test")?,
            FunctionSourceRole::TestAttribute
        );
        assert_eq!(
            file_role_of(&index, "src/tests.rs", "unattributed_helper")?,
            FunctionSourceRole::CfgTestModule,
            "per-file facts and the flat list must stay in sync"
        );
        // Composition only mints evidence-only roles; executable-test
        // membership stays TestFact-driven (law 9).
        let names = test_names(&index);
        assert!(names.contains(&"real_test".to_string()));
        assert!(!names.contains(&"unattributed_helper".to_string()));
        // Provenance records the granting chain.
        let provenance = &index.files[Path::new("src/tests.rs")].role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        assert_eq!(provenance.edges.len(), 1);
        let edge = &provenance.edges[0];
        assert_eq!(edge.kind, SourceRoleProvenanceEdgeKind::Module);
        assert_eq!(edge.parent, Path::new("src/lib.rs"));
        assert_eq!(edge.child, Path::new("src/tests.rs"));
        assert!(edge.requires_test);
        assert_eq!(edge.declaration, "mod tests;");
        assert!(edge.line >= 3);
        // Standalone files record no provenance.
        let parent_provenance = &index.files[Path::new("src/lib.rs")].role_provenance;
        assert!(parent_provenance.edges.is_empty());
        Ok(())
    }

    /// Control: a production `mod child;` keeps its child
    /// production-subject eligible (law 2).
    #[test]
    fn production_module_child_control_stays_production() -> Result<(), String> {
        let root = temp_dir("production-child")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "mod child;\n")?,
            write(
                &root,
                "src/child.rs",
                "pub fn child_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/child.rs", "child_helper")?,
            FunctionSourceRole::Production
        );
        // The chain is recorded without granting: the edge resolved but does
        // not require a test build.
        let provenance = &index.files[Path::new("src/child.rs")].role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        assert_eq!(provenance.edges.len(), 1);
        assert!(!provenance.edges[0].requires_test);
        Ok(())
    }

    /// The declaration chain composes transitively: a test-required module
    /// under an ordinary production parent still gates its own child file.
    #[test]
    fn nested_chain_through_production_parent_composes() -> Result<(), String> {
        let root = temp_dir("nested-chain")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "mod ordinary;\n")?,
            write(
                &root,
                "src/ordinary.rs",
                "pub fn ordinary_helper() -> i32 { 1 }\n\n#[cfg(test)]\nmod tests;\n",
            )?,
            write(
                &root,
                "src/ordinary/tests.rs",
                "pub fn nested_helper() -> i32 { 2 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/ordinary.rs", "ordinary_helper")?,
            FunctionSourceRole::Production
        );
        assert_eq!(
            role_of(&index, "src/ordinary/tests.rs", "nested_helper")?,
            FunctionSourceRole::CfgTestModule
        );
        let provenance = &index.files[Path::new("src/ordinary/tests.rs")].role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        let edges = &provenance.edges;
        assert_eq!(edges.len(), 2, "both chain edges are recorded");
        assert_eq!(edges[0].parent, Path::new("src/lib.rs"));
        assert_eq!(edges[0].child, Path::new("src/ordinary.rs"));
        assert!(!edges[0].requires_test, "the outer parent is production");
        assert_eq!(edges[1].parent, Path::new("src/ordinary.rs"));
        assert_eq!(edges[1].child, Path::new("src/ordinary/tests.rs"));
        assert!(edges[1].requires_test);
        Ok(())
    }

    /// An exact `#[path]` redirection follows the declaring module's cfg
    /// context (law 5).
    #[test]
    fn exact_path_attribute_module_follows_declaring_context() -> Result<(), String> {
        let root = temp_dir("path-attribute")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)]\n#[path = \"support/shared.rs\"]\nmod support;\n",
            )?,
            write(
                &root,
                "src/support/shared.rs",
                "pub fn redirected_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/support/shared.rs", "redirected_helper")?,
            FunctionSourceRole::CfgTestModule
        );
        Ok(())
    }

    /// A dynamic `#[path]` expression must not fall back to default name
    /// resolution: the typed unknown fails closed (law 6).
    #[test]
    fn dynamic_path_attribute_fails_closed() -> Result<(), String> {
        let root = temp_dir("dynamic-path")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)]\n#[path = concat!(env!(\"OUT_DIR\"), \"generated.rs\")]\nmod generated;\n",
            )?,
            write(
                &root,
                "src/generated.rs",
                "pub fn generated_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/generated.rs", "generated_helper")?,
            FunctionSourceRole::Production,
            "an unresolvable #[path] must not compose a role, even though a same-named file exists at the default location"
        );
        Ok(())
    }

    /// A file-level include fragment inherits the including context: under a
    /// test-required module the fragment is evidence-role, and its own exact
    /// test attributes still win (evidence union).
    #[test]
    fn include_fragment_inherits_test_required_context() -> Result<(), String> {
        let root = temp_dir("include-test-context")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "#[cfg(test)]\nmod tests;\n")?,
            write(&root, "src/tests.rs", "include!(\"fragment.rs\");\n")?,
            write(
                &root,
                "src/fragment.rs",
                "pub fn fragment_helper() -> i32 { 3 }\n\n#[test]\nfn fragment_test() { assert_eq!(fragment_helper(), 3); }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/fragment.rs", "fragment_helper")?,
            FunctionSourceRole::CfgTestModule
        );
        assert_eq!(
            role_of(&index, "src/fragment.rs", "fragment_test")?,
            FunctionSourceRole::TestAttribute
        );
        let names = test_names(&index);
        assert!(names.contains(&"fragment_test".to_string()));
        assert!(!names.contains(&"fragment_helper".to_string()));
        let provenance = &index.files[Path::new("src/fragment.rs")].role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        let edges = &provenance.edges;
        assert_eq!(edges.len(), 2, "module edge then include edge");
        assert_eq!(edges[0].kind, SourceRoleProvenanceEdgeKind::Module);
        assert_eq!(edges[1].kind, SourceRoleProvenanceEdgeKind::Include);
        assert_eq!(edges[1].parent, Path::new("src/tests.rs"));
        assert_eq!(edges[1].child, Path::new("src/fragment.rs"));
        assert!(edges[1].requires_test);
        Ok(())
    }

    /// Both sides production stays production: a fragment included by a
    /// production compilation unit keeps production-subject eligibility
    /// (law 4, second half).
    #[test]
    fn production_include_requires_both_sides_production() -> Result<(), String> {
        let root = temp_dir("include-production")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "include!(\"shared.rs\");\n")?,
            write(
                &root,
                "src/shared.rs",
                "pub fn shared_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/shared.rs", "shared_helper")?,
            FunctionSourceRole::Production
        );
        Ok(())
    }

    /// Two parent declarations claiming the same child file fail closed and
    /// name the edge; the child keeps its standalone roles.
    #[test]
    fn ambiguous_module_parents_fail_closed() -> Result<(), String> {
        let root = temp_dir("ambiguous-module")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "mod other;\n\n#[path = \"shared.rs\"]\nmod from_lib;\n",
            )?,
            write(
                &root,
                "src/other.rs",
                "#[path = \"shared.rs\"]\nmod from_other;\n",
            )?,
            write(
                &root,
                "src/shared.rs",
                "pub fn contested_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/shared.rs", "contested_helper")?,
            FunctionSourceRole::Production
        );
        let provenance = &index.files[Path::new("src/shared.rs")].role_provenance;
        assert_eq!(
            provenance.earliest_unresolved_reason.as_deref(),
            Some(REASON_MODULE_AMBIGUOUS_PARENT),
            "the earliest unresolved edge must be named"
        );
        Ok(())
    }

    /// One file both declared as a module and included as a fragment, with
    /// the two contexts disagreeing on test requirement, fails closed
    /// (law 10: duplicate contextual ownership names the edge).
    #[test]
    fn module_include_context_conflict_fails_closed() -> Result<(), String> {
        let root = temp_dir("context-conflict")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "mod shared;\n\n#[cfg(test)]\nmod tests;\n",
            )?,
            write(&root, "src/tests.rs", "include!(\"shared.rs\");\n")?,
            write(
                &root,
                "src/shared.rs",
                "pub fn dual_context_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/shared.rs", "dual_context_helper")?,
            FunctionSourceRole::Production
        );
        let provenance = &index.files[Path::new("src/shared.rs")].role_provenance;
        assert_eq!(
            provenance.earliest_unresolved_reason.as_deref(),
            Some(REASON_MODULE_CONTEXT_CONFLICT)
        );
        Ok(())
    }

    /// The agreeing dual-context shape composes: both contexts require a
    /// test build, so the evidence grant is sound.
    #[test]
    fn agreeing_dual_context_composes() -> Result<(), String> {
        let root = temp_dir("dual-agree")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)]\nmod shared;\n\n#[cfg(test)]\nmod tests;\n",
            )?,
            write(&root, "src/tests.rs", "include!(\"shared.rs\");\n")?,
            write(
                &root,
                "src/shared.rs",
                "pub fn agreed_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/shared.rs", "agreed_helper")?,
            FunctionSourceRole::CfgTestModule
        );
        let provenance = &index.files[Path::new("src/shared.rs")].role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        assert!(
            provenance.edges.len() >= 3,
            "include chain plus module edge are all recorded"
        );
        Ok(())
    }

    /// Composed roles survive the warm per-file fact cache: cached entries
    /// carry the module-declaration producer output (FILE_FACT cache schema
    /// 0.8), and the composition pass re-derives roles on warm runs.
    #[test]
    fn composed_roles_survive_warm_file_fact_cache() -> Result<(), String> {
        let root = temp_dir("warm-cache")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "pub fn production_control() -> i32 { 1 }\n\n#[cfg(test)]\nmod tests;\n",
            )?,
            write(
                &root,
                "src/tests.rs",
                "pub fn unattributed_helper() -> i32 { 2 }\n",
            )?,
        ];
        let mut loaded: Vec<(PathBuf, Vec<u8>)> = Vec::new();
        for file in &files {
            let bytes = fs::read(root.join(file)).map_err(|error| error.to_string())?;
            loaded.push((file.clone(), bytes));
        }

        let cold = build_index_from_loaded_files_with_cache(&root, &loaded)
            .map_err(|error| error.to_string())?;
        assert_eq!(
            role_of(&cold.index, "src/tests.rs", "unattributed_helper")?,
            FunctionSourceRole::CfgTestModule
        );

        let warm = build_index_from_loaded_files_with_cache(&root, &loaded)
            .map_err(|error| error.to_string())?;
        assert!(
            warm.file_fact_cache.hits > 0,
            "the warm run must hit the per-file fact cache"
        );
        assert_eq!(
            role_of(&warm.index, "src/tests.rs", "unattributed_helper")?,
            FunctionSourceRole::CfgTestModule,
            "a warm cache hit must not serve composition-blind facts"
        );
        Ok(())
    }
}
