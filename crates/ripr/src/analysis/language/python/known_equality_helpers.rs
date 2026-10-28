//! numpy.testing and assertpy equality helpers read as the equality they run
//! (#10936, RIPR-SPEC-0238 rule 28).
//!
//! `np.testing.assert_array_equal(parse("1"), [2])` and
//! `assert_that(parse("1")).is_equal_to(2)` fail exactly when the two operands
//! differ, like `assert parse("1") == [2]`. Extraction reads any
//! `assert_*`-named call or `assert_that(..)` chain as an unclassified custom
//! helper (rule 3), because it cannot tell whose helper the name is. This pass
//! runs once the test file is known and re-reads a candidate as
//! `assert left == right` when the file's own imports prove the name is
//! numpy's or assertpy's. The reported oracle text stays the user's call.
//!
//! "Proves" is deliberately narrow, because a wrong exact credit hides a
//! missing test. Every one of these must hold, or the call keeps its reading:
//!
//! - the helper is `numpy.testing.assert_array_equal` or
//!   `numpy.testing.assert_equal` (exactly two positional operands, keywords
//!   only `err_msg`, `verbose`, `strict`), or `assertpy.assert_that(A)`
//!   followed by `.is_equal_to(B)` and nothing else;
//! - its head name is bound exactly once in the file, by a top-level
//!   `import` or `from .. import` that resolves it to that library, and no
//!   name after the head is bound anywhere (`np.testing = fake` binds
//!   `testing`);
//! - the head name is otherwise only used as `head.` (a module alias) or
//!   `head(` (an imported callable), so it is never stored or passed along
//!   where a second name could be rebound;
//! - the file does not rebind names dynamically (`globals(`, `vars(`, `exec(`,
//!   `eval(`, `__dict__`, `__code__`, `sys.modules`, `importlib`, `__import__`,
//!   `import *`), and no line that patches, mocks or `setattr`s mentions the
//!   library or the helper;
//! - the call is not under a `with`, `try` or `match`, where it may be
//!   expected to fail (`with pytest.raises(AssertionError)`); extraction
//!   already dropped the candidate there.
//!
//! Tolerance helpers (`assert_allclose`, `assert_almost_equal`,
//! `assert_array_almost_equal`, `assert_array_less`) and every other
//! `assert_*` name are not candidates.

use super::bool_oracle::count_bindings;
use super::source_facts::parse_module_result;
use super::{KnownEqualityLibrary, PythonKnownEqualityHelper, PythonTest};
use crate::domain::{OracleKind, OracleStrength};
use rustpython_parser::ast::{Mod, Stmt};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Re-read every provable numpy.testing / assertpy equality candidate as the
/// comparison it runs.
pub(super) fn credit_known_equality_helpers<'a>(
    tests: &mut [PythonTest],
    read_source: impl Fn(&Path) -> Option<&'a str>,
) {
    let mut files: BTreeMap<PathBuf, Option<FileImports>> = BTreeMap::new();
    for test in tests.iter_mut() {
        if test
            .assertions
            .iter()
            .all(|assertion| assertion.known_equality_helper.is_none())
        {
            continue;
        }
        let imports = files.entry(test.file.clone()).or_insert_with(|| {
            read_source(&test.file).and_then(|s| FileImports::read(&test.file, s))
        });
        let Some(imports) = imports else {
            continue;
        };
        for assertion in &mut test.assertions {
            let Some(helper) = &assertion.known_equality_helper else {
                continue;
            };
            if imports.proves(helper) && !broadcasts_a_scalar(helper) {
                assertion.gate_text = format!("assert {} == {}", helper.left, helper.right);
                assertion.oracle_kind = OracleKind::ExactValue;
                assertion.oracle_strength = OracleStrength::Strong;
                assertion.oracle_shape = super::PythonOracleShape::ExactAssertion;
            }
        }
    }
}

/// `assert_array_equal(f(), 3)` and `assert_equal(f(), 3)` broadcast a scalar,
/// so they pass for `[3]` as well as `3`; a shape change would go unnoticed
/// and the call is not the `==` it would be elsewhere. assertpy compares
/// with `==` and is not affected.
fn broadcasts_a_scalar(helper: &PythonKnownEqualityHelper) -> bool {
    helper.library == KnownEqualityLibrary::NumpyTesting
        && (scalar_literal(&helper.left) || scalar_literal(&helper.right))
}

