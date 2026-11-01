//! A module-level constant edit read through the assertions that depend on it
//! (#9423).
//!
//! `LIMIT = 10` -> `LIMIT = 12` has no callable owner: the edit sits at module
//! scope, so only a filename heuristic related tests and the finding abstained.
//! The edit is still observable by a test whose assertion statically depends
//! on the value, and this pass reads exactly that and nothing more:
//!
//! - the changed line is the one top-level `NAME = <literal>` statement that
//!   binds `NAME` ([`module_literal_constants`]: bound once, no `global`, no
//!   walrus, no attribute write, no star import, no dynamic namespace writer),
//!   and it is the file's only changed line, so the old module is the new one
//!   with that line put back;
//! - the assertion is a straight-line `assert` of a test that imports the
//!   module, and its condition is evaluated for both values. It reads the
//!   constant itself (`from settings import LIMIT`) or calls a module function
//!   with literal arguments (`over_limit(11)`) whose body is one `return
//!   <expression>` over its parameters, literals and the constant;
//! - the assertion holds under the old value and fails under the new one.
//!
//! Anything the evaluator cannot decide (a name, a call, an operator or a
//! value kind it does not model) is not a credit, and the finding keeps the
//! `missing_import_graph` abstention. A value no assertion distinguishes is not
//! a gap either: other code or tests may observe it.

use super::bool_oracle::count_bindings;
use super::module_constants::{
    ModuleBindingFacts, PythonModuleConstant, assigned_attributes, mentions_identifier,
    module_literal_constants, module_table_binding_at,
};
use super::related_tests::{
    import_source_module_matches_owner, owner_module_callees, submodule_receivers,
};
use super::source_facts::parse_module_result;
use super::source_utils::{SourceText, is_test_file};
use super::table_reads::{
    LexedSource, dict_literal_names, dict_literal_names_lexed, unsafe_table_uses,
    unsafe_table_uses_lexed,
};
use super::{PythonAssertion, PythonOwner, PythonTest, expr_full_name};
use crate::domain::OwnerKind;
use rayon::prelude::*;
use rustpython_parser::ast::{self, Expr, Mod, Stmt};
use std::path::Path;

/// Words that mark a file able to rewrite another module's attribute at
/// runtime without an assignment statement.
const DYNAMIC_WRITERS: [&str; 21] = [
    "setattr",
    "__globals__",
    "f_globals",
    "f_locals",
    "_getframe",
    "currentframe",
    "f_back",
    "tb_frame",
    "get_referrers",
    "get_referents",
    "__setattr__",
    "delattr",
    "patch",
    "monkeypatch",
    "reload",
    "importlib",
    "__import__",
    "__dict__",
    "globals",
    "vars",
    "exec",
];

/// What the workspace's other Python files can do to a module's attributes:
/// assigned attribute names, and every word of a file that holds a dynamic
/// writer (`setattr`, `patch`, `monkeypatch`, `importlib.reload`,
/// `sys.modules`, ...). Filled for the module owner of a changed file.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct ForeignWrites {
    attributes: WrittenWords,
    dynamic_words: WrittenWords,
    /// Dict-literal module constants some other file may mutate or alias.
    table_unsafe: WrittenWords,
    /// Every word of the files that can reach a module's objects reflectively.
    reflective_words: WrittenWords,
    /// Module names (file stems and package directories) in the import
    /// component of some other reflective file: it may reach those modules
    /// through an alias or re-export no word of it names.
    reflective_reach: WrittenWords,
    /// Some other reflective file also loads modules dynamically, so the
    /// modules it reaches are unknown.
    reflective_opaque: bool,
    /// Some other file reflects ([`mentions_reflection`]): it may reach a
    /// module's namespace under a name built at runtime (#9226).
    reflection_elsewhere: bool,
    /// Some other file has the module's module name (its file stem, or its
    /// package directory for `__init__.py`): under pytest's `prepend` import
    /// mode, an extra import root, or a package beating a same-named module,
    /// an import of the module's dotted path may load that file instead
    /// (#9226).
    shares_module_name: bool,
    /// Some other file may bind names it does not spell byte for byte
    /// ([`names_may_differ`]), so a byte-exact name match cannot bound what
    /// it binds (#9226).
    names_may_differ_elsewhere: bool,
    /// A workspace file could not be read or the scan hit its bound, so the
    /// absence of a write is unknown.
    unknown: bool,
    /// Submodule stems the owning package `__init__.py` binds in a way that
    /// may replace `import pkg.submod` with a package attribute (#11135).
    #[serde(default)]
    init_submodule_shadows: std::collections::BTreeSet<String>,
    /// The owning package `__init__.py` star-imports or did not parse, so no
    /// submodule receiver along this module path is certain (#11135).
    #[serde(default)]
    init_submodule_receiver_opaque: bool,
    /// Sibling modules of the module's regular package that may forward a
    /// call to one of its readers (#9619). Filled per run, never cached.
    #[serde(skip)]
    forwarders: Vec<ForwarderModule>,
}

/// One write set of a [`ForeignWrites`]: the words some source other than
/// the excluded ones holds, in order. The workspace's per-word source counts
/// are shared by every changed file's view, which keeps only the words its
/// exclusion hides, so a view, its clones and its drop cost one file's words
/// instead of a copy of every workspace word (#10914). It serializes as the
/// sorted word list.
#[derive(Clone, Default)]
pub(super) struct WrittenWords {
    /// How many indexed sources hold each word.
    counts: std::sync::Arc<std::collections::BTreeMap<String, usize>>,
    /// Words of `counts` that only excluded sources hold.
    hidden: std::collections::BTreeSet<String>,
}

impl WrittenWords {
    /// The words of `counts` some source other than the excluded ones holds.
    /// `excluded` counts how many excluded sources hold each word.
    fn held_by_others(
        counts: &std::sync::Arc<std::collections::BTreeMap<String, usize>>,
        excluded: &std::collections::BTreeMap<&str, usize>,
    ) -> Self {
        Self {
            counts: std::sync::Arc::clone(counts),
            hidden: excluded
                .iter()
                .filter(|(word, held)| counts.get(**word).is_some_and(|count| count <= *held))
                .map(|(word, _)| word.to_string())
                .collect(),
        }
    }

    fn of_words(words: impl IntoIterator<Item = String>) -> Self {
        Self {
            counts: std::sync::Arc::new(words.into_iter().map(|word| (word, 1)).collect()),
            hidden: std::collections::BTreeSet::new(),
        }
    }

    pub(super) fn contains(&self, word: &str) -> bool {
        self.counts.contains_key(word) && !self.hidden.contains(word)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &str> {
        self.counts
            .keys()
            .map(String::as_str)
            .filter(|word| !self.hidden.contains(*word))
    }

    /// The held words that start with `prefix`, in order.
    fn starting_with<'w>(&'w self, prefix: &'w str) -> impl Iterator<Item = &'w str> {
        self.counts
            .range::<str, _>((
                std::ops::Bound::Included(prefix),
                std::ops::Bound::Unbounded,
            ))
            .map(|(word, _)| word.as_str())
            .take_while(move |word| word.starts_with(prefix))
            .filter(|word| !self.hidden.contains(*word))
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.iter().next().is_none()
    }
}

impl PartialEq for WrittenWords {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl Eq for WrittenWords {}

impl std::fmt::Debug for WrittenWords {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_list().entries(self.iter()).finish()
    }
}

impl serde::Serialize for WrittenWords {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl<'de> serde::Deserialize<'de> for WrittenWords {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<String>::deserialize(deserializer).map(Self::of_words)
    }
}

/// A sibling module of a changed module's regular package whose source names
/// `from .<stem> import`, with the writes of every other workspace file
/// (#9619).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ForwarderModule {
    pub(super) file: std::path::PathBuf,
    pub(super) source: String,
    pub(super) foreign: ForeignWrites,
}

/// The most sibling modules read as forwarders of one changed module; more
/// reads none, so the scan stays bounded.
const MAX_FORWARDER_MODULES: usize = 16;

/// The sibling modules that may forward a call to a reader of `owner`
/// through `from .<stem> import ...`: the same regular package (its
/// `__init__.py` is a workspace source), not a test, `__init__.py` or
/// `conftest.py`, whose source spells that import. More than
/// [`MAX_FORWARDER_MODULES`] reads none.
pub(super) fn forwarder_candidates<'s>(
    owner: &Path,
    sources: impl Iterator<Item = (&'s Path, &'s str)>,
    is_source: impl Fn(&Path) -> bool,
) -> Vec<(&'s Path, &'s str)> {
    let (Some(directory), Some(stem)) = (
        owner.parent(),
        owner.file_stem().and_then(|stem| stem.to_str()),
    ) else {
        return Vec::new();
    };
    if is_test_file(owner)
        || owner.file_name().is_some_and(|name| name == "__init__.py")
        || !is_source(&directory.join("__init__.py"))
    {
        return Vec::new();
    }
    let needle = format!("from .{stem} import");
    let mut out: Vec<(&Path, &str)> = sources
        .filter(|(file, source)| {
            file.parent() == Some(directory)
                && *file != owner
                && file.extension().is_some_and(|extension| extension == "py")
                && !is_test_file(file)
                && !file
                    .file_name()
                    .is_some_and(|name| name == "__init__.py" || name == "conftest.py")
                && source.contains(&needle)
        })
        .collect();
    if out.len() > MAX_FORWARDER_MODULES {
        return Vec::new();
    }
    out.sort();
    out
}

/// Most distinct words retained in the workspace reference index.
const MAX_WORKSPACE_WORDS: usize = 200_000;
/// Sources parsed per parallel batch before the batch is folded, so a tripped
/// [`MAX_WORKSPACE_WORDS`] cap stops parsing within one batch.
const FOREIGN_PARSE_BATCH_FILES: usize = 256;

/// Spellings that can hide an imported module's name from the word index.
const DYNAMIC_LOADERS: [&str; 13] = [
    "importlib",
    "__import__",
    "exec",
    "eval",
    "compile",
    "runpy",
    "load_module",
    "exec_module",
    "import_module",
    "getattr",
    "builtins",
    "__builtins__",
    "__dict__",
];

/// Words that reach a module namespace through a frame, a function's globals
/// or the garbage collector's referrers (#9226).
const FRAME_WALKERS: [&str; 9] = [
    "__globals__",
    "f_globals",
    "f_locals",
    "_getframe",
    "currentframe",
    "f_back",
    "tb_frame",
    "get_referrers",
    "get_referents",
];

/// Whether `source` reflects: reaches objects through a frame, a function's
/// globals, the garbage collector (`gc.get_objects()`), or a spelling that
/// takes the name at runtime (`getattr`, `dir`, `inspect`, ...). Such code can
/// write a module namespace under a name built at runtime, so no name it
/// mentions bounds what it changes (#9226).
/// The import system counts too: a file that may replace a module in
/// `sys.modules` (however spelled: `from sys import modules`), install an
/// import hook, or extend a package's `__path__` hands every importer a
/// different module.
pub(super) fn mentions_reflection(source: &str) -> bool {
    mentions_reflection_with(&|identifier| mentions_identifier(source, identifier))
}

/// [`mentions_reflection`] through a whole-word `mentions` test, so a caller
/// holding the source's sorted words looks each spelling up instead of
/// scanning the source once per spelling (#10914).
fn mentions_reflection_with(mentions: &dyn Fn(&str) -> bool) -> bool {
    REFLECTIVE_SPELLINGS
        .iter()
        .chain(&FRAME_WALKERS)
        .chain(&IMPORT_SYSTEM)
        .chain(&[
            "get_objects",
            "setattr",
            "__setattr__",
            "delattr",
            "__dict__",
            "vars",
        ])
        .any(|spelling| mentions(spelling))
        || (mentions("sys") && (mentions("modules") || mentions("path")))
}

/// Words that replace what an `import` binds (#9226).
const IMPORT_SYSTEM: [&str; 12] = [
    "__path__",
    "addsitedir",
    "pkgutil",
    "extend_path",
    "meta_path",
    "path_hooks",
    "path_importer_cache",
    "importlib",
    "__import__",
    "import_module",
    "exec_module",
    "reload",
];

/// Spellings that reach a module's objects without writing their names.
const REFLECTIVE_SPELLINGS: [&str; 16] = [
    "exec",
    "getattr",
    "dir",
    "getmembers",
    "locals",
    "__globals__",
    "attrgetter",
    "__getattribute__",
    "eval",
    "compile",
    "inspect",
    "gc",
    // Replacing a reader's code or defaults changes what it returns without
    // rebinding its name.
    "__code__",
    "__defaults__",
    "__kwdefaults__",
    "__closure__",
];

/// Function attributes whose assignment replaces what a reader returns without
/// rebinding its name (`price.__code__ = other.__code__`).
const CODE_SLOTS: [&str; 4] = ["__code__", "__defaults__", "__kwdefaults__", "__closure__"];

/// What each workspace source contributes to [`ForeignWrites`], parsed and
/// tokenized once so a diff that changes many Python files does not repeat
/// the whole-workspace work per file (#10125).
pub(super) struct ForeignIndex<'a> {
    files: Vec<IndexedSource<'a>>,
    /// Each word's only source, or None when two sources mention it. A
    /// malformed file's own function/name is not an import.
    /// Hashed: the fold inserts every distinct word of every file and only
    /// point lookups read it (#10914).
    references: std::collections::HashMap<&'a str, Option<&'a Path>>,
    hidden_import: bool,
    /// For each reflective file, the module names in its import component.
    /// Files of one import component share one reach.
    reflective_reach: Vec<(&'a Path, Reach)>,
    /// The reference index passed its bound or a file went unread.
    unknown: bool,
    /// Per-write source counts over `files` and `reflective_reach`.
    totals: ForeignTotals,
}

/// How many indexed sources hold each write, so the writes of every source
/// but one are a subtraction instead of a rebuild over the whole workspace
/// for each changed file (#10857).
#[derive(Default)]
struct ForeignTotals {
    attributes: WordCounts,
    dynamic_words: WordCounts,
    table_unsafe: WordCounts,
    reflective_words: WordCounts,
    reflective_reach: WordCounts,
}

/// Per-word source counts, shared with every [`WrittenWords`] built from them.
type WordCounts = std::sync::Arc<std::collections::BTreeMap<String, usize>>;

