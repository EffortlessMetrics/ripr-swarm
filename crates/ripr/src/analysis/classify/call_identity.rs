//! Which same-named free function a test's call names (#6292, #6537,
//! #6544).
//!
//! A call matched by its last path segment alone lets a test of `b::render`
//! relate to, reach, and lend tokens or pins to a changed `a::render`. This
//! module is the one rule diff-mode related tests, repo-mode test grip and
//! the owner-return pin share to tell those definitions apart.
//!
//! The rule only applies when the owner is a module-level `fn` and another
//! module-level `fn` of the same name exists in the owner's crate. For
//! each call of the name in the test body it reads the spelled path:
//!
//! - a qualified call's leading `crate`, `self` or `super` resolves from
//!   the test's module; any other leading segment must be bound by a `use`
//!   around the test (followed through its path, renames included), a `mod`
//!   declared there, or one of the owner's own crate names as seen from the
//!   test's crate. A dependency's `other_dep::a::render(..)` stays unsettled;
//! - a bare call is bound by the innermost scope that binds the name: a `use`
//!   at the test body's top level, then each enclosing module's own `use`
//!   items and item definitions, following `use super::*` outward and
//!   reading other globs. A binding inside a nested block of the test leaves
//!   every call of that name unsettled.
//!
//! A call settles on the owner only when the spelled path matches the
//! owner and nothing else. It settles on another definition only when it
//! matches that definition and not the owner. Everything else (a renamed
//! bare import, an unparseable file, a re-export path, a definition the
//! parser could not place) stays unsettled, and callers keep their prior
//! behavior: an unsettled call never grants new credit.
//!
//! A test that may reach the owner without a spelled call of its name (the
//! owner passed as a value, `.map(crate::a::render)`, or called through a
//! renaming import, `use crate::a::render as fast;`) never settles on
//! another definition: it stays unsettled. A test in another package, or a
//! module that matches no placed definition, is unsettled the same way.
//!
//! [`OwnerCallIdentity::resolve_test`] aggregates a test's calls for coarse
//! related-test selection; the owner-return pin resolves its asserted call
//! alone with [`OwnerCallIdentity::resolve_call`].
//!
//! This is bounded syntax, not name resolution. File-backed module paths
//! come from the file's location under `src/`; a package with any `#[path]`
//! attribute switches the rule off. In a package with both `src/lib.rs` and
//! `src/main.rs`, a `src/` module belongs to the root whose top-level `mod`
//! declarations name it; one neither or both roots name switches the rule
//! off for its definitions and leaves its tests outside the owner's crate. Macro-generated items are a documented
//! residual that reads unsettled or mismatched, never as a new grant.

use super::super::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::{
    NameScope, NameScopes, RustModuleTreeEdge, UseBinding, name_scopes_for_fn,
    rust_module_tree_scan,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// What a test's calls of the owner's name settle on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum CallTarget {
    /// At least one call names the owner and nothing else.
    Owner,
    /// Every call names another definition, never the owner.
    Other,
    /// No call settles the question.
    Unsettled,
}

type TestKey = (PathBuf, usize);

/// One identity per owner definition per run, built on first ask. Keyed by
/// the owner's parser symbol id, which is unique per definition even when
/// two same-named owners share one source line (inline sibling modules).
type SharedIdentities = BTreeMap<String, Option<std::sync::Arc<OwnerCallIdentity>>>;

/// Per-index memos for the call-identity family (#7716 residual).
///
/// Every producer that classifies probes rebuilds its [`RustIndex`] per
/// analysis run (the CLI diff pass, an MCP refresh, an LSP analysis), so a
/// memo held here lives exactly one run and can never outlive the sources it
/// was computed from. A cloned index starts with empty memos and recomputes
/// them on demand. Without the memo each construction repeated a whole-index
/// layout scan and each probe re-parsed every candidate test: measured
/// ~28 ms of related-test work per probe pair on a 20-package corpus where
/// the work is identical across probes.
#[derive(Default)]
pub(crate) struct CallIdentityMemos {
    /// One workspace shape pass per index, shared by every
    /// [`OwnerCallIdentity`] construction and the `#[path]` remap check.
    shapes: std::sync::OnceLock<BTreeMap<Option<String>, std::sync::Arc<PackageCallIdentityShape>>>,
    /// One identity per owner definition per run. Its per-test memo and
    /// scope parses then serve every probe and every pin of the run instead
    /// of dying with each construction.
    identities: Mutex<SharedIdentities>,
    /// RIPR-SPEC-0035: every workspace `const`/`static` by name with its
    /// file and masked initializer, built once per index on first use by
    /// the owner-return pin's self-computed expected-value check.
    constants: std::sync::OnceLock<ConstantTable>,
}

impl Clone for CallIdentityMemos {
    fn clone(&self) -> Self {
        // A cloned index recomputes: the memos describe THIS index's sources,
        // and a clone may be mutated independently of the original.
        Self::default()
    }
}

impl std::fmt::Debug for CallIdentityMemos {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CallIdentityMemos")
            .finish_non_exhaustive()
    }
}

/// The whole-index answers one package's call-identity checks need: whether
/// any of its files carries a `#[path]` attribute (which switches the rule
/// off) and how the package splits `src/` between its crates.
struct PackageCallIdentityShape {
    remaps_module_paths: bool,
    layout: CrateLayout,
}

/// One workspace pass over `index.files()`, grouping the `#[path]` remap
/// flag and the crate-root sources by package key. Mirrors the per-package
/// scans it replaces exactly: `has_path_attribute` over every file of the
/// package (`package_remaps_module_paths`), and the `lib.rs`/`main.rs`
/// root-source collection of [`CrateLayout::new`] with the same
/// `under_tests_dir` and `src_body` filters.
fn scan_call_identity_shapes(
    index: &RustIndex,
) -> BTreeMap<Option<String>, std::sync::Arc<PackageCallIdentityShape>> {
    let mut remaps: BTreeMap<Option<String>, bool> = BTreeMap::new();
    let mut libs: BTreeMap<Option<String>, Vec<std::sync::Arc<str>>> = BTreeMap::new();
    let mut mains: BTreeMap<Option<String>, Vec<std::sync::Arc<str>>> = BTreeMap::new();
    for (path, facts) in index.files().iter() {
        let package = package_key(path);
        let remapped = remaps.entry(package.clone()).or_insert(false);
        if !*remapped && has_path_attribute(&facts.data().source) {
            *remapped = true;
        }
        if under_tests_dir(path) {
            continue;
        }
        match src_body(&super::related_tests::normalize_path(path)) {
            Some("lib.rs") => libs
                .entry(package)
                .or_default()
                .push(std::sync::Arc::clone(&facts.data().source)),
            Some("main.rs") => mains
                .entry(package)
                .or_default()
                .push(std::sync::Arc::clone(&facts.data().source)),
            _ => {}
        }
    }
    // Every file's package key is inserted into `remaps` above, so it
    // already holds every package with an indexed file.
    remaps
        .into_iter()
        .map(|(package, remaps_module_paths)| {
            let lib = libs.get(&package).cloned().unwrap_or_default();
            let main = mains.get(&package).cloned().unwrap_or_default();
            let declared = |sources: Vec<std::sync::Arc<str>>| match sources.as_slice() {
                [source] => declared_first_segments(source),
                _ => None,
            };
            let main_only = lib.is_empty() && !main.is_empty();
            let roots =
                (!lib.is_empty() && !main.is_empty()).then(|| (declared(lib), declared(main)));
            let shape = PackageCallIdentityShape {
                remaps_module_paths,
                layout: CrateLayout {
                    package: package.clone(),
                    roots,
                    main_only,
                },
            };
            (package, std::sync::Arc::new(shape))
        })
        .collect()
}

impl CallIdentityMemos {
    /// Forget every memo: an index mutation (a new or replaced file, new
    /// functions) can change both the workspace shape answers and each
    /// owner's rival set, so a memo computed earlier must not survive it.
    /// Build-time inserts pay nothing (the memos are still empty); the
    /// producers never mutate a live index mid-analysis.
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// The memoized workspace constant table, built by `build` on first use.
    pub(crate) fn workspace_constants(
        &self,
        build: impl FnOnce() -> ConstantTable,
    ) -> &ConstantTable {
        self.constants.get_or_init(build)
    }
}

/// Workspace `const`/`static` items by name: file, masked initializer, and
/// the declared `T` of `: T` when the item states one (#8112).
pub(crate) type ConstantTable = BTreeMap<String, Vec<(std::path::PathBuf, String, Option<String>)>>;

/// The memoized (or freshly scanned) whole-index answers for `package`.
fn call_identity_shape(
    index: &RustIndex,
    package: Option<&str>,
) -> std::sync::Arc<PackageCallIdentityShape> {
    let shapes = index
        .call_identity_memos
        .shapes
        .get_or_init(|| scan_call_identity_shapes(index));
    shapes
        .get(&package.map(str::to_string))
        .cloned()
        .unwrap_or_else(|| {
            // A package with no indexed file: the per-package scans it
            // replaces both answered `false`/no-roots for this case.
            std::sync::Arc::new(PackageCallIdentityShape {
                remaps_module_paths: false,
                layout: CrateLayout {
                    package: package.map(str::to_string),
                    roots: None,
                    main_only: false,
                },
            })
        })
}

/// The owner's call identity for this analysis run, shared by every caller:
/// the related-test relation, the owner-return pin and the dependent-scope
/// pin-scope check all ask for the same (owner, index) answer, and the
/// identity's own per-test memo and scope parses are worth sharing across
/// probes (#7716). `None` exactly when a fresh [`OwnerCallIdentity::new`]
/// construction would also answer `None` (no same-package rival, a mixed
/// index, a `#[path]` remap, an unclassifiable layout).
pub(in crate::analysis) fn shared_owner_call_identity(
    index: &RustIndex,
    owner: &FunctionSummary,
) -> Option<std::sync::Arc<OwnerCallIdentity>> {
    let key = owner.id.0.clone();
    if let Some(memo) = index
        .call_identity_memos
        .identities
        .lock()
        .ok()
        .and_then(|identities| identities.get(&key).cloned())
    {
        return memo;
    }
    let built =
        OwnerCallIdentity::new(owner, index.functions().iter(), index).map(std::sync::Arc::new);
    // A poisoned lock only costs the sharing, never a wrong answer: the
    // fallback rebuilds fresh, exactly as before the memo existed.
    if let Ok(mut identities) = index.call_identity_memos.identities.lock() {
        // A concurrent miss may have stored its construction first; serve
        // the stored instance so the per-test memo and scope parses stay
        // shared, and drop this redundant twin.
        return identities.entry(key).or_insert(built).clone();
    }
    built
}

/// The owner and its same-named, same-package module-level rivals.
pub(in crate::analysis) struct OwnerCallIdentity {
    name: String,
    /// The owner's crate within its package (`CrateLayout::class`).
    owner_crate: Option<String>,
    /// How the owner's package splits `src/` between its crates.
    layout: CrateLayout,
    owner_file: PathBuf,
    owner_package: Option<String>,
    /// Module path of each definition: the owner first. `None` when the
    /// definition could not be placed, so it matches every spelled path.
    definitions: Vec<Option<Vec<String>>>,
    memo: Mutex<BTreeMap<TestKey, CallTarget>>,
    scopes: Mutex<BTreeMap<TestKey, Option<Arc<NameScopes>>>>,
}

/// Which definitions a spelled path can name.
enum Matches {
    /// Any definition: the path does not narrow the set.
    Any,
    /// These definition indices (0 is the owner).
    Some(BTreeSet<usize>),
}

impl Matches {
    fn none() -> Self {
        Self::Some(BTreeSet::new())
    }

    fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Some(mut left), Self::Some(right)) => {
                left.extend(right);
                Self::Some(left)
            }
            _ => Self::Any,
        }
    }
}

