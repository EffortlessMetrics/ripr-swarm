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
//!   creates executable `TestFact`s — that stays behind the #3499/#3532
//!   executable-test authority. The one removal is the tests of a file no
//!   Cargo target compiles (#8999), which never run.
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
use super::invisible_owners::InvisibleOwners;
use super::model::{
    ModuleDeclarationFact, ModulePathTarget, ResolvedIncludeParent, RustIndex, SourceRoleProvenance,
};
use super::stray_files;
use super::{SourceRoleProvenanceEdge, SourceRoleProvenanceEdgeKind};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// Chain-walk bound, mirroring the include pass's depth limit. Module and
/// include chains past this bound are exotic input that fails closed instead
/// of risking unbounded recursion.
pub(super) const MAX_CONTEXT_CHAIN_DEPTH: usize = 32;

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
    /// The declaration's only cfg guard is exactly `#[cfg(kani)]` (#13736):
    /// the child exists only in a Kani verification build.
    cfg_kani: bool,
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
pub(super) fn compose_index_source_roles(index: &mut RustIndex, workspace_root: &Path) {
    let mut execution_parents = BTreeMap::new();
    let module_edges = resolved_module_edges(index, workspace_root, &mut execution_parents);
    index.execution_module_parents = Some(execution_parents);
    // Phase 1 (immutable): resolve every file's context and provenance.
    // The resolver borrows `include_parents`, so all mutations wait for it
    // to go out of scope.
    let cfg_kani_files: BTreeSet<PathBuf>;
    let resolutions: Vec<(PathBuf, Context, SourceRoleProvenance)> = {
        let mut resolver = ContextResolver {
            module_edges,
            include_parents: &index.include_parents,
            memo: BTreeMap::new(),
            kani_memo: BTreeMap::new(),
            invisible_owners: InvisibleOwners::new(index, workspace_root),
        };
        let resolutions = index
            .files
            .keys()
            .cloned()
            .map(|file| {
                let (context, provenance) = resolver.context_with_provenance(&file);
                (file, context, provenance)
            })
            .collect::<Vec<_>>();
        cfg_kani_files = index
            .files
            .keys()
            .filter(|file| resolver.cfg_kani_only(file, 0))
            .cloned()
            .collect::<BTreeSet<_>>();
        resolutions
    };
    index.cfg_kani_module_files = cfg_kani_files;

    let outside_module_tree = files_outside_module_tree(index, workspace_root, &resolutions);

    // Phase 2 (mutable): record provenance and grant evidence roles.
    let mut granted: BTreeSet<PathBuf> = BTreeSet::new();
    for (file, context, mut provenance) in resolutions {
        if context == Context::RequiresTest {
            granted.insert(file.clone());
        }
        provenance.outside_module_tree = outside_module_tree.contains(&file);
        let has_provenance = !provenance.edges.is_empty()
            || provenance.earliest_unresolved_reason.is_some()
            || provenance.outside_module_tree;
        if has_provenance && let Some(facts) = index.files.get_mut(&file) {
            facts.role_provenance = provenance;
        } else if let Some(facts) = index.files.get_mut(&file) {
            // Incremental reuse keeps earlier file facts: a verdict the
            // latest pass no longer reaches must not linger.
            facts.role_provenance.outside_module_tree = false;
        }
    }
    withhold_tests_outside_module_tree(index, &outside_module_tree);
    let skipped_unit_tests = files_in_skipped_unit_test_targets(index, workspace_root);
    withhold_tests_outside_module_tree(index, &skipped_unit_tests);
    index.cargo_test_skipped_files = skipped_unit_tests;
    if granted.is_empty() {
        return;
    }
    apply_cfg_test_module_grants(index, &granted);
}

/// Drops the executable tests of files that no Cargo target compiles (#8999):
/// they never run, so they are not related-test evidence. Only the test
/// registrations go; the files, their functions and their provenance stay, so
/// an owner in such a file still reads as outside the module tree. Every
/// composition pass starts from freshly normalized tests. Same-run index
/// extension refuses a merge that links a file the core withheld here when
/// that revived file has tests, and falls back to a fresh build (#12947): the
/// revived tests would be rebuilt from function facts rather than the parser's
/// test facts. A revived file without tests extends.
fn withhold_tests_outside_module_tree(index: &mut RustIndex, outside: &BTreeSet<PathBuf>) {
    if outside.is_empty() {
        return;
    }
    for file in outside {
        if let Some(facts) = index.files.get_mut(file) {
            facts.tests.clear();
        }
    }
    let test_facts = &index.test_facts;
    index
        .test_order
        .retain(|&id| !outside.contains(&test_facts[id].file));
}

/// The test-holding files whose every execution root is a crate root that
/// `cargo test` does not build as a libtest binary (#14962): `[lib] test =
/// false`, `[[bin]] test = false`, or a bin's unmet default-feature
/// `required-features`, as cargo metadata reports them through
/// [`ManifestInventory`]. #14972 adds an example with metadata `test =
/// false` (the default), and a conventional root (`src/main.rs`,
/// `src/lib.rs`, ...) that cargo did not discover as any target because the
/// package turns an `auto*` key off. The `#[test]` functions such a module tree
/// compiles print `running 0 tests`, so they are not discriminators.
/// Doctests are a separate rustdoc run (`[lib] doctest`) and register after
/// this pass, so they keep their credit. A skipped `[[test]]` target keeps
/// its existing owners (#7265 harness verdict, #8016 file drop).
///
/// Fail closed toward credit. A file whose module ancestry is unknown
/// (include edge, lexical fallback, unresolved or cyclic edge) keeps its
/// tests; so does a file reached from any root cargo runs, a root cargo
/// metadata does not list (unless the package turns auto-discovery off) or
/// cannot report, a root an example with `test = true` owns, and any root a
/// `bench` or build-script target owns. The metadata probe runs only when an
/// execution root's package manifest spells `test = false` on its `[lib]`
/// or a `[[bin]]` table, a bin names `required-features`, an `[[example]]`
/// table or a false `auto*` key exists, or the root sits under the
/// package's `examples/` directory (#14972), and the pass
/// stands down while a `Cargo.toml` is dirty in a committed-history check,
/// because the probe reads the working tree rather than the `HEAD` bytes the
/// files were loaded from (the #8016 rule).
///
/// The index is only part of the workspace: a draft or instant run indexes
/// less, and a build-script `include!` never enters it. So a skip also needs
/// [`WorkspaceMountScan`] over every workspace source on disk: no source
/// outside the file's own skipped ancestry may spell its file name or
/// declare a `mod` of its stem, no source may build a `mod`/`include!`
/// operand dynamically, and cargo metadata may list the file as no live or
/// unmodeled target (#14962 review). The scan asks about every ancestry
/// member except a lib or bin root metadata lists as skipped: an
/// undiscovered root and a skipped example or bench root are asked too
/// (#14972 review, #15262). The verdict then does not depend on how much of the workspace
/// the run indexed. A holder whose own package
/// manifest switches nothing off is never asked, even when `#[path]` places
/// it under another package's skipped root (it keeps its credit).
///
/// Cost: one `cargo metadata --no-deps --offline` probe per composition pass
/// that has a premise (a check builds up to two indexes), plus one walk of
/// the workspace sources once a candidate passes the metadata checks.
fn files_in_skipped_unit_test_targets(
    index: &RustIndex,
    workspace_root: &Path,
) -> BTreeSet<PathBuf> {
    let mut skipped = BTreeSet::new();
    let holders: Vec<&PathBuf> = index
        .files
        .iter()
        .filter(|(_, facts)| !facts.tests.is_empty() && !facts.role_provenance.outside_module_tree)
        .map(|(file, _)| file)
        .collect();
    if holders.is_empty() {
        return skipped;
    }
    if crate::analysis::committed_source::current_overlay().is_some_and(|overlay| {
        overlay
            .dirty_paths()
            .any(|path| path.rsplit('/').next() == Some("Cargo.toml"))
    }) {
        return skipped;
    }
    let mut crate_roots = CrateRoots::default();
    let mut premises: BTreeMap<PathBuf, SkipPremise> = BTreeMap::new();
    let mut premise_of =
        |crate_roots: &mut CrateRoots, path: &Path| -> Option<(PathBuf, SkipPremise)> {
            let package_dir = crate_roots.package_manifest_dir(workspace_root, path)?;
            let premise = *premises
                .entry(package_dir.clone())
                .or_insert_with(|| manifest_skip_premise(workspace_root, &package_dir));
            // #14972: an `examples/` source is a default `test = false` target
            // whatever the manifest says, and so is a `benches/` source
            // (#15262).
            let in_default_skipped_dir = path.strip_prefix(&package_dir).is_ok_and(|relative| {
                relative.starts_with("examples") || relative.starts_with("benches")
            });
            Some((
                package_dir,
                SkipPremise {
                    probe: premise.probe || in_default_skipped_dir,
                    ..premise
                },
            ))
        };
    let mut inventory = crate::analysis::workspace::ManifestInventory::default();
    let mut verdicts: BTreeMap<PathBuf, bool> = BTreeMap::new();
    // Skipped roots the mount scan still asks about like any other ancestry
    // member (#14972): a root cargo did not discover (metadata pins no
    // target to it) and a skipped example or bench root, since a running
    // target can mount one through `include!` inside a module body, an edge
    // the index does not record (#14972 review, #15262). Lib and bin `test = false`
    // roots stay exempt (#14962).
    let mut scanned_roots: BTreeSet<PathBuf> = BTreeSet::new();
    let mut mounts: Option<Option<WorkspaceMountScan>> = None;
    for file in holders {
        // The cheap manifest premise first: a holder whose own package
        // switches no target off skips no ancestry walk.
        let holder_premise =
            premise_of(&mut crate_roots, file).is_some_and(|(_, premise)| premise.probe);
        if !holder_premise {
            continue;
        }
        let Some(context) = index.execution_context(file) else {
            continue;
        };
        if context.roots.is_empty() {
            continue;
        }
        let all_skipped = context.roots.iter().all(|root| {
            if let Some(verdict) = verdicts.get(root) {
                return *verdict;
            }
            let verdict =
                premise_of(&mut crate_roots, root).is_some_and(|(package_dir, premise)| {
                    if !premise.probe {
                        return false;
                    }
                    if inventory.crate_root_unit_tests_skipped(workspace_root, root) {
                        if inventory.crate_root_skipped_example_or_bench(workspace_root, root) {
                            scanned_roots.insert(root.clone());
                        }
                        return true;
                    }
                    let unlisted = premise.undiscovered_roots
                        && inventory.crate_root_undiscovered(workspace_root, root, &package_dir);
                    if unlisted {
                        scanned_roots.insert(root.clone());
                    }
                    unlisted
                });
            verdicts.insert(root.clone(), verdict);
            verdict
        });
        if !all_skipped {
            continue;
        }
        // #14962 review: the index is a subset of the workspace (a draft or
        // instant run indexes less, and a generated `include!` never enters
        // it), so a running target may mount the file where no indexed edge
        // shows it. Every workspace source on disk is asked instead.
        let scan = mounts.get_or_insert_with(|| {
            inventory
                .workspace_package_dirs(workspace_root)
                .and_then(|package_dirs| WorkspaceMountScan::read(workspace_root, &package_dirs))
        });
        let Some(scan) = scan.as_ref() else {
            return BTreeSet::new();
        };
        let mut chain: BTreeSet<&Path> = context.roots.iter().map(PathBuf::as_path).collect();
        for edge in &context.edges {
            chain.insert(&edge.parent);
            chain.insert(&edge.child);
        }
        chain.insert(file);
        // Every file of the ancestry, not only the holder: an intermediate
        // module that is itself a running target root (`[[test]] path =
        // "src/container.rs"`) or that another source mounts compiles the
        // holder into a target `cargo test` runs (#14962 review, Devin). A
        // root cargo did not discover is no listed target, and a skipped
        // example or bench is no target another crate builds (#14972,
        // #15262), so a source
        // outside the chain that names either may mount it into a running
        // target where the indexed edges do not show it.
        let ancestry_may_run = chain.iter().any(|member| {
            inventory.crate_root_may_run(workspace_root, member)
                || ((scanned_roots.contains(*member)
                    || !context.roots.iter().any(|root| root == member))
                    && scan.may_mount(member, &chain))
        });
        if !ancestry_may_run && !scan.may_forward_module_outside(&chain) {
            skipped.insert(file.clone());
        }
    }
    skipped
}

pub(super) fn may_build_mount_operands(source: &str) -> bool {
    !matches!(mount_operand_doubt(source), MountOperandDoubt::None)
}

enum MountOperandDoubt {
    None,
    ForwardedModule,
    Unbound,
}

/// Whether `source` may mount a module through a file the scan cannot
/// name or read, judged on Rust tokens so comments, whitespace and string
/// contents cannot hide or fake a mount (#14962 review):
///
/// - a `macro_rules!` or `macro` body holding a `mod`, `include` or
///   attribute-position `path` token, since its expansion can mount any
///   file (`mod /* c */ $n;`); a macro without them, such as `($e:expr) =>
///   { $e * 2 }`, is no doubt;
/// - any `include` token other than a plain metavariable or an `include!`
///   (any delimiter) whose
///   operand is a string literal naming a `.rs` file: a generated
///   `include!(concat!(env!("OUT_DIR"), ..))`, an `include!("gen.inc")` the
///   scan does not read (it reads only `.rs` sources), and an alias such as
///   `use std::include as inc;` whose `inc!` no rule could follow;
/// - a `path` attribute whose value is a metavariable or a string literal
///   that does not name a `.rs` file (`#[path = "m.txt"]`);
/// - `mod $name` anywhere or a literal `mod` inside a macro invocation's
///   token tree, where the macro may forward it into a declaration.
fn mount_operand_doubt(source: &str) -> MountOperandDoubt {
    let tokens = mount_tokens(source);
    let ident = |at: usize, name: &str| matches!(tokens.get(at), Some(MountToken::Ident(word)) if word == name);
    let punct = |at: usize, mark: char| matches!(tokens.get(at), Some(MountToken::Punct(found)) if *found == mark);
    let is_metavariable = |at: usize| at.checked_sub(1).is_some_and(|prev| punct(prev, '$'));
    // A literal with an escape (`"\u{73}hared.rs"`) decodes to a name the
    // substring scan cannot match, so it names no source the scan can tie
    // back (#14962 review, Devin).
    let names_rust_source = |at: usize| {
        matches!(tokens.get(at), Some(MountToken::Str(text))
            if text.ends_with(".rs") && !text.contains('\\'))
    };
    let mut invocation_depth = 0usize;
    let mut doubt = MountOperandDoubt::None;
    for at in 0..tokens.len() {
        match tokens[at] {
            MountToken::Punct('(' | '[' | '{')
                if invocation_depth > 0
                    || (at >= 2
                        && punct(at - 1, '!')
                        && matches!(tokens[at - 2], MountToken::Ident(_))) =>
            {
                invocation_depth += 1;
            }
            MountToken::Punct(')' | ']' | '}') => {
                invocation_depth = invocation_depth.saturating_sub(1);
            }
            _ => {}
        }
        if ident(at, "macro_rules") && punct(at + 1, '!') {
            if macro_body_may_mount(&tokens, at + 2, true) {
                return MountOperandDoubt::Unbound;
            }
        } else if ident(at, "macro")
            && matches!(tokens.get(at + 1), Some(MountToken::Ident(_)))
            && macro_body_may_mount(&tokens, at + 2, false)
        {
            return MountOperandDoubt::Unbound;
        }
        if ident(at, "include")
            && (!is_metavariable(at) || punct(at + 1, '!'))
            && !(punct(at + 1, '!')
                && (punct(at + 2, '(') || punct(at + 2, '{') || punct(at + 2, '['))
                && names_rust_source(at + 3))
        {
            return MountOperandDoubt::Unbound;
        }
        if ident(at, "path")
            && punct(at + 1, '=')
            && (punct(at + 2, '$')
                || (at > 0
                    && (punct(at - 1, '[') || punct(at - 1, ','))
                    && matches!(tokens.get(at + 2), Some(MountToken::Str(_)))
                    && !names_rust_source(at + 2)))
        {
            return MountOperandDoubt::Unbound;
        }
        if ident(at, "mod") {
            if punct(at + 1, '$') {
                return MountOperandDoubt::Unbound;
            }
            if invocation_depth > 0 && !is_metavariable(at) {
                doubt = MountOperandDoubt::ForwardedModule;
            }
        }
    }
    doubt
}