impl ForeignTotals {
    fn build(files: &[IndexedSource<'_>], reflective_reach: &[(&Path, Reach)]) -> Self {
        Self {
            attributes: count_sources(files, |source| {
                source
                    .assigned
                    .iter()
                    .flatten()
                    .map(String::as_str)
                    .collect()
            }),
            dynamic_words: count_sources(files, |source| {
                source.dynamic_words.iter().flatten().copied().collect()
            }),
            table_unsafe: count_sources(files, |source| {
                source.table_unsafe.iter().map(String::as_str).collect()
            }),
            reflective_words: count_sources(files, |source| {
                source.reflective_words.iter().flatten().copied().collect()
            }),
            reflective_reach: count_shared_reaches(reflective_reach),
        }
    }
}

/// The module names one reflective file may reach, shared by every
/// reflective file of its import component.
type Reach = std::sync::Arc<std::collections::BTreeSet<String>>;

/// How many reflective files reach each module name. Files sharing a
/// component's reach count once per name through that shared reach, so a
/// reach is read once however many files hold it (#10914).
fn count_shared_reaches(reflective_reach: &[(&Path, Reach)]) -> WordCounts {
    // Keyed by the shared allocation: one entry per component's reach.
    let mut holders: std::collections::BTreeMap<
        *const std::collections::BTreeSet<String>,
        (&Reach, usize),
    > = std::collections::BTreeMap::new();
    for (_, reach) in reflective_reach {
        holders
            .entry(std::sync::Arc::as_ptr(reach))
            .or_insert((reach, 0))
            .1 += 1;
    }
    let mut weighted: Vec<(&str, usize)> = holders
        .values()
        .flat_map(|(reach, files)| reach.iter().map(move |name| (name.as_str(), *files)))
        .collect();
    weighted.par_sort_unstable();
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (name, files) in weighted {
        match counts.last_mut() {
            Some((last, count)) if last == name => *count += files,
            _ => counts.push((name.to_string(), files)),
        }
    }
    std::sync::Arc::new(counts.into_iter().collect())
}

/// How many of `sources` hold each word, a source counting once per word.
/// Each source's distinct words are gathered and sorted in parallel and the
/// counts read off the sorted runs, which costs a sort instead of merging
/// per-source maps (#10914).
fn count_sources<'s, S: Sync>(
    sources: &'s [S],
    words: impl Fn(&'s S) -> Vec<&'s str> + Sync,
) -> WordCounts {
    let mut held: Vec<&str> = sources
        .par_iter()
        .flat_map_iter(|source| {
            let mut distinct = words(source);
            distinct.sort_unstable();
            distinct.dedup();
            distinct
        })
        .collect();
    held.par_sort_unstable();
    let mut counts: Vec<(String, usize)> = Vec::new();
    for word in held {
        match counts.last_mut() {
            Some((last, count)) if last == word => *count += 1,
            _ => counts.push((word.to_string(), 1)),
        }
    }
    std::sync::Arc::new(counts.into_iter().collect())
}

/// One or more sources' distinct writes per set, keyed by borrowed words.
#[derive(Default)]
struct SourceCounts<'w> {
    attributes: std::collections::BTreeMap<&'w str, usize>,
    dynamic_words: std::collections::BTreeMap<&'w str, usize>,
    table_unsafe: std::collections::BTreeMap<&'w str, usize>,
    reflective_words: std::collections::BTreeMap<&'w str, usize>,
    reflective_reach: std::collections::BTreeMap<&'w str, usize>,
}

impl<'w> SourceCounts<'w> {
    /// The writes one source contributes to each [`ForeignWrites`] set:
    /// assigned attributes, dynamic words, unsafe tables, reflective words.
    fn of(source: &'w IndexedSource<'_>) -> Self {
        let mut counts = Self::default();
        count_distinct(
            &mut counts.attributes,
            source.assigned.iter().flatten().map(String::as_str),
        );
        count_distinct(
            &mut counts.dynamic_words,
            source.dynamic_words.iter().flatten().copied(),
        );
        count_distinct(
            &mut counts.table_unsafe,
            source.table_unsafe.iter().map(String::as_str),
        );
        count_distinct(
            &mut counts.reflective_words,
            source.reflective_words.iter().flatten().copied(),
        );
        counts
    }

    fn merge(mut self, other: Self) -> Self {
        fn add<'w>(
            into: &mut std::collections::BTreeMap<&'w str, usize>,
            from: std::collections::BTreeMap<&'w str, usize>,
        ) {
            for (word, count) in from {
                *into.entry(word).or_default() += count;
            }
        }
        add(&mut self.attributes, other.attributes);
        add(&mut self.dynamic_words, other.dynamic_words);
        add(&mut self.table_unsafe, other.table_unsafe);
        add(&mut self.reflective_words, other.reflective_words);
        add(&mut self.reflective_reach, other.reflective_reach);
        self
    }
}

/// Add one source's distinct `words` to borrowed-key `counts`.
fn count_distinct<'w>(
    counts: &mut std::collections::BTreeMap<&'w str, usize>,
    words: impl Iterator<Item = &'w str>,
) {
    let distinct: std::collections::BTreeSet<&str> = words.collect();
    for word in distinct {
        *counts.entry(word).or_default() += 1;
    }
}

// Parses the foreign index has done on this thread, so a test can pin that
// the workspace is parsed once per run and not once per changed file (#10125).
// It counts files folded into the index on the calling thread; the parses
// themselves run on rayon workers (#10441).
#[cfg(test)]
thread_local! {
    pub(super) static FOREIGN_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The per-file lexes the foreign index needs, functions of the source text
/// alone: computed once by the source-fact pass and persisted with its
/// facts, so a warm run does not lex every workspace file again (#10914).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct ForeignLexes {
    /// [`dict_literal_names`], sorted.
    pub(super) dict_literal_names: Vec<String>,
    /// [`import_words`], in source order.
    pub(super) import_words: Vec<String>,
    /// [`unsafe_table_uses`] of every name, sorted: the uses of the
    /// workspace's tables are this list filtered to them.
    pub(super) unsafe_names: Vec<String>,
    /// [`names_may_differ`], which lexes a source holding non-ASCII text.
    pub(super) loose_names: bool,
}

impl ForeignLexes {
    pub(super) fn of(source: &str) -> Self {
        let lexed = LexedSource::new(source);
        Self {
            dict_literal_names: dict_literal_names_lexed(&lexed).into_iter().collect(),
            import_words: import_words_lexed(source, &lexed)
                .into_iter()
                .map(str::to_string)
                .collect(),
            unsafe_names: unsafe_table_uses_lexed(source, &lexed, &|_| true, None)
                .into_iter()
                .collect(),
            loose_names: names_may_differ(source),
        }
    }
}

/// What the source-fact pass already knows about one workspace file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct PriorFileFacts {
    /// The attribute names its statements assign; None when it does not parse.
    pub(super) assigned: Option<Vec<String>>,
    pub(super) lexes: Option<ForeignLexes>,
}

struct IndexedSource<'a> {
    file: &'a Path,
    /// None when the file does not parse.
    assigned: Option<Vec<String>>,
    /// Holds a dynamic writer, or does not parse: every word is a possible write.
    dynamic_words: Option<Vec<&'a str>>,
    /// Dict-literal constants (of any workspace module) this file mentions
    /// other than as an import or a pure read (#9619).
    table_unsafe: Vec<String>,
    /// Holds a reflective spelling: every word is a possible table access.
    reflective_words: Option<Vec<&'a str>>,
    /// Words of the lines that import, to link modules into components.
    import_words: Vec<&'a str>,
    /// The reflective file also loads modules by name at runtime.
    reflective_opaque: bool,
    /// The file reflects, whatever tables the workspace holds.
    reflects: bool,
    /// The file may bind names it does not spell byte for byte ([`names_may_differ`]).
    loose_names: bool,
    /// For a package `__init__.py`, submodule stems its bindings may shadow.
    init_submodule_shadows: std::collections::BTreeSet<String>,
    /// For a package `__init__.py`, star import or an unparsed body (#11135).
    init_submodule_receiver_opaque: bool,
}

impl<'a> ForeignIndex<'a> {
    /// The index parsing every file's assigned attributes itself.
    #[cfg(test)]
    pub(super) fn build(
        sources: impl Iterator<Item = (&'a Path, &'a str)>,
        unread_files: bool,
    ) -> Self {
        static NONE: std::collections::BTreeMap<std::path::PathBuf, PriorFileFacts> =
            std::collections::BTreeMap::new();
        Self::build_with_prior(sources, unread_files, &NONE)
    }

    /// [`Self::build`], reading each file's assigned attributes and lexes
    /// from `prior` (the source-fact pass already derived them) and parsing
    /// or lexing only what it does not hold (#10914).
    pub(super) fn build_with_prior(
        sources: impl Iterator<Item = (&'a Path, &'a str)>,
        unread_files: bool,
        prior: &'a std::collections::BTreeMap<std::path::PathBuf, PriorFileFacts>,
    ) -> Self {
        let mut files = Vec::new();
        let mut references = std::collections::HashMap::new();
        let mut hidden_import = false;
        // Parsing each source dominates the index on large workspaces, so each
        // batch is parsed in parallel and then folded sequentially in input
        // order: word ownership and the word cap behave as before, and a
        // tripped word cap stops parsing within one batch (#10441).
        let sources: Vec<(&'a Path, &'a str)> = sources.collect();
        let token = crate::analysis::cancellation::current_token();
        // Every per-file scan below lexes or parses the whole source, so all
        // of them run inside the parallel stage; only word ownership and the
        // word cap stay in the sequential fold (#10441, #10735). The stage
        // also sorts each source's distinct words: the fold then visits each
        // word once per file, which leaves ownership and the cap unchanged
        // because a repeat changes neither (#10914).
        let tables: std::collections::BTreeSet<String> = sources
            .par_iter()
            .map(|(file, source)| {
                crate::analysis::cancellation::with_optional_token(token.as_ref(), || {
                    match prior.get(*file).and_then(|facts| facts.lexes.as_ref()) {
                        Some(lexes) => {
                            debug_assert_eq!(
                                lexes.dict_literal_names,
                                dict_literal_names(source).into_iter().collect::<Vec<_>>(),
                                "{}: supplied dict-literal names differ from the source's",
                                file.display()
                            );
                            lexes.dict_literal_names.iter().cloned().collect()
                        }
                        None => dict_literal_names(source),
                    }
                })
            })
            .reduce(std::collections::BTreeSet::new, |mut all, names| {
                all.extend(names);
                all
            });
        for batch in sources.chunks(FOREIGN_PARSE_BATCH_FILES) {
            let scanned: Vec<((bool, IndexedSource<'a>), Vec<&'a str>)> = batch
                .par_iter()
                .map(|&(file, source)| {
                    crate::analysis::cancellation::with_optional_token(token.as_ref(), || {
                        let mut words: Vec<&'a str> = source_words(source).collect();
                        words.sort_unstable();
                        words.dedup();
                        let indexed = scan_source(file, source, &words, &tables, prior.get(file));
                        (indexed, words)
                    })
                })
                .collect();
            for (&(file, _), ((hides_import, indexed), words)) in batch.iter().zip(scanned) {
                hidden_import |= hides_import;
                for word in words {
                    let first = references.entry(word).or_insert(Some(file));
                    if first.is_some_and(|first| first != file) {
                        *first = None;
                    }
                    if references.len() > MAX_WORKSPACE_WORDS {
                        return Self {
                            files: Vec::new(),
                            references: std::collections::HashMap::new(),
                            hidden_import,
                            unknown: true,
                            reflective_reach: Vec::new(),
                            totals: ForeignTotals::default(),
                        };
                    }
                }
                #[cfg(test)]
                FOREIGN_PARSES.with(|parses| parses.set(parses.get() + 1));
                files.push(indexed);
            }
        }
        let reflective_reach = reflective_reaches(&files);
        let totals = ForeignTotals::build(&files, &reflective_reach);
        Self {
            files,
            references,
            hidden_import,
            unknown: unread_files,
            reflective_reach,
            totals,
        }
    }

    /// The writes in every source but `own`, with `unknown` when some
    /// workspace file went unread.
    pub(super) fn excluding(&self, own: &Path) -> ForeignWrites {
        if self.unknown && self.files.is_empty() {
            return ForeignWrites {
                unknown: true,
                ..ForeignWrites::default()
            };
        }
        let mut excluded = SourceCounts::default();
        let mut reflective_opaque = false;
        let mut reflection_elsewhere = false;
        let mut shares_module_name = false;
        let mut names_may_differ_elsewhere = false;
        let mut unparsed = Vec::new();
        for indexed in &self.files {
            if indexed.file == own {
                excluded = excluded.merge(SourceCounts::of(indexed));
                continue;
            }
            reflective_opaque |= indexed.reflective_opaque;
            reflection_elsewhere |= indexed.reflects;
            shares_module_name |= same_module_name(indexed.file, own);
            names_may_differ_elsewhere |= indexed.loose_names;
            if indexed.assigned.is_none() {
                unparsed.push(indexed.file);
            }
        }
        for (file, reach) in &self.reflective_reach {
            if *file == own {
                count_distinct(
                    &mut excluded.reflective_reach,
                    reach.iter().map(String::as_str),
                );
            }
        }
        // An unreferenced malformed production module cannot poison every
        // binding. Retain its words as potential writes, and fail closed on
        // references, implicit collection/initializers and hidden imports.
        let unknown = self.unknown
            || (!unparsed.is_empty() && self.hidden_import)
            || unparsed.iter().any(|file| {
                is_test_file(file)
                    || file
                        .file_name()
                        .is_some_and(|name| name == "__init__.py" || name == "conftest.py")
                    || file
                        .file_stem()
                        .and_then(|name| name.to_str())
                        .is_none_or(|stem| {
                            self.references
                                .get(stem)
                                .is_some_and(|first| first.is_none_or(|first| first != *file))
                        })
            });
        let totals = &self.totals;
        let (init_submodule_shadows, init_submodule_receiver_opaque) = own
            .parent()
            .map(|directory| directory.join("__init__.py"))
            .and_then(|init_path| {
                self.files
                    .iter()
                    .find(|indexed| indexed.file == init_path.as_path())
            })
            .map(|indexed| {
                (
                    indexed.init_submodule_shadows.clone(),
                    indexed.init_submodule_receiver_opaque,
                )
            })
            .unwrap_or_default();
        // A dotted import traverses every ancestor package attribute: an
        // ancestor `__init__.py` that rebinds the next path segment (or is
        // opaque) means no receiver along this module path is certain.
        let ancestor_refuses = {
            let mut refuses = false;
            let mut directory = own.parent();
            while let Some(current) = directory {
                let Some(parent) = current.parent() else {
                    break;
                };
                let segment = current.file_name().and_then(|name| name.to_str());
                let init_path = parent.join("__init__.py");
                if let Some(indexed) = self
                    .files
                    .iter()
                    .find(|indexed| indexed.file == init_path.as_path())
                    && (indexed.init_submodule_receiver_opaque
                        || segment
                            .is_none_or(|segment| indexed.init_submodule_shadows.contains(segment)))
                {
                    refuses = true;
                    break;
                }
                directory = Some(parent);
            }
            refuses
        };
        let init_submodule_receiver_opaque = init_submodule_receiver_opaque || ancestor_refuses;
        ForeignWrites {
            attributes: WrittenWords::held_by_others(&totals.attributes, &excluded.attributes),
            dynamic_words: WrittenWords::held_by_others(
                &totals.dynamic_words,
                &excluded.dynamic_words,
            ),
            table_unsafe: WrittenWords::held_by_others(
                &totals.table_unsafe,
                &excluded.table_unsafe,
            ),
            reflective_words: WrittenWords::held_by_others(
                &totals.reflective_words,
                &excluded.reflective_words,
            ),
            reflective_reach: WrittenWords::held_by_others(
                &totals.reflective_reach,
                &excluded.reflective_reach,
            ),
            reflective_opaque,
            reflection_elsewhere,
            shares_module_name,
            names_may_differ_elsewhere,
            unknown,
            init_submodule_shadows,
            init_submodule_receiver_opaque,
            forwarders: Vec::new(),
        }
    }

    /// The per-file rebuild [`Self::excluding`] replaced: the union of every
    /// other source's writes. An independent oracle for the subtraction.
    #[cfg(test)]
    pub(super) fn rebuilt_excluding(&self, own: &Path) -> ForeignWrites {
        let mut attributes = std::collections::BTreeSet::new();
        let mut words = std::collections::BTreeSet::new();
        let mut table_unsafe = std::collections::BTreeSet::new();
        let mut reflective_words = std::collections::BTreeSet::new();
        for indexed in self.files.iter().filter(|indexed| indexed.file != own) {
            table_unsafe.extend(indexed.table_unsafe.iter().cloned());
            if let Some(reflective) = &indexed.reflective_words {
                reflective_words.extend(reflective.iter().map(|word| word.to_string()));
            }
            if let Some(assigned) = &indexed.assigned {
                attributes.extend(assigned.iter().cloned());
            }
            if let Some(dynamic) = &indexed.dynamic_words {
                words.extend(dynamic.iter().map(|word| word.to_string()));
            }
        }
        ForeignWrites {
            attributes: WrittenWords::of_words(attributes),
            dynamic_words: WrittenWords::of_words(words),
            table_unsafe: WrittenWords::of_words(table_unsafe),
            reflective_words: WrittenWords::of_words(reflective_words),
            reflective_reach: WrittenWords::of_words(
                self.reflective_reach
                    .iter()
                    .filter(|(file, _)| *file != own)
                    .flat_map(|(_, reach)| reach.iter().cloned())
                    .collect::<std::collections::BTreeSet<_>>(),
            ),
            ..self.excluding(own)
        }
    }
}

/// Whether CPython may bind names `source` does not spell byte for byte: it
/// declares a source encoding other than UTF-8 ([`declares_foreign_encoding`]),
/// holds a NUL or a carriage return not followed by a newline (CPython
/// breaks lines on a lone `\r`, the scan does not, so the two disagree on
/// what each line is), or spells a name or an f-string with a non-ASCII
/// character, or fails to lex. Python NFKC-normalises identifiers, so
/// `def _\u{ff50}ad()` rebinds `_pad`, while every name comparison here is
/// byte-exact (#9226).
pub(super) fn names_may_differ(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(at, byte)| *byte == 0 || (*byte == b'\r' && bytes.get(at + 1) != Some(&b'\n')))
        || declares_foreign_encoding(source)
        || spells_non_ascii_name(source)
}