/// What one test's calls are resolved against: the test file's module
/// path, the lexical scopes around the test, and the identifiers that name
/// the owner's own crate from the test's crate.
struct TestScope {
    /// The test file's module path in the owner's crate; `None` when the
    /// test is in another crate (an integration test of the owner's
    /// library), whose `crate::`, `self::`, `super::` and local modules
    /// name nothing of the owner's crate.
    file_segments: Option<Vec<String>>,
    scopes: Option<Arc<NameScopes>>,
    crate_names: BTreeSet<String>,
}

impl OwnerCallIdentity {
    /// `None` when the rule does not apply: the owner is not a placed
    /// module-level `fn`, no other module-level `fn` of its name exists in
    /// its crate among `same_named`, the package remaps a module file with
    /// `#[path]` (so the physical layout may not be the module path), the
    /// owner or a same-named rival sits in a `src/` module whose crate the
    /// roots' `mod` declarations do not settle, or a same-named definition
    /// is spelled in the other path form (absolute against relative).
    /// Callers then keep their prior behavior, which never grants new
    /// credit.
    pub(in crate::analysis) fn new<'a>(
        owner: &FunctionSummary,
        same_named: impl IntoIterator<Item = &'a FunctionSummary>,
        index: &RustIndex,
    ) -> Option<Self> {
        if owner.item.container != FunctionContainer::Free {
            return None;
        }
        let owner_path = definition_module_path(owner)?;
        let owner_package = package_key(&owner.file);
        let owner_absolute = super::related_tests::path_is_absolute_form(&owner.file);
        let mut rivals = Vec::new();
        for function in same_named {
            if function.name != owner.name
                || (function.file == owner.file && function.start_line == owner.start_line)
            {
                continue;
            }
            if function.item.container == FunctionContainer::Free
                && super::related_tests::path_is_absolute_form(&function.file) != owner_absolute
            {
                // A mixed index cannot be told apart by package: rule off.
                return None;
            }
            if package_key(&function.file) == owner_package {
                rivals.push(function);
            }
        }
        if rivals.is_empty() || package_remaps_module_paths(index, owner_package.as_deref()) {
            return None;
        }
        let layout = CrateLayout::new(index, owner_package.as_deref());
        let CrateClass::Crate(owner_crate) = layout.classify(&owner.file) else {
            return None;
        };
        let mut definitions = vec![Some(owner_path)];
        for function in rivals {
            match function.item.container {
                FunctionContainer::Free if function.item.has_body => {
                    match layout.classify(&function.file) {
                        // Only a library-shaped `src/` module is ambiguous,
                        // and it may compose into either root's crate.
                        CrateClass::Ambiguous if owner_crate == "lib" || owner_crate == "main" => {
                            return None;
                        }
                        CrateClass::Crate(class) if class == owner_crate => {
                            definitions.push(definition_module_path(function));
                        }
                        _ => {}
                    }
                }
                FunctionContainer::Unknown => definitions.push(None),
                _ => {}
            }
        }
        if definitions.len() < 2 {
            return None;
        }
        Some(Self {
            name: owner.name.clone(),
            owner_crate: Some(owner_crate),
            layout,
            owner_file: owner.file.clone(),
            owner_package,
            definitions,
            memo: Mutex::new(BTreeMap::new()),
            scopes: Mutex::new(BTreeMap::new()),
        })
    }

    /// Whether `function` is one of the rivals this identity tells apart
    /// from the owner per test: a module-level `fn` of the owner's name in
    /// its package.
    /// Another crate's definition (an integration test's or a binary's
    /// root, an `extern` declaration without a body) is not settled here
    /// and keeps competing.
    pub(in crate::analysis) fn settles(&self, function: &FunctionSummary) -> bool {
        function.name == self.name
            && function.item.container == FunctionContainer::Free
            && function.item.has_body
            && package_key(&function.file) == self.owner_package
            && self.layout.class(&function.file) == self.owner_crate
    }

    /// What `test`'s calls of the owner's name settle on, taken together:
    /// `Owner` when any call names the owner alone. Coarse, for related-test
    /// selection; a pin on one asserted call resolves that call with
    /// [`Self::resolve_call`].
    pub(in crate::analysis) fn resolve_test(
        &self,
        test: &TestSummary,
        index: &RustIndex,
    ) -> CallTarget {
        let key = (test.file.clone(), test.start_line);
        if let Some(known) = self
            .memo
            .lock()
            .ok()
            .and_then(|memo| memo.get(&key).copied())
        {
            return known;
        }
        let target = self.resolve_test_uncached(test, index);
        if let Ok(mut memo) = self.memo.lock() {
            memo.insert(key, target);
        }
        target
    }

    /// What one call of the owner's name in `test`'s body, spelled with the
    /// module path `qualifier` (empty for a bare call), settles on.
    pub(in crate::analysis) fn resolve_call(
        &self,
        test: &TestSummary,
        index: &RustIndex,
        qualifier: &[String],
    ) -> CallTarget {
        let scope = self.test_scope(test, index);
        self.target_of(&self.resolve_spelled(qualifier, &scope))
    }

    fn resolve_test_uncached(&self, test: &TestSummary, index: &RustIndex) -> CallTarget {
        let qualifiers = call_qualifiers(&test.body, &self.name);
        if qualifiers.is_empty() {
            return CallTarget::Unsettled;
        }
        let scope = self.test_scope(test, index);
        let mut owner = false;
        let mut unsettled = false;
        for qualifier in qualifiers {
            let matches = match qualifier {
                None => Matches::Any,
                Some(segments) => self.resolve_spelled(&segments, &scope),
            };
            match self.target_of(&matches) {
                CallTarget::Owner => owner = true,
                CallTarget::Unsettled => unsettled = true,
                CallTarget::Other => {}
            }
        }
        if owner {
            CallTarget::Owner
        } else if unsettled || self.reached_without_a_call(&test.body, &scope) {
            CallTarget::Unsettled
        } else {
            CallTarget::Other
        }
    }

    /// Whether the test may reach the owner without a spelled call of its
    /// name: the owner passed as a value (`.map(crate::a::render)`, `let f =
    /// render;`) or called through a renaming import (`use crate::a::render
    /// as fast;`). Such a test is unsettled, never `Other`, so it keeps the
    /// name-only behavior; it never becomes `Owner` either.
    fn reached_without_a_call(&self, body: &str, test: &TestScope) -> bool {
        let reaches = |matches: Matches| self.target_of(&matches) != CallTarget::Other;
        if value_mentions(body, &self.name)
            .into_iter()
            .any(|qualifier| match qualifier {
                None => true,
                Some(segments) => reaches(self.resolve_spelled(&segments, test)),
            })
        {
            return true;
        }
        let Some(scopes) = test.scopes.as_deref() else {
            return false;
        };
        for scope in &scopes.scopes {
            if scope.block_rename_targets.contains(&self.name) {
                return true;
            }
            let base = test
                .file_segments
                .as_deref()
                .map(|file| join(file, &scope.modules));
            for import in &scope.imports {
                let UseBinding::Renamed(path) = &import.binding else {
                    continue;
                };
                let Some((original, module)) = path.split_last() else {
                    continue;
                };
                if *original != self.name || !mentions_word(body, &import.bound) {
                    continue;
                }
                let module = (!module.is_empty())
                    .then(|| use_target(module, base.as_deref(), scope, &test.crate_names))
                    .flatten();
                if reaches(self.matches_module(module)) {
                    return true;
                }
            }
        }
        false
    }

    fn test_scope(&self, test: &TestSummary, index: &RustIndex) -> TestScope {
        let key = (test.file.clone(), test.start_line);
        let cached = self
            .scopes
            .lock()
            .ok()
            .and_then(|scopes| scopes.get(&key).cloned());
        let scopes = match cached {
            Some(scopes) => scopes,
            None => {
                let scopes = index.files().get(&test.file).and_then(|facts| {
                    name_scopes_for_fn(&facts.data().source, test.start_line, &test.name)
                        .map(Arc::new)
                });
                if let Ok(mut memo) = self.scopes.lock() {
                    memo.insert(key, scopes.clone());
                }
                scopes
            }
        };
        // `crate::`, `self::`, `super::` and local modules name the owner's
        // modules only from the owner's own crate: same package, same crate
        // within it.
        let same_crate = self.owner_crate.is_some()
            && self.layout.class(&test.file) == self.owner_crate
            && package_key(&test.file) == self.owner_package;
        TestScope {
            file_segments: file_module_segments(&test.file).filter(|_| same_crate),
            scopes,
            crate_names: self.own_crate_names(test, index),
        }
    }

    /// The identifiers that name the owner's own crate from `test`'s crate.
    /// Empty for a test composed into the owner's library itself, which
    /// cannot name its own crate by name. From an integration test of the
    /// owner's package: the root manifest's package and library names when
    /// the owner is the root package. From any other crate: the import
    /// names the manifests give the owner's library (`MemberCrates`).
    fn own_crate_names(&self, test: &TestSummary, index: &RustIndex) -> BTreeSet<String> {
        let test_package = package_key(&test.file);
        let same_package = test_package == self.owner_package;
        let mut names = BTreeSet::new();
        if same_package && !under_tests_dir(&test.file) {
            return names;
        }
        // An integration test imports only the package's library, never a
        // binary's crate.
        if same_package
            && self.owner_package.is_none()
            && self.owner_crate.as_deref() == Some("lib")
        {
            names.extend(index.package_names.iter().cloned());
        }
        if let Some(owner_root) =
            super::owner_pin::target_root(&self.owner_file, index, &BTreeSet::new())
        {
            names.extend(index.member_crates.import_names(&test.file, &owner_root));
        }
        names
    }

    fn target_of(&self, matches: &Matches) -> CallTarget {
        let Matches::Some(set) = matches else {
            return CallTarget::Unsettled;
        };
        if set.is_empty() {
            return CallTarget::Unsettled;
        }
        // An unplaced definition matches every path, so it is in `set`
        // whenever a path could reach it.
        if set.contains(&0) {
            if set.len() == 1 {
                CallTarget::Owner
            } else {
                CallTarget::Unsettled
            }
        } else {
            CallTarget::Other
        }
    }

    fn resolve_spelled(&self, qualifier: &[String], test: &TestScope) -> Matches {
        let Some(scopes) = test.scopes.as_deref() else {
            return Matches::Any;
        };
        let file_segments = test.file_segments.as_deref();
        if qualifier.is_empty() {
            self.resolve_bare(file_segments, scopes, &test.crate_names)
        } else {
            self.resolve_qualified(qualifier, file_segments, scopes, &test.crate_names)
        }
    }

    /// The definitions a bare call binds to, scope by scope.
    fn resolve_bare(
        &self,
        file_segments: Option<&[String]>,
        scopes: &NameScopes,
        crate_names: &BTreeSet<String>,
    ) -> Matches {
        let name = self.name.as_str();
        let mut from_globs = Matches::none();
        for scope in &scopes.scopes {
            let base = file_segments.map(|file| join(file, &scope.modules));
            let base = base.as_deref();
            if scope.block_binds(name) {
                return Matches::Any;
            }
            let imports: Vec<&UseBinding> = scope.imports_of(name).collect();
            if !imports.is_empty() {
                let mut matches = Matches::none();
                for import in imports {
                    let found = match import {
                        UseBinding::Path(path) => match path.split_last() {
                            Some((_, module)) if !module.is_empty() => {
                                self.matches_module(use_target(module, base, scope, crate_names))
                            }
                            _ => Matches::Any,
                        },
                        UseBinding::Renamed(_) | UseBinding::Opaque => Matches::Any,
                    };
                    matches = matches.union(found);
                }
                return from_globs.union(matches);
            }
            if scope.defines.contains(name) {
                return from_globs.union(self.matches_module(base.map(<[String]>::to_vec)));
            }
            if scope.fn_body {
                // A glob in a function body may bind the name there,
                // shadowing every enclosing module.
                if !scope.globs.is_empty() {
                    return Matches::Any;
                }
                continue;
            }
            let mut follows_super = false;
            for glob in &scope.globs {
                match glob {
                    Some(glob) if glob.len() == 1 && glob[0] == "super" => follows_super = true,
                    Some(glob) => {
                        from_globs = from_globs.union(self.matches_module(use_target(
                            glob,
                            base,
                            scope,
                            crate_names,
                        )))
                    }
                    None => return Matches::Any,
                }
            }
            if !follows_super {
                return match from_globs {
                    Matches::Some(set) if set.is_empty() => Matches::Any,
                    other => other,
                };
            }
        }
        Matches::Any
    }

    /// The definitions `a::b::name(..)` names, with `segments` the spelled
    /// `a::b`. A leading `crate`, `self` or `super` is resolved from the
    /// test's module. Any other leading segment must be bound: by a `use`
    /// in a scope around the test (resolved through its path, a rename
    /// included), by a `mod` declared there, or, failing both, by being one
    /// of the identifiers that name the owner's own crate. A segment bound
    /// by nothing ripr reads (a dependency's crate name, a glob) leaves the
    /// call unsettled.
    fn resolve_qualified(
        &self,
        segments: &[String],
        file_segments: Option<&[String]>,
        scopes: &NameScopes,
        crate_names: &BTreeSet<String>,
    ) -> Matches {
        let Some((first, rest)) = segments.split_first() else {
            return Matches::Any;
        };
        if matches!(first.as_str(), "crate" | "self" | "super") {
            let base = scopes
                .scopes
                .first()
                .zip(file_segments)
                .map(|(scope, file)| join(file, &scope.modules));
            return self.matches_module(base.and_then(|base| keyword_module(segments, &base)));
        }
        if first == "Self" {
            return Matches::Any;
        }
        // Modules a glob of the owner's own crate can bring in under `first`
        // (`use mycrate::*;` makes `b::render(..)` the crate's `b`).
        let mut from_globs = Matches::none();
        for scope in &scopes.scopes {
            let base = file_segments.map(|file| join(file, &scope.modules));
            let base = base.as_deref();
            if scope.block_binds(first) {
                return Matches::Any;
            }
            let imports: Vec<&UseBinding> = scope.imports_of(first).collect();
            if !imports.is_empty() {
                let [binding] = imports.as_slice() else {
                    return Matches::Any;
                };
                // A nearer glob may bind `first` to a different module than
                // this outer import; which wins is not read, so fail closed.
                if !matches!(&from_globs, Matches::Some(set) if set.is_empty()) {
                    return Matches::Any;
                }
                let module = match binding {
                    UseBinding::Path(path) | UseBinding::Renamed(path) => {
                        use_target(path, base, scope, crate_names)
                    }
                    UseBinding::Opaque => None,
                };
                return self.matches_module(module.map(|module| join(&module, rest)));
            }
            if scope.child_modules.contains(first) {
                return self.matches_module(base.map(|base| join(base, segments)));
            }
            if scope.defines.contains(first) {
                return Matches::Any;
            }
            if scope.fn_body {
                if !scope.globs.is_empty() {
                    return Matches::Any;
                }
                continue;
            }
            let mut follows_super = false;
            for glob in &scope.globs {
                match glob {
                    Some(glob) if glob.len() == 1 && glob[0] == "super" => follows_super = true,
                    // A glob that names a module of the owner's crate can
                    // bring in only that module's child modules, so a placed
                    // `<glob>::<first>` is what `first` may name. Any other
                    // glob (a dependency, an unreadable prefix) may bring the
                    // segment in from somewhere ripr cannot see.
                    Some(glob) => match use_target(glob, base, scope, crate_names) {
                        Some(module) => {
                            let candidate = join(&module, segments);
                            if self.module_is_placed(&candidate) {
                                from_globs = from_globs.union(self.matches_module(Some(candidate)));
                            }
                        }
                        None => return Matches::Any,
                    },
                    None => return Matches::Any,
                }
            }
            if !follows_super {
                break;
            }
        }
        let by_crate_name = crate_names.contains(first);
        let named = if by_crate_name {
            self.matches_module(Some(rest.to_vec()))
        } else {
            Matches::none()
        };
        match from_globs.union(named) {
            // Nothing placed was named: the segment is bound by nothing
            // ripr reads.
            Matches::Some(set) if set.is_empty() => Matches::Any,
            found => found,
        }
    }

    /// Whether some placed definition sits directly in the crate-anchored
    /// `module`.
    fn module_is_placed(&self, module: &[String]) -> bool {
        self.definitions
            .iter()
            .flatten()
            .any(|path| path.as_slice() == module)
    }

    /// The definitions declared directly in the crate-anchored `module`;
    /// `Any` when the module could not be resolved. An unplaced definition
    /// matches every module, so a module that matches no placed definition
    /// (a re-export module, a path ripr misread) is `Any` too: an unplaced
    /// rival alone never settles a call away from the owner.
    fn matches_module(&self, module: Option<Vec<String>>) -> Matches {
        let Some(module) = module else {
            return Matches::Any;
        };
        if !self
            .definitions
            .iter()
            .flatten()
            .any(|path| *path == module)
        {
            return Matches::Any;
        }
        let set = self
            .definitions
            .iter()
            .enumerate()
            .filter(|(_, path)| path.as_ref().is_none_or(|path| *path == module))
            .map(|(index, _)| index)
            .collect();
        Matches::Some(set)
    }
}

