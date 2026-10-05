//! Which same-named free function a test's call names (#6292, #6537,
//! #6544).
//!
//! A call matched by its last path segment alone lets a test of `b::render`
//! relate to, reach, and lend tokens or pins to a changed `a::render`. This
//! module is the one rule diff-mode related tests, repo-mode test grip and
//! the owner-return pin share to tell those definitions apart.
//!
//! The rule only applies when the owner is a module-level `fn` and another
//! module-level `fn` of the same name exists in the owner's package. For
//! each call of the name in the test body it reads the spelled path:
//!
//! - a qualified call (`b::render(..)`, `crate::a::render(..)`,
//!   `super::a::render(..)`) names the definitions whose module path the
//!   qualifier spells;
//! - a bare call (`render(..)`) is bound by the innermost scope that binds
//!   the name: a `use` in the test body, then each enclosing module's own
//!   `use` items and item definitions, following `use super::*` outward and
//!   reading other globs.
//!
//! A call settles on the owner only when the spelled path matches the
//! owner and nothing else. It settles on another definition only when it
//! matches that definition and not the owner. Everything else (a renamed
//! import, an unparseable file, a re-export path, a definition the parser
//! could not place) stays unsettled, and callers keep their prior
//! behavior: an unsettled call never grants new credit.
//!
//! This is bounded syntax, not name resolution. File-backed module paths
//! come from the file's location under `src/`; `#[path]` remaps and
//! macro-generated items are documented residuals that read unsettled or
//! mismatched, never as a new grant.

use super::super::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::facts::FunctionContainer;
use crate::analysis::syntax::{NameScopes, UseBinding, name_scopes_for_fn};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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

/// The owner and its same-named, same-package module-level rivals.
pub(in crate::analysis) struct OwnerCallIdentity {
    name: String,
    owner_package: Option<String>,
    /// Module path of each definition: the owner first. `None` when the
    /// definition could not be placed, so it matches every spelled path.
    definitions: Vec<Option<Vec<String>>>,
    /// Every module segment the definitions' paths spell.
    known_segments: BTreeSet<String>,
    memo: Mutex<BTreeMap<(PathBuf, usize), CallTarget>>,
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

/// A spelled module path: anchored at the crate root, or matched as a
/// suffix of a definition's module path.
struct PathSpec {
    segments: Vec<String>,
    anchored: bool,
}

impl OwnerCallIdentity {
    /// `None` when the rule does not apply: the owner is not a placed
    /// module-level `fn`, or no other module-level `fn` of its name exists
    /// in its package among `same_named`.
    pub(in crate::analysis) fn new<'a>(
        owner: &FunctionSummary,
        same_named: impl IntoIterator<Item = &'a FunctionSummary>,
    ) -> Option<Self> {
        if owner.item.container != FunctionContainer::Free {
            return None;
        }
        let owner_path = definition_module_path(owner)?;
        let owner_package = super::related_tests::package_prefix(&owner.file);
        let mut definitions = vec![Some(owner_path)];
        for function in same_named {
            if function.name != owner.name
                || (function.file == owner.file && function.start_line == owner.start_line)
                || super::related_tests::package_prefix(&function.file) != owner_package
            {
                continue;
            }
            match function.item.container {
                FunctionContainer::Free => definitions.push(definition_module_path(function)),
                FunctionContainer::Unknown => definitions.push(None),
                _ => {}
            }
        }
        if definitions.len() < 2 {
            return None;
        }
        let known_segments = definitions.iter().flatten().flatten().cloned().collect();
        Some(Self {
            name: owner.name.clone(),
            owner_package,
            definitions,
            known_segments,
            memo: Mutex::new(BTreeMap::new()),
        })
    }

    /// Whether `function` is one of the rivals this identity tells apart
    /// from the owner per test: a module-level `fn` of the owner's name in
    /// its package.
    pub(in crate::analysis) fn settles(&self, function: &FunctionSummary) -> bool {
        function.name == self.name
            && function.item.container == FunctionContainer::Free
            && super::related_tests::package_prefix(&function.file) == self.owner_package
    }