fn spells_non_ascii_name(source: &str) -> bool {
    use rustpython_parser::{Mode, Tok, lexer::lex};
    !source.is_ascii()
        && lex(source, Mode::Module).any(|token| match token {
            Err(_) => true,
            Ok((Tok::Name { name }, _)) => !name.is_ascii(),
            Ok((Tok::String { value, kind, .. }, _)) => kind.is_any_fstring() && !value.is_ascii(),
            Ok(_) => false,
        })
}

/// Whether `source` declares a PEP 263 encoding other than UTF-8 in a
/// comment on its first two lines (`# coding: utf-7`). CPython decodes the
/// file with that codec, so an all-ASCII file can hold code the scan reads
/// as a comment (`# +AAo-_pad = 2` is a second line in UTF-7) (#9226).
fn declares_foreign_encoding(source: &str) -> bool {
    source.lines().take(2).any(|line| {
        let line = line
            .trim_start_matches('\u{feff}')
            .trim_start_matches([' ', '\t', '\x0c']);
        let Some(comment) = line.strip_prefix('#') else {
            return false;
        };
        let Some(declared) = comment.match_indices("coding").find_map(|(at, word)| {
            let rest = &comment[at + word.len()..];
            rest.strip_prefix([':', '='])
        }) else {
            return false;
        };
        let name: String = declared
            .trim_start_matches([' ', '\t'])
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
            .map(|ch| {
                if ch == '_' {
                    '-'
                } else {
                    ch.to_ascii_lowercase()
                }
            })
            .collect();
        name != "utf-8" && name != "utf8"
    })
}

/// Whether the directory of the module `file` (relative to `root`) holds a
/// compiled extension module of the same name (`ledger.abi3.so`,
/// `ledger.cpython-313-x86_64-linux-gnu.so`, `ledger.so`, `ledger.pyd`). The
/// import system tries extensions before `.py` source, so an import of the
/// module's dotted path loads that file instead; for a package's
/// `__init__.py`, an `__init__.*.so` beside it wins the same way. An
/// unreadable directory counts as one (#9226).
pub(super) fn extension_beside(root: &Path, file: &Path) -> bool {
    let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
        return true;
    };
    let directory = root.join(file).parent().map(Path::to_path_buf);
    let Some(Ok(entries)) = directory.map(std::fs::read_dir) else {
        return true;
    };
    let prefix = format!("{stem}.");
    entries.into_iter().any(|entry| {
        let Ok(entry) = entry else {
            return true;
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return true;
        };
        name.starts_with(&prefix) && (name.ends_with(".so") || name.ends_with(".pyd"))
    })
}

/// Whether some workspace source has an unchecked-hash pyc (PEP 552,
/// `compileall --invalidation-mode unchecked-hash`) in its `__pycache__`:
/// CPython loads it without comparing it with the source, so the code that
/// runs may not be the code the scan read. Timestamp and checked-hash pycs
/// are trusted the way CPython trusts them: it recompiles them when the
/// source's mtime, size or hash moves. An unreadable cache or pyc counts as
/// unchecked (#9226).
pub(super) fn unchecked_bytecode<'a>(root: &Path, files: impl Iterator<Item = &'a Path>) -> bool {
    let mut stems: std::collections::BTreeMap<&Path, std::collections::BTreeSet<&str>> =
        std::collections::BTreeMap::new();
    for file in files {
        let (Some(directory), Some(stem)) = (
            file.parent(),
            file.file_stem().and_then(|stem| stem.to_str()),
        ) else {
            return true;
        };
        stems.entry(directory).or_default().insert(stem);
    }
    stems.iter().any(|(directory, stems)| {
        let entries = match std::fs::read_dir(root.join(directory).join("__pycache__")) {
            Ok(entries) => entries,
            Err(error) => return error.kind() != std::io::ErrorKind::NotFound,
        };
        entries.into_iter().any(|entry| {
            let Ok(entry) = entry else {
                return true;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return true;
            };
            name.ends_with(".pyc")
                && name
                    .split_once('.')
                    .is_some_and(|(stem, _)| stems.contains(stem))
                && pyc_unchecked(&entry.path())
        })
    })
}

/// Whether the pyc at `pyc` is hash-based without the source check: its
/// flags word (bytes 4..8) has bit 0 set and bit 1 clear (PEP 552).
fn pyc_unchecked(pyc: &Path) -> bool {
    use std::io::Read;
    let mut header = [0_u8; 8];
    let Ok(mut opened) = std::fs::File::open(pyc) else {
        return true;
    };
    if opened.read_exact(&mut header).is_err() {
        return true;
    }
    let flags = u32::from_le_bytes([header[4], header[5], header[6], header[7]]);
    flags & 0b11 == 0b01
}

/// Whether a pytest or tox configuration in the workspace sets an import
/// path (`pythonpath`, `python_paths`, a `PYTHONPATH` environment entry)
/// other than its own directory (`.`). Any other entry can hold code the
/// scan does not read: an archive, a directory the workspace walk excludes,
/// a root holding only compiled extensions, a path outside the workspace or
/// an expansion such as `{toxinidir}`. A setting whose entries cannot be
/// read off its lines counts as one. Every directory holding a
/// source, and each of its ancestors, is checked, since pytest reads the
/// first configuration above its arguments. An unreadable configuration
/// counts as such a setting (#9226).
pub(super) fn import_path_configured<'a>(
    root: &Path,
    files: impl Iterator<Item = &'a Path>,
) -> bool {
    let mut directories = std::collections::BTreeSet::new();
    directories.insert(Path::new(""));
    for file in files {
        directories.extend(file.ancestors().skip(1));
    }
    directories.iter().any(|directory| {
        let base = root.join(directory);
        [
            "pytest.ini",
            ".pytest.ini",
            "pytest.toml",
            ".pytest.toml",
            "pyproject.toml",
            "setup.cfg",
            "tox.ini",
        ]
        .iter()
        .any(|name| match std::fs::read_to_string(base.join(name)) {
            Ok(text) => sets_unread_import_path(name.ends_with(".toml"), &text),
            Err(error) => error.kind() != std::io::ErrorKind::NotFound,
        })
    })
}

/// Whether the configuration `text` sets an import path other than `.`, or
/// sets `addopts`, whose shell-split `-o`/`--override-ini` (or `-p`,
/// `--rootdir`, `--import-mode`) can set one under any quoting. A TOML file
/// is always parsed, since an escaped key spells `pythonpath` without the
/// word: every `pythonpath` or `python_paths` key at any depth must hold `.`
/// alone, no key may be `addopts`, and a textual mention the parsed keys do
/// not account for refuses. In an INI file the entries are the
/// whitespace-separated words after the key's `=` through its continuation
/// lines, which, as in iniconfig, run on across blank and `#`/`;` comment
/// lines.
fn sets_unread_import_path(toml_file: bool, text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("addopts") {
        return true;
    }
    let mentions = lowered.matches("pythonpath").count() + lowered.matches("python_paths").count();
    if mentions == 0 && !toml_file {
        return false;
    }
    if toml_file {
        let Ok(table) = text.parse::<toml::Table>() else {
            return true;
        };
        let mut settings = Vec::new();
        let mut overrides = false;
        let table = toml::Value::Table(table);
        import_path_values(&table, &mut settings, &mut overrides);
        return overrides
            || settings.len() != mentions
            || settings.iter().any(|value| match value {
                toml::Value::String(entry) => !own_directory(entry),
                toml::Value::Array(entries) => {
                    entries.is_empty()
                        || entries
                            .iter()
                            .any(|entry| entry.as_str().is_none_or(|entry| !own_directory(entry)))
                }
                _ => true,
            });
    }
    let lines: Vec<&str> = text.lines().collect();
    lines.iter().enumerate().any(|(at, line)| {
        let lowered = line.to_ascii_lowercase();
        if !(lowered.contains("pythonpath") || lowered.contains("python_paths")) {
            return false;
        }
        let Some((key, value)) = line.split_once('=') else {
            return true;
        };
        let key = key.trim().to_ascii_lowercase();
        if key != "pythonpath" && key != "python_paths" {
            return true;
        }
        let mut words: Vec<&str> = value.split_whitespace().collect();
        for line in &lines[at + 1..] {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(['#', ';']) {
                continue;
            }
            if !line.starts_with([' ', '\t']) {
                break;
            }
            words.extend(trimmed.split_whitespace());
        }
        words.is_empty() || words.iter().any(|word| !own_directory(word))
    })
}

/// Every value under a `pythonpath` or `python_paths` key in `value`, and
/// whether any key is `addopts`.
fn import_path_values<'v>(
    value: &'v toml::Value,
    settings: &mut Vec<&'v toml::Value>,
    overrides: &mut bool,
) {
    match value {
        toml::Value::Table(table) => {
            for (key, value) in table {
                let key = key.to_ascii_lowercase();
                if key == "pythonpath" || key == "python_paths" {
                    settings.push(value);
                } else {
                    *overrides |= key == "addopts";
                    import_path_values(value, settings, overrides);
                }
            }
        }
        toml::Value::Array(values) => {
            for value in values {
                import_path_values(value, settings, overrides);
            }
        }
        _ => {}
    }
}

/// Whether an import path entry names the configuration's own directory.
fn own_directory(entry: &str) -> bool {
    entry == "." || entry == "./"
}

/// The module names of the compiled extension modules (`name.*.so`,
/// `name.so`, `name.*.pyd`, `name.pyd`) and sourceless bytecode
/// (`name.pyc`) in every directory holding a workspace source or above one,
/// since pytest's prepend mode can put any of them on `sys.path`, with a compiled `__init__` named by
/// its package, or `None` when one cannot be read. With more than one
/// import root, an extension in another root can shadow a source module or
/// a package of the same name (#9226).
pub(super) fn extension_names<'a>(
    root: &Path,
    files: impl Iterator<Item = &'a Path>,
) -> Option<std::collections::BTreeSet<String>> {
    let directories: std::collections::BTreeSet<&Path> =
        files.flat_map(|file| file.ancestors().skip(1)).collect();
    let mut names = std::collections::BTreeSet::new();
    for directory in directories {
        for entry in std::fs::read_dir(root.join(directory)).ok()? {
            let name = entry.ok()?.file_name();
            let name = name.to_str()?;
            if name.ends_with(".so") || name.ends_with(".pyd") || name.ends_with(".pyc") {
                // A compiled package initializer stands for its package.
                let module = name.split('.').next().unwrap_or_default();
                if module == "__init__" {
                    names.insert(directory.file_name()?.to_str()?.to_string());
                } else {
                    names.insert(module.to_string());
                }
            }
        }
    }
    Some(names)
}

/// Whether one of `names` (from [`extension_names`]) is the module name of
/// `file` or of a package above it.
pub(super) fn shadowed_by_extension(
    names: &std::collections::BTreeSet<String>,
    file: &Path,
) -> bool {
    let stem = file.file_stem().and_then(|stem| stem.to_str());
    stem.is_none_or(|stem| stem != "__init__" && names.contains(stem))
        || file
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .any(|part| {
                part.as_os_str()
                    .to_str()
                    .is_none_or(|part| names.contains(part))
            })
}

/// Whether a package directory below an import root, along a dotted path
/// the owner `file` can be imported by, holds compiled code (`.so`, `.pyd`,
/// sourceless `.pyc`) for the next component of that path or for the
/// package's `__init__`. Under pytest's prepend mode a test directory goes
/// first on `sys.path`, so `tests/reach/ledger.pyc` in a directory with no
/// source is what `import reach.ledger` loads. A directory named for the
/// whole path with an initializer of any kind (`tests/reach/ledger/
/// __init__.pyc`) is a package that wins over the module the same way,
/// unless it is the owner's own package. Along the owner's whole dotted
/// path from a root, a package of any initializer or a source module named
/// for a leading component that is not the owner's own ancestor shadows the
/// owner's package (`tests/shapes/__init__.py` or `tests/shapes.py` for
/// `shapes.rates.table`): the import then fails or loads another module
/// (#9619), unless that root is itself a package below the workspace root
/// (`tests/__init__.py`), which pytest never puts on `sys.path`. `roots`
/// are every directory holding a source or above one; their
/// own compiled entries are left to `extension_names`. An unreadable
/// directory or name counts as one (#9226).
pub(super) fn compiled_package_shadow(
    root: &Path,
    roots: &std::collections::BTreeSet<&Path>,
    file: &Path,
) -> bool {
    let module = file.with_extension("");
    let mut parts = Vec::new();
    for part in module.components() {
        let Some(part) = part.as_os_str().to_str() else {
            return true;
        };
        parts.push(part);
    }
    let own_package = if parts.last() == Some(&"__init__") {
        parts.pop();
        file.parent().map(|package| root.join(package))
    } else {
        None
    };
    // The starts that give an importable name: the whole path, and the path
    // below a `src` layout root at any depth or a `lib` root at the top.
    let leading_starts: Vec<usize> = std::iter::once(0)
        .chain(
            parts
                .iter()
                .enumerate()
                .take(parts.len().saturating_sub(1))
                .filter(|(index, part)| **part == "src" || (*index == 0 && **part == "lib"))
                .map(|(index, _)| index + 1),
        )
        .collect();
    let own_ancestors: Vec<std::path::PathBuf> = file
        .ancestors()
        .skip(1)
        .map(|ancestor| root.join(ancestor))
        .collect();
    roots.iter().any(|base| {
        // pytest's prepend mode inserts the first directory above a test that
        // has no `__init__.py`, so a package directory below the workspace
        // root is never an import root of its own for the prefix rule.
        let package_base =
            !base.as_os_str().is_empty() && root.join(base).join("__init__.py").is_file();
        (0..parts.len()).any(|start| {
            let mut directory = root.join(base);
            for (depth, name) in parts[start..].iter().enumerate() {
                if depth > 0 && compiled_entry(&directory, name) {
                    return true;
                }
                if !package_base
                    && leading_starts.contains(&start)
                    && start + depth + 1 < parts.len()
                {
                    let path = directory.join(name);
                    if !own_ancestors.contains(&path)
                        && (path.with_extension("py").is_file() || initializer_in_package(&path))
                    {
                        return true;
                    }
                }
                directory.push(name);
                if !directory.is_dir() {
                    return false;
                }
            }
            own_package.as_ref() != Some(&directory) && initializer_in(&directory)
        })
    })
}