/// The crate-anchored module a `use` path in `scope` names, whose module is
/// `base`. Its first segment must be `crate`, `self` or `super`, a `mod`
/// declared in that same scope, or one of the owner's own crate names;
/// anything else (a dependency, a name a glob brings in) is `None`.
fn use_target(
    path: &[String],
    base: Option<&[String]>,
    scope: &NameScope,
    crate_names: &BTreeSet<String>,
) -> Option<Vec<String>> {
    let first = path.first()?;
    match first.as_str() {
        "crate" | "self" | "super" => keyword_module(path, base?),
        "Self" => None,
        _ if scope.child_modules.contains(first) => Some(join(base?, path)),
        _ if crate_names.contains(first) && !crate_name_rebound(scope, first) => {
            Some(path[1..].to_vec())
        }
        _ => None,
    }
}

/// Whether `scope` binds the crate-name identifier `name` to something other
/// than the crate itself (`use other_dep as globs;`), so a path starting with
/// it no longer names the owner's crate.
fn crate_name_rebound(scope: &NameScope, name: &str) -> bool {
    scope.block_binds(name)
        || scope.defines.contains(name)
        || scope
            .imports_of(name)
            .any(|binding| !matches!(binding, UseBinding::Path(path) if path.len() == 1))
}

/// The crate-anchored module of a path that starts with `crate`, `self` or
/// `super`, spelled in module `base`.
fn keyword_module(path: &[String], base: &[String]) -> Option<Vec<String>> {
    match path.first()?.as_str() {
        "crate" => Some(path[1..].to_vec()),
        "self" => Some(join(base, &path[1..])),
        "super" => {
            let supers = path.iter().take_while(|s| *s == "super").count();
            let kept = base.len().checked_sub(supers)?;
            Some(join(&base[..kept], &path[supers..]))
        }
        _ => None,
    }
}

fn join(left: &[String], right: &[String]) -> Vec<String> {
    left.iter().chain(right).cloned().collect()
}

/// Whether any file of `package` carries a `#[path]` attribute (bare or
/// inside `cfg_attr`), which may remap a module file away from the layout
/// [`file_module_segments`] reads. A raw scan: a match inside a string or
/// comment only switches the rule off.
fn package_remaps_module_paths(index: &RustIndex, package: Option<&str>) -> bool {
    call_identity_shape(index, package).remaps_module_paths
}

pub(super) fn has_path_attribute(source: &str) -> bool {
    let bytes = source.as_bytes();
    let skip_space = |mut at: usize| {
        while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
            at += 1;
        }
        at
    };
    for (hash, _) in source.match_indices('#') {
        let mut at = skip_space(hash + 1);
        if bytes.get(at) == Some(&b'!') {
            at = skip_space(at + 1);
        }
        if bytes.get(at) != Some(&b'[') {
            continue;
        }
        let open = at;
        let mut depth = 0usize;
        let mut close = None;
        for (offset, byte) in bytes[open..].iter().enumerate() {
            match byte {
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let attribute = &source[open + 1..close.unwrap_or(source.len())];
        if attribute_sets_path(attribute) {
            return true;
        }
    }
    false
}

/// Rust identifiers continue with any XID_Continue char, which includes
/// combining marks (`mo\u{301}dulo`) and connector punctuation. Every
/// non-ASCII char counts as identifier-continuing, so a boundary is never
/// found inside a longer name (fails closed).
fn is_ident_char(ch: char) -> bool {
    !ch.is_ascii() || ch.is_ascii_alphanumeric() || ch == '_'
}

/// Whether the char ending just before byte `at` continues an identifier.
/// Char-aware, so `dulo` inside `módulo` is not read as a whole word.
fn ident_char_before(text: &str, at: usize) -> bool {
    text.get(..at)
        .and_then(|before| before.chars().next_back())
        .is_some_and(is_ident_char)
}

/// Whether the char starting at byte `at` continues an identifier.
fn ident_char_at(text: &str, at: usize) -> bool {
    text.get(at..)
        .and_then(|rest| rest.chars().next())
        .is_some_and(is_ident_char)
}

/// Whether the attribute text holds `path =` as a whole word.
fn attribute_sets_path(attribute: &str) -> bool {
    attribute.match_indices("path").any(|(start, _)| {
        let end = start + "path".len();
        if ident_char_before(attribute, start) {
            return false;
        }
        let tail = attribute[end..].trim_start();
        tail.starts_with('=') && !tail.starts_with("==")
    })
}

/// The module path of a definition: its file's module segments, then the
/// inline modules its parser id records. `None` when either is unknown or
/// the id is not a parser id of a module-level item.
fn definition_module_path(function: &FunctionSummary) -> Option<Vec<String>> {
    let mut path = file_module_segments(&function.file)?;
    let prefix = format!("{}::", crate::analysis::stable_path_text(&function.file));
    let rest = function.id.0.strip_prefix(&prefix)?;
    let mut segments: Vec<&str> = rest.split("::").collect();
    if segments.pop()? != function.name {
        return None;
    }
    if segments.iter().any(|segment| segment.starts_with("impl ")) {
        return None;
    }
    path.extend(segments.into_iter().map(str::to_string));
    Some(path)
}

/// The package a file belongs to. A relative path keeps
/// [`super::related_tests::package_prefix`] (`crates/<name>/`, the directory
/// above `src/` or `tests/`, or `None` for the root package). That leaves an
/// absolute path outside a `crates/` layout unscoped (#3235) and drops the
/// workspace directory from an absolute `crates/<name>/`, so two packages
/// under absolute roots could read as one (#7576). An absolute path is keyed
/// by the whole directory above its one `src/` or `tests/` component; a path
/// with several (an ancestor `/srv/src/`) cannot be split by its layout,
/// so it is keyed as its own package and never shares a crate with another
/// file.
fn package_key(path: &Path) -> Option<String> {
    if !super::related_tests::path_is_absolute_form(path) {
        return super::related_tests::package_prefix(path);
    }
    let normalized = super::related_tests::normalize_path(path);
    let markers: Vec<usize> = normalized
        .match_indices("/src/")
        .chain(normalized.match_indices("/tests/"))
        .map(|(at, _)| at)
        .collect();
    match markers.as_slice() {
        [at] => Some(format!("{}/", &normalized[..*at])),
        _ => Some(format!("{normalized}?")),
    }
}

/// The part of a normalized path below its last `src/` directory.
fn src_body(normalized: &str) -> Option<&str> {
    normalized
        .rfind("/src/")
        .map(|index| &normalized[index + "/src/".len()..])
        .or_else(|| normalized.strip_prefix("src/"))
}

/// Which crate of its package a file composes into.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CrateClass {
    /// Outside `src/` and `tests/`.
    Outside,
    /// A `src/` module of a package with both `src/lib.rs` and
    /// `src/main.rs` whose crate the roots' `mod` declarations do not
    /// settle.
    Ambiguous,
    Crate(String),
}