/// True when the macro definition whose name or body starts at `from` holds
/// a `mod`, `include` or non-`::` `path` token. A `macro_rules!` body is its
/// first delimited group; a `macro` 2.0 definition runs through its first
/// `{ .. }` group, covering an argument list before it.
fn macro_body_may_mount(tokens: &[MountToken], from: usize, first_group_only: bool) -> bool {
    let mut at = from;
    if matches!(tokens.get(at), Some(MountToken::Ident(_))) {
        at += 1;
    }
    let mut depth = 0usize;
    while let Some(token) = tokens.get(at) {
        match token {
            MountToken::Punct('(' | '[' | '{') => depth += 1,
            MountToken::Punct(close @ (')' | ']' | '}')) => {
                if depth <= 1 && (first_group_only || *close == '}') {
                    return false;
                }
                depth = depth.saturating_sub(1);
            }
            MountToken::Ident(word) if depth > 0 => {
                let in_path = |offset: Option<usize>| {
                    offset.is_some_and(|near| {
                        matches!(tokens.get(near), Some(MountToken::Punct(':')))
                    })
                };
                let metavariable = at > 0 && matches!(tokens[at - 1], MountToken::Punct('$'));
                let mounts = match word.as_str() {
                    "mod" => !metavariable,
                    // A metavariable followed by `!` can expand to a mount.
                    "include" => {
                        !metavariable || matches!(tokens.get(at + 1), Some(MountToken::Punct('!')))
                    }
                    "path" => !in_path(at.checked_sub(1)) && !in_path(Some(at + 1)),
                    _ => false,
                };
                if mounts {
                    return true;
                }
            }
            _ if depth == 0 => return false,
            _ => {}
        }
        at += 1;
    }
    false
}

/// One Rust token as the mount check needs it: comments and whitespace are
/// dropped, and a string literal keeps its raw contents.
#[derive(Debug, PartialEq)]
enum MountToken {
    Ident(String),
    Str(String),
    Punct(char),
    Other,
}

/// A small Rust lexer: identifiers, string literals (plain, byte, C and raw
/// with any `#` count), char and byte literals, lifetimes, numbers, nested
/// block comments and line comments. An unterminated literal or comment runs
/// to the end of the text.
fn mount_tokens(source: &str) -> Vec<MountToken> {
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let at_is = |at: usize, mark: char| chars.get(at) == Some(&mark);
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && at_is(i + 1, '/') {
            while i < len && chars[i] != '\n' {
                i += 1;
            }
        } else if ch == '/' && at_is(i + 1, '*') {
            let mut depth = 0usize;
            while i < len {
                if chars[i] == '/' && at_is(i + 1, '*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && at_is(i + 1, '/') {
                    depth = depth.saturating_sub(1);
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if ch == '"' {
            let (text, next) = quoted_literal(&chars, i + 1);
            tokens.push(MountToken::Str(text));
            i = next;
        } else if ch == '\'' {
            i = char_or_lifetime_end(&chars, i);
            tokens.push(MountToken::Other);
        } else if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if matches!(word.as_str(), "r" | "br" | "cr")
                && let Some((text, next)) = raw_literal(&chars, i)
            {
                tokens.push(MountToken::Str(text));
                i = next;
            } else if matches!(word.as_str(), "b" | "c") && at_is(i, '"') {
                let (text, next) = quoted_literal(&chars, i + 1);
                tokens.push(MountToken::Str(text));
                i = next;
            } else if word == "b" && at_is(i, '\'') {
                i = char_or_lifetime_end(&chars, i);
                tokens.push(MountToken::Other);
            } else {
                tokens.push(MountToken::Ident(word));
            }
        } else if ch.is_ascii_digit() {
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            tokens.push(MountToken::Other);
        } else {
            tokens.push(MountToken::Punct(ch));
            i += 1;
        }
    }
    tokens
}

/// The contents of a `"`-quoted literal whose body starts at `start`, and
/// the index after its closing quote; an escaped quote does not close it.
fn quoted_literal(chars: &[char], start: usize) -> (String, usize) {
    let mut at = start;
    while at < chars.len() && chars[at] != '"' {
        at += if chars[at] == '\\' { 2 } else { 1 };
    }
    let end = at.min(chars.len());
    (
        chars[start..end].iter().collect(),
        (at + 1).min(chars.len()),
    )
}

/// A raw literal (`r"..."`, `r#"..."#`) whose `#`s or quote start at `at`,
/// or `None` for a raw identifier such as `r#mod`.
fn raw_literal(chars: &[char], at: usize) -> Option<(String, usize)> {
    let mut hashes = 0usize;
    while chars.get(at + hashes) == Some(&'#') {
        hashes += 1;
    }
    let open = at + hashes;
    if chars.get(open) != Some(&'"') {
        return None;
    }
    let start = open + 1;
    let mut end = start;
    while end < chars.len() {
        if chars[end] == '"' && (1..=hashes).all(|offset| chars.get(end + offset) == Some(&'#')) {
            break;
        }
        end += 1;
    }
    let text = chars[start..end.min(chars.len())].iter().collect();
    Some((text, (end + 1 + hashes).min(chars.len())))
}

/// The index after a char literal (`'x'`, `'\''`, `'"'`) or after the quote
/// of a lifetime (`'a`), whose quote is at `at`. A lifetime's name is lexed
/// as an identifier after it.
fn char_or_lifetime_end(chars: &[char], at: usize) -> usize {
    if chars.get(at + 1) == Some(&'\\') {
        let mut end = at + 3;
        while end < chars.len() && chars[end] != '\'' {
            end += 1;
        }
        (end + 1).min(chars.len())
    } else if chars.get(at + 2) == Some(&'\'') {
        at + 3
    } else {
        at + 1
    }
}

/// Upper bound on the entries read below one symlinked directory.
const LINKED_DIR_ENTRY_LIMIT: usize = 1_000;

/// True when the symlinked directory `link` may hold a Rust source: it holds
/// a `.rs` file or another symlink at any depth, or cannot be read within
/// [`LINKED_DIR_ENTRY_LIMIT`] entries (#14962 review n3).
fn linked_dir_may_hold_sources(link: &Path) -> bool {
    let mut pending = vec![link.to_path_buf()];
    let mut seen = 0usize;
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return true;
        };
        for entry in entries {
            let Ok(entry) = entry else {
                return true;
            };
            seen += 1;
            if seen > LINKED_DIR_ENTRY_LIMIT {
                return true;
            }
            let Ok(kind) = entry.file_type() else {
                return true;
            };
            if kind.is_symlink() || entry.file_name().to_string_lossy().ends_with(".rs") {
                return true;
            }
            if kind.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    false
}

/// Upper bound on the files the mount scan reads; a larger tree is doubt.
const MOUNT_SCAN_FILE_LIMIT: usize = 20_000;

/// Every Rust source and `Cargo.toml` of the workspace on disk (#14962
/// review), read once per composition pass that has a skip candidate. The
/// walk covers the workspace root and every metadata member directory,
/// skipping `target` and hidden directories. In a committed-history check a
/// dirty file contributes both its working-tree and its `HEAD` text, so a
/// mention in either keeps the file.
struct WorkspaceMountScan {
    /// Workspace-relative path (when the file sits under the root), its
    /// lowercased text, and the module names it declares.
    sources: Vec<(Option<PathBuf>, String, BTreeSet<String>)>,
    forwarding_sources: Vec<Option<PathBuf>>,
}

impl WorkspaceMountScan {
    /// `None` is doubt for the whole workspace: an unreadable file, a tree
    /// past [`MOUNT_SCAN_FILE_LIMIT`], or any source that may assemble a
    /// `mod` or `include!` operand the text cannot name (a dynamic
    /// `include!(concat!(env!("OUT_DIR"), ..))`, a macro-built `mod`) or that
    /// spells a `path`/`include!` operand with escapes.
    fn read(workspace_root: &Path, package_dirs: &BTreeSet<PathBuf>) -> Option<Self> {
        let absolute_root = std::path::absolute(workspace_root).ok()?;
        let canonical_root = std::fs::canonicalize(workspace_root).ok();
        let relative = |path: &Path| -> Option<PathBuf> {
            path.strip_prefix(&absolute_root)
                .ok()
                .or_else(|| {
                    canonical_root
                        .as_ref()
                        .and_then(|root| path.strip_prefix(root).ok())
                })
                .map(Path::to_path_buf)
        };
        // Only Cargo's build directories are skipped: `target` directly under
        // the workspace root or a member root, and `CARGO_TARGET_DIR`. A
        // `target` or hidden directory anywhere else may hold a mounted
        // module (`pub mod target;`, `#[path = ".gen/m.rs"]`), so it is read
        // (#14962 review). `.git` holds no sources.
        let mut build_dirs: BTreeSet<PathBuf> = BTreeSet::new();
        build_dirs.insert(absolute_root.join("target"));
        for dir in package_dirs {
            build_dirs.insert(dir.join("target"));
        }
        if let Some(target_dir) = std::env::var_os("CARGO_TARGET_DIR")
            && let Ok(target_dir) = std::path::absolute(PathBuf::from(target_dir))
        {
            build_dirs.insert(target_dir);
        }
        let mut files: BTreeSet<PathBuf> = BTreeSet::new();
        let mut pending: Vec<PathBuf> = vec![absolute_root.clone()];
        pending.extend(package_dirs.iter().cloned());
        let mut visited: BTreeSet<PathBuf> = BTreeSet::new();
        while let Some(dir) = pending.pop() {
            if !visited.insert(dir.clone()) {
                continue;
            }
            let entries = std::fs::read_dir(&dir).ok()?;
            for entry in entries {
                let entry = entry.ok()?;
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                let kind = entry.file_type().ok()?;
                if kind.is_symlink() {
                    // A symlinked source or directory can alias the file
                    // under another name (`src/alias.rs -> ../../a/src/
                    // shared.rs`) that no text scan ties back to it: doubt.
                    // A directory link without Rust sources (`node_modules`,
                    // assets) aliases nothing (#14962 review n3).
                    let target_is_dir = std::fs::metadata(&path).is_ok_and(|meta| meta.is_dir());
                    if name.ends_with(".rs")
                        || (target_is_dir && linked_dir_may_hold_sources(&path))
                    {
                        return None;
                    }
                } else if kind.is_dir() {
                    if name != ".git" && !build_dirs.contains(&path) {
                        pending.push(path);
                    }
                } else if name.ends_with(".rs") || name == "Cargo.toml" {
                    files.insert(path);
                }
                if files.len() + pending.len() > MOUNT_SCAN_FILE_LIMIT {
                    return None;
                }
            }
        }
        let mut sources = Vec::with_capacity(files.len());
        for path in files {
            let text = std::fs::read_to_string(&path).ok()?;
            let manifest = path.ends_with("Cargo.toml");
            sources.push((relative(&path), text, manifest));
        }
        if let Some(overlay) = crate::analysis::committed_source::current_overlay() {
            for dirty in overlay.dirty_paths() {
                let dirty = PathBuf::from(dirty);
                if let Ok(Some(bytes)) =
                    crate::analysis::committed_source::read_source_bytes(workspace_root, &dirty)
                {
                    let manifest = dirty.ends_with("Cargo.toml");
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    sources.push((Some(dirty), text, manifest));
                }
            }
        }
        let mut lowered = Vec::with_capacity(sources.len());
        let mut forwarding_sources = Vec::new();
        for (path, text, manifest) in sources {
            if stray_files::spells_a_path_with_escapes(&text) {
                return None;
            }
            if manifest {
                // A manifest names a file only as a target `path` or a
                // `build` script, and cargo metadata already lists every
                // target's `src_path`: `crate_root_may_run` asks it.
                continue;
            }
            match mount_operand_doubt(&text) {
                MountOperandDoubt::Unbound => return None,
                MountOperandDoubt::ForwardedModule => forwarding_sources.push(path.clone()),
                MountOperandDoubt::None => {}
            }
            let mut declared = BTreeSet::new();
            stray_files::collect_declared_modules(&text, &mut declared);
            lowered.push((path, text.to_ascii_lowercase(), declared));
        }
        Some(Self {
            sources: lowered,
            forwarding_sources,
        })
    }

    /// An invocation in a skipped ancestor cannot itself bring that ancestor
    /// into a running target. Outside the chain the macro may change the name
    /// or shape of the declaration, so keep the possible route.
    fn may_forward_module_outside(&self, chain: &BTreeSet<&Path>) -> bool {
        self.forwarding_sources
            .iter()
            .any(|path| path.as_deref().is_none_or(|path| !chain.contains(path)))
    }

    /// True when a source outside `chain` (the file and its own skipped
    /// ancestry) may compile `file` into another target: it spells the file
    /// name anywhere (a `#[path]`, an `include!`, a manifest `path`, a build
    /// script that writes a mount) or declares a `mod` of the file's stem. A
    /// `.../<dir>/mod.rs` is keyed on its directory (#14962 review): its stem
    /// is `<dir>`, and any spelling of `mod.rs` outside the chain counts as
    /// naming it, since a relative operand (`#[path = "../mod.rs"]`,
    /// `"foo/./mod.rs"`) need not spell `<dir>`. Names compare
    /// ASCII-case-insensitively.
    fn may_mount(&self, file: &Path, chain: &BTreeSet<&Path>) -> bool {
        let Some(name) = file.file_name().and_then(|name| name.to_str()) else {
            return true;
        };
        let stem_source = if name == "mod.rs" {
            file.parent().and_then(Path::file_name)
        } else {
            file.file_stem()
        };
        let Some(stem) = stem_source.and_then(|stem| stem.to_str()) else {
            return true;
        };
        let name = name.to_ascii_lowercase();
        let stem = stem.to_ascii_lowercase();
        self.sources.iter().any(|(path, text, declared)| {
            !path.as_deref().is_some_and(|path| chain.contains(path))
                && (text.contains(name.as_str()) || declared.contains(&stem))
        })
    }
}

/// What one package manifest lets the skip pass ask cargo metadata.
#[derive(Clone, Copy, Debug, Default)]
struct SkipPremise {
    /// The manifest may switch a crate root's unit tests off, so the
    /// metadata probe is worth spending (#14962, #14972).
    probe: bool,
    /// The manifest turns target auto-discovery off (`autolib`, `autobins`,
    /// `autoexamples`, `autotests` or `autobenches` = false), so a
    /// conventional crate root cargo metadata does not list is no target
    /// (#14972).
    undiscovered_roots: bool,
}

/// Cheap premise for the metadata probe (#14962): the package manifest sets
/// `test = false` on its `[lib]` table or on a `[[bin]]` entry, a `[[bin]]`
/// names `required-features`, an `[[example]]` or `[[bench]]` table exists,
/// a `[package]` `auto*` key is false, or an edition 2015 manifest declares
/// a target of a kind it then stops discovering (#14972, #15262, #15877). Sources under
/// `examples/` or `benches/` are a premise on their own (the caller adds
/// it). Anything else cannot
/// skip a crate root's unit tests, so no `cargo metadata` subprocess is
/// spent. An unreadable or unparsable manifest is no premise.
fn manifest_skip_premise(workspace_root: &Path, package_dir: &Path) -> SkipPremise {
    let Some(value) = crate::bounded_input::read_metadata_toml(
        workspace_root.join(package_dir).join("Cargo.toml"),
    )
    .ok()
    .and_then(|text| toml::from_str::<toml::Value>(&text).ok()) else {
        return SkipPremise::default();
    };
    let switches_off = |table: &toml::Value| {
        table.get("test").and_then(toml::Value::as_bool) == Some(false)
            || table.get("required-features").is_some()
    };
    // An explicit `auto* = false`, or edition 2015 with a declared target of
    // that kind (#15877), leaves a conventional root undiscovered.
    let undiscovered_roots = value.get("package").is_some_and(|package| {
        package.get("autolib").and_then(toml::Value::as_bool) == Some(false)
    }) || [
        ("autobins", "bin"),
        ("autoexamples", "example"),
        ("autotests", "test"),
        ("autobenches", "bench"),
    ]
    .iter()
    .any(|(key, section)| !crate::analysis::workspace::discovers_targets(&value, key, section));
    let probe = undiscovered_roots
        || value.get("lib").is_some_and(switches_off)
        || value.get("example").is_some()
        || value.get("bench").is_some()
        || value
            .get("bin")
            .and_then(toml::Value::as_array)
            .is_some_and(|entries| entries.iter().any(switches_off));
    SkipPremise {
        probe,
        undiscovered_roots,
    }
}

/// The files positively outside every crate's module tree (#7426): plain
/// `src/<name>.rs` files of a package whose crate root is indexed, that no
/// resolved module or include edge reaches.
///
/// Fail closed. Nothing is reported while the workspace has an unresolved
/// include, a lexical-fallback file (which emits no `mod` declarations) or a
/// `#[path]` whose target is unknown, because any of those may compile an
/// unlinked file. Only the direct `src/<name>.rs` shape qualifies: its sole
/// possible parent is a crate root's top-level `mod <name>;`, which the
/// producer captures. Deeper files could be declared from an inline module,
/// which the producer does not capture, so they are never reported.
fn files_outside_module_tree(
    index: &RustIndex,
    workspace_root: &Path,
    resolutions: &[(PathBuf, Context, SourceRoleProvenance)],
) -> BTreeSet<PathBuf> {
    if !index.include_limitations.is_empty() {
        return BTreeSet::new();
    }
    let is_poisoning = |facts: &super::index::IndexedFileFacts| {
        facts.used_lexical_fallback
            || facts
                .module_declarations
                .iter()
                .any(|declaration| declaration.path_target == ModulePathTarget::Unknown)
    };
    let mut crate_roots = CrateRoots::default();
    // Files no known edge reaches and that are not a crate root.
    let unlinked: Vec<&PathBuf> = resolutions
        .iter()
        .filter(|(file, context, provenance)| {
            *context == Context::Production
                && provenance.edges.is_empty()
                && provenance.earliest_unresolved_reason.is_none()
                && !index.include_parents.contains_key(file)
                && !crate_roots.is_crate_root(workspace_root, file)
        })
        .map(|(file, _, _)| file)
        .collect();
    // Any lexical-fallback file or unknown `#[path]` may compile an unlinked
    // file, so the `src/<name>.rs` shape needs none anywhere. The stray shape
    // tolerates them in files that are themselves unlinked: a file no root
    // reaches never compiles its own declarations either, and a poisoning
    // file that is a root or is linked blocks every claim.
    let unresolved_composition = index.files.values().any(&is_poisoning);
    let poison_only_in_unlinked = index
        .files
        .iter()
        .filter(|(_, facts)| is_poisoning(facts))
        .all(|(file, _)| unlinked.contains(&file));
    let mut outside = BTreeSet::new();
    let mut stray_candidates = Vec::new();
    let mut manifest_dirs: BTreeSet<PathBuf> = BTreeSet::new();
    for file in unlinked {
        let Some(package_dir) = crate_roots.package_manifest_dir(workspace_root, file) else {
            continue;
        };
        let Ok(relative) = file.strip_prefix(&package_dir) else {
            continue;
        };
        let package_root_indexed = ["lib.rs", "main.rs"].iter().any(|root| {
            index
                .files
                .contains_key(&package_dir.join("src").join(root))
        });
        if !package_root_indexed {
            continue;
        }
        let direct_src_module = matches!(
            relative.components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(dir), Component::Normal(name)]
                if dir.to_str() == Some("src")
                    && name.to_str().is_some_and(|name| name.ends_with(".rs"))
        );
        if direct_src_module {
            if !unresolved_composition
                && !module_may_be_declared_invisibly(index, workspace_root, &package_dir, file)
            {
                outside.insert(file.clone());
            }
        } else if poison_only_in_unlinked && stray_files::is_stray_shape(relative) {
            stray_candidates.push((file.clone(), package_dir.clone()));
            manifest_dirs.insert(package_dir);
        }
    }
    if !stray_candidates.is_empty() {
        // Every package manifest and member manifest may name a target path.
        for file in index.files.keys() {
            if let Some(dir) = crate_roots.package_manifest_dir(workspace_root, file) {
                manifest_dirs.insert(dir);
            }
        }
        manifest_dirs.extend(index.workspace_member_dirs.iter().map(|dir| {
            dir.strip_prefix(workspace_root)
                .unwrap_or(dir)
                .to_path_buf()
        }));
        outside.extend(stray_files::never_compiled(
            index,
            workspace_root,
            &manifest_dirs,
            &stray_candidates,
        ));
    }
    outside
}