/// Whether `directory` is a directory holding a package initializer of any
/// kind; a path that is not a directory holds none.
fn initializer_in_package(directory: &Path) -> bool {
    directory.is_dir() && initializer_in(directory)
}

/// Whether `directory` holds a package initializer of any kind (`__init__`
/// source, bytecode or extension), or cannot be read.
fn initializer_in(directory: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return true;
    };
    entries.into_iter().any(|entry| {
        let Ok(entry) = entry else {
            return true;
        };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            return true;
        };
        file_name.split('.').next() == Some("__init__")
            && [".py", ".pyc", ".so", ".pyd"]
                .iter()
                .any(|suffix| file_name.ends_with(suffix))
    })
}

/// Whether `directory` holds compiled code named `name` or `__init__`, or
/// cannot be read.
fn compiled_entry(directory: &Path, name: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return true;
    };
    entries.into_iter().any(|entry| {
        let Ok(entry) = entry else {
            return true;
        };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            return true;
        };
        let module = file_name.split('.').next().unwrap_or_default();
        (module == name || module == "__init__")
            && (file_name.ends_with(".so")
                || file_name.ends_with(".pyd")
                || file_name.ends_with(".pyc"))
    })
}

/// Whether the top-level name of the dotted module `module` is a standard
/// library module. `sys.meta_path` tries the built-in and frozen importers
/// before any path entry, and pytest has imported many pure-Python stdlib
/// modules before a test runs, so `from math import floor` never loads a
/// workspace `math.py` (#9226).
pub(super) fn names_stdlib_module(module: &str) -> bool {
    let top = module.split('.').next().unwrap_or_default();
    STDLIB_MODULE_NAMES
        .split_whitespace()
        .any(|name| name == top)
}

/// Every top-level standard-library module name from CPython 3.9 to 3.14:
/// 3.13 `sys.stdlib_module_names`, the names 3.14 adds, the modules removed
/// since 3.9, and the frozen `__hello__` family.
const STDLIB_MODULE_NAMES: &str = "\
__future__ __hello__ __hello_alias__ __hello_only__ __phello__ \
__phello_alias__ _abc _aix_support _android_support _apple_support _ast \
_ast_unparse _asyncio _bisect _blake2 _bootlocale _bz2 _codecs _codecs_cn \
_codecs_hk _codecs_iso2022 _codecs_jp _codecs_kr _codecs_tw _collections \
_collections_abc _colorize _compat_pickle _compression _contextvars _crypt \
_csv _ctypes _curses _curses_panel _datetime _dbm _decimal _dummy_thread \
_elementtree _frozen_importlib _frozen_importlib_external _functools _gdbm \
_hashlib _heapq _hmac _imp _interpchannels _interpqueues _interpreters _io \
_ios_support _json _locale _lsprof _lzma _markupbase _md5 _msi \
_multibytecodec _multiprocessing _opcode _opcode_metadata _operator \
_osx_support _overlapped _pickle _posixshmem _posixsubprocess _py_abc \
_py_warnings _pydatetime _pydecimal _pyio _pylong _pyrepl _queue _random \
_remote_debugging _scproxy _sha1 _sha2 _sha3 _signal _sitebuiltins _socket \
_sqlite3 _sre _ssl _stat _statistics _string _strptime _struct _suggestions \
_symtable _sysconfig _testinternalcapi _thread _threading_local _tkinter \
_tokenize _tracemalloc _types _typing _uuid _warnings _weakref _weakrefset \
_winapi _wmi _xxtestfuzz _zoneinfo _zstd abc aifc annotationlib antigravity \
argparse array ast asynchat asyncio asyncore atexit audioop base64 bdb \
binascii binhex bisect builtins bz2 cProfile calendar cgi cgitb chunk cmath \
cmd code codecs codeop collections colorsys compileall compression \
concurrent configparser contextlib contextvars copy copyreg crypt csv ctypes \
curses dataclasses datetime dbm decimal difflib dis distutils doctest \
dummy_threading email encodings ensurepip enum errno faulthandler fcntl \
filecmp fileinput fnmatch formatter fractions ftplib functools gc \
genericpath getopt getpass gettext glob graphlib grp gzip hashlib heapq hmac \
html http idlelib imaplib imghdr imp importlib inspect io ipaddress \
itertools json keyword lib2to3 linecache locale logging lzma macpath mailbox \
mailcap marshal math mimetypes mmap modulefinder msilib msvcrt \
multiprocessing netrc nis nntplib nt ntpath nturl2path numbers opcode \
operator optparse os ossaudiodev parser pathlib pdb pickle pickletools pipes \
pkgutil platform plistlib poplib posix posixpath pprint profile pstats pty \
pwd py_compile pyclbr pydoc pydoc_data pyexpat queue quopri random re \
readline reprlib resource rlcompleter runpy sched secrets select selectors \
shelve shlex shutil signal site smtpd smtplib sndhdr socket socketserver \
spwd sqlite3 sre_compile sre_constants sre_parse ssl stat statistics string \
stringprep struct subprocess sunau symbol symtable sys sysconfig syslog \
tabnanny tarfile telnetlib tempfile termios test textwrap this threading \
time timeit tkinter token tokenize tomllib trace traceback tracemalloc tty \
turtle turtledemo types typing unicodedata unittest urllib uu uuid venv \
warnings wave weakref webbrowser winreg winsound wsgiref xdrlib xml xmlrpc \
xxsubtype zipapp zipfile zipimport zlib zoneinfo ";

/// Whether two files would bind the same last module-name component: a
/// file's stem, or its package directory's name for `__init__.py`. A file
/// whose module name cannot be read counts as a match (#9226).
fn same_module_name(file: &Path, own: &Path) -> bool {
    fn module_name(file: &Path) -> Option<&std::ffi::OsStr> {
        if file.file_name().is_some_and(|name| name == "__init__.py") {
            file.parent().and_then(Path::file_name)
        } else {
            file.file_stem()
        }
    }
    match (module_name(file), module_name(own)) {
        (Some(file), Some(own)) => file == own,
        _ => true,
    }
}

impl ForeignWrites {
    /// Which write sets hold anything: attributes, dynamic words, unsafe
    /// tables, reflective words, reflective reach.
    #[cfg(test)]
    pub(super) fn written_sets(&self) -> [bool; 5] {
        [
            !self.attributes.is_empty(),
            !self.dynamic_words.is_empty(),
            !self.table_unsafe.is_empty(),
            !self.reflective_words.is_empty(),
            !self.reflective_reach.is_empty(),
        ]
    }

    /// Whether every write set of `self` and `other` reads the same shared
    /// per-word counts rather than a copy of them.
    #[cfg(test)]
    pub(super) fn shares_word_counts(&self, other: &Self) -> bool {
        [
            (&self.attributes, &other.attributes),
            (&self.dynamic_words, &other.dynamic_words),
            (&self.table_unsafe, &other.table_unsafe),
            (&self.reflective_words, &other.reflective_words),
            (&self.reflective_reach, &other.reflective_reach),
        ]
        .iter()
        .all(|(left, right)| std::sync::Arc::ptr_eq(&left.counts, &right.counts))
    }

    /// The writes in every source but `own`; one-shot form of
    /// [`ForeignIndex`] for callers that scan a single file.
    #[cfg(test)]
    pub(super) fn scan<'a>(
        own: &Path,
        sources: impl Iterator<Item = (&'a Path, &'a str)>,
        unread_files: bool,
    ) -> Self {
        ForeignIndex::build(sources, unread_files).excluding(own)
    }

    /// Whether another file may rebind `name` of the module at `module_file`:
    /// it assigns an attribute of that name, or holds a dynamic writer and
    /// mentions the name or the module's file stem. An unfilled value (a
    /// context that never scanned) rebinds nothing it knows of; the diff
    /// pipeline always fills it.
    pub(super) fn may_rebind(&self, name: &str, module_file: &Path) -> bool {
        let stem = module_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        self.unknown
            || self.attributes.contains(name)
            || self.attributes.contains("*")
            || self.dynamic_words.contains(name)
            || (!stem.is_empty() && self.dynamic_words.contains(stem))
    }
}

impl ForeignWrites {
    /// Whether some other workspace file reflects ([`mentions_reflection`]),
    /// so it may write this module's namespace under a name built at runtime.
    pub(super) fn reflection_elsewhere(&self) -> bool {
        self.unknown || self.reflection_elsewhere
    }

    /// Whether another workspace file has this module's module name, so an
    /// import of its dotted path may load that file instead.
    pub(super) fn shares_module_name(&self) -> bool {
        self.unknown || self.shares_module_name
    }

    /// Records that compiled code may run in place of the scanned source: an
    /// extension module beside it ([`extension_beside`]) or an unchecked pyc
    /// in the workspace ([`unchecked_bytecode`]).
    pub(super) fn mark_compiled_shadow(&mut self) {
        self.shares_module_name = true;
    }

    /// Records the sibling modules that may forward to the module's readers.
    pub(super) fn set_forwarders(&mut self, forwarders: Vec<ForwarderModule>) {
        self.forwarders = forwarders;
    }

    /// Whether some other workspace file may bind names it does not spell
    /// byte for byte (a non-ASCII name, or a non-UTF-8 source encoding).
    pub(super) fn names_may_differ_elsewhere(&self) -> bool {
        self.unknown || self.names_may_differ_elsewhere
    }

    /// Whether some workspace file assigns a dunder attribute (`__new__`,
    /// `__getattribute__`, `__class__`, ...), which can change what any
    /// class builds or looks up without naming the method (#9226).
    pub(super) fn writes_dunder(&self) -> bool {
        self.attributes
            .starting_with("__")
            .any(|attribute| attribute.len() > 4 && attribute.ends_with("__"))
    }

    /// Whether some workspace file assigns a function's code slots, which can
    /// replace any reader without rebinding its name (#10626). Checked where a
    /// reader is evaluated, not for plain names such as `version_info`.
    pub(super) fn may_replace_reader_code(&self) -> bool {
        CODE_SLOTS.iter().any(|slot| self.attributes.contains(slot))
    }

    /// Whether another file may change the contents of the dict-literal
    /// constant `name` of the module at `module_file` in place, or alias it.
    pub(super) fn may_mutate_table(&self, name: &str, module_file: &Path) -> bool {
        let stem = module_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default();
        self.may_rebind(name, module_file)
            || self.reflective_opaque
            || (!stem.is_empty() && self.reflective_reach.contains(stem))
            || self.table_unsafe.contains(name)
            || self.reflective_words.contains(name)
            || (!stem.is_empty() && self.reflective_words.contains(stem))
    }
}

/// Spellings that load a module chosen at runtime.
const MODULE_LOADERS: [&str; 11] = [
    // Evaluated code can import any module and mutate its tables.
    "eval",
    "exec",
    "compile",
    "importlib",
    "__import__",
    "import_module",
    "load_module",
    "exec_module",
    "runpy",
    "builtins",
    "__builtins__",
];

/// Whether the owning package's `__init__.py` may bind this module stem so an
/// `import ….<stem>` loads a package attribute instead of the submodule file
/// (#11135).
fn package_init_refuses_submodule(module_file: &Path, foreign: &ForeignWrites) -> bool {
    if foreign.init_submodule_receiver_opaque {
        return true;
    }
    let Some(stem) = module_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| *stem != "__init__")
    else {
        return false;
    };
    foreign.init_submodule_shadows.contains(stem)
}

#[cfg(test)]
pub(super) fn package_init_refuses_submodule_for_test(
    module_file: &Path,
    foreign: &ForeignWrites,
) -> bool {
    package_init_refuses_submodule(module_file, foreign)
}

/// Submodule stems a package initializer binds in a way that shadows the
/// submodule import path, and whether the initializer is opaque (#11135).
fn package_init_submodule_facts(
    file: &Path,
    source: &str,
) -> (std::collections::BTreeSet<String>, bool) {
    if file.file_name().is_none_or(|name| name != "__init__.py") {
        return (std::collections::BTreeSet::new(), false);
    }
    let Ok(Mod::Module(module)) = parse_module_result(file, source) else {
        return (std::collections::BTreeSet::new(), true);
    };
    let mut shadows = std::collections::BTreeSet::new();
    for statement in &module.body {
        if init_stmt_shadows_submodule(statement, &mut shadows) {
            return (shadows, true);
        }
    }
    (shadows, false)
}

#[cfg(test)]
pub(super) fn package_init_submodule_facts_for_test(
    file: &Path,
    source: &str,
) -> (std::collections::BTreeSet<String>, bool) {
    package_init_submodule_facts(file, source)
}

fn init_stmt_shadows_submodule(
    statement: &Stmt,
    shadows: &mut std::collections::BTreeSet<String>,
) -> bool {
    match statement {
        Stmt::Assign(assign) => {
            for target in &assign.targets {
                init_target_names(target, shadows);
            }
            false
        }
        Stmt::AnnAssign(assign) => {
            init_target_names(assign.target.as_ref(), shadows);
            false
        }
        Stmt::AugAssign(assign) => {
            init_target_names(assign.target.as_ref(), shadows);
            false
        }
        Stmt::FunctionDef(function) => {
            shadows.insert(function.name.to_string());
            false
        }
        Stmt::AsyncFunctionDef(function) => {
            shadows.insert(function.name.to_string());
            false
        }
        Stmt::ClassDef(class) => {
            shadows.insert(class.name.to_string());
            false
        }
        Stmt::ImportFrom(import) => {
            for alias in &import.names {
                if alias.name.as_str() == "*" {
                    return true;
                }
                if init_import_from_shadows_submodule(import, alias) {
                    let bound = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                    shadows.insert(bound);
                }
            }
            false
        }
        Stmt::Import(import) => {
            for alias in &import.names {
                let bound = alias
                    .asname
                    .as_ref()
                    .map(|asname| asname.to_string())
                    .unwrap_or_else(|| {
                        alias
                            .name
                            .as_str()
                            .split('.')
                            .next()
                            .unwrap_or_default()
                            .to_string()
                    });
                if alias.asname.is_some() {
                    shadows.insert(bound);
                }
            }
            false
        }
        Stmt::Delete(delete) => {
            for target in &delete.targets {
                init_target_names(target, shadows);
            }
            false
        }
        Stmt::For(for_stmt) => {
            init_target_names(&for_stmt.target, shadows);
            init_block_shadows(&for_stmt.body, shadows)
                || init_block_shadows(&for_stmt.orelse, shadows)
        }
        Stmt::AsyncFor(for_stmt) => {
            init_target_names(&for_stmt.target, shadows);
            init_block_shadows(&for_stmt.body, shadows)
                || init_block_shadows(&for_stmt.orelse, shadows)
        }
        Stmt::While(while_stmt) => {
            init_block_shadows(&while_stmt.body, shadows)
                || init_block_shadows(&while_stmt.orelse, shadows)
        }
        Stmt::If(if_stmt) => {
            init_block_shadows(&if_stmt.body, shadows)
                || init_block_shadows(&if_stmt.orelse, shadows)
        }
        Stmt::With(with_stmt) => {
            for item in &with_stmt.items {
                if let Some(target) = &item.optional_vars {
                    init_target_names(target, shadows);
                }
            }
            init_block_shadows(&with_stmt.body, shadows)
        }
        Stmt::AsyncWith(with_stmt) => {
            for item in &with_stmt.items {
                if let Some(target) = &item.optional_vars {
                    init_target_names(target, shadows);
                }
            }
            init_block_shadows(&with_stmt.body, shadows)
        }
        Stmt::Try(try_stmt) => init_try_shadows(
            &try_stmt.body,
            &try_stmt.handlers,
            &try_stmt.orelse,
            &try_stmt.finalbody,
            shadows,
        ),
        Stmt::TryStar(try_stmt) => init_try_shadows(
            &try_stmt.body,
            &try_stmt.handlers,
            &try_stmt.orelse,
            &try_stmt.finalbody,
            shadows,
        ),
        // Capture patterns bind names without a target expression; treat the
        // whole initializer as opaque rather than enumerate them.
        Stmt::Match(_) => true,
        _ => false,
    }
}