/// The first module segments one crate root declares; `None` when unknown.
type DeclaredModules = Option<BTreeSet<String>>;

/// How the owner's package splits its `src/` modules between the library
/// and the `src/main.rs` binary (#7576). With only `src/lib.rs`, every
/// `src/` module outside `src/bin/` is the library's; with only
/// `src/main.rs`, the binary's. With both, a module belongs to the root
/// whose top-level `mod` declarations name its first segment; a module both
/// or neither root names, or any module when a root's scan is incomplete
/// (an item-position macro, `include!`, a parse error), is
/// [`CrateClass::Ambiguous`].
#[derive(Debug, Default)]
struct CrateLayout {
    package: Option<String>,
    /// The first module segments `src/lib.rs` and `src/main.rs` declare,
    /// when the package has both; `None` for a root this scan cannot read
    /// completely.
    roots: Option<(DeclaredModules, DeclaredModules)>,
    /// The package has `src/main.rs` and no `src/lib.rs`: every `src/`
    /// module outside `src/bin/` that compiles is the binary's.
    main_only: bool,
}

impl CrateLayout {
    fn new(index: &RustIndex, package: Option<&str>) -> Self {
        // The whole-index answers are memoized per index (one scan serves
        // every construction and every probe of the run; #7716).
        let shape = call_identity_shape(index, package);
        CrateLayout {
            package: package.map(str::to_string),
            roots: shape.layout.roots.clone(),
            main_only: shape.layout.main_only,
        }
    }

    fn classify(&self, file: &Path) -> CrateClass {
        let Some(shape) = crate_class(file) else {
            return CrateClass::Outside;
        };
        if self.main_only && shape == "lib" && package_key(file) == self.package {
            return CrateClass::Crate("main".to_string());
        }
        let Some((lib, main)) = &self.roots else {
            return CrateClass::Crate(shape);
        };
        let normalized = super::related_tests::normalize_path(file);
        if shape != "lib"
            || src_body(&normalized) == Some("lib.rs")
            || package_key(file) != self.package
        {
            return CrateClass::Crate(shape);
        }
        let Some(first) = file_module_segments(file).and_then(|segments| segments.first().cloned())
        else {
            return CrateClass::Ambiguous;
        };
        let names =
            |root: &Option<BTreeSet<String>>| root.as_ref().map(|names| names.contains(&first));
        match (names(lib), names(main)) {
            (Some(true), Some(false)) => CrateClass::Crate("lib".to_string()),
            (Some(false), Some(true)) => CrateClass::Crate("main".to_string()),
            _ => CrateClass::Ambiguous,
        }
    }

    /// The file's crate; `None` outside the layout or when ambiguous, so it
    /// never equals a placed owner's crate.
    fn class(&self, file: &Path) -> Option<String> {
        match self.classify(file) {
            CrateClass::Crate(class) => Some(class),
            CrateClass::Outside | CrateClass::Ambiguous => None,
        }
    }
}

/// The first module segment of every module a crate root declares: `mod a;`
/// gives `a`, and so does `mod a { mod b; }`. `None` when the scan is
/// incomplete outside function bodies (an item-position macro, a parse
/// error) or the root `include!`s another file, which may declare more. A
/// body macro (`log::info!` in `fn main`) can only declare a module local
/// to its function, never a crate-level one, so it does not count.
fn declared_first_segments(source: &str) -> Option<BTreeSet<String>> {
    let scan = rust_module_tree_scan(source);
    if !scan.items_complete {
        return None;
    }
    let mut names = BTreeSet::new();
    for edge in scan.edges {
        match edge {
            RustModuleTreeEdge::Default { inline, name } => {
                names.insert(inline.into_iter().next().unwrap_or(name));
            }
            RustModuleTreeEdge::Path(_)
            | RustModuleTreeEdge::Include(_)
            | RustModuleTreeEdge::UnresolvedPath { .. } => return None,
        }
    }
    Some(names)
}

/// The crate a file composes into within its package, read from the
/// layout: `tests:<name>` for an integration test, `bin:<name>` for a
/// `src/bin` target, `main` for `src/main.rs`, and `lib` for every other
/// file under `src/`. `None` for a file outside both.
fn crate_class(file: &Path) -> Option<String> {
    let normalized = super::related_tests::normalize_path(file);
    if under_tests_dir(file) {
        let rest = normalized
            .rfind("/tests/")
            .map(|index| &normalized[index + "/tests/".len()..])
            .or_else(|| normalized.strip_prefix("tests/"))?;
        let first = rest.split('/').next()?;
        return Some(format!("tests:{}", first.trim_end_matches(".rs")));
    }
    let body = src_body(&normalized)?;
    if let Some(bin) = body.strip_prefix("bin/") {
        let first = bin.split('/').next()?;
        return Some(format!("bin:{}", first.trim_end_matches(".rs")));
    }
    Some(if body == "main.rs" { "main" } else { "lib" }.to_string())
}

/// The file sits under a `tests/` directory and no `src/` directory: an
/// integration-test crate.
fn under_tests_dir(file: &Path) -> bool {
    let normalized = super::related_tests::normalize_path(file);
    !(normalized.contains("/src/") || normalized.starts_with("src/"))
        && super::super::rust_index::is_test_file(file)
}

/// Module segments of a file below its crate root: `src/lib.rs` and
/// `src/main.rs` are the root, `src/a/mod.rs` and `src/a.rs` are `a`.
/// `None` for a file outside `src/` or `tests/`. A `#[path]` remap is not
/// read; [`OwnerCallIdentity::new`] switches the rule off for a package
/// that has one.
fn file_module_segments(file: &Path) -> Option<Vec<String>> {
    let normalized = super::related_tests::normalize_path(file);
    let (body, under_tests) = match normalized
        .rfind("/src/")
        .map(|index| &normalized[index + "/src/".len()..])
        .or_else(|| normalized.strip_prefix("src/"))
    {
        Some(body) => (body, false),
        None => (
            normalized
                .rfind("/tests/")
                .map(|index| &normalized[index + "/tests/".len()..])
                .or_else(|| normalized.strip_prefix("tests/"))?,
            true,
        ),
    };
    let body = body.strip_suffix(".rs")?;
    let mut segments: Vec<String> = body.split('/').map(str::to_string).collect();
    if !under_tests && segments.first().is_some_and(|first| first == "bin") {
        // `src/bin/x.rs` is its own crate root, and so is `src/bin/x/main.rs`;
        // `src/bin/x/y/mod.rs` is that crate's module `y`.
        segments.drain(..2.min(segments.len()));
        if segments.len() == 1 && segments[0] == "main" {
            segments.clear();
        }
        if segments.last().is_some_and(|last| last == "mod") {
            segments.pop();
        }
        return Some(segments);
    }
    if under_tests {
        // `tests/x.rs` is an integration-test crate root, `tests/x/y.rs`
        // its module `y`, and `tests/common/mod.rs` a module `common`.
        if segments.len() == 1 {
            return Some(Vec::new());
        }
        if segments.last().is_some_and(|last| last != "mod") {
            segments.remove(0);
        }
    }
    match segments.last().map(String::as_str) {
        Some("mod") => {
            segments.pop();
        }
        Some("lib" | "main") if segments.len() == 1 && !under_tests => {
            segments.pop();
        }
        _ => {}
    }
    Some(segments)
}

/// The spelled module path of every call of `name` in `body`: `Some([])`
/// for a bare call, `Some([a, b])` for `a::b::name(..)`, `None` for a
/// qualifier this scan cannot read (`<T as Tr>::name`, a generic segment).
/// Method calls (`x.name(..)`) and the `fn name` definition are skipped.
fn call_qualifiers(body: &str, name: &str) -> Vec<Option<Vec<String>>> {
    let masked = mask_comments_and_strings(body);
    let mut calls = Vec::new();
    for (start, _) in masked.match_indices(name) {
        let end = start + name.len();
        if ident_char_before(&masked, start) {
            continue;
        }
        if ident_char_at(&masked, end) {
            continue;
        }
        let tail = masked[end..].trim_start();
        if !(tail.starts_with('(') || tail.starts_with("::<")) {
            continue;
        }
        let head = masked[..start].trim_end();
        if head.ends_with('.') || head_word(head) == "fn" {
            continue;
        }
        calls.push(qualifier_before(head));
    }
    calls
}

/// The spelled module path of every mention of `name` in `body` that is no
/// call and could be the function used as a value (`.map(a::render)`, `let
/// f = render;`), in the same shape as [`call_qualifiers`]. Calls, method
/// and field accesses, macro invocations, path segments (`render::x`),
/// `name:` fields or ascriptions, definitions, local bindings and `use`
/// items are skipped; anything else counts, which only fails closed.
fn value_mentions(body: &str, name: &str) -> Vec<Option<Vec<String>>> {
    let masked = mask_comments_and_strings(body);
    let use_items = use_item_ranges(&masked);
    let mut mentions = Vec::new();
    for (start, _) in masked.match_indices(name) {
        let end = start + name.len();
        if ident_char_before(&masked, start) {
            continue;
        }
        if ident_char_at(&masked, end) {
            continue;
        }
        if use_items.iter().any(|range| range.contains(&start)) {
            continue;
        }
        let tail = masked[end..].trim_start();
        // `name!` is a macro, but `name != x` compares the name as a value.
        let is_macro = tail.starts_with('!') && !tail.starts_with("!=");
        if tail.starts_with('(') || tail.starts_with(':') || is_macro {
            continue;
        }
        let head = masked[..start].trim_end();
        if head.ends_with('.')
            || head.ends_with('\'')
            || matches!(
                head_word(head),
                "fn" | "let"
                    | "mut"
                    | "ref"
                    | "struct"
                    | "enum"
                    | "union"
                    | "mod"
                    | "const"
                    | "static"
                    | "type"
                    | "trait"
            )
        {
            continue;
        }
        mentions.push(qualifier_before(head));
    }
    mentions
}

/// Byte ranges of the `use` items in masked `text`, each from its `use`
/// keyword through its `;`.
fn use_item_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    text.match_indices("use")
        .filter(|(start, _)| !ident_char_before(text, *start) && !ident_char_at(text, start + 3))
        .map(|(start, _)| {
            let end = text[start..]
                .find(';')
                .map_or(text.len(), |offset| start + offset + 1);
            start..end
        })
        .collect()
}

/// Whether `word` occurs in `body`'s code (not a comment or string) as a
/// whole identifier.
fn mentions_word(body: &str, word: &str) -> bool {
    let masked = mask_comments_and_strings(body);
    masked.match_indices(word).any(|(start, _)| {
        !ident_char_before(&masked, start) && !ident_char_at(&masked, start + word.len())
    })
}

/// The identifier ending `head`. Identifiers may be non-ASCII
/// (`módulo`), so the start is a char boundary, never `index + 1`.
fn head_word(head: &str) -> &str {
    let start = head
        .char_indices()
        .rev()
        .find(|(_, ch)| !is_ident_char(*ch))
        .map_or(0, |(index, ch)| index + ch.len_utf8());
    head.get(start..).unwrap_or_default()
}