fn scalar_literal(operand: &str) -> bool {
    let text = operand.trim().trim_start_matches(['-', '+']).trim();
    let text = text.trim_start_matches('(').trim_end_matches(')').trim();
    matches!(text, "True" | "False" | "None")
        || text.replace('_', "").parse::<f64>().is_ok()
        || text.ends_with(['j', 'J'])
            && text[..text.len() - 1]
                .replace('_', "")
                .parse::<f64>()
                .is_ok()
        || (text.len() >= 2
            && (text.starts_with('"') || text.starts_with('\''))
            && text.ends_with(text.chars().next().unwrap_or('"'))
            && !text[1..text.len() - 1].contains(text.chars().next().unwrap_or('"')))
}

/// One test file's top-level imports and the checks that keep them trusted.
struct FileImports<'a> {
    source: &'a str,
    body: Vec<Stmt>,
    /// Bound name to the dotted object its top-level import brings in.
    bound: BTreeMap<String, String>,
}

impl<'a> FileImports<'a> {
    fn read(path: &Path, source: &'a str) -> Option<Self> {
        if writes_names_dynamically(source) {
            return None;
        }
        let Ok(Mod::Module(module)) = parse_module_result(path, source) else {
            return None;
        };
        let mut bound: BTreeMap<String, Option<String>> = BTreeMap::new();
        let mut record = |name: String, target: String| {
            // A second top-level import of the same name is ambiguous.
            bound
                .entry(name)
                .and_modify(|existing| *existing = None)
                .or_insert(Some(target));
        };
        for stmt in &module.body {
            match stmt {
                Stmt::Import(import) => {
                    for alias in &import.names {
                        let full = alias.name.as_str();
                        match &alias.asname {
                            Some(asname) => record(asname.to_string(), full.to_string()),
                            None => {
                                let head = full.split('.').next().unwrap_or(full);
                                record(head.to_string(), head.to_string());
                            }
                        }
                    }
                }
                Stmt::ImportFrom(import) => {
                    // Relative imports and star imports name nothing we trust.
                    if import.level.is_some_and(|level| level.to_u32() > 0) {
                        continue;
                    }
                    let Some(module_name) = &import.module else {
                        continue;
                    };
                    for alias in &import.names {
                        if alias.name.as_str() == "*" {
                            continue;
                        }
                        let name = alias.asname.as_ref().unwrap_or(&alias.name);
                        record(
                            name.to_string(),
                            format!("{}.{}", module_name.as_str(), alias.name.as_str()),
                        );
                    }
                }
                _ => {}
            }
        }
        Some(Self {
            source,
            body: module.body,
            bound: bound
                .into_iter()
                .filter_map(|(name, target)| Some((name, target?)))
                .collect(),
        })
    }

    /// Whether the helper's callee resolves to the library function its
    /// shape names, with nothing in the file able to rebind it.
    fn proves(&self, helper: &PythonKnownEqualityHelper) -> bool {
        let mut segments = helper.callee.split('.');
        let Some(head) = segments.next() else {
            return false;
        };
        let rest: Vec<&str> = segments.collect();
        let Some(target) = self.bound.get(head) else {
            return false;
        };
        let resolved = std::iter::once(target.as_str())
            .chain(rest.iter().copied())
            .collect::<Vec<_>>()
            .join(".");
        let expected: &[&str] = match helper.library {
            KnownEqualityLibrary::NumpyTesting => &[
                "numpy.testing.assert_array_equal",
                "numpy.testing.assert_equal",
            ],
            KnownEqualityLibrary::Assertpy => &["assertpy.assert_that"],
        };
        if !expected.contains(&resolved.as_str()) {
            return false;
        }
        // The head is bound once, by its import. Names after it are reached
        // as attributes; any binding of one (`np.testing = fake`) rebinds.
        if count_bindings(&self.body, head) != 1
            || rest
                .iter()
                .any(|segment| count_bindings(&self.body, segment) != 0)
        {
            return false;
        }
        let method = helper.library == KnownEqualityLibrary::Assertpy;
        if method && count_bindings(&self.body, "is_equal_to") != 0 {
            return false;
        }
        let callable_import = rest.is_empty();
        if !head_used_only_as_written(self.source, head, callable_import) {
            return false;
        }
        // `np.testing` must itself only be dotted into, never stored.
        if !rest.is_empty()
            && rest.len() > 1
            && !prefix_used_only_dotted(self.source, head, rest[0])
        {
            return false;
        }
        !patches_the_helper(self.source, head, &rest, helper.library)
    }
}