/// Collect every plain name a binding or `del` target introduces, through
/// tuple, list and starred unpacking. Attribute and subscript targets bind
/// no module-scope name.
fn init_target_names(target: &Expr, shadows: &mut std::collections::BTreeSet<String>) {
    match target {
        Expr::Name(name) => {
            shadows.insert(name.id.to_string());
        }
        Expr::Tuple(tuple) => {
            for element in &tuple.elts {
                init_target_names(element, shadows);
            }
        }
        Expr::List(list) => {
            for element in &list.elts {
                init_target_names(element, shadows);
            }
        }
        Expr::Starred(starred) => init_target_names(&starred.value, shadows),
        _ => {}
    }
}

/// Walk a nested block of a package initializer at module scope; true when a
/// nested star import or other opaque statement makes the initializer opaque.
fn init_block_shadows(block: &[Stmt], shadows: &mut std::collections::BTreeSet<String>) -> bool {
    block
        .iter()
        .any(|statement| init_stmt_shadows_submodule(statement, shadows))
}

fn init_try_shadows(
    body: &[Stmt],
    handlers: &[ast::ExceptHandler],
    orelse: &[Stmt],
    finalbody: &[Stmt],
    shadows: &mut std::collections::BTreeSet<String>,
) -> bool {
    if init_block_shadows(body, shadows) {
        return true;
    }
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(name) = &handler.name {
            shadows.insert(name.to_string());
        }
        if init_block_shadows(&handler.body, shadows) {
            return true;
        }
    }
    init_block_shadows(orelse, shadows) || init_block_shadows(finalbody, shadows)
}

/// Whether one `from … import …` in a package initializer binds a name that
/// may replace a submodule import. `from . import name` re-exports the
/// submodule and does not shadow (#11135).
fn init_import_from_shadows_submodule(import: &ast::StmtImportFrom, alias: &ast::Alias) -> bool {
    if alias.asname.is_some() {
        return true;
    }
    // `from . import name` re-exports the submodule; other forms bind a package
    // attribute that may replace `import pkg.name`.
    !matches!(
        (
            import.level.map(|level| level.to_u32()),
            import.module.as_deref()
        ),
        (Some(1), None)
    ) || alias.name.as_str() == "*"
}

/// Every name of the logical statements that hold an `import` keyword. The
/// lexer ends a statement, so a parenthesis or backslash continuation reads
/// whole (`from . import (\n    settings as hub,\n)`) while a bracket inside
/// a comment or string does not. A lexing error reads every word.
/// One file's foreign-index facts, plus whether it hides an import from the
/// static graph. A pure function of the file and the workspace's dict-literal
/// names, so [`ForeignIndex::build`] runs it in parallel.
fn scan_source<'a>(
    file: &'a Path,
    source: &'a str,
    words: &[&'a str],
    tables: &std::collections::BTreeSet<String>,
    prior: Option<&'a PriorFileFacts>,
) -> (bool, IndexedSource<'a>) {
    // `words` is the source's sorted distinct words, so a whole-word mention
    // is a lookup instead of one scan of the source per spelling.
    let mentions = |identifier: &str| {
        let found = words.binary_search(&identifier).is_ok();
        debug_assert_eq!(found, mentions_identifier(source, identifier));
        found
    };
    let hides_import = DYNAMIC_LOADERS.iter().any(|loader| mentions(loader))
        || source.contains("sys.modules")
        || (file.file_name().is_some_and(|name| name == "__init__.py") && mentions("__all__"));
    let assigned = prior
        .map(|facts| facts.assigned.clone())
        .unwrap_or_else(|| assigned_attributes(source));
    let dynamic = assigned.is_none()
        || DYNAMIC_WRITERS.iter().any(|writer| mentions(writer))
        || source.contains("sys.modules");
    let lexes = prior.and_then(|facts| facts.lexes.as_ref());
    let table_unsafe: Vec<String> = if tables.is_empty() || !mentions_any_table(words, tables) {
        Vec::new()
    } else if let Some(lexes) = lexes {
        let supplied: Vec<String> = lexes
            .unsafe_names
            .iter()
            .filter(|name| tables.contains(name.as_str()))
            .cloned()
            .collect();
        debug_assert_eq!(
            supplied,
            unsafe_table_uses(source, &|word| tables.contains(word), None)
                .into_iter()
                .collect::<Vec<_>>(),
            "{}: supplied unsafe table uses differ from the source's",
            file.display()
        );
        supplied
    } else {
        unsafe_table_uses(source, &|word| tables.contains(word), None)
            .into_iter()
            .collect()
    };
    // Reflection reaches a module's dict without naming it, or with a name
    // built at runtime (`getattr(module, n)`, `dir(module)`,
    // `inspect.getmembers`, `locals()`, `fn.__globals__`).
    let reflective = !tables.is_empty()
        && REFLECTIVE_SPELLINGS
            .iter()
            .any(|spelling| mentions(spelling));
    let (init_submodule_shadows, init_submodule_receiver_opaque) =
        package_init_submodule_facts(file, source);
    let indexed = IndexedSource {
        file,
        assigned,
        dynamic_words: dynamic.then(|| words.to_vec()),
        table_unsafe,
        reflective_words: reflective.then(|| words.to_vec()),
        import_words: if tables.is_empty() {
            Vec::new()
        } else {
            match lexes {
                Some(lexes) => {
                    debug_assert_eq!(
                        lexes.import_words,
                        import_words(source),
                        "{}: supplied import words differ from the source's",
                        file.display()
                    );
                    lexes.import_words.iter().map(String::as_str).collect()
                }
                None => import_words(source),
            }
        },
        reflects: mentions_reflection_with(&mentions),
        loose_names: match lexes {
            Some(lexes) => {
                debug_assert_eq!(
                    lexes.loose_names,
                    names_may_differ(source),
                    "{}: supplied loose-name answer differs from the source's",
                    file.display()
                );
                lexes.loose_names
            }
            None => names_may_differ(source),
        },
        reflective_opaque: reflective
            && (source.contains("sys.modules")
                || MODULE_LOADERS.iter().any(|loader| mentions(loader))),
        init_submodule_shadows,
        init_submodule_receiver_opaque,
    };
    (hides_import, indexed)
}

/// Whether one of the sorted distinct `words` names a table, probing from
/// the smaller side: pandas holds a few tables and ~300 words per file.
fn mentions_any_table(words: &[&str], tables: &std::collections::BTreeSet<String>) -> bool {
    if tables.len() < words.len() {
        tables
            .iter()
            .any(|table| words.binary_search(&table.as_str()).is_ok())
    } else {
        words.iter().any(|word| tables.contains(*word))
    }
}

fn import_words(source: &str) -> Vec<&str> {
    import_words_lexed(source, &LexedSource::new(source))
}

fn import_words_lexed<'s>(source: &'s str, lexed: &LexedSource) -> Vec<&'s str> {
    use rustpython_parser::Tok;
    if !lexed.complete() {
        return source_words(source).collect();
    }
    let mut words = Vec::new();
    let mut statement: Vec<&str> = Vec::new();
    let mut imports = false;
    for (tok, start, end) in lexed.tokens() {
        match tok {
            Tok::Newline | Tok::Semi => {
                if imports {
                    words.append(&mut statement);
                }
                statement.clear();
                imports = false;
            }
            Tok::Import => imports = true,
            Tok::Name { .. } => {
                if let Some(text) = source.get(*start..*end) {
                    statement.push(text);
                }
            }
            _ => {}
        }
    }
    if imports {
        words.append(&mut statement);
    }
    words
}

/// For each reflective file, the names of the modules in its import
/// component: files linked by an import word naming another file's stem or
/// package `__init__.py`, in either direction, plus every `conftest.py`
/// component (a fixture hands a module to any test). A reflective file can
/// only reach a module that is connected to it, even through a helper that
/// receives the module as an argument.
fn reflective_reaches<'a>(files: &[IndexedSource<'a>]) -> Vec<(&'a Path, Reach)> {
    if files
        .iter()
        .all(|indexed| indexed.reflective_words.is_none())
    {
        return Vec::new();
    }
    // A package name stands for its `__init__.py` alone: importing `pkg`
    // runs that file, not its sibling modules.
    let names = |file: &Path| -> Vec<String> {
        let stem = file.file_stem().and_then(|stem| stem.to_str());
        let package = file
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .filter(|_| stem == Some("__init__"));
        stem.into_iter()
            .chain(package)
            .map(str::to_string)
            .collect()
    };
    let mut named: std::collections::BTreeMap<String, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (index, indexed) in files.iter().enumerate() {
        for name in names(indexed.file) {
            named.entry(name).or_default().push(index);
        }
    }
    let mut parent: Vec<usize> = (0..files.len()).collect();
    fn find(parent: &mut [usize], mut index: usize) -> usize {
        while parent[index] != index {
            parent[index] = parent[parent[index]];
            index = parent[index];
        }
        index
    }
    for (index, indexed) in files.iter().enumerate() {
        for word in &indexed.import_words {
            for &other in named.get(*word).into_iter().flatten() {
                let (a, b) = (find(&mut parent, index), find(&mut parent, other));
                parent[a] = b;
            }
        }
    }
    let mut members: std::collections::BTreeMap<usize, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut conftest_roots = std::collections::BTreeSet::new();
    for (index, indexed) in files.iter().enumerate() {
        let root = find(&mut parent, index);
        members.entry(root).or_default().extend(names(indexed.file));
        if indexed
            .file
            .file_name()
            .is_some_and(|name| name == "conftest.py")
        {
            conftest_roots.insert(root);
        }
    }
    let conftest: std::collections::BTreeSet<String> = conftest_roots
        .iter()
        .filter_map(|root| members.get(root))
        .flatten()
        .cloned()
        .collect();
    let mut reaches = Vec::new();
    let mut by_root: std::collections::BTreeMap<usize, Reach> = std::collections::BTreeMap::new();
    for (index, indexed) in files.iter().enumerate() {
        if indexed.reflective_words.is_none() {
            continue;
        }
        let root = find(&mut parent, index);
        let reach = by_root.entry(root).or_insert_with(|| {
            let mut reach = members.get(&root).cloned().unwrap_or_default();
            reach.extend(conftest.iter().cloned());
            std::sync::Arc::new(reach)
        });
        reaches.push((indexed.file, std::sync::Arc::clone(reach)));
    }
    reaches
}

fn source_words(source: &str) -> impl Iterator<Item = &str> {
    source
        .split(|ch: char| !(ch.is_alphanumeric() || ch == '_'))
        .filter(|word| !word.is_empty())
}

/// One assertion that fails when the constant takes its new value.
pub(super) struct ModuleValueCredit<'a> {
    pub(super) test: &'a PythonTest,
    pub(super) assertion: &'a PythonAssertion,
    /// The edited constant, with its old and new literal text.
    pub(super) constant: String,
    pub(super) old: String,
    pub(super) new: String,
    /// The module function the assertion reads the constant through, when it
    /// does not read the constant directly.
    pub(super) reader: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    Int(i128),
    Bool(bool),
    Str(String),
    /// Only inside a dataclass default read (`dataclass_default.rs`), where
    /// `Scope::floats` admits it; a module constant never evaluates to one.
    Float(f64),
    /// A dict literal of `str` or `int` keys, each once, to scalar values
    /// (#9619). Only a module function reads it, through `.get` or a subscript.
    Table(Vec<(Value, Value)>),
}

/// Floats are compared and combined exactly as Python does only while an
/// integer converts to one without rounding.
const FLOAT_EXACT_LIMIT: i128 = 1 << 53;

impl Value {
    pub(super) fn truthy(&self) -> bool {
        match self {
            Self::Int(value) => *value != 0,
            Self::Bool(value) => *value,
            Self::Str(value) => !value.is_empty(),
            Self::Float(value) => *value != 0.0,
            Self::Table(entries) => !entries.is_empty(),
        }
    }

    /// The value as a float, when an integer converts to it exactly.
    fn float(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(*value),
            _ => {
                let number = self.number()?;
                (number.abs() < FLOAT_EXACT_LIMIT).then_some(number as f64)
            }
        }
    }

    fn number(&self) -> Option<i128> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Bool(value) => Some(i128::from(*value)),
            Self::Str(_) | Self::Float(_) | Self::Table(_) => None,
        }
    }

    pub(super) fn render(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Bool(true) => "True".to_string(),
            Self::Bool(false) => "False".to_string(),
            Self::Str(value) => format!("{value:?}"),
            Self::Float(value) => format!("{value:?}"),
            Self::Table(entries) => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key.render(), value.render()))
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
        }
    }

    /// The value a dict maps `key` to. A `bool` key is not looked up: Python
    /// hashes `True` as `1`.
    fn lookup(&self, key: &Value) -> Option<&Value> {
        let Self::Table(entries) = self else {
            return None;
        };
        if !matches!(key, Value::Int(_) | Value::Str(_)) {
            return None;
        }
        entries
            .iter()
            .find_map(|(candidate, value)| (candidate == key).then_some(value))
    }
}

/// A module function the evaluator can run: positional parameters and one
/// `return` expression.
pub(super) struct Reader {
    pub(super) parameters: Vec<String>,
    pub(super) expression: Expr,
}

/// What an expression's names and calls mean while it is evaluated.
pub(super) struct Scope<'a> {
    pub(super) constant: &'a str,
    pub(super) value: &'a Value,
    /// Reader mode: the parameter values of one call. `None` evaluates a test
    /// expression, where no bare name but a constant spelling is known.
    pub(super) parameters: Option<&'a [(String, Value)]>,
    /// Test mode: the spellings that read the constant itself.
    pub(super) constant_spellings: &'a [String],
    /// Test mode: the spellings that call the reader, and the reader.
    pub(super) reader_spellings: &'a [String],
    pub(super) reader: Option<&'a Reader>,
    /// Test mode of a dataclass default read: the class the assertion builds.
    pub(super) model: Option<&'a super::dataclass_default::ClassModel>,
    /// Reader mode of a dataclass method: the receiver name and its fields.
    pub(super) receiver: Option<(&'a str, &'a [(String, Value)])>,
    /// Floats and `round` are modelled: a dataclass default read only.
    pub(super) floats: bool,
    /// A forwarder's call scope (#9619): its local name for an owner-module
    /// reader, which runs with the owner's constant in scope.
    pub(super) callee: Option<Callee<'a>>,
}