/// The `a::b::` path segments ending `head`, outermost first.
fn qualifier_before(head: &str) -> Option<Vec<String>> {
    // A raw identifier (`b::r#render`) is not read: `#` would otherwise
    // look like a boundary and turn the path into a bare call.
    if head.ends_with('#') {
        return None;
    }
    let mut segments = Vec::new();
    let mut rest = head;
    while let Some(before) = rest.strip_suffix("::") {
        let before = before.trim_end();
        if before.ends_with('>') {
            return None;
        }
        let word = head_word(before);
        if word.is_empty() {
            // A leading `::name` (2015 extern path) spells no module.
            return None;
        }
        if before
            .get(..before.len() - word.len())
            .is_some_and(|rest| rest.ends_with('#'))
        {
            return None;
        }
        segments.push(word.to_string());
        rest = before
            .get(..before.len() - word.len())
            .unwrap_or_default()
            .trim_end();
    }
    segments.reverse();
    Some(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::summarize_file;

    #[test]
    fn a_non_ascii_qualifier_is_read_whole_without_slicing_inside_a_char() {
        assert_eq!(head_word("assert_eq!(crate::módulo"), "módulo");
        assert_eq!(
            qualifier_before("assert_eq!(crate::módulo::"),
            Some(strings(&["crate", "módulo"]))
        );
    }

    #[test]
    fn a_name_inside_a_longer_non_ascii_identifier_is_not_a_call_of_it() {
        assert!(call_qualifiers("fn t() { assert_eq!(módulo(), 2); }", "dulo").is_empty());
        assert!(value_mentions("fn t() { let x = mó; }", "m").is_empty());
        assert!(!mentions_word("let x = módulo;", "dulo"));
        let index = index(&[(
            "src/lib.rs",
            "pub mod a {\n    pub fn dulo() -> u32 { 1 }\n    pub fn módulo() -> u32 { 2 }\n    \
             #[cfg(test)]\n    mod tests {\n        use super::*;\n        \
             #[test]\n        fn only_modulo() { assert_eq!(módulo(), 2); }\n        \
             #[test]\n        fn only_qualified_modulo() { assert_eq!(crate::a::módulo(), 2); }\n    }\n}\n\
             pub mod b {\n    pub fn dulo() -> u32 { 3 }\n}\n",
        )]);
        assert_ne!(
            settle(&index, "::a::dulo", "only_modulo"),
            Some(CallTarget::Owner)
        );
        assert_ne!(
            settle(&index, "::a::dulo", "only_qualified_modulo"),
            Some(CallTarget::Owner)
        );
    }

    #[test]
    fn decomposed_and_raw_identifiers_never_read_as_a_bare_call() {
        assert!(call_qualifiers("fn t() { assert_eq!(mo\u{301}dulo(), 2); }", "dulo").is_empty());
        assert!(call_qualifiers("fn t() { assert_eq!(x\u{203F}dulo(), 2); }", "dulo").is_empty());
        assert_eq!(
            call_qualifiers("fn t() { assert_eq!(crate::b::r#render(2), 4); }", "render"),
            vec![None]
        );
        assert_eq!(
            call_qualifiers("fn t() { assert_eq!(crate::r#b::render(2), 4); }", "render"),
            vec![None]
        );
    }

    fn strings(segments: &[&str]) -> Vec<String> {
        segments.iter().map(|s| s.to_string()).collect()
    }

    fn index(files: &[(&str, &str)]) -> RustIndex {
        let mut index = RustIndex::default();
        index.package_names.insert("modules".to_string());
        for (path, text) in files {
            let facts = summarize_file(PathBuf::from(path), (*text).to_string());
            index.extend_functions(facts.functions.iter().cloned());
            index.extend_tests(facts.tests.iter().cloned());
            index.insert_file_only(PathBuf::from(path), facts);
        }
        index
    }

    /// What the test named `test_name` settles on for the owner whose
    /// parser id ends with `owner_suffix`. `None` when the rule does not
    /// apply (no rival) or a fixture item is missing.
    fn settle(index: &RustIndex, owner_suffix: &str, test_name: &str) -> Option<CallTarget> {
        let owner = index
            .functions()
            .iter()
            .find(|function| function.id.0.ends_with(owner_suffix));
        assert!(owner.is_some(), "owner `{owner_suffix}` must be indexed");
        let owner = owner?;
        let test = index.tests().iter().find(|test| test.name == test_name);
        assert!(test.is_some(), "test `{test_name}` must be indexed");
        let identity = OwnerCallIdentity::new(owner, index.functions().iter(), index)?;
        Some(identity.resolve_test(test?, index))
    }

    const RENDER_LIB: &str = "\
pub mod a {
    pub fn render(x: i32) -> i32 {
        x + 1
    }
}

pub mod b {
    pub fn render(x: i32) -> i32 {
        x * 2
    }
}

pub mod c {
    pub use crate::a::render;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_two() {
        assert_eq!(b::render(2), 4);
    }

    #[test]
    fn crate_path_names_a() {
        assert_eq!(crate::a::render(1), 2);
    }

    #[test]
    fn reexport_path_is_unsettled() {
        assert_eq!(c::render(1), 2);
    }

    #[test]
    fn bare_call_bound_by_nothing() {
        assert_eq!(render(1), 2);
    }
}

#[cfg(test)]
mod imported {
    use crate::a::render;

    #[test]
    fn imported_from_a() {
        assert_eq!(render(1), 2);
    }
}

#[cfg(test)]
mod renamed {
    use crate::b::render as other;
    use crate::a::render as render;

    #[test]
    fn rename_to_the_same_name_keeps_the_path() {
        assert_eq!(render(1), 2);
        let _ = other(1);
    }
}

#[cfg(test)]
mod foreign {
    use super::*;

    #[test]
    fn foreign_namesake_path() {
        assert_eq!(other_dep::a::render(2), 3);
    }

    #[test]
    fn own_name_inside_the_library() {
        assert_eq!(modules::a::render(1), 2);
    }
}

#[cfg(test)]
mod aliased {
    use crate::b as a;
    use crate::a as picked;
    use crate::b;

    #[test]
    fn alias_to_rival() {
        assert_eq!(a::render(2), 4);
    }

    #[test]
    fn alias_to_owner() {
        assert_eq!(picked::render(1), 2);
    }

    #[test]
    fn imported_module_path() {
        assert_eq!(b::render(2), 4);
    }
}

#[cfg(test)]
mod blocks {
    use crate::b::render;

    #[test]
    fn block_import_does_not_reach_the_assert() {
        {
            use crate::a::render;
            let _ = render(1);
        }
        assert_eq!(render(2), 4);
    }

    #[test]
    fn top_level_import() {
        use crate::a::render;
        assert_eq!(render(1), 2);
    }

    #[test]
    fn mixed_owner_path_and_rival_bare() {
        let _ = crate::a::render(1);
        assert_eq!(render(2), 4);
    }

    #[test]
    fn block_alias_of_a_path_segment() {
        {
            use crate::b as a;
            let _ = a::render(2);
        }
        assert_eq!(crate::b::render(2), 4);
    }
}
";

    /// #6292: a qualified call spells the module, so `b::render(2)` names
    /// `b::render` and never the changed `a::render`.
    #[test]
    fn qualified_calls_settle_on_the_module_they_spell() {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        assert_eq!(
            settle(&index, "::a::render", "doubles_two"),
            Some(CallTarget::Other)
        );
        assert_eq!(
            settle(&index, "::b::render", "doubles_two"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::a::render", "crate_path_names_a"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::b::render", "crate_path_names_a"),
            Some(CallTarget::Other)
        );
        // A re-export path spells neither module: never a new grant, and
        // never a withdrawal either.
        assert_eq!(
            settle(&index, "::a::render", "reexport_path_is_unsettled"),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(&index, "::b::render", "reexport_path_is_unsettled"),
            Some(CallTarget::Unsettled)
        );
        // `use super::*` from a root that binds no `render` itself.
        assert_eq!(
            settle(&index, "::a::render", "bare_call_bound_by_nothing"),
            Some(CallTarget::Unsettled)
        );
    }

    /// The shared per-analysis identity is one instance per owner: the
    /// related-test relation, the owner-return pin and the dependent-scope
    /// check each ask for the same (owner, index) answer on every probe, so
    /// a fresh construction per ask re-runs the whole-index layout scan and
    /// re-parses every candidate test per probe (#7716, measured ~28 ms per
    /// probe pair on a 20-package corpus). A mutant that rebuilds instead
    /// of reusing fails the pointer identity below.
    #[test]
    fn shared_identity_is_one_instance_per_owner_per_index() -> Result<(), String> {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        let find_owner = |suffix: &str| {
            index
                .functions()
                .iter()
                .find(|function| function.id.0.ends_with(suffix))
                .cloned()
                .ok_or_else(|| format!("owner `{suffix}` must be indexed"))
        };
        let owner = find_owner("::a::render")?;
        let ok = || "two module-level rivals give an identity".to_string();
        let first = super::shared_owner_call_identity(&index, &owner).ok_or_else(ok)?;
        let second = super::shared_owner_call_identity(&index, &owner).ok_or_else(ok)?;
        assert!(
            std::sync::Arc::ptr_eq(&first, &second),
            "the second ask must return the memoized identity; rebuilding it \
             repeats the whole-index scan and every per-test parse per probe"
        );
        // Behavior parity: the shared instance settles a test exactly as a
        // fresh construction does — sharing never strengthens or weakens
        // the rule.
        let fresh = super::OwnerCallIdentity::new(&owner, index.functions().iter(), &index)
            .ok_or_else(|| "a fresh construction matches the shared one".to_string())?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "doubles_two")
            .ok_or_else(|| "the fixture test must be indexed".to_string())?;
        assert_eq!(
            first.resolve_test(test, &index),
            fresh.resolve_test(test, &index)
        );
        Ok(())
    }

    /// The whole-index shape answers (per-package `#[path]` remap flag and
    /// crate layout) are computed in one pass and then shared across
    /// packages and constructions. A mutant that rescans per construction
    /// leaves the memo unpopulated and fails the map assertion below.
    #[test]
    fn identity_shapes_are_one_scan_per_index_shared_across_packages() -> Result<(), String> {
        let index = index(&[
            ("src/lib.rs", RENDER_LIB),
            (
                "crates/other/src/lib.rs",
                "pub mod a {\n    pub fn render(x: i32) -> i32 { x + 1 }\n}\n\npub mod b {\n    pub fn render(x: i32) -> i32 { x * 2 }\n}\n",
            ),
        ]);
        let find_owner = |prefix: &str| {
            index
                .functions()
                .iter()
                .find(|function| function.id.0.starts_with(prefix) && function.name == "render")
                .cloned()
                .ok_or_else(|| format!("`{prefix}` render must be indexed"))
        };
        let root_owner = find_owner("src/lib.rs")?;
        let other_owner = find_owner("crates/other")?;
        let root_identity = super::shared_owner_call_identity(&index, &root_owner);
        let other_identity = super::shared_owner_call_identity(&index, &other_owner);
        let shapes = index.call_identity_memos.shapes.get().ok_or_else(|| {
            "the first identity construction must populate the shape memo".to_string()
        })?;
        assert_eq!(shapes.len(), 2, "one pass answers every package at once");
        let root_shape = shapes
            .get(&None)
            .ok_or_else(|| "the root package must be scanned".to_string())?;
        let other_shape = shapes
            .get(&Some("crates/other/".to_string()))
            .ok_or_else(|| "the other package must be scanned".to_string())?;
        assert!(!root_shape.remaps_module_paths);
        assert!(!other_shape.remaps_module_paths);
        // Two owners never share one identity instance.
        if let (Some(root), Some(other)) = (root_identity, other_identity) {
            assert!(!std::sync::Arc::ptr_eq(&root, &other));
        }
        Ok(())
    }

    /// Two same-named owners on one source line (inline sibling modules)
    /// are distinct identities: the shared memo keys on the owner's parser
    /// symbol id, never on (file, start_line), which both share (#7716
    /// review). A (file, line) key made the second owner reuse the first's
    /// identity, discarding the other owner's tests as rivals.
    #[test]
    fn shared_identity_distinguishes_same_line_same_name_owners() -> Result<(), String> {
        // `a::render` and `b::render` share one source line (and so one
        // (file, start_line)); `c::render` sits on its own line, which is
        // what lets `OwnerCallIdentity::new` build an identity for either
        // same-line owner at all. The shared memo keys on the owner's
        // symbol id: a (file, line) key handed `b::render` the identity
        // built for `a::render`, settling `b::render`'s tests as rivals
        // (#7716 review).
        let one_line = "mod a { pub fn render(x: i32) -> i32 { x + 1 } } mod b { pub fn render(x: i32) -> i32 { x * 2 } }

mod c { pub fn render(x: i32) -> i32 { x * 3 } }

#[cfg(test)]
mod tests { use super::b::render; #[test] fn doubles_two() { assert_eq!(render(2), 4); } }
";
        let index = index(&[("src/lib.rs", one_line)]);
        let find = |module: &str| {
            index
                .functions()
                .iter()
                .find(|function| function.id.0.ends_with(&format!("::{module}::render")))
                .cloned()
                .ok_or_else(|| format!("`{module}::render` must be indexed"))
        };
        let a = find("a")?;
        let b = find("b")?;
        let a_identity =
            super::shared_owner_call_identity(&index, &a).ok_or("identity for a::render")?;
        let b_identity =
            super::shared_owner_call_identity(&index, &b).ok_or("identity for b::render")?;
        assert!(
            !std::sync::Arc::ptr_eq(&a_identity, &b_identity),
            "same-line same-name owners are distinct definitions; sharing one              identity settles the other owner's tests as rivals"
        );
        // Behavior parity for the second owner against a fresh construction.
        let fresh_b = super::OwnerCallIdentity::new(&b, index.functions().iter(), &index)
            .ok_or("a fresh construction for b::render matches")?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "doubles_two")
            .ok_or("the fixture test must be indexed")?;
        assert_eq!(
            b_identity.resolve_test(test, &index),
            fresh_b.resolve_test(test, &index)
        );
        Ok(())
    }

    /// #6544: a `use` that imports one rival settles a bare call on it.
    #[test]
    fn a_use_import_settles_a_bare_call() {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        assert_eq!(
            settle(&index, "::a::render", "imported_from_a"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::b::render", "imported_from_a"),
            Some(CallTarget::Other)
        );
        assert_eq!(
            settle(
                &index,
                "::a::render",
                "rename_to_the_same_name_keeps_the_path"
            ),
            Some(CallTarget::Owner)
        );
    }

    const HEATERS: &str = "\
pub fn delay(warm: bool) -> u32 {
    if warm { 5 } else { 50 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_delay() {
        assert_eq!(delay(false), 50);
    }
}
";

    /// #6537: a bare call in a sibling file's `mod tests` with `use
    /// super::*` binds that file's own `delay`, not the changed one.
    #[test]
    fn a_bare_call_binds_its_own_files_definition_through_use_super_glob() {
        let coolers = HEATERS.replace("cold_delay", "warm_delay");
        let index = index(&[
            ("src/heaters.rs", HEATERS),
            ("src/coolers.rs", coolers.as_str()),
        ]);
        assert_eq!(
            settle(&index, "heaters.rs::delay", "cold_delay"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "heaters.rs::delay", "warm_delay"),
            Some(CallTarget::Other)
        );
        assert_eq!(
            settle(&index, "coolers.rs::delay", "warm_delay"),
            Some(CallTarget::Owner)
        );
    }

    /// An integration test reaches the library through the crate's name,
    /// and an explicit import there settles on the module it names.
    #[test]
    fn integration_test_paths_through_the_crate_name_settle() {
        let tests = "\
use modules::a::render;

#[test]
fn from_outside() {
    assert_eq!(render(1), 2);
    assert_eq!(modules::b::render(2), 4);
}

#[test]
fn only_b_from_outside() {
    assert_eq!(modules::b::render(2), 4);
}
";
        let index = index(&[("src/lib.rs", RENDER_LIB), ("tests/api.rs", tests)]);
        assert_eq!(
            settle(&index, "::a::render", "from_outside"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::a::render", "only_b_from_outside"),
            Some(CallTarget::Other)
        );
    }

    /// No rival, no rule: the identity applies only among same-named
    /// module-level functions of one package.
    #[test]
    fn a_unique_or_method_owner_has_no_identity() {
        let index = index(&[(
            "src/lib.rs",
            "pub fn solo() -> u32 { 1 }\npub struct S;\nimpl S { pub fn render(&self) -> u32 { 2 } }\npub fn render() -> u32 { 3 }\n",
        )]);
        let solo = index.functions().iter().find(|f| f.name == "solo");
        assert!(solo.is_some());
        assert!(
            solo.and_then(|owner| OwnerCallIdentity::new(owner, index.functions().iter(), &index))
                .is_none()
        );
        let free_render = index
            .functions()
            .iter()
            .find(|f| f.name == "render" && f.item.container == FunctionContainer::Free);
        assert!(free_render.is_some());
        assert!(
            free_render
                .and_then(|owner| OwnerCallIdentity::new(owner, index.functions().iter(), &index))
                .is_none(),
            "an inherent method is no module-level rival"
        );
    }

    /// Review finding 1: a leading segment is stripped only when it is
    /// `crate`/`self`/`super` or a name of the owner's own crate. A
    /// dependency's `other_dep::a::render` is not the local `a::render`,
    /// and the library cannot name itself by its crate name.
    #[test]
    fn a_foreign_crate_namesake_path_is_unsettled() {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        assert_eq!(
            settle(&index, "::a::render", "foreign_namesake_path"),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(&index, "::b::render", "foreign_namesake_path"),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(&index, "::a::render", "own_name_inside_the_library"),
            Some(CallTarget::Unsettled)
        );
    }

    /// Review finding 1, positive and foreign control from an integration
    /// test: the own crate's name anchors the path; another crate's does not.
    #[test]
    fn an_own_crate_name_path_settles_and_a_foreign_one_does_not() {
        let tests = "\
#[test]
fn own_crate() {
    assert_eq!(modules::a::render(1), 2);
}

#[test]
fn other_crate() {
    assert_eq!(other_dep::a::render(1), 2);
}
";
        let index = index(&[("src/lib.rs", RENDER_LIB), ("tests/api.rs", tests)]);
        assert_eq!(
            settle(&index, "::a::render", "own_crate"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::b::render", "own_crate"),
            Some(CallTarget::Other)
        );
        assert_eq!(
            settle(&index, "::a::render", "other_crate"),
            Some(CallTarget::Unsettled)
        );
    }

    /// A `use <own crate>::*;` at the top of an integration test is the
    /// usual spelling. It must not hide which module a qualified call names:
    /// the glob brings in only the crate's own modules, and the crate's name
    /// still anchors a fully spelled path beside it. A glob of a dependency,
    /// or an unrelated module, keeps the call unsettled.
    #[test]
    fn an_own_crate_glob_does_not_hide_a_qualified_call() {
        let tests = "\
use modules::*;

#[test]
fn glob_module_path() {
    assert_eq!(b::render(1), 2);
}

#[test]
fn glob_crate_path() {
    assert_eq!(modules::b::render(1), 2);
}

#[test]
fn glob_owner_module_path() {
    assert_eq!(a::render(1), 2);
}

#[test]
fn glob_unknown_first_segment() {
    assert_eq!(other_dep::a::render(1), 2);
}
";
        let own = index(&[("src/lib.rs", RENDER_LIB), ("tests/api.rs", tests)]);
        for test in ["glob_module_path", "glob_crate_path"] {
            assert_eq!(settle(&own, "::a::render", test), Some(CallTarget::Other));
            assert_eq!(settle(&own, "::b::render", test), Some(CallTarget::Owner));
        }
        assert_eq!(
            settle(&own, "::a::render", "glob_owner_module_path"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&own, "::a::render", "glob_unknown_first_segment"),
            Some(CallTarget::Unsettled)
        );
        let dependency_glob = "\
use other_dep::*;

#[test]
fn foreign_glob() {
    assert_eq!(b::render(1), 2);
}
";
        let foreign = index(&[
            ("src/lib.rs", RENDER_LIB),
            ("tests/api.rs", dependency_glob),
        ]);
        assert_eq!(
            settle(&foreign, "::a::render", "foreign_glob"),
            Some(CallTarget::Unsettled),
            "`b` may come from the dependency's glob"
        );
    }

    /// Review finding 2: the first segment of a qualified call is resolved
    /// through a `use .. as X` or `use path::X` that binds it.
    #[test]
    fn a_qualified_call_resolves_through_a_module_alias() {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        assert_eq!(
            settle(&index, "::a::render", "alias_to_rival"),
            Some(CallTarget::Other),
            "`use crate::b as a;` makes `a::render` b's"
        );
        assert_eq!(
            settle(&index, "::b::render", "alias_to_rival"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::a::render", "alias_to_owner"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::b::render", "alias_to_owner"),
            Some(CallTarget::Other)
        );
        assert_eq!(
            settle(&index, "::b::render", "imported_module_path"),
            Some(CallTarget::Owner)
        );
    }

    /// Review finding 3: a `use` in a nested block binds only in that block,
    /// so it settles no bare or qualified call of the name in the test; the
    /// same import at the body's top level does (control).
    #[test]
    fn a_block_local_binding_leaves_the_test_unsettled() {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        assert_eq!(
            settle(
                &index,
                "::a::render",
                "block_import_does_not_reach_the_assert"
            ),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(
                &index,
                "::b::render",
                "block_import_does_not_reach_the_assert"
            ),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(&index, "::a::render", "top_level_import"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::b::render", "top_level_import"),
            Some(CallTarget::Other)
        );
        // `crate::b::render` settles on b; the block's `a::render` alias
        // leaves its own call unsettled, never the owner's.
        assert_eq!(
            settle(&index, "::a::render", "block_alias_of_a_path_segment"),
            Some(CallTarget::Unsettled)
        );
    }

    /// Review finding 4 support: one asserted call is resolved on its own
    /// spelling, not by what the test's other calls settle on.
    #[test]
    fn resolve_call_settles_one_spelled_call() {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        let owner = index
            .functions()
            .iter()
            .find(|function| function.id.0.ends_with("::a::render"));
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "block_alias_of_a_path_segment");
        assert!(owner.is_some() && test.is_some());
        let (Some(owner), Some(test)) = (owner, test) else {
            return;
        };
        let identity = OwnerCallIdentity::new(owner, index.functions().iter(), &index);
        assert!(identity.is_some());
        let Some(identity) = identity else { return };
        assert_eq!(
            identity.resolve_call(test, &index, &strings(&["crate", "b"])),
            CallTarget::Other
        );
        assert_eq!(
            identity.resolve_call(test, &index, &strings(&["crate", "a"])),
            CallTarget::Owner
        );
        assert_eq!(
            identity.resolve_call(test, &index, &strings(&["a"])),
            CallTarget::Unsettled,
            "the block-local alias of `a` leaves `a::render` unsettled"
        );
    }

    /// Review finding 4: with `use crate::b::render;`, a test that also
    /// calls `crate::a::render(1)` settles on the owner as a whole (coarse
    /// related-test selection), but its asserted bare `render(2)` is b's.
    #[test]
    fn a_mixed_test_settles_coarsely_on_the_owner_but_its_bare_call_on_the_rival() {
        let index = index(&[("src/lib.rs", RENDER_LIB)]);
        let owner = index
            .functions()
            .iter()
            .find(|function| function.id.0.ends_with("::a::render"));
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "mixed_owner_path_and_rival_bare");
        assert!(owner.is_some() && test.is_some());
        let (Some(owner), Some(test)) = (owner, test) else {
            return;
        };
        let identity = OwnerCallIdentity::new(owner, index.functions().iter(), &index);
        assert!(identity.is_some());
        let Some(identity) = identity else { return };
        assert_eq!(identity.resolve_test(test, &index), CallTarget::Owner);
        assert_eq!(identity.resolve_call(test, &index, &[]), CallTarget::Other);
        assert_eq!(
            identity.resolve_call(test, &index, &strings(&["crate", "a"])),
            CallTarget::Owner
        );
    }

    /// A `#[path]` remap anywhere in the package may move a module file, so
    /// the physical layout is no module path and the rule switches off.
    #[test]
    fn a_path_remapped_package_has_no_identity() {
        let remapped = format!("{RENDER_LIB}\n#[path = \"elsewhere.rs\"]\nmod moved;\n");
        let index_remapped = index(&[("src/lib.rs", remapped.as_str())]);
        assert_eq!(settle(&index_remapped, "::a::render", "doubles_two"), None);
        let index_plain = index(&[("src/lib.rs", RENDER_LIB)]);
        assert_eq!(
            settle(&index_plain, "::a::render", "doubles_two"),
            Some(CallTarget::Other)
        );
        assert!(has_path_attribute("#[path = \"x.rs\"]\nmod x;"));
        assert!(has_path_attribute(
            "#[cfg_attr(test, path = \"y.rs\")]\nmod y;"
        ));
        assert!(!has_path_attribute("fn f() { let path = 1; }"));
        assert!(!has_path_attribute("#[doc = \"x\"]\n#[cfg(test)]\nmod z;"));
    }

    const SPLIT_A: &str = "pub fn render(x: i32) -> i32 {\n    x + 1\n}\n";

    const SPLIT_B: &str = "\
pub fn render(x: i32) -> i32 {
    x * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_owner_as_value() {
        let v: Vec<i32> = [1, 2].iter().copied().map(crate::a::render).collect();
        assert_eq!(render(3), 6);
        assert_eq!(v, [2, 3]);
    }

    #[test]
    fn renamed_owner_import() {
        use crate::a::render as fast;
        assert_eq!(fast(2), render(2) - 1);
    }

    #[test]
    fn block_renamed_owner_import() {
        {
            use crate::a::render as fast;
            let _ = fast(1);
        }
        assert_eq!(render(2), 4);
    }

    #[test]
    fn bare_value_mention() {
        let f: fn(i32) -> i32 = render;
        assert_eq!(f(2), render(2));
    }

    #[test]
    fn rival_only_value_and_rename() {
        use crate::b::render as twice;
        let v: Vec<i32> = [1].iter().copied().map(crate::b::render).collect();
        assert_eq!(twice(2), render(2));
        assert_eq!(v, [2]);
    }
}

#[cfg(test)]
mod renamed_in_module {
    use super::*;
    use crate::a::render as fast;

    #[test]
    fn module_renamed_owner_import() {
        assert_eq!(fast(2), render(2) - 1);
    }
}
";

    /// Review gap 1: a test that reaches the owner without spelling a call
    /// of its name (a function value, a renaming import at the body, block
    /// or module level) is unsettled, not settled on the rival; it is never
    /// credited to the owner either. A test that only reaches the rival by
    /// value and rename stays `Other` (control).
    #[test]
    fn reaching_the_owner_without_a_spelled_call_is_unsettled() {
        let index = index(&[("src/a.rs", SPLIT_A), ("src/b.rs", SPLIT_B)]);
        for test in [
            "passes_owner_as_value",
            "renamed_owner_import",
            "block_renamed_owner_import",
            "module_renamed_owner_import",
        ] {
            assert_eq!(
                settle(&index, "a.rs::render", test),
                Some(CallTarget::Unsettled),
                "{test} may reach a::render without a spelled call"
            );
        }
        assert_eq!(
            settle(&index, "b.rs::render", "passes_owner_as_value"),
            Some(CallTarget::Owner),
            "the bare `render(3)` is b's own"
        );
        assert_eq!(
            settle(&index, "a.rs::render", "rival_only_value_and_rename"),
            Some(CallTarget::Other)
        );
        assert_eq!(
            settle(&index, "a.rs::render", "bare_value_mention"),
            Some(CallTarget::Other),
            "a bare value `render` is resolved like a bare call: b's own"
        );
        assert_eq!(
            settle(&index, "b.rs::render", "rival_only_value_and_rename"),
            Some(CallTarget::Owner)
        );
    }

    const NESTED_FN_LIB: &str = "\
pub mod a {
    pub fn render(x: i32) -> i32 {
        x + 1
    }
}