/// True when a `mod <stem>` declaration for `file` could exist where the
/// producer does not capture it: anywhere in the package's sources or build
/// script as text (inline or nested modules, macro token trees), or when the
/// package defines or uses a module-generating macro (`macro_rules!`,
/// `cfg_if!`). Unreadable sources count as doubt.
fn module_may_be_declared_invisibly(
    index: &RustIndex,
    workspace_root: &Path,
    package_dir: &Path,
    file: &Path,
) -> bool {
    let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
        return true;
    };
    let declaration = format!("mod {stem}");
    let mut sources: Vec<PathBuf> = index
        .files
        .keys()
        .filter(|candidate| candidate.starts_with(package_dir))
        .cloned()
        .collect();
    sources.push(package_dir.join("build.rs"));
    sources.iter().any(|source| {
        let full = workspace_root.join(source);
        match std::fs::read_to_string(&full) {
            Ok(text) => {
                text.contains("macro_rules!")
                    || text.contains("cfg_if")
                    || text.match_indices(&declaration).any(|(at, _)| {
                        !text[at + declaration.len()..]
                            .chars()
                            .next()
                            .is_some_and(|next| next.is_alphanumeric() || next == '_')
                    })
            }
            Err(_) => source != &package_dir.join("build.rs"),
        }
    })
}

/// Grants the evidence-only `CfgTestModule` role to `Production` functions of
/// the given files, in both the per-file facts and the flat function list.
/// Existing evidence roles (executable tests, promoted expansions, same-file
/// cfg-test helpers) are never demoted: evidence roles union. A test-input
/// generator in a test-only module is test code like its inline twin, not a
/// visible stimulus change of compiled product source (#11545).
fn apply_cfg_test_module_grants(index: &mut RustIndex, granted: &BTreeSet<PathBuf>) {
    let grantable = |role: FunctionSourceRole| {
        matches!(
            role,
            FunctionSourceRole::Production | FunctionSourceRole::TestInputGenerator
        )
    };
    let local = granted
        .iter()
        .filter_map(|file| index.files.get(file))
        .flat_map(|file| file.functions.iter().copied())
        .filter(|&id| grantable(index.function_facts[id].source_role))
        .map(|id| (id, FunctionSourceRole::CfgTestModule))
        .collect::<Vec<_>>();
    let flat = index
        .function_order
        .iter()
        .copied()
        .filter(|&id| {
            let function = &index.function_facts[id];
            granted.contains(&function.file) && grantable(function.source_role)
        })
        .map(|id| (id, FunctionSourceRole::CfgTestModule))
        .collect::<Vec<_>>();
    index.apply_function_roles(flat, local);
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
fn resolved_module_edges(
    index: &RustIndex,
    workspace_root: &Path,
    execution_parents: &mut BTreeMap<PathBuf, Vec<SourceRoleProvenanceEdge>>,
) -> BTreeMap<PathBuf, Result<ModuleEdge, &'static str>> {
    let mut candidates: BTreeMap<PathBuf, BTreeSet<(PathBuf, usize, String, bool)>> =
        BTreeMap::new();
    let mut ambiguous: BTreeSet<PathBuf> = BTreeSet::new();
    // (declaring file, `mod` line, declaration text) -> simple cfg guard, for
    // exclusive-alternative and Kani-only detection below. The declaration
    // text is part of the key because two declarations can share a line
    // (`#[cfg(kani)] mod proofs; mod helpers;`, #13736 review): a guard must
    // never be read for a sibling declaration. A key reached by declarations
    // that disagree holds `None`, which grants nothing.
    let mut guards: GuardMap = BTreeMap::new();
    let containing_directory_files = index
        .files
        .iter()
        .filter(|(_, facts)| !facts.used_lexical_fallback)
        .flat_map(|(file, facts)| {
            let directory = file.parent().unwrap_or_else(|| Path::new(""));
            facts
                .module_declarations
                .iter()
                .filter_map(move |declaration| {
                    let ModulePathTarget::Literal(literal) = &declaration.path_target else {
                        return None;
                    };
                    resolve_relative(directory, literal)
                })
        })
        .filter(|file| index.files.contains_key(file))
        .chain(index.include_parents.keys().cloned())
        .chain(index.include_targets.iter().cloned())
        .collect();
    let resolver = DeclarationResolver {
        index,
        directory_anchors: module_directory_anchors(
            index,
            workspace_root,
            &containing_directory_files,
        ),
    };

    for (file, facts) in &index.files {
        if facts.used_lexical_fallback || facts.module_declarations.is_empty() {
            continue;
        }
        // Both literal paths and default declarations inside include
        // fragments resolve from the physical fragment's directory. The
        // including unit remains an identity/role parent, not a file-search
        // anchor; rebasing it here can select a different physical child.
        for declaration in &facts.module_declarations {
            match resolver.declaration_targets(file, declaration) {
                DeclarationTargets::Unresolvable => {
                    // Dynamic, conditional, or out-of-repository `#[path]`
                    // targets grant nothing; there is no child identity to
                    // attach provenance to.
                }
                DeclarationTargets::Ambiguous(children) => {
                    ambiguous.extend(children);
                }
                DeclarationTargets::Exact(child) => {
                    let text = format!("mod {};", declaration.name);
                    guards
                        .entry((file.clone(), declaration.line, text.clone()))
                        .and_modify(|existing| {
                            if *existing != declaration.cfg_guard {
                                *existing = None;
                            }
                        })
                        .or_insert_with(|| declaration.cfg_guard.clone());
                    candidates.entry(child).or_default().insert((
                        file.clone(),
                        declaration.line,
                        text,
                        declaration.requires_test,
                    ));
                }
            }
        }
    }

    let mut edges = BTreeMap::new();
    // Ambiguity is fail-closed per child: another declaration's multiple
    // default layouts mark the child ambiguous even when this declaration
    // has a single exact owner (two declarations can both reach the same
    // child file). The candidates loop runs first and skips ambiguous
    // children; the moved set then seeds their errors.
    let mut crate_roots = CrateRoots::default();
    for (child, owners) in &candidates {
        if ambiguous.contains(child)
            || (owners.len() > 1
                && !is_shared_integration_test_module(child, owners, &candidates)
                && !are_distinct_target_crate_roots(
                    index,
                    owners,
                    &candidates,
                    &ambiguous,
                    &mut crate_roots,
                    workspace_root,
                )
                && !are_exclusive_cfg_alternatives(owners, &guards))
        {
            edges.insert(child.clone(), Err(REASON_MODULE_AMBIGUOUS_PARENT));
        }
    }
    for child in ambiguous {
        edges
            .entry(child)
            .or_insert_with(|| Err(REASON_MODULE_AMBIGUOUS_PARENT));
    }
    for (child, owners) in candidates {
        // Multiple owners reach here only as the shared integration-test
        // helper layout, whose owners agree on context; the first owner in
        // deterministic order stands for all of them.
        if edges.contains_key(&child) {
            continue;
        }
        // Role agreement admits the edges, not a unique execution scope.
        // Retain every approved parent before selecting the role explanation.
        execution_parents.insert(
            child.clone(),
            owners
                .iter()
                .map(
                    |(parent, line, declaration, requires_test)| SourceRoleProvenanceEdge {
                        kind: SourceRoleProvenanceEdgeKind::Module,
                        parent: parent.clone(),
                        child: child.clone(),
                        declaration: declaration.clone(),
                        line: *line,
                        requires_test: *requires_test,
                    },
                )
                .collect(),
        );
        // Only the exact positive `#[cfg(kani)]` guard: `not(kani)`,
        // `any(kani, ..)`, several cfgs and `cfg_attr` carry no simple guard
        // or a negated one, and keep the child in production. Every admitted
        // owner must carry it: exclusive `cfg(kani)` / `cfg(not(kani))`
        // alternatives compile the child in ordinary builds too.
        let cfg_kani = owners.iter().all(|(parent, line, declaration, _)| {
            guard_of(&guards, parent, *line, declaration)
                .is_some_and(|(predicate, negated)| predicate == "kani" && !negated)
        });
        if let Some((parent, line, declaration, requires_test)) = owners.into_iter().next() {
            edges.insert(
                child,
                Ok(ModuleEdge {
                    parent,
                    declaration,
                    line,
                    requires_test,
                    cfg_kani,
                }),
            );
        }
    }
    edges
}