/// Markers of rebinding the AST does not show. A hit anywhere in the file
/// refuses it (the common `monkeypatch.setenv` is not among them).
fn writes_names_dynamically(source: &str) -> bool {
    [
        "globals(",
        "vars(",
        "exec(",
        "eval(",
        "__dict__",
        "__code__",
        "sys.modules",
        "importlib",
        "__import__",
        "import *",
    ]
    .iter()
    .any(|needle| source.contains(needle))
}

fn is_word(ch: Option<char>) -> bool {
    ch.is_some_and(|ch| ch == '_' || ch.is_alphanumeric())
}

fn import_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("import ") || trimmed.starts_with("from ")
}

/// Every whole-word use of `head` outside the import statements is followed by
/// `.` (a module alias) or by `(` (an imported callable), never stored,
/// returned or passed along.
fn head_used_only_as_written(source: &str, head: &str, callable: bool) -> bool {
    let wanted = if callable { '(' } else { '.' };
    source
        .lines()
        .filter(|line| !import_line(line))
        .all(|line| {
            line.match_indices(head).all(|(at, _)| {
                if is_word(line[..at].chars().next_back())
                    || is_word(line[at + head.len()..].chars().next())
                {
                    return true;
                }
                // `obj.head` is an attribute of something else, not the name.
                if line[..at].trim_end().ends_with('.') {
                    return true;
                }
                line[at + head.len()..].trim_start().starts_with(wanted)
            })
        })
}

/// `head.next` (`np.testing`) is only ever dotted into further.
fn prefix_used_only_dotted(source: &str, head: &str, next: &str) -> bool {
    source
        .lines()
        .filter(|line| !import_line(line))
        .all(|line| {
            let mut rest = line;
            while let Some(at) = rest.find(head) {
                let after = &rest[at + head.len()..];
                let before_word = is_word(rest[..at].chars().next_back());
                rest = after;
                if before_word {
                    continue;
                }
                let Some(tail) = after.trim_start().strip_prefix('.') else {
                    continue;
                };
                let tail = tail.trim_start();
                if let Some(more) = tail.strip_prefix(next)
                    && !is_word(more.chars().next())
                    && !more.trim_start().starts_with('.')
                {
                    return false;
                }
            }
            true
        })
}

/// A line that patches, mocks or `setattr`s and mentions the library or the
/// helper may swap it out.
fn patches_the_helper(
    source: &str,
    head: &str,
    rest: &[&str],
    library: KnownEqualityLibrary,
) -> bool {
    let mut sensitive: Vec<&str> = vec![head];
    sensitive.extend(rest.iter().copied());
    match library {
        KnownEqualityLibrary::NumpyTesting => {
            sensitive.extend(["numpy", "testing", "assert_array_equal", "assert_equal"]);
        }
        KnownEqualityLibrary::Assertpy => {
            sensitive.extend(["assertpy", "assert_that", "is_equal_to"]);
        }
    }
    let has_marker = |line: &str| {
        ["patch", "mock", "Mock", "setattr"]
            .iter()
            .any(|marker| line.contains(marker))
    };
    if source
        .lines()
        .any(|line| has_marker(line) && sensitive.iter().any(|word| mentions_word(line, word)))
    {
        return true;
    }
    // A patch call split across lines puts its target (`"numpy.testing.assert_array_equal"`)
    // on a line of its own, so a library word inside a string literal anywhere in a file
    // that patches or mocks at all refuses it.
    source.lines().any(has_marker)
        && source
            .lines()
            .filter(|line| !import_line(line))
            .any(|line| library_word_in_string(line, library))
}

fn library_word_in_string(line: &str, library: KnownEqualityLibrary) -> bool {
    let words: &[&str] = match library {
        KnownEqualityLibrary::NumpyTesting => {
            &["numpy", "testing", "assert_array_equal", "assert_equal"]
        }
        KnownEqualityLibrary::Assertpy => &["assertpy", "assert_that", "is_equal_to"],
    };
    words.iter().any(|word| {
        line.match_indices(word).any(|(at, _)| {
            let before = &line[..at];
            before.matches('\"').count() % 2 == 1 || before.matches('\'').count() % 2 == 1
        })
    })
}

fn mentions_word(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(at, _)| {
        !is_word(line[..at].chars().next_back()) && !is_word(line[at + word.len()..].chars().next())
    })
}