    /// What `test`'s calls of the owner's name settle on.
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

    fn resolve_test_uncached(&self, test: &TestSummary, index: &RustIndex) -> CallTarget {
        let qualifiers = call_qualifiers(&test.body, &self.name);
        if qualifiers.is_empty() {
            return CallTarget::Unsettled;
        }
        let file_segments = file_module_segments(&test.file);
        let crate_names = &index.package_names;
        let scopes = index.files().get(&test.file).and_then(|facts| {
            name_scopes_for_fn(
                &facts.data().source,
                test.start_line,
                &test.name,
                &self.name,
            )
        });
        let placed = scopes.as_ref().zip(file_segments.as_ref());
        let mut owner = false;
        let mut unsettled = false;
        for qualifier in qualifiers {
            let matches = match (qualifier, placed) {
                (None, _) => Matches::Any,
                (Some(segments), Some((scopes, file_segments))) if segments.is_empty() => {
                    self.resolve_bare(file_segments, scopes, crate_names)
                }
                (Some(segments), _) if segments.is_empty() => Matches::Any,
                (Some(segments), placed) => {
                    let base = placed.and_then(|(scopes, file_segments)| {
                        scopes
                            .scopes
                            .first()
                            .map(|scope| join(file_segments, &scope.modules))
                    });
                    self.matches_path(&segments, base.as_deref(), crate_names)
                }
            };
            match self.target_of(&matches) {
                CallTarget::Owner => owner = true,
                CallTarget::Unsettled => unsettled = true,
                CallTarget::Other => {}
            }
        }
        if owner {
            CallTarget::Owner
        } else if unsettled {
            CallTarget::Unsettled
        } else {
            CallTarget::Other
        }
    }