pub mod b {
    pub fn render(x: i32) -> i32 {
        x * 2
    }
}

#[cfg(test)]
mod tests {
    use crate::b::render;

    #[test]
    fn helper_imports_a() {
        fn via_a(x: i32) -> i32 {
            use crate::a::render;
            render(x)
        }
        assert_eq!(via_a(1), 2);
    }

    #[test]
    fn plain_b() {
        assert_eq!(render(2), 4);
    }
}
";

    /// Review gap 2: a nested fn's own `use` binds only in that fn while
    /// its calls are in the test's text, so the bare `render(x)` is
    /// unsettled for both definitions, never settled by the module's
    /// `use crate::b::render`. Without the nested fn, the module import
    /// settles the call (control).
    #[test]
    fn a_nested_fn_import_leaves_the_test_unsettled() {
        let index = index(&[("src/lib.rs", NESTED_FN_LIB)]);
        assert_eq!(
            settle(&index, "::a::render", "helper_imports_a"),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(&index, "::b::render", "helper_imports_a"),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(&index, "::a::render", "plain_b"),
            Some(CallTarget::Other)
        );
        assert_eq!(
            settle(&index, "::b::render", "plain_b"),
            Some(CallTarget::Owner)
        );
    }

    /// Review gap 3: `crate::a::render` in another package's library names
    /// that package's `a`, not the owner's: unsettled both ways. The same
    /// spelling inside the owner's package settles (control).
    #[test]
    fn crate_paths_in_another_package_are_unsettled() {
        let other = "\
#[cfg(test)]
mod tests {
    #[test]
    fn other_package_a() {
        assert_eq!(crate::a::render(1), 2);
    }

    #[test]
    fn other_package_b() {
        assert_eq!(crate::b::render(2), 4);
    }
}
";
        let index = index(&[
            ("crates/x/src/lib.rs", RENDER_LIB),
            ("crates/y/src/lib.rs", other),
        ]);
        assert_eq!(
            settle(&index, "::a::render", "other_package_a"),
            Some(CallTarget::Unsettled),
            "never owner credit from another package's `crate::a`"
        );
        assert_eq!(
            settle(&index, "::a::render", "other_package_b"),
            Some(CallTarget::Unsettled),
            "never dropped on another package's `crate::b`"
        );
        assert_eq!(
            settle(&index, "::a::render", "crate_path_names_a"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            settle(&index, "::a::render", "doubles_two"),
            Some(CallTarget::Other)
        );
    }

    /// Review gap 4: an unplaced rival matches every module, so a module
    /// that matches no placed definition is `Any` (unsettled), not a set
    /// holding only the unplaced rival (`Other`).
    #[test]
    fn a_module_matching_no_placed_definition_is_unsettled() {
        let identity = OwnerCallIdentity {
            name: "render".to_string(),
            owner_crate: Some("lib".to_string()),
            layout: CrateLayout::default(),
            owner_file: PathBuf::from("src/lib.rs"),
            owner_package: None,
            definitions: vec![Some(strings(&["a"])), Some(strings(&["b"])), None],
            memo: Mutex::new(BTreeMap::new()),
            scopes: Mutex::new(BTreeMap::new()),
        };
        let target =
            |module: &[&str]| identity.target_of(&identity.matches_module(Some(strings(module))));
        assert_eq!(target(&["c"]), CallTarget::Unsettled);
        assert_eq!(
            target(&["a"]),
            CallTarget::Unsettled,
            "the unplaced rival competes"
        );
        assert_eq!(target(&["b"]), CallTarget::Other);
        assert!(matches!(
            identity.matches_module(Some(strings(&["c"]))),
            Matches::Any
        ));
    }

    #[test]
    fn value_mentions_skip_calls_uses_fields_and_definitions() {
        let body = "fn t() {\n    use crate::a::render;\n    render(1);\n    \
                    let f = crate::b::render;\n    x.render;\n    render!(2);\n    \
                    S { render: 3 };\n    render::helper();\n    let render = 4;\n    \
                    [render];\n    // c::render\n}";
        assert_eq!(
            value_mentions(body, "render"),
            vec![Some(strings(&["crate", "b"])), Some(Vec::new())]
        );
        assert_eq!(
            value_mentions("fn t() { assert!(f != a::render); }", "render"),
            vec![Some(strings(&["a"]))]
        );
        assert_eq!(
            value_mentions("fn t() { assert!(a::render != f); }", "render"),
            vec![Some(strings(&["a"]))]
        );
        assert!(mentions_word("let x = fast(1);", "fast"));
        assert!(!mentions_word("let x = faster(1); // fast", "fast"));
    }

    #[test]
    fn call_qualifiers_read_bare_qualified_method_and_definition_spellings() {
        let body = "fn t() {\n    assert_eq!(b::render(2), 4);\n    render(1);\n    \
                    crate::a :: render(3);\n    x.render(4);\n    \
                    <T as R>::render(5);\n    // c::render(6)\n    let r = rendering(7);\n}";
        assert_eq!(
            call_qualifiers(body, "render"),
            vec![
                Some(strings(&["b"])),
                Some(Vec::new()),
                Some(strings(&["crate", "a"])),
                None,
            ]
        );
        assert!(call_qualifiers("fn render() { }", "render").is_empty());
    }

    #[test]
    fn file_module_segments_follow_the_crate_layout() {
        assert_eq!(file_module_segments(Path::new("src/lib.rs")), Some(vec![]));
        assert_eq!(
            file_module_segments(Path::new("crates/x/src/a/mod.rs")),
            Some(strings(&["a"]))
        );
        assert_eq!(
            file_module_segments(Path::new("src/a/b.rs")),
            Some(strings(&["a", "b"]))
        );
        assert_eq!(
            file_module_segments(Path::new("src/bin/tool.rs")),
            Some(vec![])
        );
        assert_eq!(
            file_module_segments(Path::new("tests/api.rs")),
            Some(vec![])
        );
        assert_eq!(
            file_module_segments(Path::new("tests/common/mod.rs")),
            Some(strings(&["common"]))
        );
        assert_eq!(
            file_module_segments(Path::new("src/bin/tool/main.rs")),
            Some(vec![])
        );
        assert_eq!(
            file_module_segments(Path::new("src/bin/tool/fmt.rs")),
            Some(strings(&["fmt"]))
        );
        assert_eq!(
            file_module_segments(Path::new("src/bin/tool/fmt/mod.rs")),
            Some(strings(&["fmt"]))
        );
        assert_eq!(
            file_module_segments(Path::new("src/bin/tool/fmt/main.rs")),
            Some(strings(&["fmt", "main"]))
        );
        assert_eq!(file_module_segments(Path::new("build.rs")), None);
    }

    const BIN_LIB: &str = "pub mod util;\n\npub fn render(x: i32) -> i32 {\n    x + 1\n}\n";
    const BIN_UTIL: &str = "pub fn render(x: i32) -> i32 {\n    x + 2\n}\n";
    const BIN_MAIN: &str =
        "mod shell;\n\nfn render(x: i32) -> i32 {\n    x - 1\n}\n\nfn main() {}\n";
    const BIN_SHELL: &str = "\
pub fn render(x: i32) -> i32 {
    x * 2
}