/// An owner-module reader a forwarder binds under `local` through `from
/// .<stem> import`, and the owner's constant that reader sees.
#[derive(Clone, Copy)]
pub(super) struct Callee<'a> {
    pub(super) local: &'a str,
    pub(super) constant: &'a str,
    pub(super) reader: &'a Reader,
}

impl Scope<'_> {
    fn floats(&self) -> bool {
        self.floats
    }
}

/// The changed line of a module-level edit and the new-side source it sits in.
pub(super) struct ModuleEdit<'a> {
    pub(super) file: &'a Path,
    pub(super) line: usize,
    pub(super) line_text: &'a str,
    pub(super) old_line_text: Option<&'a str>,
    pub(super) new_source: &'a str,
}

/// The credits for an edit of a module-level constant. Empty when the edit is
/// not a certain constant replacement or no assertion distinguishes the values.
pub(super) fn module_constant_credits<'a>(
    edit: &ModuleEdit<'_>,
    module_owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Vec<ModuleValueCredit<'a>> {
    credits(edit, module_owner, owners, all_tests).unwrap_or_default()
}

fn credits<'a>(
    edit: &ModuleEdit<'_>,
    module_owner: &PythonOwner,
    owners: &[PythonOwner],
    all_tests: &'a [PythonTest],
) -> Option<Vec<ModuleValueCredit<'a>>> {
    let ModuleEdit {
        file,
        line,
        line_text,
        old_line_text,
        new_source,
    } = *edit;
    if let Some((name, parameters, new_body)) = lambda_line(line_text) {
        return lambda_credits(
            edit,
            (&name, &parameters, &new_body),
            module_owner,
            all_tests,
        );
    }
    let (name, new_value) = constant_line(line_text)?;
    let (old_name, old_value) = constant_line(old_line_text?)?;
    if name != old_name || old_value == new_value {
        return None;
    }
    // A dict literal is read in place by module functions only, and nothing
    // may change its contents (#9619).
    let table = matches!(new_value, Value::Table(_));
    if table != matches!(old_value, Value::Table(_)) {
        return None;
    }
    // The new module, with the changed line the one constant binding.
    let Mod::Module(new_module) = parse_module_result(file, new_source).ok()? else {
        return None;
    };
    if table {
        table_constant(new_source, &new_module.body, &name, line)?;
    } else {
        literal_constant(new_source, &new_module.body, &name, line)?;
    }
    // Another workspace file (production code, a conftest, a helper module)
    // that writes the attribute or patches the module can rebind it for every
    // test.
    if module_owner.foreign_writes.may_rebind(&name, file) {
        return None;
    }
    // The owner reaching its own table reflectively (`getattr(self_module,
    // "RATES")`) is invisible to the token scan, which skips string contents.
    if table
        && REFLECTIVE_SPELLINGS
            .iter()
            .any(|spelling| mentions_identifier(new_source, spelling))
    {
        return None;
    }
    if table
        && (module_owner.foreign_writes.may_mutate_table(&name, file)
            || !unsafe_table_uses(new_source, &|word| word == name, Some(line)).is_empty())
    {
        return None;
    }
    // Another test file writing the attribute can rebind it for every test.
    if all_tests
        .iter()
        .any(|test| test.constant_rebinding.attributes.contains(&name))
    {
        return None;
    }
    // The old module: the new one with the one changed line put back.
    let old_source = with_line_replaced(new_source, line, old_line_text?)?;
    let Mod::Module(old_module) = parse_module_result(file, &old_source).ok()? else {
        return None;
    };
    if table {
        table_constant(&old_source, &old_module.body, &name, line)?;
    } else {
        literal_constant(&old_source, &old_module.body, &name, line)?;
    }

    let new_facts = ModuleBindingFacts::of(new_source, &new_module.body);
    let old_facts = ModuleBindingFacts::of(&old_source, &old_module.body);
    // Module functions that read the constant, as the old and new module
    // define them.
    let mut readers: Vec<(&PythonOwner, Reader, Reader)> = Vec::new();
    for owner in owners.iter().filter(|owner| {
        owner.file == file
            && owner.owner_kind == Some(OwnerKind::Function)
            && owner.qualified_name == owner.name
            && owner.class_path.is_empty()
            && owner.decorators.is_empty()
            && owner.name != name
    }) {
        if !new_facts.bound_once(&owner.name)
            || !old_facts.bound_once(&owner.name)
            || module_owner.foreign_writes.may_rebind(&owner.name, file)
            || module_owner.foreign_writes.may_replace_reader_code()
        {
            continue;
        }
        let (Some(new_reader), Some(old_reader)) = (
            reader_of(&new_module.body, &owner.name, &name),
            reader_of(&old_module.body, &owner.name, &name),
        ) else {
            continue;
        };
        readers.push((owner, old_reader, new_reader));
    }

    let forwarders = forwarders_of(module_owner, file, new_source, &readers, owners);

    let mut out = Vec::new();
    for test in all_tests {
        // A test file edited in the same diff may assert the new value; only
        // an assertion the diff leaves alone is read as a pre-existing oracle.
        if super::test_activation::activation_control(test).is_some()
            || test.constant_rebinding.edited_in_diff
            || test.constant_rebinding.assigns_attribute(&name)
        {
            continue;
        }
        // A test never reads a table directly: only module functions do.
        let constant_spellings = if table {
            Vec::new()
        } else {
            constant_spellings(test, module_owner, &name)
        };
        for assertion in &test.assertions {
            let Some(expression) = assertion_expression(assertion) else {
                continue;
            };
            if !straight_line_assertion(test, assertion) {
                continue;
            }
            let Some(preceding) = test
                .assertions
                .iter()
                .filter(|prior| prior.line < assertion.line)
                .map(assertion_expression)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            // The constant read directly.
            if !constant_spellings.is_empty()
                && flips(
                    &expression,
                    &name,
                    (&old_value, &new_value),
                    &constant_spellings,
                    &[],
                    [(None, None), (None, None)],
                    &preceding,
                )
            {
                out.push(credit(
                    test,
                    assertion,
                    &name,
                    (&old_value, &new_value),
                    None,
                ));
                break;
            }
            // The constant read through a module function the test calls.
            let mut credited = false;
            for (owner, old_reader, new_reader) in &readers {
                if test.constant_rebinding.assigns_attribute(&owner.name) {
                    continue;
                }
                let spellings = reader_spellings(test, owner, &module_owner.foreign_writes);
                if spellings.is_empty()
                    || !flips(
                        &expression,
                        &name,
                        (&old_value, &new_value),
                        &constant_spellings,
                        &spellings,
                        [(Some(old_reader), None), (Some(new_reader), None)],
                        &preceding,
                    )
                {
                    continue;
                }
                out.push(credit(
                    test,
                    assertion,
                    &name,
                    (&old_value, &new_value),
                    Some(owner.name.clone()),
                ));
                credited = true;
                break;
            }
            if credited {
                break;
            }
            // The constant read through a reader a sibling module's function
            // forwards to (#9619).
            for forwarder in &forwarders {
                let (_, old_reader, new_reader) = &readers[forwarder.reader];
                if test
                    .constant_rebinding
                    .assigns_attribute(&forwarder.owner.name)
                    || test.imports.iter().any(|import| {
                        names_stdlib_module(&import.source_module)
                            && import_source_module_matches_owner(
                                import,
                                forwarder.owner,
                                &test.file,
                            )
                    })
                {
                    continue;
                }
                // Only `from <package>.<module> import forwarder`: a path that
                // drops the forwarder's package (a `src` package imported from
                // inside) breaks its relative import, and a module receiver
                // may be a name the package `__init__` rebinds.
                if test.imports.iter().any(|import| {
                    (import_source_module_matches_owner(import, forwarder.owner, &test.file)
                        || !submodule_receivers(import, forwarder.owner, &test.file).is_empty())
                        && !(import.imported == forwarder.owner.name
                            && import
                                .source_module
                                .split('.')
                                .collect::<Vec<_>>()
                                .ends_with(&[
                                    forwarder.package.as_str(),
                                    forwarder.module.as_str(),
                                ]))
                }) {
                    continue;
                }
                let spellings =
                    reader_spellings(test, forwarder.owner, &module_owner.foreign_writes);
                let callee = |reader| Callee {
                    local: &forwarder.local,
                    constant: &name,
                    reader,
                };
                if spellings.is_empty()
                    || !flips(
                        &expression,
                        "",
                        (&old_value, &new_value),
                        &[],
                        &spellings,
                        [
                            (Some(&forwarder.body), Some(callee(old_reader))),
                            (Some(&forwarder.body), Some(callee(new_reader))),
                        ],
                        &preceding,
                    )
                {
                    continue;
                }
                out.push(credit(
                    test,
                    assertion,
                    &name,
                    (&old_value, &new_value),
                    Some(forwarder.owner.name.clone()),
                ));
                credited = true;
                break;
            }
            if credited {
                break;
            }
        }
    }
    Some(out)
}

/// A sibling module's function that forwards to a reader of the constant:
/// `def quote(tier, units): return lookup(tier) * units` beside `from .table
/// import lookup` (#9619).
struct Forwarder<'o> {
    owner: &'o PythonOwner,
    /// The forwarder module's package directory name and module name.
    package: String,
    module: String,
    body: Reader,
    /// The forwarder module's name for the reader.
    local: String,
    /// The reader's index in the owner module's readers.
    reader: usize,
}

/// The forwarders of `readers` in the sibling modules the workspace pass
/// recorded for the owner module at `file`. A module counts only when its
/// own source and every other file's writes leave its bindings certain: no
/// reflection, dynamic writer, code-slot or dunder write, non-byte-exact
/// name, or other file or compiled module of its module name. The reader is
/// bound by one level-1 `from .<stem> import reader [as local]`, and both
/// names are bound once and rebound by no other file.
fn forwarders_of<'o>(
    module_owner: &PythonOwner,
    file: &Path,
    owner_source: &str,
    readers: &[(&PythonOwner, Reader, Reader)],
    owners: &'o [PythonOwner],
) -> Vec<Forwarder<'o>> {
    let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
        return Vec::new();
    };
    // The forwarder's relative import loads the owner's file only while no
    // other file or compiled module shares its name.
    if module_owner.foreign_writes.shares_module_name() {
        return Vec::new();
    }
    // The owner's own load must not import anything: a standard-library
    // import can resolve to a test-root module of the same name, and calls
    // such as `pickle.loads` or `pydoc.locate` import by string. Either can
    // close a cycle back to the forwarder module.
    let owner_inert = match parse_module_result(file, owner_source) {
        Ok(Mod::Module(parsed)) => inert_module(&parsed.body, None),
        _ => false,
    };
    let mut out = Vec::new();
    for module in &module_owner.foreign_writes.forwarders {
        let foreign = &module.foreign;
        let source = module.source.as_str();
        if foreign.may_replace_reader_code()
            || foreign.writes_dunder()
            || foreign.reflection_elsewhere()
            || foreign.shares_module_name()
            || foreign.names_may_differ_elsewhere()
            || mentions_reflection(source)
            || names_may_differ(source)
            || super::call_chain::has_dynamic_word(source)
            || DYNAMIC_WRITERS
                .iter()
                .any(|word| mentions_identifier(source, word))
        {
            continue;
        }
        let (Some(module_stem), Some(package)) = (
            module.file.file_stem().and_then(|stem| stem.to_str()),
            module
                .file
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str()),
        ) else {
            continue;
        };
        // The owner importing the forwarder module makes the import order
        // matter: loading it first can meet a partly initialized module.
        if mentions_identifier(owner_source, module_stem) || !owner_inert {
            continue;
        }
        let Ok(Mod::Module(parsed)) = parse_module_result(&module.file, source) else {
            continue;
        };
        let facts = ModuleBindingFacts::of(source, &parsed.body);
        if facts.assigns_dunder() || !inert_module(&parsed.body, Some(stem)) {
            continue;
        }
        let mut callees: Vec<(String, usize)> = Vec::new();
        for statement in &parsed.body {
            let Stmt::ImportFrom(import) = statement else {
                continue;
            };
            if import.level.map(|level| level.to_u32()) != Some(1)
                || import.module.as_deref() != Some(stem)
            {
                continue;
            }
            for alias in &import.names {
                let local = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                let Some(index) = readers
                    .iter()
                    .position(|(reader, _, _)| reader.name == alias.name.as_str())
                else {
                    continue;
                };
                if facts.bound_once(&local) && !foreign.may_rebind(&local, &module.file) {
                    callees.push((local, index));
                }
            }
        }
        for (local, index) in callees {
            for owner in owners.iter().filter(|owner| {
                owner.file == module.file
                    && owner.owner_kind == Some(OwnerKind::Function)
                    && owner.qualified_name == owner.name
                    && owner.class_path.is_empty()
                    && owner.decorators.is_empty()
            }) {
                if !facts.bound_once(&owner.name) || foreign.may_rebind(&owner.name, &module.file) {
                    continue;
                }
                let Some(body) = reader_of(&parsed.body, &owner.name, &local) else {
                    continue;
                };
                out.push(Forwarder {
                    owner,
                    package: package.to_string(),
                    module: module_stem.to_string(),
                    body,
                    local: local.clone(),
                    reader: index,
                });
            }
        }
    }
    out
}

/// Whether loading a module runs nothing but a docstring, `pass`,
/// `from __future__` imports, assignments of literals, undecorated `def`s
/// whose defaults are literals and whose annotations are names or strings,
/// and, for a forwarder module, its `from .<relative>` import of the owner.
/// Any other statement could call the forwarder, or import a module that
/// does, before the forwarder's import binds the reader.
fn inert_module(body: &[Stmt], relative: Option<&str>) -> bool {
    // Without `from __future__ import annotations` an annotation runs while
    // the module loads (before 3.14), and a name it reads may be unbound.
    let postponed = body.iter().any(|statement| {
        matches!(statement, Stmt::ImportFrom(import)
            if import.module.as_deref() == Some("__future__")
                && import.names.iter().any(|alias| alias.name.as_str() == "annotations"))
    });
    // A literal bound to a dunder (`__package__ = ""`) changes what the
    // relative import resolves to; `__all__` alone is inert.
    let plain_target = |target: &Expr| {
        matches!(target, Expr::Name(name)
            if name.id.as_str() == "__all__" || !name.id.as_str().starts_with("__"))
    };
    let annotation = |expr: Option<&Expr>| match expr {
        None => true,
        Some(Expr::Constant(constant)) => matches!(constant.value, ast::Constant::Str(_)),
        Some(Expr::Name(_)) => postponed,
        Some(_) => false,
    };
    fn literal(expr: &Expr) -> bool {
        match expr {
            Expr::Constant(_) => true,
            Expr::List(list) => list.elts.iter().all(literal),
            Expr::Tuple(tuple) => tuple.elts.iter().all(literal),
            Expr::Dict(dict) => {
                dict.keys
                    .iter()
                    .all(|key| key.as_ref().is_some_and(literal))
                    && dict.values.iter().all(literal)
            }
            _ => false,
        }
    }
    body.iter().all(|statement| match statement {
        Stmt::Pass(_) => true,
        Stmt::Expr(expr) => matches!(expr.value.as_ref(), Expr::Constant(_)),
        Stmt::ImportFrom(import) => match import.level.map(|level| level.to_u32()) {
            Some(1) => relative.is_some() && import.module.as_deref() == relative,
            None | Some(0) => import.module.as_deref() == Some("__future__"),
            Some(_) => false,
        },
        Stmt::Assign(assign) => assign.targets.iter().all(plain_target) && literal(&assign.value),
        Stmt::AnnAssign(assign) => {
            plain_target(&assign.target)
                && annotation(Some(&assign.annotation))
                && assign.value.as_deref().is_none_or(literal)
        }
        Stmt::FunctionDef(function) => {
            let args = &function.args;
            function.decorator_list.is_empty()
                && annotation(function.returns.as_deref())
                && args
                    .posonlyargs
                    .iter()
                    .chain(&args.args)
                    .chain(&args.kwonlyargs)
                    .all(|arg| {
                        annotation(arg.def.annotation.as_deref())
                            && arg.default.as_deref().is_none_or(literal)
                    })
                && args
                    .vararg
                    .iter()
                    .chain(&args.kwarg)
                    .all(|arg| annotation(arg.annotation.as_deref()))
        }
        _ => false,
    })
}