/// The module name of a `mod <name>;` declaration text.
fn declared_name(declaration: &str) -> &str {
    declaration
        .strip_prefix("mod ")
        .and_then(|rest| rest.strip_suffix(';'))
        .unwrap_or(declaration)
}

/// Simple cfg guards keyed by (declaring file, `mod` line, declaration text).
type GuardMap = BTreeMap<(PathBuf, usize, String), Option<(String, bool)>>;

/// The simple cfg guard of exactly one declaration, if it has one.
fn guard_of<'a>(
    guards: &'a GuardMap,
    file: &Path,
    line: usize,
    declaration: &str,
) -> Option<&'a (String, bool)> {
    guards
        .get(&(file.to_path_buf(), line, declaration.to_string()))
        .and_then(Option::as_ref)
}

/// True when the owners are exactly two declarations in one file, guarded by
/// `#[cfg(X)]` and `#[cfg(not(X))]` for the same simple predicate, with the
/// same declaration text and test requirement. At most one is ever compiled,
/// so they are alternatives of one owner rather than competing parents (the
/// `rust-lang/log` `src/kv/mod.rs` `mod source;` shape). Any other shape,
/// including non-complementary or compound cfgs, stays ambiguous.
fn are_exclusive_cfg_alternatives(
    owners: &BTreeSet<(PathBuf, usize, String, bool)>,
    guards: &GuardMap,
) -> bool {
    let mut iter = owners.iter();
    let (Some(first), Some(second), None) = (iter.next(), iter.next(), iter.next()) else {
        return false;
    };
    if first.0 != second.0 || first.2 != second.2 || first.3 != second.3 {
        return false;
    }
    match (
        guard_of(guards, &first.0, first.1, &first.2),
        guard_of(guards, &second.0, second.1, &second.2),
    ) {
        (Some((left, left_negated)), Some((right, right_negated))) => {
            left == right && left_negated != right_negated
        }
        _ => false,
    }
}

/// The idiomatic shared integration-test helper layout: several Cargo
/// integration-test crate roots (`tests/<name>.rs`) each declare the same
/// `mod common;` for `tests/common/mod.rs` (or `tests/common.rs`). Every
/// integration test is its own crate, so each owner compiles its own copy of
/// the module; the owners are not competing parents. The layout composes as
/// one context only when every owner is a direct `tests/` crate root in the
/// child's `tests/` directory, no owner is itself a module child, and every
/// declaration agrees on name and test requirement. Anything else stays
/// ambiguous and fails closed.
fn is_shared_integration_test_module(
    child: &Path,
    owners: &BTreeSet<(PathBuf, usize, String, bool)>,
    candidates: &BTreeMap<PathBuf, BTreeSet<(PathBuf, usize, String, bool)>>,
) -> bool {
    let Some(tests_dir) = integration_test_module_tests_dir(child) else {
        return false;
    };
    let mut declarations = owners
        .iter()
        .map(|(_, _, declaration, requires_test)| (declaration, requires_test));
    let Some(first) = declarations.next() else {
        return false;
    };
    declarations.all(|other| other == first)
        && owners.iter().all(|(parent, _, _, _)| {
            parent.parent() == Some(tests_dir)
                && parent.extension().and_then(|ext| ext.to_str()) == Some("rs")
                && !candidates.contains_key(parent)
        })
}

/// Several Cargo target crate roots (benches, examples, bins, integration
/// tests, or manifest-declared target paths) each declare the same shared file,
/// typically with `#[path = "../shared.rs"] mod shared;` (the tracing
/// `benches/shared.rs` and crossbeam-channel `benchmarks/message.rs` shapes).
/// Every root is its own compilation unit, so the child is compiled once per
/// crate and the owners never compete inside one module graph. Holds only when
/// every owner is a distinct file that is a Cargo target root, no owner is
/// itself a module child or an `include!` target (an included root can sit
/// behind a `#[cfg(test)]` or other context the owners would no longer share),
/// and all declarations agree on name and test requirement. Non-root owners (two ordinary modules of one crate) stay
/// ambiguous and fail closed.
fn are_distinct_target_crate_roots(
    index: &RustIndex,
    owners: &BTreeSet<(PathBuf, usize, String, bool)>,
    candidates: &BTreeMap<PathBuf, BTreeSet<(PathBuf, usize, String, bool)>>,
    ambiguous: &BTreeSet<PathBuf>,
    crate_roots: &mut CrateRoots,
    workspace_root: &Path,
) -> bool {
    // An unresolved include anywhere may reach an owner through context the
    // index cannot see, so no owner's context is established (the same
    // fail-closed guard as `files_outside_module_tree`).
    if !index.include_limitations.is_empty() {
        return false;
    }
    let mut declarations = owners
        .iter()
        .map(|(_, _, declaration, requires_test)| (declaration, requires_test));
    let Some(first) = declarations.next() else {
        return false;
    };
    let distinct_files = owners
        .iter()
        .map(|(parent, _, _, _)| parent)
        .collect::<BTreeSet<_>>()
        .len();
    declarations.all(|other| other == first)
        && distinct_files == owners.len()
        && owners.iter().all(|(parent, _, _, _)| {
            !candidates.contains_key(parent)
                && !ambiguous.contains(parent)
                && !index.include_parents.contains_key(parent)
                && !index.include_targets.contains(parent)
                && crate_roots.is_crate_root(workspace_root, parent)
        })
}

/// The `tests/` directory owning a `tests/<m>/mod.rs` or `tests/<m>.rs`
/// module file, when the file has that shape.
fn integration_test_module_tests_dir(child: &Path) -> Option<&Path> {
    let parent = child.parent()?;
    let tests_dir = if child.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
        parent.parent()?
    } else {
        parent
    };
    (tests_dir.file_name().and_then(|name| name.to_str()) == Some("tests")).then_some(tests_dir)
}

/// Retain each physical file's possible search directories. A file reached
/// through an ordinary module and a literal path/include can have two anchors.
/// Propagating both through default declarations prevents a descendant from
/// silently disappearing when the single-identity index cannot represent the
/// two occurrences separately. Each file has at most a containing-directory
/// and an ordinary stem-directory anchor, so the work queue remains bounded.
fn module_directory_anchors(
    index: &RustIndex,
    workspace_root: &Path,
    containing_directory_files: &BTreeSet<PathBuf>,
) -> BTreeMap<PathBuf, BTreeSet<PathBuf>> {
    let mut crate_roots = CrateRoots::default();
    let mut pending = index
        .files
        .iter()
        .filter(|(_, facts)| !facts.used_lexical_fallback && !facts.module_declarations.is_empty())
        .map(|(file, _)| {
            let file_name = file.file_name().and_then(|name| name.to_str());
            let directory = if containing_directory_files.contains(file)
                || matches!(file_name, Some("mod.rs" | "lib.rs" | "main.rs"))
                || crate_roots.is_crate_root(workspace_root, file)
            {
                directory_of(file)
            } else {
                ordinary_module_directory(file)
            };
            (file.clone(), directory)
        })
        .collect::<Vec<_>>();
    let mut anchors: BTreeMap<PathBuf, BTreeSet<PathBuf>> = BTreeMap::new();
    while let Some((file, directory)) = pending.pop() {
        let Some(facts) = index.files.get(&file) else {
            continue;
        };
        if facts.used_lexical_fallback || facts.module_declarations.is_empty() {
            continue;
        }
        if !anchors
            .entry(file.clone())
            .or_default()
            .insert(directory.clone())
        {
            continue;
        }
        for declaration in &facts.module_declarations {
            match &declaration.path_target {
                ModulePathTarget::Unknown => {}
                ModulePathTarget::Literal(literal) => {
                    if let Some(child) = resolve_relative(&directory_of(&file), literal)
                        && index.files.contains_key(&child)
                    {
                        pending.push((child.clone(), directory_of(&child)));
                    }
                }
                ModulePathTarget::Default => {
                    for child in default_candidates(&directory, &declaration.name) {
                        if index.files.contains_key(&child) {
                            pending.push((child.clone(), ordinary_module_directory(&child)));
                        }
                    }
                }
            }
        }
    }
    anchors
}

fn ordinary_module_directory(file: &Path) -> PathBuf {
    let directory = directory_of(file);
    if file.file_name().is_some_and(|name| name == "mod.rs") {
        return directory;
    }
    match file.file_stem() {
        Some(stem) => directory.join(stem),
        None => directory,
    }
}

fn default_candidates(directory: &Path, name: &str) -> [PathBuf; 2] {
    let name = name.strip_prefix("r#").unwrap_or(name);
    [
        directory.join(format!("{name}.rs")),
        directory.join(name).join("mod.rs"),
    ]
}

/// Resolves declarations against all directory anchors of their source file.
struct DeclarationResolver<'index> {
    index: &'index RustIndex,
    directory_anchors: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
}

impl DeclarationResolver<'_> {
    /// Resolves one declaration against the index. An exact string-literal
    /// `#[path]` resolves relative to the directory of the file physically
    /// containing the declaration (the Rust reference rule for non-inline
    /// `#[path]` targets, which differs from the default stem-directory
    /// rule). Default resolution checks both `<module-dir>/<name>.rs` and
    /// `<module-dir>/<name>/mod.rs`, retaining ambiguity when both exist.
    fn declaration_targets(
        &self,
        physical_file: &Path,
        declaration: &ModuleDeclarationFact,
    ) -> DeclarationTargets {
        match &declaration.path_target {
            ModulePathTarget::Unknown => DeclarationTargets::Unresolvable,
            ModulePathTarget::Literal(literal) => {
                match resolve_relative(&directory_of(physical_file), literal) {
                    Some(child) if self.index.files.contains_key(&child) => {
                        DeclarationTargets::Exact(child)
                    }
                    _ => DeclarationTargets::Unresolvable,
                }
            }
            ModulePathTarget::Default => {
                let Some(directories) = self.directory_anchors.get(physical_file) else {
                    return DeclarationTargets::Unresolvable;
                };
                let indexed = directories
                    .iter()
                    .flat_map(|directory| default_candidates(directory, &declaration.name))
                    .filter(|candidate| self.index.files.contains_key(candidate))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();
                if directories.len() > 1 && !indexed.is_empty() {
                    // Even if only one candidate exists in this index, the
                    // other occurrence has an unresolved physical context.
                    return DeclarationTargets::Ambiguous(indexed);
                }
                match indexed.as_slice() {
                    [] => DeclarationTargets::Unresolvable,
                    [single] => DeclarationTargets::Exact(single.clone()),
                    // Both default layouts indexed at once: ambiguous.
                    [..] => DeclarationTargets::Ambiguous(indexed.clone()),
                }
            }
        }
    }
}

/// Crate-root identity for default module resolution (#3533 review).
///
/// Rust resolves `mod name;` in a crate root relative to the root file's
/// containing directory regardless of the root's filename, while an ordinary
/// module file resolves children under the directory named after its own
/// stem. Treating every non-`lib.rs`/`main.rs` file as an ordinary module
/// mis-anchors a custom-named crate root (`[lib] path = "source/root.rs"`)
/// and every integration-test, bench, and example target file.
///
/// Crate roots are recognized statically, in two fail-closed ways:
/// - layout autodiscovery, relative to the nearest ancestor directory that
///   contains a `Cargo.toml`: `src/lib.rs`, `src/main.rs`, one file directly
///   under `src/bin/`, and one file directly under `tests/`, `benches/`, or
///   `examples/`;
/// - manifest declaration: any `path = ...` on `[lib]`, `[[bin]]`,
///   `[[test]]`, `[[bench]]`, or `[[example]]` of that manifest.
///
/// A file with no ancestor manifest keeps the stem-directory rule (the
/// status quo for fixture trees and manifest-less scans). Every lookup is
/// memoized for the composition pass, so the filesystem cost is bounded by
/// the distinct declaring files and their bounded ancestor walks.
#[derive(Default)]
struct CrateRoots {
    verdicts: BTreeMap<PathBuf, bool>,
    manifest_dirs: BTreeMap<PathBuf, Option<PathBuf>>,
    declared_roots: BTreeMap<PathBuf, Vec<PathBuf>>,
}

impl CrateRoots {
    fn is_crate_root(&mut self, workspace_root: &Path, file: &Path) -> bool {
        if let Some(verdict) = self.verdicts.get(file) {
            return *verdict;
        }
        let verdict = self.compute_is_crate_root(workspace_root, file);
        self.verdicts.insert(file.to_path_buf(), verdict);
        verdict
    }

    fn compute_is_crate_root(&mut self, workspace_root: &Path, file: &Path) -> bool {
        let Some(package_dir) = self.package_manifest_dir(workspace_root, file) else {
            return false;
        };
        let Ok(relative) = file.strip_prefix(&package_dir) else {
            return false;
        };
        if is_layout_autodiscovered_root(relative) {
            return true;
        }
        let declared = self.manifest_declared_roots(workspace_root, &package_dir);
        declared
            .iter()
            .any(|target| package_dir.join(target) == file)
    }

    /// The nearest ancestor directory (including the workspace root itself)
    /// that contains a `Cargo.toml`, memoized per directory. Walks are
    /// depth-bounded: exotic nesting past the bound keeps the stem rule.
    ///
    /// Every directory visited by a walk is memoized with the walk's resolved
    /// answer, not just with "this directory itself has no manifest" (#3533
    /// review): a cached `None` for an intermediate directory would stop a
    /// later sibling's walk before it reached an ancestor manifest, so two
    /// files in the same directory could disagree on crate-root identity
    /// depending on index iteration order.
    fn package_manifest_dir(&mut self, workspace_root: &Path, file: &Path) -> Option<PathBuf> {
        let mut cursor = file.parent().map(Path::to_path_buf)?;
        let mut visited: Vec<PathBuf> = Vec::new();
        let resolved: Option<PathBuf> = 'walk: {
            for _ in 0..=MAX_CONTEXT_CHAIN_DEPTH {
                if let Some(found) = self.manifest_dirs.get(&cursor) {
                    break 'walk found.clone();
                }
                if workspace_root.join(&cursor).join("Cargo.toml").is_file() {
                    self.manifest_dirs
                        .insert(cursor.clone(), Some(cursor.clone()));
                    break 'walk Some(cursor);
                }
                visited.push(cursor.clone());
                if !cursor.pop() {
                    break 'walk None;
                }
            }
            // Depth bound exhausted: exotic nesting keeps the fail-closed
            // no-manifest answer.
            None
        };
        // Every visited directory shares this walk's nearest-ancestor answer:
        // they are all strict descendants of the directory it resolved at.
        for directory in visited {
            self.manifest_dirs.insert(directory, resolved.clone());
        }
        resolved
    }

    fn manifest_declared_roots(
        &mut self,
        workspace_root: &Path,
        package_dir: &Path,
    ) -> Vec<PathBuf> {
        if let Some(declared) = self.declared_roots.get(package_dir) {
            return declared.clone();
        }
        let declared = crate::bounded_input::read_metadata_toml(
            workspace_root.join(package_dir).join("Cargo.toml"),
        )
        .map(|text| {
            crate::analysis::workspace::declared_crate_root_paths_from_manifest(&text, package_dir)
        })
        .unwrap_or_default();
        self.declared_roots
            .insert(package_dir.to_path_buf(), declared.clone());
        declared
    }
}