#[cfg(test)]
mod tests {
    #[test]
    fn binary_root_render() {
        assert_eq!(crate::render(2), 1);
    }

    #[test]
    fn binary_shell_render() {
        assert_eq!(crate::shell::render(2), 4);
    }
}
";

    /// #7576 part 1: `src/shell.rs` is declared by `src/main.rs`, so it is the
    /// binary's module. Its `crate::render` names `src/main.rs`'s `render`,
    /// never the library's, and its `render` is a rival of the binary root's
    /// `render` only.
    #[test]
    fn binary_owned_modules_compose_into_the_binary() {
        let index = index(&[
            ("src/lib.rs", BIN_LIB),
            ("src/util.rs", BIN_UTIL),
            ("src/main.rs", BIN_MAIN),
            ("src/shell.rs", BIN_SHELL),
        ]);
        let lib_owner = index
            .functions()
            .iter()
            .find(|function| function.file == Path::new("src/lib.rs"));
        assert!(lib_owner.is_some(), "the library's render must be indexed");
        let Some(lib_owner) = lib_owner else {
            return;
        };
        let identity = OwnerCallIdentity::new(lib_owner, index.functions().iter(), &index);
        assert!(identity.is_some(), "the library's render has util's rival");
        let Some(identity) = identity else {
            return;
        };
        let test = |name: &str| index.tests().iter().find(|test| test.name == name);
        assert!(test("binary_root_render").is_some());
        let Some(root_test) = test("binary_root_render") else {
            return;
        };
        assert_eq!(
            identity.resolve_test(root_test, &index),
            CallTarget::Unsettled,
            "the binary's `crate::render` is never the library's render"
        );
        assert_eq!(identity.definitions.len(), 2, "shell's render is no rival");

        let main_owner = index.functions().iter().find(|function| {
            function.file == Path::new("src/main.rs") && function.name == "render"
        });
        assert!(
            main_owner.is_some(),
            "the binary root's render must be indexed"
        );
        let Some(main_owner) = main_owner else {
            return;
        };
        let identity = OwnerCallIdentity::new(main_owner, index.functions().iter(), &index);
        assert!(
            identity.is_some(),
            "the binary root's render has shell's rival"
        );
        let Some(identity) = identity else {
            return;
        };
        assert_eq!(identity.resolve_test(root_test, &index), CallTarget::Owner);
        assert!(test("binary_shell_render").is_some());
        let Some(cli_test) = test("binary_shell_render") else {
            return;
        };
        assert_eq!(identity.resolve_test(cli_test, &index), CallTarget::Other);
    }

    /// A module both roots declare, or one whose root `include!`s more, has
    /// no settled crate: the rule switches off for its definitions.
    #[test]
    fn a_module_both_roots_declare_switches_the_rule_off() {
        let shared_lib = format!("pub mod shell;\n{BIN_LIB}");
        let shared = index(&[
            ("src/lib.rs", shared_lib.as_str()),
            ("src/util.rs", BIN_UTIL),
            ("src/main.rs", BIN_MAIN),
            ("src/shell.rs", BIN_SHELL),
        ]);
        // The library's render has util's rival; shell's render may be either
        // crate's, so the rule switches off.
        assert!(identity_for(&shared, "src/lib.rs").is_none());
        // Neither root declares `shell`: an orphan has no settled crate either.
        let orphan = index(&[
            ("src/lib.rs", BIN_LIB),
            ("src/util.rs", BIN_UTIL),
            ("src/main.rs", "fn main() {}\n"),
            ("src/shell.rs", BIN_SHELL),
        ]);
        assert!(identity_for(&orphan, "src/lib.rs").is_none());
        // An item-position macro in a root may declare any module.
        let macro_main =
            format!("cfg_if::cfg_if! {{ if #[cfg(unix)] {{ mod shell; }} }}\n{BIN_MAIN}");
        let macro_root = index(&[
            ("src/lib.rs", BIN_LIB),
            ("src/util.rs", BIN_UTIL),
            ("src/main.rs", macro_main.as_str()),
            ("src/shell.rs", BIN_SHELL),
        ]);
        assert!(identity_for(&macro_root, "src/lib.rs").is_none());
        assert_eq!(
            declared_first_segments("mod a;\nmod b { mod c; }\n"),
            Some(["a", "b"].iter().map(|name| name.to_string()).collect())
        );
        assert_eq!(
            declared_first_segments("include!(\"mods.rs\");\nmod a;\n"),
            None
        );
    }

    /// The first `render` in `file`'s identity, `None` when the rule is off.
    fn identity_for(index: &RustIndex, file: &str) -> Option<OwnerCallIdentity> {
        let owner = index
            .functions()
            .iter()
            .find(|function| function.file == Path::new(file) && function.name == "render");
        assert!(owner.is_some(), "a render in `{file}` must be indexed");
        OwnerCallIdentity::new(owner?, index.functions().iter(), index)
    }

    /// A body macro in `fn main` (`log::info!`) cannot declare a crate-level
    /// module, so the roots still settle which crate `shell` is in; an
    /// integration test never names the binary's crate, so its
    /// `modules::render` stays unsettled for the binary root's render.
    #[test]
    fn a_body_macro_in_main_keeps_the_split_and_tests_never_name_the_binary() {
        let logged_main = "mod shell;\n\nfn render(x: i32) -> i32 {\n    x - 1\n}\n\nfn main() {\n    log::info!(\"start\");\n}\n";
        let api = "#[test]\nfn api_render() {\n    assert_eq!(modules::render(2), 1);\n}\n";
        let index = index(&[
            ("src/lib.rs", BIN_LIB),
            ("src/util.rs", BIN_UTIL),
            ("src/main.rs", logged_main),
            ("src/shell.rs", BIN_SHELL),
            ("tests/api.rs", api),
        ]);
        let identity = identity_for(&index, "src/main.rs");
        assert!(
            identity.is_some(),
            "shell's render is the binary root's rival"
        );
        let Some(identity) = identity else {
            return;
        };
        let test = index.tests().iter().find(|test| test.name == "api_render");
        assert!(test.is_some());
        let Some(test) = test else {
            return;
        };
        assert_eq!(identity.resolve_test(test, &index), CallTarget::Unsettled);
        let lib = identity_for(&index, "src/lib.rs");
        assert!(lib.is_some(), "the library's render has util's rival");
        if let Some(lib) = lib {
            assert_eq!(lib.resolve_test(test, &index), CallTarget::Owner);
        }
    }

    /// A package with `src/main.rs` and no `src/lib.rs` composes its `src/`
    /// modules into the binary: the root's `render` and `shell`'s are
    /// rivals, so `crate::render` from `shell`'s tests settles on the root.
    #[test]
    fn a_binary_only_package_composes_its_modules_into_the_binary() {
        let index = index(&[("src/main.rs", BIN_MAIN), ("src/shell.rs", BIN_SHELL)]);
        let identity = identity_for(&index, "src/main.rs");
        assert!(identity.is_some(), "shell's render is the root's rival");
        let Some(identity) = identity else {
            return;
        };
        let test = |name: &str| index.tests().iter().find(|test| test.name == name);
        assert!(test("binary_root_render").is_some());
        assert!(test("binary_shell_render").is_some());
        let (Some(root_test), Some(shell_test)) =
            (test("binary_root_render"), test("binary_shell_render"))
        else {
            return;
        };
        assert_eq!(identity.resolve_test(root_test, &index), CallTarget::Owner);
        assert_eq!(identity.resolve_test(shell_test, &index), CallTarget::Other);
    }

    /// A `src/bin` owner never shares a crate with a library-shaped module,
    /// so an orphan `src/` namesake leaves its identity on.
    #[test]
    fn an_ambiguous_module_leaves_a_bin_owner_alone() {
        let index = index(&[
            ("src/lib.rs", BIN_LIB),
            ("src/main.rs", "fn main() {}\n"),
            ("src/old.rs", BIN_UTIL),
            (
                "src/bin/tool.rs",
                "mod fmt;\n\nfn render(x: i32) -> i32 {\n    x - 1\n}\n\nfn main() {}\n",
            ),
            ("src/bin/tool/fmt.rs", BIN_UTIL),
        ]);
        assert!(identity_for(&index, "src/bin/tool.rs").is_some());
        assert!(identity_for(&index, "src/lib.rs").is_none());
    }

    /// A `src/bin/<name>/main.rs` root is the crate root: its `crate::render`
    /// names its own `render`.
    #[test]
    fn a_bin_directory_main_is_its_crate_root() {
        let main = "\
mod fmt;

fn render(x: i32) -> i32 {
    x - 1
}

fn main() {}

#[cfg(test)]
mod tests {
    #[test]
    fn tool_root_render() {
        assert_eq!(crate::render(2), 1);
    }
}
";
        let index = index(&[
            ("src/bin/tool/main.rs", main),
            ("src/bin/tool/fmt.rs", BIN_UTIL),
        ]);
        assert_eq!(
            settle(&index, "src/bin/tool/main.rs::render", "tool_root_render"),
            Some(CallTarget::Owner)
        );
    }

    /// #7576 part 2: two packages under an absolute root without a
    /// `crates/` directory stay apart, so `crate::a::render` in `b` never
    /// names `a`'s render. Same shape as
    /// `crate_paths_in_another_package_are_unsettled`.
    #[test]
    fn absolute_packages_outside_a_crates_layout_stay_apart() {
        let other = "\
#[cfg(test)]
mod tests {
    #[test]
    fn other_package_a() {
        assert_eq!(crate::a::render(1), 2);
    }
}
";
        let index = index(&[("/w/x/src/lib.rs", RENDER_LIB), ("/w/y/src/lib.rs", other)]);
        assert_eq!(
            settle(&index, "::a::render", "other_package_a"),
            Some(CallTarget::Unsettled)
        );
        assert_eq!(
            settle(&index, "::a::render", "crate_path_names_a"),
            Some(CallTarget::Owner)
        );
        assert_eq!(
            package_key(Path::new("/w/x/src/a/b.rs")),
            Some("/w/x/".to_string())
        );
        assert_eq!(
            package_key(Path::new("/w/x/tests/api.rs")),
            Some("/w/x/".to_string())
        );
        assert_eq!(package_key(Path::new("src/lib.rs")), None);
        // An ancestor `src/` leaves the layout unreadable: the file is its
        // own package, never the package of `/srv/src/proj/src/lib.rs`.
        assert_eq!(
            package_key(Path::new("/srv/src/proj/tests/api.rs")),
            Some("/srv/src/proj/tests/api.rs?".to_string())
        );
        assert_ne!(
            package_key(Path::new("/srv/src/proj/src/lib.rs")),
            package_key(Path::new("/srv/src/proj/tests/api.rs"))
        );
        // Absolute `crates/<name>/` keeps its workspace directory.
        assert_ne!(
            package_key(Path::new("/tmp/one/crates/api/src/a.rs")),
            package_key(Path::new("/tmp/two/crates/api/src/a.rs"))
        );
        assert_eq!(
            package_key(Path::new("crates/api/src/a.rs")),
            Some("crates/api/".to_string())
        );
    }

    /// An index spelling one same-named definition absolute and another
    /// relative cannot be split by package: the rule switches off.
    #[test]
    fn a_mixed_path_form_index_has_no_identity() {
        let index = index(&[("src/lib.rs", RENDER_LIB), ("/w/x/src/lib.rs", BIN_UTIL)]);
        assert_eq!(settle(&index, "src/lib.rs::a::render", "doubles_two"), None);
    }
}