/// The credits for a body-only edit of a top-level `NAME = lambda ...`
/// binding: an assertion that holds with the old body and fails with the new
/// one when the test calls the lambda with literal arguments.
fn lambda_credits<'a>(
    edit: &ModuleEdit<'_>,
    (name, parameters, new_body): (&str, &[String], &Expr),
    module_owner: &PythonOwner,
    all_tests: &'a [PythonTest],
) -> Option<Vec<ModuleValueCredit<'a>>> {
    let ModuleEdit {
        file,
        line,
        old_line_text,
        new_source,
        ..
    } = *edit;
    let old_line_text = old_line_text?;
    let (old_name, old_parameters, old_body) = lambda_line(old_line_text)?;
    // The binding header (target and any annotation) is evaluated at import,
    // so a changed one is not a body-only edit.
    if old_name != name
        || old_parameters != parameters
        || binding_header(old_line_text) != binding_header(edit.line_text)
    {
        return None;
    }
    // Another workspace file or test file that writes the attribute or patches
    // the module can rebind the name for every test.
    if module_owner.foreign_writes.may_rebind(name, file)
        || all_tests.iter().any(|test| {
            test.constant_rebinding
                .attributes
                .contains(&name.to_string())
        })
    {
        return None;
    }
    // The name is bound once, by this line, in both the new and the old module.
    let old_source = with_line_replaced(new_source, line, old_line_text)?;
    for source in [new_source, old_source.as_str()] {
        let Mod::Module(module) = parse_module_result(file, source).ok()? else {
            return None;
        };
        if !ModuleBindingFacts::of(source, &module.body).bound_once(name) {
            return None;
        }
    }
    let (old_reader, new_reader) = (
        Reader {
            parameters: parameters.to_vec(),
            expression: old_body,
        },
        Reader {
            parameters: parameters.to_vec(),
            expression: new_body.clone(),
        },
    );
    // `owner_module_callees` reads function owners, not the module owner.
    let mut callee = module_owner.clone();
    callee.owner_kind = Some(OwnerKind::Function);
    callee.name = name.to_string();
    callee.qualified_name = name.to_string();
    let unused = Value::Bool(false);

    let mut out = Vec::new();
    for test in all_tests {
        if super::test_activation::activation_control(test).is_some()
            || test.constant_rebinding.edited_in_diff
            || test.constant_rebinding.assigns_attribute(name)
        {
            continue;
        }
        let spellings = reader_spellings(test, &callee, &module_owner.foreign_writes);
        if spellings.is_empty() {
            continue;
        }
        for assertion in &test.assertions {
            let Some(expression) = assertion_expression(assertion) else {
                continue;
            };
            if !straight_line_assertion(test, assertion) {
                continue;
            }
            let Some(preceding) = test
                .assertions
                .iter()
                .filter(|prior| prior.line < assertion.line)
                .map(assertion_expression)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            if flips(
                &expression,
                "",
                (&unused, &unused),
                &[],
                &spellings,
                [(Some(&old_reader), None), (Some(&new_reader), None)],
                &preceding,
            ) {
                out.push(ModuleValueCredit {
                    test,
                    assertion,
                    constant: name.to_string(),
                    old: lambda_rendering(old_line_text),
                    new: lambda_rendering(edit.line_text),
                    reader: Some(name.to_string()),
                });
                break;
            }
        }
    }
    Some(out)
}

/// `NAME = lambda a, b: body` on one line: the name, its positional-only
/// parameters (no defaults, `*args` or keywords) and the body.
fn lambda_line(text: &str) -> Option<(String, Vec<String>, Expr)> {
    let Mod::Module(module) = parse_module_result(Path::new("line.py"), text.trim_end()).ok()?
    else {
        return None;
    };
    let [stmt] = module.body.as_slice() else {
        return None;
    };
    let (target, value) = match stmt {
        Stmt::Assign(assign) => match assign.targets.as_slice() {
            [target] => (target, assign.value.as_ref()),
            _ => return None,
        },
        Stmt::AnnAssign(assign) if assign.simple => {
            (assign.target.as_ref(), assign.value.as_deref()?)
        }
        _ => return None,
    };
    let (Expr::Name(name), Expr::Lambda(lambda)) = (target, value) else {
        return None;
    };
    let args = &lambda.args;
    if args.vararg.is_some()
        || args.kwarg.is_some()
        || !args.kwonlyargs.is_empty()
        || args
            .posonlyargs
            .iter()
            .chain(args.args.iter())
            .any(|arg| arg.default.is_some())
    {
        return None;
    }
    let parameters = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|arg| arg.def.arg.to_string())
        .collect();
    Some((
        name.id.to_string(),
        parameters,
        lambda.body.as_ref().clone(),
    ))
}

/// The text before the `=` of a one-line binding: its target and annotation.
fn binding_header(line: &str) -> &str {
    line.split_once('=')
        .map_or(line, |(header, _)| header)
        .trim()
}

/// The lambda expression of an edited line, for the evidence text.
fn lambda_rendering(line: &str) -> String {
    line.split_once('=')
        .map_or(line, |(_, value)| value)
        .trim()
        .to_string()
}

fn credit<'a>(
    test: &'a PythonTest,
    assertion: &'a PythonAssertion,
    name: &str,
    values: (&Value, &Value),
    reader: Option<String>,
) -> ModuleValueCredit<'a> {
    ModuleValueCredit {
        test,
        assertion,
        constant: name.to_string(),
        old: values.0.render(),
        new: values.1.render(),
        reader,
    }
}

/// Whether the assertion holds under the old value and fails under the new
/// one, with every part of both evaluations decided.
fn flips(
    expression: &Expr,
    name: &str,
    values: (&Value, &Value),
    constant_spellings: &[String],
    reader_spellings: &[String],
    sides: [(Option<&Reader>, Option<Callee<'_>>); 2],
    preceding: &[Expr],
) -> bool {
    let outcome = |value: &Value, reader: Option<&Reader>, callee: Option<Callee<'_>>| {
        let scope = Scope {
            constant: name,
            value,
            parameters: None,
            constant_spellings,
            reader_spellings,
            reader,
            model: None,
            receiver: None,
            floats: false,
            callee,
        };
        if !preceding
            .iter()
            .all(|prior| evaluate(prior, &scope).is_some_and(|value| value.truthy()))
        {
            return None;
        }
        evaluate(expression, &scope).map(|result| result.truthy())
    };
    let [(old_reader, old_callee), (new_reader, new_callee)] = sides;
    outcome(values.0, old_reader, old_callee) == Some(true)
        && outcome(values.1, new_reader, new_callee) == Some(false)
}

fn constant_line(text: &str) -> Option<(String, Value)> {
    let module = parse_module_result(Path::new("line.py"), text.trim_end()).ok()?;
    let Mod::Module(module) = module else {
        return None;
    };
    let [stmt] = module.body.as_slice() else {
        return None;
    };
    let (target, value) = match stmt {
        Stmt::Assign(assign) => match assign.targets.as_slice() {
            [target] => (target, assign.value.as_ref()),
            _ => return None,
        },
        Stmt::AnnAssign(assign) if assign.simple => {
            (assign.target.as_ref(), assign.value.as_deref()?)
        }
        _ => return None,
    };
    let Expr::Name(name) = target else {
        return None;
    };
    let empty = Scope {
        constant: "",
        value: &Value::Bool(false),
        parameters: None,
        constant_spellings: &[],
        reader_spellings: &[],
        reader: None,
        model: None,
        receiver: None,
        floats: false,
        callee: None,
    };
    if let Expr::Dict(dict) = value {
        return Some((name.id.to_string(), table_of(dict, &empty)?));
    }
    Some((name.id.to_string(), evaluate(value, &empty)?))
}

/// A dict literal of unique `str` or `int` keys and scalar values.
fn table_of(dict: &ast::ExprDict, scope: &Scope<'_>) -> Option<Value> {
    const MAX_ENTRIES: usize = 1024;
    if dict.keys.len() > MAX_ENTRIES {
        return None;
    }
    let mut entries: Vec<(Value, Value)> = Vec::new();
    for (key, value) in dict.keys.iter().zip(&dict.values) {
        let key = evaluate(key.as_ref()?, scope)?;
        let value = evaluate(value, scope)?;
        if !matches!(key, Value::Int(_) | Value::Str(_))
            || matches!(value, Value::Table(_))
            || entries.iter().any(|(seen, _)| *seen == key)
        {
            return None;
        }
        entries.push((key, value));
    }
    Some(Value::Table(entries))
}

fn literal_constant(
    source: &str,
    body: &[Stmt],
    name: &str,
    line: usize,
) -> Option<PythonModuleConstant> {
    let text = SourceText::new(source);
    module_literal_constants(&text, body)
        .into_iter()
        .find(|constant| constant.name == name && constant.line == line)
}

fn table_constant(source: &str, body: &[Stmt], name: &str, line: usize) -> Option<()> {
    let text = SourceText::new(source);
    module_table_binding_at(&text, body, name, line).then_some(())
}

/// `source` with the 1-indexed `line` replaced by one line of `replacement`.
pub(super) fn with_line_replaced(source: &str, line: usize, replacement: &str) -> Option<String> {
    if replacement.contains('\n') || replacement.contains('\r') {
        return None;
    }
    let mut out = String::with_capacity(source.len());
    let mut replaced = false;
    for (index, text) in source.split_inclusive('\n').enumerate() {
        if index + 1 == line {
            out.push_str(replacement);
            if text.ends_with('\n') {
                out.push('\n');
            }
            replaced = true;
        } else {
            out.push_str(text);
        }
    }
    replaced.then_some(out)
}

/// The top-level function `function` as an evaluable reader of `constant`:
/// undecorated, positional parameters only, a body of one `return` expression
/// (after an optional docstring) that names the constant. Defined exactly once
/// in the module.
fn reader_of(module: &[Stmt], function: &str, constant: &str) -> Option<Reader> {
    if count_bindings(module, function) != 1 {
        return None;
    }
    let definition = module.iter().find_map(|stmt| match stmt {
        Stmt::FunctionDef(definition) if definition.name.as_str() == function => Some(definition),
        _ => None,
    })?;
    let args = &definition.args;
    if !definition.decorator_list.is_empty()
        || args.vararg.is_some()
        || args.kwarg.is_some()
        || !args.kwonlyargs.is_empty()
        || args
            .posonlyargs
            .iter()
            .chain(args.args.iter())
            .any(|arg| arg.default.is_some())
    {
        return None;
    }
    let parameters: Vec<String> = args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .map(|arg| arg.def.arg.to_string())
        .collect();
    // A parameter named like the constant shadows it.
    if parameters.iter().any(|parameter| parameter == constant) {
        return None;
    }
    let body = match definition.body.as_slice() {
        [Stmt::Expr(docstring), rest @ ..]
            if matches!(docstring.value.as_ref(), Expr::Constant(_)) && !rest.is_empty() =>
        {
            rest
        }
        all => all,
    };
    let [Stmt::Return(ret)] = body else {
        return None;
    };
    let expression = ret.value.as_deref()?.clone();
    names_in(&expression)
        .iter()
        .any(|used| used == constant)
        .then_some(Reader {
            parameters,
            expression,
        })
}

/// Every bare name an expression reads.
fn names_in(expr: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    collect_names(expr, &mut out);
    out
}

fn collect_names(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Name(name) => out.push(name.id.to_string()),
        Expr::BoolOp(op) => op.values.iter().for_each(|value| collect_names(value, out)),
        Expr::BinOp(op) => {
            collect_names(&op.left, out);
            collect_names(&op.right, out);
        }
        Expr::UnaryOp(op) => collect_names(&op.operand, out),
        Expr::IfExp(op) => {
            collect_names(&op.test, out);
            collect_names(&op.body, out);
            collect_names(&op.orelse, out);
        }
        Expr::Compare(op) => {
            collect_names(&op.left, out);
            op.comparators
                .iter()
                .for_each(|value| collect_names(value, out));
        }
        Expr::Attribute(attribute) => collect_names(&attribute.value, out),
        Expr::Subscript(subscript) => {
            collect_names(&subscript.value, out);
            collect_names(&subscript.slice, out);
        }
        Expr::Call(call) => {
            collect_names(&call.func, out);
            call.args.iter().for_each(|value| collect_names(value, out));
        }
        _ => {}
    }
}

/// The spellings with which `test` reads the module's constant: the name it
/// imports from the module (`from settings import LIMIT as L`) and `module.LIMIT`
/// for a receiver that reaches the module. A spelling whose root the test binds
/// itself is not one.
pub(super) fn constant_spellings(
    test: &PythonTest,
    module_owner: &PythonOwner,
    name: &str,
) -> Vec<String> {
    let mut spellings = Vec::new();
    for import in &test.imports {
        if import.imported == name
            && import_source_module_matches_owner(import, module_owner, &test.file)
        {
            spellings.push(import.alias.clone());
        }
        for receiver in submodule_receivers(import, module_owner, &test.file) {
            if !package_init_refuses_submodule(&module_owner.file, &module_owner.foreign_writes) {
                spellings.push(format!("{receiver}.{name}"));
            }
        }
    }
    keep_unshadowed(test, spellings)
}

pub(super) fn reader_spellings(
    test: &PythonTest,
    owner: &PythonOwner,
    module_foreign: &ForeignWrites,
) -> Vec<String> {
    let mut callees = owner_module_callees(test, owner);
    if package_init_refuses_submodule(&owner.file, module_foreign) {
        let blocked = test
            .imports
            .iter()
            .flat_map(|import| submodule_receivers(import, owner, &test.file))
            .collect::<std::collections::BTreeSet<_>>();
        callees.retain(|callee| {
            blocked
                .iter()
                .all(|receiver| !callee.starts_with(&format!("{receiver}.")))
        });
    }
    keep_unshadowed(test, callees)
}

pub(super) fn keep_unshadowed(test: &PythonTest, mut spellings: Vec<String>) -> Vec<String> {
    spellings.retain(|spelling| {
        let root = spelling.split('.').next().unwrap_or(spelling);
        !test.constant_rebinding.rebinds(root)
            && !test.constant_rebinding.module_rebinds(root)
            && !test.fixtures.iter().any(|fixture| fixture == root)
    });
    spellings.sort();
    spellings.dedup();
    spellings
}