/// True when `relative` (a file path relative to its package directory)
/// matches Cargo's autodiscovered target layout: `src/lib.rs`, `src/main.rs`,
/// one file directly under `src/bin/`, or one file directly under `tests/`,
/// `benches/`, or `examples/`. Deeper files are module children, not roots.
fn is_layout_autodiscovered_root(relative: &Path) -> bool {
    let name_of = |component: &std::path::Component| match component {
        Component::Normal(name) => Some(name.to_string_lossy().to_string()),
        _ => None,
    };
    let components: Vec<_> = relative.components().collect();
    match components.as_slice() {
        [first, file_name] => {
            let (Some(dir), Some(file_name)) = (name_of(first), name_of(file_name)) else {
                return false;
            };
            match dir.as_str() {
                "src" => matches!(file_name.as_str(), "lib.rs" | "main.rs"),
                "tests" | "benches" | "examples" => file_name.ends_with(".rs"),
                _ => false,
            }
        }
        [first, second, file_name] => {
            let (Some(first), Some(second), Some(file_name)) =
                (name_of(first), name_of(second), name_of(file_name))
            else {
                return false;
            };
            first == "src" && second == "bin" && file_name.ends_with(".rs")
        }
        _ => false,
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
    include_parents: &'index BTreeMap<PathBuf, ResolvedIncludeParent>,
    memo: BTreeMap<PathBuf, (Context, SourceRoleProvenance)>,
    kani_memo: BTreeMap<PathBuf, bool>,
    invisible_owners: InvisibleOwners<'index>,
}

impl ContextResolver<'_> {
    /// True when `file` is compiled only under Kani (#13736): its resolved
    /// module edge is an exact `#[cfg(kani)]` declaration, or its module or
    /// include parent is itself Kani-only. A file both declared and included
    /// is Kani-only only when both contexts are. Unresolved, cyclic or
    /// over-deep chains, and module edges whose child could have an owner
    /// the producer does not capture (`InvisibleOwners`), answer `false`,
    /// which keeps production probing.
    fn cfg_kani_only(&mut self, file: &Path, depth: usize) -> bool {
        if let Some(&cached) = self.kani_memo.get(file) {
            return cached;
        }
        if depth > MAX_CONTEXT_CHAIN_DEPTH {
            return false;
        }
        // Seed `false` so a cycle through this file answers fail-closed.
        self.kani_memo.insert(file.to_path_buf(), false);
        let module_edge = match self.module_edges.get(file) {
            Some(Ok(edge)) => Some((
                edge.cfg_kani,
                edge.parent.clone(),
                declared_name(&edge.declaration).to_string(),
            )),
            Some(Err(_)) => {
                return false;
            }
            None => None,
        };
        let include_parent = self
            .include_parents
            .get(file)
            .map(|include| include.parent.clone());
        // Every module edge on the way must have no owner the producer could
        // not see: an inline, macro-generated or unindexed `mod name;` could
        // compile the file in an ordinary build.
        let module_kani = module_edge.map(|(gated, parent, name)| {
            (gated || self.cfg_kani_only(&parent, depth + 1))
                && self.invisible_owners.all_owners_visible(file, &name)
        });
        let include_kani = include_parent.map(|parent| self.cfg_kani_only(&parent, depth + 1));
        let kani = match (module_kani, include_kani) {
            (Some(module), Some(include)) => module && include,
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => false,
        };
        self.kani_memo.insert(file.to_path_buf(), kani);
        kani
    }

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
                    outside_module_tree: false,
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
                    outside_module_tree: false,
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
            (None, Some(include)) => {
                let (parent_context, mut provenance) =
                    self.resolve(&include.parent, visiting, depth + 1);
                let parent_unresolved = parent_context.unresolved_reason();
                // A cfg-gated include invocation only exists in test builds,
                // so the fragment's content is test-only regardless of the
                // parent context (#3533 review).
                let context = if include.requires_test {
                    Context::RequiresTest
                } else {
                    parent_context
                };
                let requires_test =
                    include.requires_test || context.requires_test().unwrap_or(false);
                provenance.edges.push(SourceRoleProvenanceEdge {
                    kind: SourceRoleProvenanceEdgeKind::Include,
                    parent: include.parent.clone(),
                    child: file.to_path_buf(),
                    declaration: "include!".to_string(),
                    // The include pass keeps only the parent map, not the
                    // directive line; the edge is exactly identified by the
                    // parent/child pair.
                    line: 0,
                    requires_test,
                });
                // The gate resolves the verdict, but an unresolved edge in
                // the ancestry still happened: keep naming it.
                if let Some(reason) = parent_unresolved
                    && provenance.earliest_unresolved_reason.is_none()
                {
                    provenance.earliest_unresolved_reason = Some(reason);
                }
                (context, provenance)
            }
            (Some(Ok(edge)), Some(include)) => {
                // Dual contextual ownership: compose only when both sides
                // agree on the verdict, naming the conflict otherwise (law 10).
                let (module_context, module_provenance) =
                    self.resolve(&edge.parent, visiting, depth + 1);
                let (include_context, include_provenance) =
                    self.resolve(&include.parent, visiting, depth + 1);
                let module_verdict = if edge.requires_test {
                    Some(true)
                } else {
                    module_context.requires_test()
                };
                // A cfg-gated invocation is test-only on its own (#3533
                // review); an unconditional include inherits its context.
                let include_verdict = if include.requires_test {
                    Some(true)
                } else {
                    include_context.requires_test()
                };
                let mut provenance = module_provenance;
                let mut edges = include_provenance.edges;
                edges.push(SourceRoleProvenanceEdge {
                    kind: SourceRoleProvenanceEdgeKind::Include,
                    parent: include.parent.clone(),
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
                        // Earliest unresolved wins across BOTH chains: the
                        // include chain's reason must survive the merge.
                        if provenance.earliest_unresolved_reason.is_none() {
                            provenance.earliest_unresolved_reason =
                                include_provenance.earliest_unresolved_reason;
                        }
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
    use crate::analysis::facts::build_index_from_loaded_files_with_cache_and_test_harnesses;
    use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn mount_operand_doubt_needs_a_dynamic_mount_operand() {
        // #14962 review a1c: only a metavariable or non-literal operand in a
        // mount position is doubt; an expression metavariable is not.
        for doubt in [
            "include!(concat!(env!(\"OUT_DIR\"), \"/m.rs\"));",
            "include! ( x );",
            "macro_rules! m { ($n:ident) => { mod $n; }; }",
            "macro_rules! m { ($p:literal) => { #[path = $p] mod x; }; }",
            "include!{concat!(env!(\"OUT_DIR\"), \"/m.rs\")}",
            "include![concat!(env!(\"OUT_DIR\"), \"/m.rs\")];",
            "include!(/* generated */ concat!(env!(\"OUT_DIR\"), \"/m.rs\"));",
            "macro_rules! m { ($n:ident) => { mod /* m */ $n; }; }",
            "macro_rules! m { ($p:literal) => { #[path = /* x */ $p] mod x; }; }",
            "macro_rules! m { ($n:ident) => { mod // m\n $n; }; }",
            // Review round 5: a non-`.rs` mount file (n5, n5p).
            "include!(\"gen.inc\");",
            "#[path = \"m.txt\"]\npub mod m;",
            "#![cfg_attr(x, path = \"m.txt\")]",
            // A string holding `//` or `/*` must not swallow the mount (n2).
            "macro_rules! m { ($n:ident) => { const U: &str = \"http://x\"; mod /* c */ $n; }; }",
            "macro_rules! m { ($n:ident) => { const U: &str = r#\"/*\"#; mod/*c*/$n; }; }",
            // Any `mod`, `include` or attribute `path` in a macro body.
            "macro_rules! m { ($f:literal) => { include!($f); }; }",
            "macro_rules! m { ($include:ident, $f:literal) => { $include!($f); }; }",
            "macro_rules! m [ ($n:ident) => { pub mod $n {} } ];",
            "macro_rules! m { ($a:meta) => { #[$a] mod inner; }; }",
            "macro m($n:ident) { mod $n; }",
            "macro_rules! m { ($n:ident) => { r#mod $n; }; }",
            // Review round 6 (n7): an aliased or re-exported `include`.
            "use std::include as inc;\ninc!(concat!(env!(\"OUT_DIR\"), \"/plain.rs\"));",
            "use core::include;",
            "pub use ::std::{include as inc};",
            // An escaped literal decodes to a name no scan matches.
            "include /* c */ !(\"../../a/src/\\u{73}hared.rs\");",
            "#[path = \"../../a/src/\\x73hared.rs\"]\nmod m;",
        ] {
            assert!(may_build_mount_operands(doubt), "{doubt}");
        }
        for plain in [
            "macro_rules! twice { ($e:expr) => { $e * 2 }; }",
            "macro_rules! m { ($mod:ident) => { $mod::One }; }\nmod tests {}",
            "macro_rules! m { ($include:expr) => { $include }; }",
            "include!(\"gen.rs\");",
            "include!{\"gen.rs\"}",
            "include![\"gen.rs\"];",
            "mod tests;\nfn f() -> u32 { 1 }",
            "#[path = \"gen.rs\"]\nmod g;",
            "let path = \"dir/m.txt\";",
            "const U: &str = \"http://x\"; mod tests;",
            "const S: &str = \"macro_rules! m { mod $n }\";",
            "let quote = '\"'; let l: &'static str = \"\"; mod tests;",
            "// macro_rules! m { ($n:ident) => { mod $n; }; }\nfn f() {}",
            "macro_rules! p { () => { std::path::Path::new(\"x\") }; }",
            "macro_rules! v { ($e:expr) => { vec![$e] }; }\nfn f() { let _ = v!(1); }",
        ] {
            assert!(!may_build_mount_operands(plain), "{plain}");
        }
    }

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
            .functions()
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
            .files()
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
        index.tests().iter().map(|test| test.name.clone()).collect()
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
        let provenance = &index
            .files()
            .at(Path::new("src/tests.rs"))
            .data()
            .role_provenance;
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
        let parent_provenance = &index
            .files()
            .at(Path::new("src/lib.rs"))
            .data()
            .role_provenance;
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
        let provenance = &index
            .files()
            .at(Path::new("src/child.rs"))
            .data()
            .role_provenance;
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
        let provenance = &index
            .files()
            .at(Path::new("src/ordinary/tests.rs"))
            .data()
            .role_provenance;
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

    /// `#[r#path]` is the same module-path attribute as `#[path]`. Default-layout
    /// decoys stay production; the redirected file matches the plain-`#[path]`
    /// control's composed role and parent chain (#7161).
    #[test]
    fn raw_identifier_path_attribute_module_matches_plain_path_control() -> Result<(), String> {
        let root = temp_dir("raw-path-attribute")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)]\n#[path = \"support/plain.rs\"]\nmod plain;\n\
                 #[cfg(test)]\n#[r#path = \"support/raw.rs\"]\nmod raw;\n",
            )?,
            write(
                &root,
                "src/support/plain.rs",
                "pub fn plain_helper() -> i32 { 1 }\n",
            )?,
            write(
                &root,
                "src/support/raw.rs",
                "pub fn raw_helper() -> i32 { 1 }\n",
            )?,
            write(&root, "src/plain.rs", "pub fn plain_decoy() -> i32 { 0 }\n")?,
            write(&root, "src/raw.rs", "pub fn raw_decoy() -> i32 { 0 }\n")?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/support/plain.rs", "plain_helper")?,
            FunctionSourceRole::CfgTestModule,
            "fixture: the plain #[path] control must compose"
        );
        assert_eq!(
            role_of(&index, "src/support/raw.rs", "raw_helper")?,
            FunctionSourceRole::CfgTestModule,
            "#[r#path] must compose the redirected file, not default-layout src/raw.rs"
        );
        assert_eq!(
            role_of(&index, "src/plain.rs", "plain_decoy")?,
            FunctionSourceRole::Production
        );
        assert_eq!(
            role_of(&index, "src/raw.rs", "raw_decoy")?,
            FunctionSourceRole::Production,
            "default-layout decoy must not receive the composed grant"
        );

        let plain_provenance = &index
            .files()
            .at(Path::new("src/support/plain.rs"))
            .data()
            .role_provenance;
        let raw_provenance = &index
            .files()
            .at(Path::new("src/support/raw.rs"))
            .data()
            .role_provenance;
        assert!(plain_provenance.earliest_unresolved_reason.is_none());
        assert_eq!(
            raw_provenance.earliest_unresolved_reason,
            plain_provenance.earliest_unresolved_reason
        );
        assert_eq!(plain_provenance.edges.len(), 1, "plain control chain");
        assert_eq!(
            raw_provenance.edges.len(),
            plain_provenance.edges.len(),
            "r#path chain length must match the plain #[path] control"
        );
        let plain_edge = &plain_provenance.edges[0];
        let raw_edge = &raw_provenance.edges[0];
        assert_eq!(plain_edge.kind, SourceRoleProvenanceEdgeKind::Module);
        assert_eq!(raw_edge.kind, plain_edge.kind);
        assert_eq!(plain_edge.parent, Path::new("src/lib.rs"));
        assert_eq!(raw_edge.parent, plain_edge.parent);
        assert_eq!(plain_edge.child, Path::new("src/support/plain.rs"));
        assert_eq!(raw_edge.child, Path::new("src/support/raw.rs"));
        assert!(plain_edge.requires_test);
        assert_eq!(raw_edge.requires_test, plain_edge.requires_test);
        assert_eq!(plain_edge.declaration, "mod plain;");
        assert_eq!(raw_edge.declaration, "mod raw;");
        Ok(())
    }

    #[test]
    fn path_selected_parent_resolves_default_children_from_its_directory() -> Result<(), String> {
        let root = temp_dir("path-selected-child")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)] #[path = \"nested/renamed.rs\"] mod nested;",
            )?,
            write(&root, "src/nested/renamed.rs", "mod child;")?,
            write(&root, "src/nested/child.rs", "pub fn correct_child() {}")?,
            write(
                &root,
                "src/nested/renamed/child.rs",
                "pub fn wrong_child() {}",
            )?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)?;
        assert_eq!(
            role_of(&index, "src/nested/child.rs", "correct_child")?,
            FunctionSourceRole::CfgTestModule
        );
        assert_eq!(
            role_of(&index, "src/nested/renamed/child.rs", "wrong_child")?,
            FunctionSourceRole::Production
        );
        Ok(())
    }

    #[test]
    fn included_fragment_resolves_default_children_from_its_physical_directory()
    -> Result<(), String> {
        let root = temp_dir("included-default-child")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)] include!(\"fragments/body.rs\");",
            )?,
            write(&root, "src/fragments/body.rs", "mod child;")?,
            write(&root, "src/fragments/child.rs", "pub fn correct_child() {}")?,
            write(&root, "src/child.rs", "pub fn wrong_child() {}")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)?;
        assert_eq!(
            role_of(&index, "src/fragments/child.rs", "correct_child")?,
            FunctionSourceRole::CfgTestModule
        );
        assert_eq!(
            role_of(&index, "src/child.rs", "wrong_child")?,
            FunctionSourceRole::Production
        );
        Ok(())
    }

    fn check_mixed_directory_children(label: &str, root_source: &str) -> Result<(), String> {
        let root = temp_dir(label)?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", root_source)?,
            write(&root, "src/alias.rs", "mod child;")?,
            write(&root, "src/alias/child.rs", "pub fn ordinary_child() {}")?,
            write(&root, "src/child.rs", "pub fn containing_child() {}")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)?;
        check_ambiguous_children(
            &index,
            &[
                ("src/alias/child.rs", "ordinary_child"),
                ("src/child.rs", "containing_child"),
            ],
        )
    }

    fn check_ambiguous_children(
        index: &RustIndex,
        children: &[(&str, &str)],
    ) -> Result<(), String> {
        for &(file, function) in children {
            if role_of(index, file, function)? != FunctionSourceRole::Production {
                return Err(format!("ambiguous child {file} received an evidence role"));
            }
            let facts = index
                .files()
                .get(Path::new(file))
                .ok_or_else(|| format!("missing child facts for {file}"))?;
            if facts.role_provenance.earliest_unresolved_reason.as_deref()
                != Some(REASON_MODULE_AMBIGUOUS_PARENT)
            {
                return Err(format!(
                    "both directory occurrences must stay ambiguous: {file}: {:?}",
                    facts.role_provenance.earliest_unresolved_reason
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn ordinary_and_path_occurrences_keep_both_children_ambiguous() -> Result<(), String> {
        check_mixed_directory_children(
            "ordinary-and-path",
            "#[cfg(test)] mod alias;\n#[cfg(test)] #[path = \"alias.rs\"] mod renamed;",
        )
    }

    #[test]
    fn ordinary_and_include_occurrences_keep_both_children_ambiguous() -> Result<(), String> {
        check_mixed_directory_children(
            "ordinary-and-include",
            "mod alias;\n#[cfg(test)] include!(\"alias.rs\");",
        )
    }

    #[test]
    fn mixed_directory_anchors_do_not_select_the_only_indexed_layout() -> Result<(), String> {
        let root = temp_dir("mixed-anchor-missing-layout")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)] mod alias;\n#[cfg(test)] include!(\"alias.rs\");",
            )?,
            write(&root, "src/alias.rs", "mod child;")?,
            write(&root, "src/child.rs", "pub fn containing_child() {}")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)?;
        check_ambiguous_children(&index, &[("src/child.rs", "containing_child")])
    }

    #[test]
    fn mixed_directory_anchors_propagate_through_default_children() -> Result<(), String> {
        let root = temp_dir("mixed-anchor-grandchildren")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)] mod outer;\n\
                 #[cfg(test)] #[path = \"outer.rs\"] mod renamed;\n\
                 #[cfg(test)] #[path = \"outer/alias.rs\"] mod alias_path;",
            )?,
            write(&root, "src/outer.rs", "mod alias;")?,
            write(&root, "src/alias.rs", "pub fn other_layout() {}")?,
            write(&root, "src/outer/alias.rs", "mod child;")?,
            write(
                &root,
                "src/outer/alias/child.rs",
                "pub fn ordinary_child() {}",
            )?,
            write(&root, "src/outer/child.rs", "pub fn containing_child() {}")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)?;
        check_ambiguous_children(
            &index,
            &[
                ("src/outer/alias/child.rs", "ordinary_child"),
                ("src/outer/child.rs", "containing_child"),
            ],
        )
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
        let provenance = &index
            .files()
            .at(Path::new("src/fragment.rs"))
            .data()
            .role_provenance;
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

    /// A `#[cfg(test)] include!(...)` invocation only exists in test builds,
    /// so the fragment's content is test-only regardless of the including
    /// file's own context (#3533 review). Without composing the invocation's
    /// gate, the fragment's helpers re-enter the production inventory.
    #[test]
    fn cfg_gated_include_composes_test_context() -> Result<(), String> {
        let root = temp_dir("gated-include")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "pub fn production_control() -> i32 { 1 }\n\n#[cfg(test)]\ninclude!(\"fragment.rs\");\n",
            )?,
            write(
                &root,
                "src/fragment.rs",
                "pub fn gated_fragment_helper() -> i32 { 3 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/lib.rs", "production_control")?,
            FunctionSourceRole::Production
        );
        assert_eq!(
            role_of(&index, "src/fragment.rs", "gated_fragment_helper")?,
            FunctionSourceRole::CfgTestModule,
            "the fragment of a test-gated invocation is test-only even beside a production unit"
        );
        let provenance = &index
            .files()
            .at(Path::new("src/fragment.rs"))
            .data()
            .role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        assert_eq!(provenance.edges.len(), 1);
        assert_eq!(
            provenance.edges[0].kind,
            SourceRoleProvenanceEdgeKind::Include
        );
        assert_eq!(provenance.edges[0].parent, Path::new("src/lib.rs"));
        assert!(
            provenance.edges[0].requires_test,
            "the include edge records the invocation's test requirement"
        );
        Ok(())
    }

    /// One file whose include invocations disagree on the cfg-test
    /// requirement (`include!` next to `#[cfg(test)] include!` of the same
    /// fragment) resolves to no single context: the child keeps its
    /// standalone roles and the conflict is named (#3533).
    #[test]
    fn conflicting_include_requirements_fail_closed() -> Result<(), String> {
        let root = temp_dir("conflicting-include")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "include!(\"fragment.rs\");\n\n#[cfg(test)]\ninclude!(\"fragment.rs\");\n",
            )?,
            write(
                &root,
                "src/fragment.rs",
                "pub fn contested_fragment_helper() -> i32 { 1 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(
            !index
                .include_parents
                .contains_key(Path::new("src/fragment.rs")),
            "no single context exists for the contested fragment"
        );
        assert!(
            index
                .include_limitations
                .iter()
                .any(|limitation| limitation.reason_code
                    == "rust_include_conflicting_cfg_requirement"),
            "the conflict must be named on the include limitations"
        );
        assert_eq!(
            role_of(&index, "src/fragment.rs", "contested_fragment_helper")?,
            FunctionSourceRole::Production,
            "the contested fragment keeps its standalone roles"
        );
        Ok(())
    }

    /// A conflicted include still has a physical source location for default
    /// module lookup. The conflict must suppress contextual grants, while
    /// `mod child;` in the fragment resolves beside the fragment (`src`), not
    /// under the fragment's stem directory (`src/fragment`).
    #[test]
    fn conflicted_include_preserves_physical_anchor_without_role_grant() -> Result<(), String> {
        let root = temp_dir("conflicting-include-anchor")?;
        write_manifest(&root)?;
        let mut files = vec![
            write(
                &root,
                "src/lib.rs",
                "include!(\"fragment.rs\");\n\n#[cfg(test)]\ninclude!(\"fragment.rs\");\n",
            )?,
            write(
                &root,
                "src/fragment.rs",
                "#[cfg(test)]\nmod child;\npub fn fragment_helper() -> i32 { 3 }\n",
            )?,
            write(
                &root,
                "src/child.rs",
                "pub fn physically_anchored_child() -> i32 { 1 }\n",
            )?,
            write(
                &root,
                "src/fragment/child.rs",
                "pub fn misleading_stem_child() -> i32 { 2 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(
            index.include_parents.is_empty(),
            "conflicting cfg requirements must retain fail-closed contextual ownership"
        );
        assert!(
            index.include_targets.contains(Path::new("src/fragment.rs")),
            "physical include discovery must survive contextual conflict"
        );
        assert_eq!(
            role_of(&index, "src/fragment.rs", "fragment_helper")?,
            FunctionSourceRole::Production,
            "physical discovery must not resolve the fragment's contextual role conflict"
        );
        assert!(
            index
                .files()
                .get(Path::new("src/child.rs"))
                .ok_or("physical child facts missing")?
                .role_provenance
                .edges
                .iter()
                .any(|edge| edge.kind == SourceRoleProvenanceEdgeKind::Module
                    && edge.parent == Path::new("src/fragment.rs")),
            "default module must resolve beside the physically included fragment"
        );
        assert!(
            index
                .files()
                .get(Path::new("src/fragment/child.rs"))
                .ok_or("stem child facts missing")?
                .role_provenance
                .edges
                .is_empty(),
            "the fragment stem directory must not be selected as the anchor"
        );
        assert_eq!(
            role_of(&index, "src/child.rs", "physically_anchored_child")?,
            FunctionSourceRole::CfgTestModule,
            "the module's own cfg(test) edge grants its child without resolving the conflicting include"
        );
        assert_eq!(
            role_of(&index, "src/fragment/child.rs", "misleading_stem_child")?,
            FunctionSourceRole::Production,
            "the misleading stem child must not gain an evidence role"
        );

        // Rust does not fall back to the stem layout when the physical child
        // is absent. Keep the misleading candidate in the full source index.
        let physical_child = root.join("src/child.rs");
        fs::remove_file(&physical_child).map_err(|error| error.to_string())?;
        files.retain(|file| file != Path::new("src/child.rs"));
        let missing = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(
            role_of(&missing, "src/fragment/child.rs", "misleading_stem_child")?,
            FunctionSourceRole::Production,
            "a missing physical child must not grant the stem candidate a role"
        );
        assert!(
            missing
                .files
                .get(Path::new("src/fragment/child.rs"))
                .ok_or("stem child facts missing after physical child removal")?
                .role_provenance
                .edges
                .is_empty(),
            "a missing physical child must not create a stem-directory edge"
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
        let provenance = &index
            .files()
            .at(Path::new("src/shared.rs"))
            .data()
            .role_provenance;
        assert_eq!(
            provenance.earliest_unresolved_reason.as_deref(),
            Some(REASON_MODULE_AMBIGUOUS_PARENT),
            "the earliest unresolved edge must be named"
        );
        Ok(())
    }

    fn cfg_pair_reason(first: &str, second: &str, tag: &str) -> Result<Option<String>, String> {
        let root = temp_dir(tag)?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                &format!("{first}\nmod source;\n{second}\npub mod source;\n"),
            )?,
            write(&root, "src/source.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        Ok(index
            .files()
            .at(Path::new("src/source.rs"))
            .data()
            .role_provenance
            .earliest_unresolved_reason
            .clone())
    }

    /// `#[cfg(X)] mod m;` / `#[cfg(not(X))] pub mod m;` are alternatives of
    /// one module (rust-lang/log `src/kv/mod.rs`), not competing parents.
    #[test]
    fn complementary_cfg_module_declarations_are_one_owner() -> Result<(), String> {
        for (positive, negative) in [
            (
                "#[cfg(feature = \"kv_unstable\")]",
                "#[cfg(not(feature = \"kv_unstable\"))]",
            ),
            ("#[cfg(unix)]", "#[cfg( not( unix ) )]"),
        ] {
            assert_eq!(cfg_pair_reason(positive, negative, "cfg-alt")?, None);
            assert_eq!(cfg_pair_reason(negative, positive, "cfg-alt-rev")?, None);
        }
        Ok(())
    }

    /// Punctuation inside a quoted cfg value is data, not a compound
    /// predicate; a real compound predicate still fails closed.
    #[test]
    fn quoted_cfg_value_punctuation_is_not_a_compound_predicate() -> Result<(), String> {
        assert_eq!(
            cfg_pair_reason(
                "#[cfg(mode = \"a,b)\")]",
                "#[cfg(not(mode = \"a,b)\"))]",
                "cfg-quoted"
            )?,
            None
        );
        assert_eq!(
            cfg_pair_reason("#[cfg(all(a, b))]", "#[cfg(not(all(a, b)))]", "cfg-real")?.as_deref(),
            Some(REASON_MODULE_AMBIGUOUS_PARENT)
        );
        Ok(())
    }

    /// A backslash escape in a cfg value fails closed (no guard, so the pair
    /// stays ambiguous).
    #[test]
    fn escaped_quote_in_cfg_value_fails_closed() -> Result<(), String> {
        assert_eq!(
            cfg_pair_reason(
                "#[cfg(mode = \"a\\\" b\")]",
                "#[cfg(not(mode = \"a\\\" b\"))]",
                "cfg-escape"
            )?
            .as_deref(),
            Some(REASON_MODULE_AMBIGUOUS_PARENT)
        );
        Ok(())
    }

    /// Cached facts written before `cfg_guard` existed still decode.
    #[test]
    fn module_declaration_without_cfg_guard_still_deserializes() -> Result<(), String> {
        let fact: crate::analysis::facts::ModuleDeclarationFact = serde_json::from_str(
            r#"{"name":"m","line":3,"path_target":{"kind":"default"},"requires_test":false}"#,
        )
        .map_err(|error| error.to_string())?;
        assert_eq!(fact.cfg_guard, None);
        assert_eq!(fact.name, "m");
        Ok(())
    }

    /// `cfg(test)` / `cfg(not(test))` stays ambiguous by design: merging
    /// would grant a test-only role to a child that also compiles in
    /// production.
    #[test]
    fn cfg_test_pair_stays_ambiguous() -> Result<(), String> {
        assert_eq!(
            cfg_pair_reason("#[cfg(test)]", "#[cfg(not(test))]", "cfg-test-pair")?.as_deref(),
            Some(REASON_MODULE_AMBIGUOUS_PARENT)
        );
        Ok(())
    }

    /// Other attributes cannot merge distinct module identities: a
    /// `#[path]` that redirects one alternative to a different file leaves
    /// each child with a single owner, and alternatives with different
    /// declared names are not merged even under complementary guards.
    #[test]
    fn other_attributes_do_not_merge_distinct_module_identities() -> Result<(), String> {
        let root = temp_dir("cfg-alt-path")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(unix)]\n#[path = \"other.rs\"]\nmod source;\n#[cfg(not(unix))]\nmod source;\n",
            )?,
            write(&root, "src/source.rs", "pub fn a() -> i32 { 1 }\n")?,
            write(&root, "src/other.rs", "pub fn b() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        for child in ["src/source.rs", "src/other.rs"] {
            let provenance = &index.files().at(Path::new(child)).data().role_provenance;
            assert_ne!(
                provenance.earliest_unresolved_reason.as_deref(),
                Some(REASON_MODULE_AMBIGUOUS_PARENT),
                "{child} has one owner"
            );
        }
        // Different declared names redirected to one file: not alternatives.
        let root = temp_dir("cfg-alt-names")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(unix)]\n#[path = \"shared.rs\"]\nmod a;\n#[cfg(not(unix))]\n#[path = \"shared.rs\"]\nmod b;\n",
            )?,
            write(&root, "src/shared.rs", "pub fn c() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(
            index
                .files()
                .at(Path::new("src/shared.rs"))
                .data()
                .role_provenance
                .earliest_unresolved_reason
                .as_deref(),
            Some(REASON_MODULE_AMBIGUOUS_PARENT)
        );
        Ok(())
    }

    /// Negative: non-complementary, compound or unguarded pairs stay
    /// ambiguous (fail closed).
    #[test]
    fn non_complementary_cfg_module_declarations_stay_ambiguous() -> Result<(), String> {
        for (first, second) in [
            ("#[cfg(unix)]", "#[cfg(windows)]"),
            ("#[cfg(unix)]", "#[cfg(unix)]"),
            ("#[cfg(unix)]", "#[cfg(not(windows))]"),
            ("#[cfg(unix)]", ""),
            ("#[cfg(all(unix, a))]", "#[cfg(not(all(unix, a)))]"),
            ("#[cfg(feature = \"a\")]", "#[cfg(not(feature = \"b\"))]"),
            ("#[cfg(unix)]", "#[cfg(not(unix))] #[cfg(a)]"),
        ] {
            assert_eq!(
                cfg_pair_reason(first, second, "cfg-amb")?.as_deref(),
                Some(REASON_MODULE_AMBIGUOUS_PARENT),
                "{first} / {second}"
            );
        }
        Ok(())
    }

    /// The idiomatic `tests/common/mod.rs` layout, declared by several
    /// integration-test crate roots, is not an ambiguous parent: each test is
    /// its own crate. Owners that disagree on test requirement stay ambiguous.
    #[test]
    fn shared_integration_test_common_module_is_not_ambiguous() -> Result<(), String> {
        let root = temp_dir("shared-tests-common")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(&root, "tests/alpha.rs", "mod common;\n")?,
            write(&root, "tests/beta.rs", "mod common;\n")?,
            write(
                &root,
                "tests/common/mod.rs",
                "pub fn shared_helper() -> i32 { 1 }\n",
            )?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        let provenance = &index
            .files()
            .at(Path::new("tests/common/mod.rs"))
            .data()
            .role_provenance;
        assert_eq!(provenance.earliest_unresolved_reason, None);
        assert_eq!(provenance.edges.len(), 1, "{:?}", provenance.edges);
        assert_eq!(provenance.edges[0].parent, PathBuf::from("tests/alpha.rs"));

        // Disagreeing owners are still ambiguous.
        let root = temp_dir("shared-tests-common-conflict")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(&root, "tests/alpha.rs", "mod common;\n")?,
            write(&root, "tests/beta.rs", "#[cfg(test)]\nmod common;\n")?,
            write(
                &root,
                "tests/common/mod.rs",
                "pub fn shared_helper() -> i32 { 1 }\n",
            )?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(
            index
                .files()
                .at(Path::new("tests/common/mod.rs"))
                .role_provenance
                .earliest_unresolved_reason
                .as_deref(),
            Some(REASON_MODULE_AMBIGUOUS_PARENT)
        );
        Ok(())
    }

    fn ambiguity_reason(index: &RustIndex, file: &str) -> Option<String> {
        index
            .files()
            .at(Path::new(file))
            .role_provenance
            .earliest_unresolved_reason
            .clone()
    }

    /// Bench and example targets are separate crates, so several of them
    /// including one shared file through `#[path]` (tracing `benches/shared.rs`,
    /// crossbeam-channel `benchmarks/message.rs`) is not an ambiguous parent.
    #[test]
    fn shared_bench_example_and_custom_target_includes_are_not_ambiguous() -> Result<(), String> {
        let root = temp_dir("shared-bench-path")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(
                &root,
                "benches/a.rs",
                "#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(
                &root,
                "benches/b.rs",
                "#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(
                &root,
                "benches/shared.rs",
                "pub fn bench_helper() -> i32 { 1 }\n",
            )?,
            write(
                &root,
                "examples/x.rs",
                "#[path = \"../benches/shared.rs\"]\nmod shared;\n",
            )?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), None);
        assert_eq!(
            role_of(&index, "benches/shared.rs", "bench_helper")?,
            FunctionSourceRole::Production
        );
        let provenance = &index
            .files()
            .at(Path::new("benches/shared.rs"))
            .data()
            .role_provenance;
        assert_eq!(provenance.edges.len(), 1, "{:?}", provenance.edges);
        assert_eq!(provenance.edges[0].parent, PathBuf::from("benches/a.rs"));

        // A manifest-declared target outside the autodiscovered layout is
        // also a crate root.
        let root = temp_dir("shared-custom-target-path")?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='role-composition'\nversion='0.1.0'\nedition='2024'\n\n[[bench]]\nname='one'\npath='benchmarks/one.rs'\nharness=false\n\n[[bench]]\nname='two'\npath='benchmarks/two.rs'\nharness=false\n",
        )
        .map_err(|error| error.to_string())?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(&root, "benchmarks/one.rs", "mod message;\n")?,
            write(&root, "benchmarks/two.rs", "mod message;\n")?,
            write(
                &root,
                "benchmarks/message.rs",
                "pub fn msg() -> i32 { 1 }\n",
            )?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benchmarks/message.rs"), None);
        assert_eq!(
            role_of(&index, "benchmarks/message.rs", "msg")?,
            FunctionSourceRole::Production
        );
        Ok(())
    }

    /// Fail-closed counterparts: two ordinary modules of one crate, a root
    /// mixed with a non-root module, and disagreeing declarations all stay
    /// ambiguous.
    #[test]
    fn shared_file_with_non_root_or_disagreeing_owners_stays_ambiguous() -> Result<(), String> {
        let ambiguous = Some(REASON_MODULE_AMBIGUOUS_PARENT.to_string());

        let root = temp_dir("same-crate-path-owners")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "mod a;\nmod b;\n")?,
            write(&root, "src/a.rs", "#[path = \"shared.rs\"]\nmod shared;\n")?,
            write(&root, "src/b.rs", "#[path = \"shared.rs\"]\nmod shared;\n")?,
            write(&root, "src/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "src/shared.rs"), ambiguous);

        let root = temp_dir("bench-root-plus-module-owner")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "mod a;\n")?,
            write(
                &root,
                "src/a.rs",
                "#[path = \"../benches/shared.rs\"]\nmod shared;\n",
            )?,
            write(
                &root,
                "benches/b.rs",
                "#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), ambiguous);

        let root = temp_dir("bench-roots-disagree")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(&root, "benches/a.rs", "mod shared;\n")?,
            write(&root, "benches/b.rs", "#[cfg(test)]\nmod shared;\n")?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), ambiguous);

        // A root-shaped owner that is itself a `#[path]` module child is a
        // module of another file's graph, not a separate crate root.
        let root = temp_dir("bench-root-owner-is-module-child")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(&root, "benches/a.rs", "mod shared;\n")?,
            write(&root, "benches/b.rs", "mod shared;\n")?,
            write(&root, "benches/c.rs", "#[path = \"a.rs\"]\nmod a;\n")?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), ambiguous);
        Ok(())
    }

    /// A root can itself be included behind `#[cfg(test)]`, so the owners no
    /// longer share one effective context: the shared helper must stay
    /// ambiguous and must not gain the evidence-only test role that the
    /// lexically first owner alone would give it.
    #[test]
    fn included_root_owner_keeps_shared_child_ambiguous() -> Result<(), String> {
        let root = temp_dir("included-root-owner")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)]\ninclude!(\"../benches/a.rs\");\n",
            )?,
            write(
                &root,
                "benches/a.rs",
                "#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(
                &root,
                "benches/b.rs",
                "#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert!(
            index
                .include_parents
                .contains_key(Path::new("benches/a.rs")),
            "fixture must include benches/a.rs behind cfg(test)"
        );
        assert_eq!(
            ambiguity_reason(&index, "benches/shared.rs"),
            Some(REASON_MODULE_AMBIGUOUS_PARENT.to_string())
        );
        assert_eq!(
            role_of(&index, "benches/shared.rs", "helper")?,
            FunctionSourceRole::Production
        );
        Ok(())
    }

    /// Documented non-claim (RIPR-SPEC-0169): `CrateRoots` ignores
    /// `autobenches = false`, so autodiscovered-layout files still count as
    /// roots. This pins the current behaviour so a change is deliberate.
    #[test]
    fn autobenches_false_is_not_modeled_for_crate_roots() -> Result<(), String> {
        let root = temp_dir("autobenches-false")?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='role-composition'\nversion='0.1.0'\nedition='2024'\nautobenches=false\n",
        )
        .map_err(|error| error.to_string())?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(&root, "benches/a.rs", "mod shared;\n")?,
            write(&root, "benches/b.rs", "mod shared;\n")?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), None);
        Ok(())
    }

    /// Each conjunct of the target-root exemption is isolated by a layout the
    /// other conjuncts accept: orphan owners that are neither roots, module
    /// children nor include targets; one root declaring the child twice; a
    /// root that is itself an ambiguous module child; and an unresolved
    /// include elsewhere in the workspace.
    #[test]
    fn target_root_exemption_conjuncts_each_fail_closed() -> Result<(), String> {
        let ambiguous = Some(REASON_MODULE_AMBIGUOUS_PARENT.to_string());

        // Not crate roots: undeclared `src/` files are no compilation units.
        let root = temp_dir("orphan-non-root-owners")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(
                &root,
                "src/orphan_a.rs",
                "#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(
                &root,
                "src/orphan_b.rs",
                "#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(&root, "src/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "src/shared.rs"), ambiguous);

        // One root declaring the child twice is one compilation unit.
        let root = temp_dir("same-root-duplicate-declaration")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(
                &root,
                "benches/a.rs",
                "mod shared;\n#[path = \"shared.rs\"]\nmod shared;\n",
            )?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), ambiguous);

        // A root that is itself an ambiguous module child has no context.
        // Defense in depth: today the dual directory anchors of such an owner
        // already mark the child ambiguous before the helper is reached, so
        // the `ambiguous` owner guard is not reached by this layout.
        let root = temp_dir("ambiguous-root-owner")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn lib_fn() -> i32 { 1 }\n")?,
            write(&root, "benches/a.rs", "mod shared;\n")?,
            write(&root, "benches/b.rs", "mod shared;\n")?,
            write(&root, "benches/c.rs", "mod a;\n")?,
            write(&root, "benches/a/mod.rs", "pub fn other() -> i32 { 1 }\n")?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "benches/a.rs"), ambiguous);
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), ambiguous);
        // Ancestry would name the same reason through a suppressed edge from
        // the ambiguous owner, so also require that no edge was minted.
        assert!(
            index
                .files()
                .at(Path::new("benches/shared.rs"))
                .data()
                .role_provenance
                .edges
                .is_empty(),
            "an ambiguous root owner must not mint a module edge"
        );

        // An unresolved include anywhere withholds the exemption.
        let root = temp_dir("include-limitation-present")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "include!(concat!(env!(\"OUT_DIR\"), \"/generated.rs\"));\n",
            )?,
            write(&root, "benches/a.rs", "mod shared;\n")?,
            write(&root, "benches/b.rs", "mod shared;\n")?,
            write(&root, "benches/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert!(
            !index.include_limitations.is_empty(),
            "fixture must carry an unresolved include limitation"
        );
        assert_eq!(ambiguity_reason(&index, "benches/shared.rs"), ambiguous);
        Ok(())
    }

    /// `src/lib.rs` and `src/main.rs` are separate crates of one package, so
    /// both declaring `mod shared;` is not an ambiguous parent.
    #[test]
    fn lib_and_main_roots_sharing_a_child_are_not_ambiguous() -> Result<(), String> {
        let root = temp_dir("lib-main-shared-child")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "mod shared;\n")?,
            write(&root, "src/main.rs", "mod shared;\nfn main() {}\n")?,
            write(&root, "src/shared.rs", "pub fn helper() -> i32 { 1 }\n")?,
        ];
        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;
        assert_eq!(ambiguity_reason(&index, "src/shared.rs"), None);
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
        let provenance = &index
            .files()
            .at(Path::new("src/shared.rs"))
            .data()
            .role_provenance;
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
        let provenance = &index
            .files()
            .at(Path::new("src/shared.rs"))
            .data()
            .role_provenance;
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

        let cold = build_index_from_loaded_files_with_cache_and_test_harnesses(&root, &loaded, &[])
            .map_err(|error| error.to_string())?;
        assert_eq!(
            role_of(&cold.index, "src/tests.rs", "unattributed_helper")?,
            FunctionSourceRole::CfgTestModule
        );

        let warm = build_index_from_loaded_files_with_cache_and_test_harnesses(&root, &loaded, &[])
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

    /// A `path` attribute introduced conditionally by `cfg_attr` fails
    /// closed (#3533 review): which file the compiler loads depends on the
    /// active configuration, so neither the conditional target nor the
    /// default layout may gain a composed evidence role.
    #[test]
    fn cfg_attr_introduced_path_target_fails_closed() -> Result<(), String> {
        let root = temp_dir("cfg-attr-path")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)]\n#[cfg_attr(test, path = \"test_impl.rs\")]\nmod imp;\n",
            )?,
            write(
                &root,
                "src/imp.rs",
                "pub fn default_layout_helper() -> i32 { 1 }\n",
            )?,
            write(
                &root,
                "src/test_impl.rs",
                "pub fn conditional_target_helper() -> i32 { 2 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/imp.rs", "default_layout_helper")?,
            FunctionSourceRole::Production,
            "the default file is not compiled under the conditional configuration and must not compose a role"
        );
        assert_eq!(
            role_of(&index, "src/test_impl.rs", "conditional_target_helper")?,
            FunctionSourceRole::Production,
            "the conditional target is not statically resolvable and must not be credited either"
        );
        // The producer classifies the declaration as a typed unknown.
        let facts = RaRustSyntaxAdapter
            .summarize_file(
                Path::new("src/lib.rs"),
                &std::fs::read_to_string(root.join("src/lib.rs"))
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        assert_eq!(facts.module_declarations.len(), 1);
        assert_eq!(
            facts.module_declarations[0].path_target,
            ModulePathTarget::Unknown
        );
        assert!(facts.module_declarations[0].requires_test);
        Ok(())
    }

    /// A Unicode-identifier `cfg_attr` condition is valid Rust the bounded
    /// ASCII lexer cannot read (#3533 review). Treating that lexer failure as
    /// "not a conditional path" would fall back to default name resolution
    /// and grant an unrelated default-layout file the composed evidence role;
    /// the unreadable `cfg_attr` must fail closed to a typed unknown instead.
    #[test]
    fn unlexable_unicode_cfg_attr_path_fails_closed() -> Result<(), String> {
        let root = temp_dir("unicode-cfg-attr-path")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg(test)]\n#[cfg_attr(é, path = \"alternate.rs\")]\nmod imp;\n",
            )?,
            write(
                &root,
                "src/imp.rs",
                "pub fn default_layout_helper() -> i32 { 1 }\n",
            )?,
            write(
                &root,
                "src/alternate.rs",
                "pub fn conditional_target_helper() -> i32 { 2 }\n",
            )?,
            // Non-cfg_attr Unicode control: an unlexable attribute that
            // cannot introduce a path must keep default resolution working
            // (fail closed only for cfg_attr heads, not for every unlexable
            // attribute).
            write(
                &root,
                "src/controls.rs",
                "#[cfg(test)]\n#[allow(é)]\nmod control;\n",
            )?,
            write(
                &root,
                "src/controls/control.rs",
                "pub fn control_helper() -> i32 { 3 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/imp.rs", "default_layout_helper")?,
            FunctionSourceRole::Production,
            "the unreadable cfg_attr must not resolve the default-layout file as the module child"
        );
        assert_eq!(
            role_of(&index, "src/alternate.rs", "conditional_target_helper")?,
            FunctionSourceRole::Production,
            "the conditional target is not statically resolvable either"
        );
        assert_eq!(
            role_of(&index, "src/controls/control.rs", "control_helper")?,
            FunctionSourceRole::CfgTestModule,
            "Unicode in a non-cfg_attr attribute must not block default resolution"
        );
        // The producer classifies the declaration as a typed unknown even
        // though its condition cannot be tokenized.
        let facts = RaRustSyntaxAdapter
            .summarize_file(
                Path::new("src/lib.rs"),
                &std::fs::read_to_string(root.join("src/lib.rs"))
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        assert_eq!(facts.module_declarations.len(), 1);
        assert_eq!(
            facts.module_declarations[0].path_target,
            ModulePathTarget::Unknown
        );
        Ok(())
    }

    /// A literal `#[path]` inside an included fragment resolves relative to
    /// the fragment file's directory (#3533 review) — the Rust rule for
    /// non-inline `#[path]` targets. Anchoring at the compilation unit
    /// instead would grant the evidence role to a same-named file beside the
    /// including file that Rust never compiles as this module.
    #[test]
    fn included_fragment_path_attribute_resolves_from_the_fragment() -> Result<(), String> {
        let root = temp_dir("fragment-path")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "include!(\"frags/frag.rs\");\n")?,
            write(
                &root,
                "src/frags/frag.rs",
                "#[cfg(test)]\n#[path = \"child.rs\"]\nmod child;\n",
            )?,
            write(
                &root,
                "src/frags/child.rs",
                "pub fn fragment_child_helper() -> i32 { 1 }\n",
            )?,
            write(
                &root,
                "src/child.rs",
                "pub fn unit_side_helper() -> i32 { 2 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "src/frags/child.rs", "fragment_child_helper")?,
            FunctionSourceRole::CfgTestModule,
            "the fragment-relative target inherits the include chain's context"
        );
        assert_eq!(
            role_of(&index, "src/child.rs", "unit_side_helper")?,
            FunctionSourceRole::Production,
            "the same-named file beside the compilation unit is not this module"
        );
        let provenance = &index
            .files()
            .at(Path::new("src/frags/child.rs"))
            .data()
            .role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        let edges = &provenance.edges;
        assert_eq!(
            edges.len(),
            2,
            "the include chain edge then the module edge"
        );
        assert_eq!(edges[0].kind, SourceRoleProvenanceEdgeKind::Include);
        assert_eq!(edges[0].parent, Path::new("src/lib.rs"));
        assert_eq!(edges[1].kind, SourceRoleProvenanceEdgeKind::Module);
        assert_eq!(edges[1].parent, Path::new("src/frags/frag.rs"));
        assert_eq!(edges[1].child, Path::new("src/frags/child.rs"));
        assert!(edges[1].requires_test);
        Ok(())
    }

    /// A custom-named crate root (`[lib] path = "source/root.rs"`) resolves
    /// its out-of-line modules relative to its own containing directory
    /// (#3533 review), not under a directory named after its stem.
    #[test]
    fn custom_crate_root_resolves_modules_relative_to_its_directory() -> Result<(), String> {
        let root = temp_dir("custom-crate-root")?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='role-composition'\nversion='0.1.0'\nedition='2024'\n\n[lib]\npath = \"source/root.rs\"\n",
        )
        .map_err(|error| error.to_string())?;
        let files = vec![
            write(
                &root,
                "source/root.rs",
                "pub fn root_helper() -> i32 { 1 }\n\n#[cfg(test)]\nmod tests;\n",
            )?,
            write(
                &root,
                "source/tests.rs",
                "pub fn unattributed_helper() -> i32 { 2 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "source/root.rs", "root_helper")?,
            FunctionSourceRole::Production
        );
        assert_eq!(
            role_of(&index, "source/tests.rs", "unattributed_helper")?,
            FunctionSourceRole::CfgTestModule,
            "the crate root's sibling tests.rs is the real module child"
        );
        let provenance = &index
            .files()
            .at(Path::new("source/tests.rs"))
            .data()
            .role_provenance;
        assert!(provenance.earliest_unresolved_reason.is_none());
        assert_eq!(provenance.edges.len(), 1);
        assert_eq!(provenance.edges[0].parent, Path::new("source/root.rs"));
        assert_eq!(provenance.edges[0].child, Path::new("source/tests.rs"));
        assert!(provenance.edges[0].requires_test);
        Ok(())
    }

    /// Layout-autodiscovered target files are crate roots too: one file
    /// directly under `tests/`, `benches/`, `examples/`, or `src/bin/`
    /// resolves its modules from its own directory. Controls pin the
    /// stem-directory rule for ordinary module files.
    #[test]
    fn layout_autodiscovered_targets_are_crate_roots() -> Result<(), String> {
        let root = temp_dir("layout-roots")?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='role-composition'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|error| error.to_string())?;
        let mut crate_roots = CrateRoots::default();
        for file in [
            "tests/pricing.rs",
            "benches/perf.rs",
            "examples/demo.rs",
            "src/bin/tool.rs",
        ] {
            assert!(
                crate_roots.is_crate_root(&root, Path::new(file)),
                "{file} is an autodiscovered crate root"
            );
            assert_eq!(
                directory_of(Path::new(file)),
                Path::new(file)
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_default()
            );
        }
        // Controls: ordinary module files keep the stem rule, and a file
        // with no ancestor manifest keeps the status-quo stem rule too.
        assert!(!crate_roots.is_crate_root(&root, Path::new("src/foo.rs")));
        assert!(!crate_roots.is_crate_root(&root, Path::new("tests/nested/inner.rs")));
        let orphan_root = temp_dir("layout-roots-orphan")?;
        assert!(!crate_roots.is_crate_root(&orphan_root, Path::new("loose/foo.rs")));
        Ok(())
    }

    /// Sibling files in one directory share the crate-root verdict (#3533
    /// review): the manifest walk must memoize the resolved ancestor answer,
    /// not a per-directory `None`, or the second sibling's walk would stop
    /// early and keep the stem rule — making the verdict depend on which
    /// sibling was resolved first.
    #[test]
    fn sibling_files_share_the_resolved_manifest_answer() -> Result<(), String> {
        let root = temp_dir("sibling-roots")?;
        write_manifest(&root)?;
        // Unit level: the first lookup warms the cache, the second must not
        // stop at the cached intermediate directory.
        let mut crate_roots = CrateRoots::default();
        assert!(crate_roots.is_crate_root(&root, Path::new("tests/a.rs")));
        assert!(
            crate_roots.is_crate_root(&root, Path::new("tests/b.rs")),
            "the second sibling in the same directory is a crate root too"
        );

        // End to end: tests/b.rs declares an out-of-line module and must
        // resolve it from tests/ (crate-root rule), not from tests/b/.
        let files = vec![
            write(&root, "tests/a.rs", "pub fn a_helper() -> i32 { 1 }\n")?,
            write(
                &root,
                "tests/b.rs",
                "pub fn b_helper() -> i32 { 2 }\n\n#[cfg(test)]\nmod helpers;\n",
            )?,
            write(
                &root,
                "tests/helpers.rs",
                "pub fn sibling_child_helper() -> i32 { 3 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert_eq!(
            role_of(&index, "tests/helpers.rs", "sibling_child_helper")?,
            FunctionSourceRole::CfgTestModule,
            "the second sibling's out-of-line module child composes its evidence role"
        );
        Ok(())
    }

    fn outside_tree(index: &RustIndex, file: &str) -> bool {
        index
            .files()
            .get(Path::new(file))
            .is_some_and(|facts| facts.role_provenance.outside_module_tree)
    }

    /// #7426: a `src/<name>.rs` that no `mod` declaration reaches is outside
    /// the module tree; the declared sibling is the negative control.
    #[test]
    fn undeclared_src_file_is_outside_the_module_tree() -> Result<(), String> {
        let root = temp_dir("orphan-file")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "mod linked;\n")?,
            write(&root, "src/linked.rs", "pub fn reached() -> i32 { 1 }\n")?,
            write(&root, "src/orphan.rs", "pub fn stranded() -> i32 { 2 }\n")?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(outside_tree(&index, "src/orphan.rs"));
        assert!(!outside_tree(&index, "src/linked.rs"));
        assert!(!outside_tree(&index, "src/lib.rs"));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    /// Fail closed: an unresolvable `#[path]` may compile any file, so no
    /// file is reported outside the tree.
    #[test]
    fn unresolved_path_attribute_suppresses_the_outside_tree_marker() -> Result<(), String> {
        let root = temp_dir("orphan-unresolved-path")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "#[cfg_attr(feature = \"alt\", path = \"alt/elsewhere.rs\")]\nmod elsewhere;\n",
            )?,
            write(&root, "src/orphan.rs", "pub fn stranded() -> i32 { 2 }\n")?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(!outside_tree(&index, "src/orphan.rs"));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    /// Deeper files may be declared from an inline module the producer does
    /// not capture, so only direct `src/<name>.rs` files are ever reported.
    #[test]
    fn nested_undeclared_file_is_never_reported_outside_the_tree() -> Result<(), String> {
        let root = temp_dir("orphan-nested")?;
        write_manifest(&root)?;
        let files = vec![
            write(&root, "src/lib.rs", "pub mod outer { pub mod inner; }\n")?,
            write(
                &root,
                "src/outer/inner.rs",
                "pub fn nested() -> i32 { 3 }\n",
            )?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(!outside_tree(&index, "src/outer/inner.rs"));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    /// A declaration the producer cannot see (macro token tree, inline
    /// module, `macro_rules!`) must keep the file out of the marker.
    #[test]
    fn invisibly_declared_modules_are_never_marked_outside_the_tree() -> Result<(), String> {
        let root = temp_dir("orphan-macro-declared")?;
        write_manifest(&root)?;
        let files = vec![
            write(
                &root,
                "src/lib.rs",
                "cfg_if::cfg_if! { if #[cfg(unix)] { mod unix; } else { mod windows; } }\nmacro_rules! decl { ($n:ident) => { mod $n; } }\ndecl!(foo);\n",
            )?,
            write(&root, "src/unix.rs", "pub fn a() -> i32 { 1 }\n")?,
            write(&root, "src/foo.rs", "pub fn b() -> i32 { 2 }\n")?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(!outside_tree(&index, "src/unix.rs"));
        assert!(!outside_tree(&index, "src/foo.rs"));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    /// A declaration in a build script compiles `src/helper.rs` too.
    #[test]
    fn build_script_declared_module_is_not_marked_outside_the_tree() -> Result<(), String> {
        let root = temp_dir("orphan-build-script")?;
        write_manifest(&root)?;
        write(
            &root,
            "build.rs",
            "#[path = \"src/helper.rs\"]\nmod helper;\nfn main() {}\n",
        )?;
        let files = vec![
            write(&root, "src/lib.rs", "pub fn root() {}\n")?,
            write(&root, "src/helper.rs", "pub fn h() -> i32 { 1 }\n")?,
        ];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(!outside_tree(&index, "src/helper.rs"));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    /// A package whose crate root is not indexed gives no positive evidence.
    #[test]
    fn missing_crate_root_never_marks_a_file_outside_the_tree() -> Result<(), String> {
        let root = temp_dir("orphan-no-root")?;
        write_manifest(&root)?;
        let files = vec![write(
            &root,
            "src/orphan.rs",
            "pub fn stranded() -> i32 { 2 }\n",
        )?];

        let index = crate::analysis::facts::build_index(&root, &files)
            .map_err(|error| error.to_string())?;

        assert!(!outside_tree(&index, "src/orphan.rs"));
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}

#[cfg(test)]
mod cycle_depth_tests {
    use super::*;

    fn temp_dir(name: &str) -> Result<PathBuf, String> {
        let root = std::env::temp_dir().join(format!(
            "role-cycle-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create {}: {error}", root.display()))?;
        Ok(root)
    }

    // XnBs (review): the cycle guard at resolve() must stay exercised —
    // mutual #[path] declarations keep Production roles and name the cycle
    // reason on the provenance chain.
    #[test]
    fn mutual_module_path_cycle_fails_closed_with_cycle_reason() -> Result<(), String> {
        let root = temp_dir("role-cycle-mutual")?;
        std::fs::write(
            root.join("src/a.rs"),
            "#[path = \"b.rs\"]\nmod b;\n\npub fn a_helper() -> i32 { 1 }\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            root.join("src/b.rs"),
            "#[path = \"a.rs\"]\nmod a;\n\npub fn b_helper() -> i32 { 2 }\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(root.join("src/lib.rs"), "#[path = \"a.rs\"]\nmod a;\n")
            .map_err(|error| error.to_string())?;

        let index = crate::analysis::facts::build_index(
            &root,
            &[
                PathBuf::from("src/a.rs"),
                PathBuf::from("src/b.rs"),
                PathBuf::from("src/lib.rs"),
            ],
        )
        .map_err(|error| error.to_string())?;

        // The mutual pair resolves as ambiguous parents (both declarations
        // reach both files) — fail-closed before any recursion.
        for (file, helper) in [("src/a.rs", "a_helper"), ("src/b.rs", "b_helper")] {
            let facts = index
                .files()
                .get(Path::new(file))
                .ok_or_else(|| format!("expected {file} in the index"))?;
            for function in &facts.functions {
                if function.name == helper {
                    assert_eq!(
                        function.source_role,
                        FunctionSourceRole::Production,
                        "mutual cycle must keep {helper} production"
                    );
                }
            }
            let provenance = &index.files().at(Path::new(file)).data().role_provenance;
            assert_eq!(
                provenance.earliest_unresolved_reason.as_deref(),
                Some(REASON_MODULE_AMBIGUOUS_PARENT),
                "{file} must name the ambiguity reason"
            );
        }
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn self_referential_module_path_names_the_cycle_reason() -> Result<(), String> {
        let root = temp_dir("role-cycle-self")?;
        std::fs::write(
            root.join("src/lib.rs"),
            "#[path = \"lib.rs\"]
mod self_cycle;

pub fn helper() -> i32 { 1 }
",
        )
        .map_err(|error| error.to_string())?;

        let index = crate::analysis::facts::build_index(&root, &[PathBuf::from("src/lib.rs")])
            .map_err(|error| error.to_string())?;

        let provenance = &index
            .files()
            .at(Path::new("src/lib.rs"))
            .data()
            .role_provenance;
        assert_eq!(
            provenance.earliest_unresolved_reason.as_deref(),
            Some(REASON_MODULE_CYCLE_OR_DEPTH_LIMIT),
            "a self-referential module path must name the cycle reason"
        );
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
