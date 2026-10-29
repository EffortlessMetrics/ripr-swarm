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
use super::table_reads::{dict_literal_names, unsafe_table_uses};
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
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ForeignWrites {
    attributes: Vec<String>,
    dynamic_words: Vec<String>,
    /// Dict-literal module constants some other file may mutate or alias.
    table_unsafe: Vec<String>,
    /// Every word of the files that can reach a module's objects reflectively.
    reflective_words: Vec<String>,
    /// Module names (file stems and package directories) in the import
    /// component of some other reflective file: it may reach those modules
    /// through an alias or re-export no word of it names.
    reflective_reach: Vec<String>,
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
        .any(|spelling| mentions_identifier(source, spelling))
        || (mentions_identifier(source, "sys") && mentions_identifier(source, "modules"))
}

/// Words that replace what an `import` binds (#9226).
const IMPORT_SYSTEM: [&str; 11] = [
    "__path__",
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
    references: std::collections::BTreeMap<&'a str, Option<&'a Path>>,
    hidden_import: bool,
    /// For each reflective file, the module names in its import component.
    reflective_reach: Vec<(&'a Path, std::collections::BTreeSet<String>)>,
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
    attributes: std::collections::BTreeMap<String, usize>,
    dynamic_words: std::collections::BTreeMap<String, usize>,
    table_unsafe: std::collections::BTreeMap<String, usize>,
    reflective_words: std::collections::BTreeMap<String, usize>,
    reflective_reach: std::collections::BTreeMap<String, usize>,
}

impl ForeignTotals {
    fn build(
        files: &[IndexedSource<'_>],
        reflective_reach: &[(&Path, std::collections::BTreeSet<String>)],
    ) -> Self {
        let mut totals = Self::default();
        for indexed in files {
            totals.accumulate(indexed);
        }
        for (_, reach) in reflective_reach {
            count_sources(
                &mut totals.reflective_reach,
                reach.iter().map(String::as_str),
            );
        }
        totals
    }

    /// Count the writes one source contributes to each [`ForeignWrites`] set:
    /// assigned attributes, dynamic words, unsafe tables, reflective words.
    fn accumulate(&mut self, source: &IndexedSource<'_>) {
        count_sources(
            &mut self.attributes,
            source.assigned.iter().flatten().map(String::as_str),
        );
        count_sources(
            &mut self.dynamic_words,
            source.dynamic_words.iter().flatten().copied(),
        );
        count_sources(
            &mut self.table_unsafe,
            source.table_unsafe.iter().map(String::as_str),
        );
        count_sources(
            &mut self.reflective_words,
            source.reflective_words.iter().flatten().copied(),
        );
    }
}

/// Add one source's distinct `words` to `counts`.
fn count_sources<'w>(
    counts: &mut std::collections::BTreeMap<String, usize>,
    words: impl Iterator<Item = &'w str>,
) {
    let distinct: std::collections::BTreeSet<&str> = words.collect();
    for word in distinct {
        *counts.entry(word.to_string()).or_default() += 1;
    }
}

/// The words of `counts` that some source other than the excluded ones holds,
/// in order. `excluded` counts how many excluded sources hold each word.
fn held_by_others(
    counts: &std::collections::BTreeMap<String, usize>,
    excluded: &std::collections::BTreeMap<String, usize>,
) -> Vec<String> {
    counts
        .iter()
        .filter(|(word, count)| **count > excluded.get(*word).copied().unwrap_or_default())
        .map(|(word, _)| word.clone())
        .collect()
}

// Parses the foreign index has done on this thread, so a test can pin that
// the workspace is parsed once per run and not once per changed file (#10125).
// It counts files folded into the index on the calling thread; the parses
// themselves run on rayon workers (#10441).
#[cfg(test)]
thread_local! {
    pub(super) static FOREIGN_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
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
}

impl<'a> ForeignIndex<'a> {
    /// The index parsing every file's assigned attributes itself.
    #[cfg(test)]
    pub(super) fn build(
        sources: impl Iterator<Item = (&'a Path, &'a str)>,
        unread_files: bool,
    ) -> Self {
        Self::build_with_assigned(sources, unread_files, &std::collections::BTreeMap::new())
    }

    /// [`Self::build`], reading each file's assigned attributes from
    /// `assigned` (the source-fact pass already parsed it) and parsing only
    /// the files it does not hold (#10914).
    pub(super) fn build_with_assigned(
        sources: impl Iterator<Item = (&'a Path, &'a str)>,
        unread_files: bool,
        assigned: &std::collections::BTreeMap<std::path::PathBuf, Option<Vec<String>>>,
    ) -> Self {
        let mut files = Vec::new();
        let mut references = std::collections::BTreeMap::new();
        let mut hidden_import = false;
        // Parsing each source dominates the index on large workspaces, so each
        // batch is parsed in parallel and then folded sequentially in input
        // order: word ownership and the word cap behave as before, and a
        // tripped word cap stops parsing within one batch (#10441).
        let sources: Vec<(&'a Path, &'a str)> = sources.collect();
        let token = crate::analysis::cancellation::current_token();
        // Every per-file scan below lexes or parses the whole source, so all
        // of them run inside the parallel stage; only word ownership and the
        // word cap stay in the sequential fold (#10441, #10735).
        let tables: std::collections::BTreeSet<String> = sources
            .par_iter()
            .map(|(_, source)| {
                crate::analysis::cancellation::with_optional_token(token.as_ref(), || {
                    dict_literal_names(source)
                })
            })
            .reduce(std::collections::BTreeSet::new, |mut all, names| {
                all.extend(names);
                all
            });
        for batch in sources.chunks(FOREIGN_PARSE_BATCH_FILES) {
            let scanned: Vec<(bool, IndexedSource<'a>)> = batch
                .par_iter()
                .map(|&(file, source)| {
                    crate::analysis::cancellation::with_optional_token(token.as_ref(), || {
                        scan_source(file, source, &tables, assigned.get(file))
                    })
                })
                .collect();
            for (&(file, source), (hides_import, indexed)) in batch.iter().zip(scanned) {
                hidden_import |= hides_import;
                for word in source_words(source) {
                    let first = references.entry(word).or_insert(Some(file));
                    if first.is_some_and(|first| first != file) {
                        *first = None;
                    }
                    if references.len() > MAX_WORKSPACE_WORDS {
                        return Self {
                            files: Vec::new(),
                            references: std::collections::BTreeMap::new(),
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
        let mut excluded = ForeignTotals::default();
        let mut reflective_opaque = false;
        let mut reflection_elsewhere = false;
        let mut shares_module_name = false;
        let mut names_may_differ_elsewhere = false;
        let mut unparsed = Vec::new();
        for indexed in &self.files {
            if indexed.file == own {
                excluded.accumulate(indexed);
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
                count_sources(
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
        ForeignWrites {
            attributes: held_by_others(&totals.attributes, &excluded.attributes),
            dynamic_words: held_by_others(&totals.dynamic_words, &excluded.dynamic_words),
            table_unsafe: held_by_others(&totals.table_unsafe, &excluded.table_unsafe),
            reflective_words: held_by_others(&totals.reflective_words, &excluded.reflective_words),
            reflective_reach: held_by_others(&totals.reflective_reach, &excluded.reflective_reach),
            reflective_opaque,
            reflection_elsewhere,
            shares_module_name,
            names_may_differ_elsewhere,
            unknown,
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
            attributes: attributes.into_iter().collect(),
            dynamic_words: words.into_iter().collect(),
            table_unsafe: table_unsafe.into_iter().collect(),
            reflective_words: reflective_words.into_iter().collect(),
            reflective_reach: self
                .reflective_reach
                .iter()
                .filter(|(file, _)| *file != own)
                .flat_map(|(_, reach)| reach.iter().cloned())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
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
            || self
                .attributes
                .iter()
                .any(|attribute| attribute == name || attribute == "*")
            || self.dynamic_words.iter().any(|word| word == name)
            || (!stem.is_empty() && self.dynamic_words.iter().any(|word| word == stem))
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

    /// Whether some other workspace file may bind names it does not spell
    /// byte for byte (a non-ASCII name, or a non-UTF-8 source encoding).
    pub(super) fn names_may_differ_elsewhere(&self) -> bool {
        self.unknown || self.names_may_differ_elsewhere
    }

    /// Whether some workspace file assigns a dunder attribute (`__new__`,
    /// `__getattribute__`, `__class__`, ...), which can change what any
    /// class builds or looks up without naming the method (#9226).
    pub(super) fn writes_dunder(&self) -> bool {
        self.attributes.iter().any(|attribute| {
            attribute.len() > 4 && attribute.starts_with("__") && attribute.ends_with("__")
        })
    }

    /// Whether some workspace file assigns a function's code slots, which can
    /// replace any reader without rebinding its name (#10626). Checked where a
    /// reader is evaluated, not for plain names such as `version_info`.
    pub(super) fn may_replace_reader_code(&self) -> bool {
        self.attributes
            .iter()
            .any(|attribute| CODE_SLOTS.contains(&attribute.as_str()))
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
            || (!stem.is_empty() && self.reflective_reach.iter().any(|word| word == stem))
            || self
                .table_unsafe
                .iter()
                .any(|unsafe_use| unsafe_use == name)
            || self
                .reflective_words
                .iter()
                .any(|word| word == name || (!stem.is_empty() && word == stem))
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
    tables: &std::collections::BTreeSet<String>,
    parsed_assigned: Option<&Option<Vec<String>>>,
) -> (bool, IndexedSource<'a>) {
    let hides_import = DYNAMIC_LOADERS
        .iter()
        .any(|loader| mentions_identifier(source, loader))
        || source.contains("sys.modules")
        || (file.file_name().is_some_and(|name| name == "__init__.py")
            && mentions_identifier(source, "__all__"));
    let assigned = parsed_assigned
        .cloned()
        .unwrap_or_else(|| assigned_attributes(source));
    let dynamic = assigned.is_none()
        || DYNAMIC_WRITERS
            .iter()
            .any(|writer| mentions_identifier(source, writer))
        || source.contains("sys.modules");
    let table_unsafe =
        if tables.is_empty() || !source_words(source).any(|word| tables.contains(word)) {
            Vec::new()
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
            .any(|spelling| mentions_identifier(source, spelling));
    let indexed = IndexedSource {
        file,
        assigned,
        dynamic_words: dynamic.then(|| source_words(source).collect()),
        table_unsafe,
        reflective_words: reflective.then(|| source_words(source).collect()),
        import_words: if tables.is_empty() {
            Vec::new()
        } else {
            import_words(source)
        },
        reflects: mentions_reflection(source),
        loose_names: names_may_differ(source),
        reflective_opaque: reflective
            && (source.contains("sys.modules")
                || MODULE_LOADERS
                    .iter()
                    .any(|loader| mentions_identifier(source, loader))),
    };
    (hides_import, indexed)
}

fn import_words(source: &str) -> Vec<&str> {
    use rustpython_parser::{Mode, Tok, lexer::lex};
    let mut words = Vec::new();
    let mut statement: Vec<&str> = Vec::new();
    let mut imports = false;
    for token in lex(source, Mode::Module) {
        let Ok((tok, range)) = token else {
            return source_words(source).collect();
        };
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
                if let Some(text) = source.get(usize::from(range.start())..usize::from(range.end()))
                {
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
fn reflective_reaches<'a>(
    files: &[IndexedSource<'a>],
) -> Vec<(&'a Path, std::collections::BTreeSet<String>)> {
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
    for (index, indexed) in files.iter().enumerate() {
        if indexed.reflective_words.is_none() {
            continue;
        }
        let root = find(&mut parent, index);
        let mut reach = members.get(&root).cloned().unwrap_or_default();
        reach.extend(conftest.iter().cloned());
        reaches.push((indexed.file, reach));
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
                    (None, None),
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
                let spellings = reader_spellings(test, owner);
                if spellings.is_empty()
                    || !flips(
                        &expression,
                        &name,
                        (&old_value, &new_value),
                        &constant_spellings,
                        &spellings,
                        (Some(old_reader), Some(new_reader)),
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
        }
    }
    Some(out)
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
        let spellings = reader_spellings(test, &callee);
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
                (Some(&old_reader), Some(&new_reader)),
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
    readers: (Option<&Reader>, Option<&Reader>),
    preceding: &[Expr],
) -> bool {
    let outcome = |value: &Value, reader: Option<&Reader>| {
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
        };
        if !preceding
            .iter()
            .all(|prior| evaluate(prior, &scope).is_some_and(|value| value.truthy()))
        {
            return None;
        }
        evaluate(expression, &scope).map(|result| result.truthy())
    };
    outcome(values.0, readers.0) == Some(true) && outcome(values.1, readers.1) == Some(false)
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
            spellings.push(format!("{receiver}.{name}"));
        }
    }
    keep_unshadowed(test, spellings)
}

pub(super) fn reader_spellings(test: &PythonTest, owner: &PythonOwner) -> Vec<String> {
    keep_unshadowed(test, owner_module_callees(test, owner))
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
            let inner = Scope {
                constant: scope.constant,
                value: scope.value,
                parameters: Some(&arguments),
                constant_spellings: &[],
                reader_spellings: &[],
                reader: None,
                model: None,
                receiver: None,
                floats: false,
            };
            evaluate(&reader.expression, &inner)
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