/// The condition of a bare `assert <condition>` assertion.
pub(super) fn assertion_expression(assertion: &PythonAssertion) -> Option<Expr> {
    let text = assertion.gate_text.trim();
    if !text.starts_with("assert ") {
        return None;
    }
    let Mod::Module(module) = parse_module_result(Path::new("assertion.py"), text).ok()? else {
        return None;
    };
    match module.body.as_slice() {
        [Stmt::Assert(assert)] => Some(assert.test.as_ref().clone()),
        _ => None,
    }
}

/// Statement words that can leave a test before a later assertion runs, or
/// run it conditionally.
const GUARDING_STATEMENTS: [&str; 22] = [
    "return", "raise", "yield", "if", "elif", "else", "try", "except", "finally", "with", "for",
    "while", "del", "global", "nonlocal", "def", "class", "lambda", "break", "continue", "match",
    "async",
];

/// Words that end or skip a test when called.
const LEAVING_CALLS: [&str; 12] = [
    "skipTest",
    "SkipTest",
    "quit",
    "pytest_exit",
    "Exit",
    "skip",
    "xfail",
    "importorskip",
    "exit",
    "_exit",
    "abort",
    "fail",
];

/// Whether the assertion is a direct statement of the test body that every run
/// reaches: nothing before it returns, raises, branches, loops, enters a
/// context or calls a skipping function.
pub(super) fn straight_line_assertion(test: &PythonTest, assertion: &PythonAssertion) -> bool {
    // A call of a generator test does not execute its body.
    if rustpython_parser::lexer::lex(&test.body_text, rustpython_parser::Mode::Module).any(
        |token| match token {
            Ok((rustpython_parser::Tok::Yield, _)) | Err(_) => true,
            // The lexer keeps formatted expressions inside a string token;
            // they may contain `yield` before or after this assertion.
            Ok((rustpython_parser::Tok::String { kind, .. }, _)) => kind.is_any_fstring(),
            _ => false,
        },
    ) {
        return false;
    }
    let lines: Vec<&str> = test.body_text.lines().collect();
    let Some(header) = lines.iter().position(|line| {
        let line = line.trim_start();
        line.starts_with("def ") || line.starts_with("async def ")
    }) else {
        return false;
    };
    if !lines[header].trim_end().ends_with(':') {
        return false;
    }
    let Some(index) = assertion.line.checked_sub(test.line) else {
        return false;
    };
    if index <= header || index >= lines.len() {
        return false;
    }
    let indent = |line: &str| line.len() - line.trim_start().len();
    let statement = |line: &&str| {
        let trimmed = line.trim_start();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    };
    let Some(base) = lines[header + 1..]
        .iter()
        .copied()
        .find(statement)
        .map(indent)
    else {
        return false;
    };
    let target = lines[index];
    if indent(target) != base || !target.trim_start().starts_with("assert ") {
        return false;
    }
    // Bind the extracted assertion to the statement that actually starts this
    // physical line; an earlier `assert False;` must not stand in for it.
    // Semantic gate text may be normalized by the boolean-oracle pass.
    let Some(first_line) = assertion.text.lines().next() else {
        return false;
    };
    let Some(tail) = target.trim_start().strip_prefix(first_line.trim()) else {
        return false;
    };
    if !tail.trim_start().is_empty() && !tail.trim_start().starts_with('#') {
        return false;
    }
    lines[header + 1..index].iter().all(|line| {
        let trimmed = line.trim_start();
        if !statement(line) {
            return true;
        }
        // A continuation line can finish a base-suite statement and then
        // return (`value = (\n        1); return`). Its physical indentation
        // does not make the trailing statement a nested suite.
        let guarding = trimmed.split(';').any(|part| {
            let first_word: String = part
                .trim_start()
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect();
            GUARDING_STATEMENTS.contains(&first_word.as_str())
        });
        !guarding
            && !LEAVING_CALLS
                .iter()
                .any(|word| mentions_identifier(trimmed, word))
    })
}

pub(super) fn evaluate(expr: &Expr, scope: &Scope<'_>) -> Option<Value> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Int(value) => value.to_string().parse().ok().map(Value::Int),
            ast::Constant::Bool(value) => Some(Value::Bool(*value)),
            ast::Constant::Str(value) => Some(Value::Str(value.clone())),
            ast::Constant::Float(value) if scope.floats() => Some(Value::Float(*value)),
            _ => None,
        },
        Expr::Name(name) => match scope.parameters {
            Some(parameters) => parameters
                .iter()
                .find(|(parameter, _)| parameter == name.id.as_str())
                .map(|(_, value)| value.clone())
                .or_else(|| (name.id.as_str() == scope.constant).then(|| scope.value.clone())),
            None => scope
                .constant_spellings
                .iter()
                .any(|spelling| spelling == name.id.as_str())
                .then(|| scope.value.clone()),
        },
        // `self.field` inside a dataclass method.
        Expr::Attribute(attribute) if scope.receiver.is_some() => {
            let (receiver, fields) = scope.receiver?;
            let Expr::Name(base) = attribute.value.as_ref() else {
                return None;
            };
            (base.id.as_str() == receiver)
                .then(|| {
                    fields
                        .iter()
                        .find(|(name, _)| name == attribute.attr.as_str())
                        .map(|(_, value)| value.clone())
                })
                .flatten()
        }
        // `Class(...).field` in a test.
        Expr::Attribute(attribute) if scope.model.is_some() => {
            let Expr::Call(constructor) = attribute.value.as_ref() else {
                return None;
            };
            let instance = scope.model?.instance(constructor, scope)?;
            instance
                .into_iter()
                .find(|(name, _)| name == attribute.attr.as_str())
                .and_then(|(_, value)| value)
        }
        Expr::Attribute(_) if scope.parameters.is_none() => {
            let full = expr_full_name(expr)?;
            scope
                .constant_spellings
                .contains(&full)
                .then(|| scope.value.clone())
        }
        Expr::UnaryOp(unary) => {
            let operand = evaluate(&unary.operand, scope)?;
            match unary.op {
                ast::UnaryOp::Not => Some(Value::Bool(!operand.truthy())),
                ast::UnaryOp::USub => match operand {
                    Value::Float(value) if scope.floats() => Some(Value::Float(-value)),
                    other => other.number()?.checked_neg().map(Value::Int),
                },
                ast::UnaryOp::UAdd => match operand {
                    Value::Float(value) if scope.floats() => Some(Value::Float(value)),
                    other => other.number().map(Value::Int),
                },
                ast::UnaryOp::Invert => None,
            }
        }
        Expr::BinOp(binary) => {
            let left = evaluate(&binary.left, scope)?;
            let right = evaluate(&binary.right, scope)?;
            arithmetic(&binary.op, &left, &right, scope.floats())
        }
        Expr::BoolOp(bool_op) => {
            let mut last = None;
            for operand in &bool_op.values {
                let value = evaluate(operand, scope)?;
                let stops = match bool_op.op {
                    ast::BoolOp::And => !value.truthy(),
                    ast::BoolOp::Or => value.truthy(),
                };
                if stops {
                    return Some(value);
                }
                last = Some(value);
            }
            last
        }
        Expr::IfExp(conditional) => {
            if evaluate(&conditional.test, scope)?.truthy() {
                evaluate(&conditional.body, scope)
            } else {
                evaluate(&conditional.orelse, scope)
            }
        }
        Expr::Compare(compare) => {
            let mut left = evaluate(&compare.left, scope)?;
            for (op, comparator) in compare.ops.iter().zip(&compare.comparators) {
                let right = evaluate(comparator, scope)?;
                if !comparison(op, &left, &right, scope.floats())? {
                    return Some(Value::Bool(false));
                }
                left = right;
            }
            Some(Value::Bool(true))
        }
        // `Class(...).method(...)` in a test.
        Expr::Call(call)
            if scope.model.is_some()
                && matches!(
                    call.func.as_ref(),
                    Expr::Attribute(attribute) if matches!(attribute.value.as_ref(), Expr::Call(_))
                ) =>
        {
            let Expr::Attribute(attribute) = call.func.as_ref() else {
                return None;
            };
            let Expr::Call(constructor) = attribute.value.as_ref() else {
                return None;
            };
            let model = scope.model?;
            let instance = model.instance(constructor, scope)?;
            model.run_method(
                attribute.attr.as_str(),
                &call.args,
                &call.keywords,
                &instance,
                scope,
            )
        }
        // The builtin `round(x)`, unshadowed.
        Expr::Call(call) if scope.floats() && is_builtin_round(call) => {
            let [argument] = call.args.as_slice() else {
                return None;
            };
            match evaluate(argument, scope)? {
                Value::Float(value) => {
                    let rounded = value.round_ties_even();
                    (rounded.is_finite() && rounded.abs() < FLOAT_EXACT_LIMIT as f64)
                        .then_some(Value::Int(rounded as i128))
                }
                other => other.number().map(Value::Int),
            }
        }
        Expr::Call(call) if scope.parameters.is_none() => {
            let callee = expr_full_name(&call.func)?;
            if !call.keywords.is_empty() || !scope.reader_spellings.contains(&callee) {
                return None;
            }
            let reader = scope.reader?;
            if call.args.len() != reader.parameters.len() {
                return None;
            }
            let arguments: Vec<(String, Value)> = reader
                .parameters
                .iter()
                .zip(&call.args)
                .map(|(parameter, argument)| Some((parameter.clone(), evaluate(argument, scope)?)))
                .collect::<Option<_>>()?;
            // A forwarder's names are its own module's: the owner's constant
            // is visible only inside the reader it calls.
            let inner = Scope {
                constant: if scope.callee.is_some() {
                    ""
                } else {
                    scope.constant
                },
                value: scope.value,
                parameters: Some(&arguments),
                constant_spellings: &[],
                reader_spellings: &[],
                reader: None,
                model: None,
                receiver: None,
                floats: false,
                callee: scope.callee,
            };
            evaluate(&reader.expression, &inner)
        }
        // A forwarder's call of the reader it imported, by its local name
        // and positional arguments only (#9619). One hop: the reader runs
        // with no callee of its own.
        Expr::Call(call)
            if scope.parameters.is_some()
                && scope.callee.is_some_and(|callee| {
                    matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == callee.local)
                }) =>
        {
            let callee = scope.callee?;
            if !call.keywords.is_empty() || call.args.len() != callee.reader.parameters.len() {
                return None;
            }
            let arguments: Vec<(String, Value)> = callee
                .reader
                .parameters
                .iter()
                .zip(&call.args)
                .map(|(parameter, argument)| Some((parameter.clone(), evaluate(argument, scope)?)))
                .collect::<Option<_>>()?;
            let inner = Scope {
                constant: callee.constant,
                value: scope.value,
                parameters: Some(&arguments),
                constant_spellings: &[],
                reader_spellings: &[],
                reader: None,
                model: None,
                receiver: None,
                floats: false,
                callee: None,
            };
            evaluate(&callee.reader.expression, &inner)
        }
        // A dict constant read inside a module function (#9619): `NAME[key]`.
        // A missing key raises, which decides nothing.
        Expr::Subscript(subscript) if scope.parameters.is_some() => {
            let table = evaluate(&subscript.value, scope)?;
            let key = evaluate(&subscript.slice, scope)?;
            table.lookup(&key).cloned()
        }
        // `NAME.get(key)` and `NAME.get(key, default)`. The default is
        // evaluated whether or not the key is present, as Python does.
        Expr::Call(call) if scope.parameters.is_some() => {
            let Expr::Attribute(attribute) = call.func.as_ref() else {
                return None;
            };
            if attribute.attr.as_str() != "get" || !call.keywords.is_empty() {
                return None;
            }
            let table = evaluate(&attribute.value, scope)?;
            // `.get` on a scalar raises AttributeError: undecided, not absent.
            if !matches!(table, Value::Table(_)) {
                return None;
            }
            let (key, default) = match call.args.as_slice() {
                [key] => (key, None),
                [key, default] => (key, Some(default)),
                _ => return None,
            };
            let key = evaluate(key, scope)?;
            // An unsupported key type (`True` hashes as `1`) is unknown, not
            // absent: it must not fall through to the default.
            if !matches!(key, Value::Int(_) | Value::Str(_)) {
                return None;
            }
            let fallback = match default {
                Some(default) => Some(evaluate(default, scope)?),
                None => None,
            };
            match table.lookup(&key) {
                Some(value) => Some(value.clone()),
                None => fallback,
            }
        }
        _ => None,
    }
}

pub(super) fn arithmetic(
    op: &ast::Operator,
    left: &Value,
    right: &Value,
    floats: bool,
) -> Option<Value> {
    if let (Value::Str(left), Value::Str(right), ast::Operator::Add) = (left, right, op) {
        return Some(Value::Str(format!("{left}{right}")));
    }
    if floats {
        let float_operand = matches!(left, Value::Float(_)) || matches!(right, Value::Float(_));
        if float_operand || matches!(op, ast::Operator::Div) {
            let (left, right) = (left.float()?, right.float()?);
            let result = match op {
                ast::Operator::Add => left + right,
                ast::Operator::Sub => left - right,
                ast::Operator::Mult => left * right,
                ast::Operator::Div if right != 0.0 => left / right,
                _ => return None,
            };
            return result.is_finite().then_some(Value::Float(result));
        }
    }
    let (left, right) = (left.number()?, right.number()?);
    match op {
        ast::Operator::Add => left.checked_add(right),
        ast::Operator::Sub => left.checked_sub(right),
        ast::Operator::Mult => left.checked_mul(right),
        ast::Operator::FloorDiv => {
            if right == 0 {
                return None;
            }
            let quotient = left.checked_div(right)?;
            let adjust = left.checked_rem(right)? != 0 && ((left < 0) != (right < 0));
            quotient.checked_sub(i128::from(adjust))
        }
        ast::Operator::Mod => {
            if right == 0 {
                return None;
            }
            let remainder = left.checked_rem(right)?;
            if remainder != 0 && ((remainder < 0) != (right < 0)) {
                remainder.checked_add(right)
            } else {
                Some(remainder)
            }
        }
        _ => None,
    }
    .map(Value::Int)
}

pub(super) fn comparison(
    op: &ast::CmpOp,
    left: &Value,
    right: &Value,
    floats: bool,
) -> Option<bool> {
    let ordering = match (left, right) {
        (Value::Str(left), Value::Str(right)) => Some(left.cmp(right)),
        _ if floats && (matches!(left, Value::Float(_)) || matches!(right, Value::Float(_))) => {
            left.float()?.partial_cmp(&right.float()?)
        }
        _ => left
            .number()
            .zip(right.number())
            .map(|(left, right)| left.cmp(&right)),
    };
    match op {
        ast::CmpOp::Eq => Some(ordering == Some(std::cmp::Ordering::Equal)),
        ast::CmpOp::NotEq => Some(ordering != Some(std::cmp::Ordering::Equal)),
        ast::CmpOp::Lt => Some(ordering? == std::cmp::Ordering::Less),
        ast::CmpOp::LtE => Some(ordering? != std::cmp::Ordering::Greater),
        ast::CmpOp::Gt => Some(ordering? == std::cmp::Ordering::Greater),
        ast::CmpOp::GtE => Some(ordering? != std::cmp::Ordering::Less),
        _ => None,
    }
}

/// `round(...)` as the builtin: the model checked at build time that nothing
/// in the module or the test rebinds the name.
fn is_builtin_round(call: &ast::ExprCall) -> bool {
    matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "round")
        && call.keywords.is_empty()
}