    fn target_of(&self, matches: &Matches) -> CallTarget {
        let Matches::Some(set) = matches else {
            return CallTarget::Unsettled;
        };
        if set.is_empty() {
            return CallTarget::Unsettled;
        }
        // An unplaced definition matches every spec, so it is in `set`
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

    /// The definitions a bare call binds to, scope by scope.
    fn resolve_bare(
        &self,
        file_segments: &[String],
        scopes: &NameScopes,
        crate_names: &BTreeSet<String>,
    ) -> Matches {
        let mut from_globs = Matches::none();
        for scope in &scopes.scopes {
            let base = join(file_segments, &scope.modules);
            if !scope.imports.is_empty() {
                let mut matches = Matches::none();
                for import in &scope.imports {
                    let found = match import {
                        UseBinding::Renamed => Matches::Any,
                        UseBinding::Path(path) => match path.split_last() {
                            Some((_, module)) => {
                                self.matches_path(module, Some(&base), crate_names)
                            }
                            None => Matches::Any,
                        },
                    };
                    matches = matches.union(found);
                }
                return from_globs.union(matches);
            }
            if scope.defines {
                return from_globs.union(self.matches_spec(&PathSpec {
                    segments: base,
                    anchored: true,
                }));
            }
            if scope.fn_body {
                continue;
            }
            let mut follows_super = false;
            for glob in &scope.globs {
                if glob.len() == 1 && glob[0] == "super" {
                    follows_super = true;
                } else {
                    from_globs =
                        from_globs.union(self.matches_path(glob, Some(&base), crate_names));
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

    /// The definitions a spelled module path (`a::b` of `a::b::name`)
    /// names, relative to the calling scope's module `base` when known.
    fn matches_path(
        &self,
        segments: &[String],
        base: Option<&[String]>,
        crate_names: &BTreeSet<String>,
    ) -> Matches {
        match self.spec(segments, base, crate_names) {
            Some(spec) => self.matches_spec(&spec),
            None => Matches::Any,
        }
    }

    /// `crate_names` are the workspace's package and library names: a
    /// leading one that no definition spells as a module is a crate root.
    fn spec(
        &self,
        segments: &[String],
        base: Option<&[String]>,
        crate_names: &BTreeSet<String>,
    ) -> Option<PathSpec> {
        let first = segments.first()?;
        match first.as_str() {
            "crate" => Some(PathSpec {
                segments: segments[1..].to_vec(),
                anchored: true,
            }),
            "self" => Some(PathSpec {
                segments: join(base?, &segments[1..]),
                anchored: true,
            }),
            "super" => {
                let base = base?;
                let supers = segments.iter().take_while(|s| *s == "super").count();
                let kept = base.len().checked_sub(supers)?;
                Some(PathSpec {
                    segments: join(&base[..kept], &segments[supers..]),
                    anchored: true,
                })
            }
            "Self" => None,
            // `modules::scaled` from an integration test: a crate root. The
            // definitions are all in the owner's package; a same-named item
            // at another workspace crate's root is a cross-package rival the
            // owner pin refuses on and the package-prefix guard filters.
            _ if crate_names.contains(first) && !self.known_segments.contains(first) => {
                Some(PathSpec {
                    segments: segments[1..].to_vec(),
                    anchored: true,
                })
            }
            // A leading segment no definition spells, followed by more
            // path, is the crate's own name from an integration test or a
            // dependent crate (`modules::b::render`).
            _ if segments.len() > 1 && !self.known_segments.contains(first) => Some(PathSpec {
                segments: segments[1..].to_vec(),
                anchored: false,
            }),
            _ => Some(PathSpec {
                segments: segments.to_vec(),
                anchored: false,
            }),
        }
    }

    fn matches_spec(&self, spec: &PathSpec) -> Matches {
        let set = self
            .definitions
            .iter()
            .enumerate()
            .filter(|(_, path)| match path {
                None => true,
                Some(path) if spec.anchored => *path == spec.segments,
                Some(path) => path.ends_with(&spec.segments),
            })
            .map(|(index, _)| index)
            .collect();
        Matches::Some(set)
    }
}

fn join(left: &[String], right: &[String]) -> Vec<String> {
    left.iter().chain(right).cloned().collect()
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

/// Module segments of a file below its crate root: `src/lib.rs` and
/// `src/main.rs` are the root, `src/a/mod.rs` and `src/a.rs` are `a`.
/// `None` for a file outside `src/` or `tests/`.
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
        // `src/bin/x.rs` is its own crate root.
        segments.drain(..2.min(segments.len()));
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
    let bytes = masked.as_bytes();
    let is_ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut calls = Vec::new();
    for (start, _) in masked.match_indices(name) {
        let end = start + name.len();
        if start > 0 && bytes.get(start - 1).copied().is_some_and(is_ident) {
            continue;
        }
        if bytes.get(end).copied().is_some_and(is_ident) {
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

fn head_word(head: &str) -> &str {
    let start = head
        .rfind(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .map_or(0, |index| index + 1);
    &head[start..]
}

/// The `a::b::` path segments ending `head`, outermost first.
fn qualifier_before(head: &str) -> Option<Vec<String>> {
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
        segments.push(word.to_string());
        rest = before[..before.len() - word.len()].trim_end();
    }
    segments.reverse();
    Some(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::rust_index::summarize_file;

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
        let identity = OwnerCallIdentity::new(owner, index.functions().iter())?;
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
            solo.and_then(|owner| OwnerCallIdentity::new(owner, index.functions().iter()))
                .is_none()
        );
        let free_render = index
            .functions()
            .iter()
            .find(|f| f.name == "render" && f.item.container == FunctionContainer::Free);
        assert!(free_render.is_some());
        assert!(
            free_render
                .and_then(|owner| OwnerCallIdentity::new(owner, index.functions().iter()))
                .is_none(),
            "an inherent method is no module-level rival"
        );
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
        assert_eq!(file_module_segments(Path::new("build.rs")), None);
    }
}
