use super::admission::PythonAdmissionContext;
use super::module_constants::{module_literal_constants, with_module_rebinding};
use super::owners_tests::{
    TestImportContext, collect_imports_from_statements, collect_owners_from_statements,
    collect_tests_from_statements, module_owner,
};
use super::source_utils::{
    SourceText, is_test_file, line_for_range_end, line_for_range_start, text_for_range,
};
use super::{PythonOwner, PythonTest, expr_full_name};
use crate::domain::{LanguageId as DomainLanguageId, StaticLimitKind};
use rustpython_parser::{
    Mode,
    ast::{self, Expr, Mod, Ranged, Stmt},
    parse,
    text_size::{TextRange, TextSize},
};
use std::{
    io::Read,
    ops::RangeInclusive,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonSourceFacts {
    pub(super) file: PathBuf,
    pub(super) language: &'static str,
    pub(super) owners: Vec<PythonOwner>,
    pub(super) tests: Vec<PythonTest>,
    /// Module-level `@pytest.fixture` functions (#4563).
    pub(super) fixtures: Vec<super::PythonFixtureDef>,
    pub(super) facts: Vec<PythonSourceFact>,
    pub(super) limitations: Vec<PythonSourceLimitation>,
    pub(super) docstring_line_ranges: Vec<RangeInclusive<usize>>,
    /// Physical lines of every `import` / `from ... import` statement, at any
    /// nesting depth (a multi-line parenthesized import spans several lines).
    pub(super) import_line_ranges: Vec<RangeInclusive<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonSourceFact {
    pub(super) kind: PythonSourceFactKind,
    pub(super) file: PathBuf,
    pub(super) owner: Option<String>,
    pub(super) start_line: usize,
    pub(super) end_line: usize,
    pub(super) start_byte: usize,
    pub(super) end_byte: usize,
    pub(super) text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum PythonSourceFactKind {
    Module,
    Class,
    Function,
    Method,
    Decorator,
    Parameter,
    Return,
    Raise,
    Predicate,
    Comparison,
    BooleanExpression,
    Call,
    Assignment,
    AttributeWrite,
    DictLiteral,
    ListLiteral,
    SetLiteral,
    StringLiteral,
    PrintCall,
    LogCall,
}

impl PythonSourceFactKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Class => "class",
            Self::Function => "function",
            Self::Method => "method",
            Self::Decorator => "decorator",
            Self::Parameter => "parameter",
            Self::Return => "return",
            Self::Raise => "raise",
            Self::Predicate => "predicate",
            Self::Comparison => "comparison",
            Self::BooleanExpression => "boolean_expression",
            Self::Call => "call",
            Self::Assignment => "assignment",
            Self::AttributeWrite => "attribute_write",
            Self::DictLiteral => "dict_literal",
            Self::ListLiteral => "list_literal",
            Self::SetLiteral => "set_literal",
            Self::StringLiteral => "string_literal",
            Self::PrintCall => "print_call",
            Self::LogCall => "log_call",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PythonSourceLimitation {
    pub(super) kind: StaticLimitKind,
    pub(super) evidence: String,
    pub(super) missing: String,
}

/// Detect Python test frameworks for a workspace root (#2106 / #7664).
///
/// Marker set the adapter's code-level detection implies:
///
/// - pytest: `pytest.ini`, `conftest.py`, or a pytest section in
///   `pyproject.toml` / `setup.cfg` / `tox.ini` (a bare pyproject.toml is
///   PEP 517 packaging, not pytest evidence — #2183 review);
///   when those markers are absent, bounded code evidence of
///   `import pytest` / `from pytest import ...` in a Python test file at
///   the root or in `tests/` / `test/` also counts (#7662);
/// - unittest: no config exists by design, so detection uses bounded code
///   evidence (`import unittest` in a Python test file at the root or in
///   `tests/` / `test/`), matching what the adapter detects from source.
///
/// Test-path admission reuses the shared [`is_test_file`] predicate (#7706):
/// Django-conventional `tests.py` / `test.py` modules and `*_test.py`
/// siblings enter the scan on the same terms the adapter's test index uses
/// (#7385), and discovery descends one level into immediate app directories
/// (the `django-admin startapp` layout keeps unittest tests in
/// `<app>/tests.py`). Every scan stays bounded: 64 admitted `.py` test
/// candidates per directory, sorted by file name before the cap so the
/// bounded answer is deterministic, and a 4096-byte prefix read per file.
///
/// Config-file pytest markers add pytest; they do not skip the unittest
/// scan, so a mixed repo reports both frameworks (#7664). Pytest is listed
/// first when both are present so primary-of-set consumers stay pytest-first.
/// Fail-closed: empty when no marker matches — callers must report
/// "not detected", never guess.
pub(crate) fn detect_python_test_frameworks(root: &Path) -> Vec<&'static str> {
    let mut saw_pytest = root.join("pytest.ini").exists()
        || root.join("conftest.py").exists()
        // A bare pyproject.toml is PEP 517 packaging, not pytest evidence
        // (#2183 review); only an actual pytest section counts.
        || ini_section_present(&root.join("pyproject.toml"), "[tool.pytest.ini_options]")
        || ini_section_present(&root.join("setup.cfg"), "[tool:pytest]")
        || ini_section_present(&root.join("tox.ini"), "[pytest]");
    let mut saw_unittest = false;
    'dirs: for dir in python_framework_scan_dirs(root) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        // #8706: sort by file name and count only admitted `.py` test file
        // candidates toward the per-directory cap. The #8257 review applied
        // the same discipline to the app-directory list; capping the raw
        // entry stream here let 64 non-test entries starve the scan of every
        // test file and made the surviving set depend on unspecified
        // `read_dir` enumeration order, so the same repository could report
        // a different framework set between runs and machines. The
        // `is_file` gate extends the same discipline to test-named
        // directories: a directory named `test_*.py` cannot be read for
        // evidence, so admitting it toward the cap would starve real test
        // files behind it (review of #8706, PR #8901). The cheap name filter
        // runs before the `is_file` stat so a huge directory costs one stat
        // per test-looking name, not per entry (PR #8901 review, graphite
        // finding); sorting after the filters keeps the capped answer
        // deterministic without ordering every unrelated entry.
        let mut candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".py") && is_test_file(path))
            })
            .filter(|path| {
                // Only regular files count toward the cap (RIPR-SPEC-0285):
                // `is_file` follows symlinks, and a symlinked `.py` entry
                // would consume one of the 64 candidate slots before the
                // evidence read rejects it, starving a real test file
                // (PR #8901 review, coderabbit). `symlink_metadata` never
                // follows the final component.
                std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
            })
            .collect();
        candidates.sort();
        for path in candidates.iter().take(PYTHON_FRAMEWORK_SCAN_DIR_ENTRY_CAP) {
            // Bounded evidence read: the import line is at the top of the
            // file, so a small prefix suffices. Read at most that prefix so a
            // large test file cannot exhaust memory when config markers keep
            // the unittest scan running (#7664 review).
            let Some(text) = read_framework_evidence_prefix(path) else {
                continue;
            };
            // #8707: docstrings and usage-example strings that merely SHOW an
            // import are not framework evidence; only code lines count.
            let code = code_lines_outside_triple_quoted_strings(&text);
            if !saw_pytest && code.lines().any(is_pytest_import_line) {
                saw_pytest = true;
            }
            if !saw_unittest && text_has_unittest_framework_import(&code) {
                saw_unittest = true;
            }
            if saw_pytest && saw_unittest {
                break 'dirs;
            }
        }
    }
    let mut frameworks = Vec::new();
    if saw_pytest {
        frameworks.push("pytest");
    }
    if saw_unittest {
        frameworks.push("unittest");
    }
    frameworks
}

/// Directories scanned for Python framework import evidence: the workspace
/// root, its `tests`/`test` directories, and each immediate app directory
/// one level down (the Django `startapp` layout keeps tests in
/// `<app>/tests.py` — #7706). `tests`/`test` stay listed even when they do
/// not exist; a missing directory simply skips. App directories are sorted
/// so the bounded scan order is deterministic across filesystems, and
/// capped so a repo with hundreds of packages cannot expand the read.
fn python_framework_scan_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf(), root.join("tests"), root.join("test")];
    // Review (#8257): collect every immediate directory first, then sort,
    // then cap. Capping the raw entry stream before filtering let 64
    // non-directory entries starve the scan and made the surviving set
    // depend on arbitrary filesystem order.
    let mut app_dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .filter(|path| {
                    !matches!(
                        path.file_name().and_then(|name| name.to_str()),
                        Some("tests") | Some("test")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    app_dirs.sort();
    app_dirs.truncate(64);
    dirs.extend(app_dirs);
    dirs
}

/// Primary detected Python test framework: the first entry of
/// [`detect_python_test_frameworks`] (pytest before unittest). Adapter
/// repo-input still stores a single `Option`; doctor prints the full set.
pub(crate) fn detect_python_test_framework(root: &Path) -> Option<&'static str> {
    detect_python_test_frameworks(root).into_iter().next()
}

/// Bytes inspected for framework import evidence. The read consumes at most
/// one extra byte so a cut final line can be dropped instead of classified.
const PYTHON_FRAMEWORK_EVIDENCE_PREFIX_BYTES: u64 = 4096;

/// Admitted `.py` test file candidates inspected per scanned directory. The
/// cap counts candidates after the file / `.py` / test-path admission
/// filters, never raw directory entries, so non-test files and test-named
/// directories cannot starve the scan (#8706, PR #8901 review).
const PYTHON_FRAMEWORK_SCAN_DIR_ENTRY_CAP: usize = 64;

/// Lines of `text` that are code — every line that does not fall inside a
/// triple-quoted string literal. Docstrings and usage-example strings that
/// merely SHOW an `import unittest` / `import pytest` line are not framework
/// evidence (#8707); a `tests.py` / `test.py` module is admitted by file name
/// alone (#7706), so string awareness is what keeps a helper module's
/// example text from reporting a confirmed framework. The scanner tracks
/// the dominant triple-quote state: it stops scanning a line at an
/// unquoted `#` and skips ordinary single-line string contents before
/// looking for triple-quote delimiters, so a comment like
/// `# use """ for docstrings` or a literal like `x = "'''"` cannot open a
/// phantom string that swallows later real imports (review of #8707,
/// PR #8901). This remains a bounded heuristic, not a full tokenization:
/// single-line strings cannot place an import at line start, and the
/// import matchers already reject lookalike tokens.
fn code_lines_outside_triple_quoted_strings(text: &str) -> String {
    let mut out = String::new();
    let mut open: Option<char> = None;
    for line in text.lines() {
        if open.is_none() {
            out.push_str(line);
        }
        out.push('\n');
        update_triple_quote_state(line, &mut open);
    }
    out
}

/// Advance the triple-quote state across one line. Outside a string, an
/// unquoted `#` ends the scannable line and an ordinary single-line string
/// is skipped to its matching unescaped quote, so delimiters inside
/// comments and string literals never toggle the state (review of #8707,
/// PR #8901). Inside a triple-quoted string only the matching closing
/// delimiter matters; `#` and quotes of the other delimiter are content.
fn update_triple_quote_state(line: &str, open: &mut Option<char>) {
    let mut rest = line;
    loop {
        if let Some(delimiter) = *open {
            match find_closing_triple(rest, delimiter) {
                Some(end) => {
                    *open = None;
                    rest = &rest[end..];
                }
                None => return,
            }
            continue;
        }
        match find_triple_open(rest) {
            Some((delimiter, after)) => {
                *open = Some(delimiter);
                rest = &rest[after..];
            }
            None => return,
        }
    }
}

/// End (exclusive) of the first unescaped closing triple delimiter in
/// `text`, or `None` when the string continues past this line.
fn find_closing_triple(text: &str, delimiter: char) -> Option<usize> {
    let bytes = text.as_bytes();
    let quote = delimiter as u8;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            // A backslash escapes the next character.
            index += 2;
            continue;
        }
        if bytes[index] == quote && bytes[index..].starts_with(&[quote, quote, quote]) {
            return Some(index + 3);
        }
        index += 1;
    }
    None
}

/// Scan code outside any string for the first triple-quote delimiter.
/// Returns the delimiter and the index just past the opening triple quote
/// when one starts on this line. An unquoted `#` ends the scan (the rest is
/// a comment) and an ordinary single-line string is skipped to its
/// matching unescaped closing quote, so neither can contribute a phantom
/// delimiter.
fn find_triple_open(code: &str) -> Option<(char, usize)> {
    let bytes = code.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            // The rest of the line is a comment.
            b'#' => return None,
            quote @ (b'"' | b'\'') => {
                if bytes[index..].starts_with(&[quote, quote, quote]) {
                    return Some((quote as char, index + 3));
                }
                // Ordinary single-line string: skip to its matching
                // unescaped closing quote on the same line.
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index += 2;
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn read_framework_evidence_prefix(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(PYTHON_FRAMEWORK_EVIDENCE_PREFIX_BYTES + 1)
        .read_to_end(&mut buf)
        .ok()?;
    drop_truncated_final_line(&mut buf, PYTHON_FRAMEWORK_EVIDENCE_PREFIX_BYTES as usize);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// When the file continues past `prefix_bytes`, the last inspected line may
/// be a cut identifier (`import unittest` vs `import unittest_extra`). Keep
/// that line only when it is newline-terminated, including the case where
/// the newline sits in the extra lookahead byte.
fn drop_truncated_final_line(buf: &mut Vec<u8>, prefix_bytes: usize) {
    let Some(&extra) = buf.get(prefix_bytes) else {
        return;
    };
    buf.truncate(prefix_bytes);
    if buf.last() == Some(&b'\n') || extra == b'\n' {
        return;
    }
    match buf.iter().rposition(|&b| b == b'\n') {
        Some(idx) => buf.truncate(idx + 1),
        None => buf.clear(),
    }
}

/// Whether a line is a real pytest import statement — `import pytest`,
/// `import pytest as ...`, or `from pytest import ...` — at a token
/// boundary. Comment lines and lookalike identifiers (`import pytesting`)
/// do not count (#7662).
fn is_pytest_import_line(line: &str) -> bool {
    is_top_level_module_import_line(line, "pytest")
}

/// Whether prefix text contains unittest-framework import evidence.
/// Parenthesized `from unittest import (...)` and explicit-backslash
/// continuations are accumulated across lines before mock-only
/// classification (#7664 review).
fn text_has_unittest_framework_import(text: &str) -> bool {
    let mut pending = String::new();
    let mut pending_backslash = false;
    for line in text.lines() {
        let trimmed = strip_python_line_comment(line);
        if trimmed.is_empty() {
            continue;
        }
        if pending.is_empty() {
            if unittest_from_import_is_open_paren(trimmed) {
                pending.push_str(trimmed);
                continue;
            }
            if unittest_from_import_is_backslash_continuation(trimmed) {
                pending.push_str(without_trailing_backslash(trimmed));
                pending_backslash = true;
                continue;
            }
            if is_unittest_import_line(trimmed) {
                return true;
            }
        } else {
            pending.push(' ');
            let continues = trimmed.ends_with('\\');
            pending.push_str(without_trailing_backslash(trimmed));
            let finished = if pending_backslash {
                !continues
            } else {
                pending.contains(')')
            };
            if finished {
                pending_backslash = false;
                let complete = std::mem::take(&mut pending);
                if is_unittest_import_line(&complete) {
                    return true;
                }
            }
        }
    }
    false
}

fn strip_python_line_comment(line: &str) -> &str {
    match line.split_once('#') {
        Some((code, _)) => code.trim(),
        None => line.trim(),
    }
}

fn unittest_from_import_is_open_paren(trimmed: &str) -> bool {
    let Some(imported) = unittest_from_import_names(trimmed) else {
        return false;
    };
    imported.contains('(') && !imported.contains(')')
}

fn unittest_from_import_is_backslash_continuation(trimmed: &str) -> bool {
    unittest_from_import_names(trimmed).is_some() && trimmed.ends_with('\\')
}

fn without_trailing_backslash(trimmed: &str) -> &str {
    match trimmed.strip_suffix('\\') {
        Some(rest) => rest.trim_end(),
        None => trimmed,
    }
}

/// Whether a line is a real unittest import statement — `import unittest`,
/// `import unittest as ...`, or `from unittest import ...` — at a token
/// boundary. Comment lines and lookalike identifiers (`import unittesting`)
/// do not count (#2106 review). `from unittest import mock` is the mock
/// library, not unittest-framework evidence (#7664 review).
fn is_unittest_import_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return false;
    }
    if let Some(imported) = unittest_from_import_names(trimmed) {
        return !unittest_from_import_is_mock_only(imported);
    }
    is_top_level_module_import_line(line, "unittest")
}

fn unittest_from_import_names(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("from ")?;
    let after_module = rest.strip_prefix("unittest")?;
    after_module.strip_prefix(" import")
}

/// `from unittest import mock` / `from unittest import mock as umock` is not
/// a test runner. A mixed pytest repo that only mocks through unittest must
/// not grow a `python: unittest` doctor surface. Parentheses are stripped so
/// `from unittest import (mock)` stays mock-only.
fn unittest_from_import_is_mock_only(imported: &str) -> bool {
    let imported = imported.replace(['(', ')'], " ");
    let mut saw_name = false;
    for name in imported
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        saw_name = true;
        let Some(ident) = name.split_whitespace().next() else {
            return false;
        };
        if ident != "mock" {
            return false;
        }
    }
    saw_name
}

/// Token-boundary `import <module>` / `from <module> import ...` matcher.
/// A following space after the module name admits aliases such as
/// `import pytest as pt`; a glued lookalike (`import pytesting`) does not.
fn is_top_level_module_import_line(line: &str, module: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return false;
    }
    if let Some(rest) = trimmed.strip_prefix("from ") {
        return rest
            .strip_prefix(module)
            .is_some_and(|rest| rest.starts_with(" import"));
    }
    trimmed.strip_prefix("import ").is_some_and(|rest| {
        rest.strip_prefix(module)
            .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with(' '))
    })
}

/// Whether an INI-style file exists and contains the given section header.
fn ini_section_present(path: &Path, section: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|text| text.lines().any(|line| line.trim() == section))
        .unwrap_or(false)
}

pub(super) fn parse_module_result(path: &Path, source: &str) -> Result<Mod, String> {
    if let Some(reason) = super::parse_budget::nesting_budget_reason(source) {
        return Err(reason);
    }
    let source_path = path.to_string_lossy();
    let module = parse(source, Mode::Module, source_path.as_ref())
        .map_err(|err| format!("parse_error: {err}"))?;
    match module {
        Mod::Module(_) => Ok(module),
        _ => Err("parse_error: expected Python module".to_string()),
    }
}

#[cfg(test)]
pub(super) fn parse_module(path: &Path, source: &str) -> Option<Mod> {
    parse_module_result(path, source).ok()
}

/// The producer-owned parse-error row of a failed snapshot, if any.
///
/// `extract_source_facts` types a failed module parse as an
/// `UnsupportedSyntax` limitation whose evidence carries the
/// `source_fact_parse_error` prefix; this keeps the failure-shape knowledge
/// inside the producer that owns it. The repo-mode evidence producer
/// (#3554 PR B) uses this to emit a typed `ParseFailure` limitation and drop
/// the file from the analyzed denominator.
pub(super) fn source_facts_parse_error(
    facts: &PythonSourceFacts,
) -> Option<&PythonSourceLimitation> {
    facts.limitations.iter().find(|limitation| {
        limitation.kind == StaticLimitKind::UnsupportedSyntax
            && limitation.evidence.starts_with("source_fact_parse_error")
    })
}

/// The producer-owned *syntax* parse-error row: a failed module parse the
/// nesting budget did not cause. The PR-local diff/worktree pass (#6824)
/// promotes this into a run-level `producer_failure` limitation so an
/// unparseable changed file can never yield an analyzed, limitations-empty
/// zero. Budget refusals keep their own typed `parse_budget` promotion and
/// stay excluded here, so a budget-capped file never emits two limitations.
pub(super) fn source_facts_syntax_parse_error(
    facts: &PythonSourceFacts,
) -> Option<&PythonSourceLimitation> {
    facts.limitations.iter().find(|limitation| {
        limitation.kind == StaticLimitKind::UnsupportedSyntax
            && limitation.evidence.starts_with("source_fact_parse_error")
            && !limitation.evidence.contains("parse_budget:")
    })
}

pub(super) fn extract_source_facts(file: &Path, source: &str) -> PythonSourceFacts {
    let mut snapshot = PythonSourceFacts {
        file: file.to_path_buf(),
        language: DomainLanguageId::Python.as_str(),
        owners: Vec::new(),
        tests: Vec::new(),
        fixtures: Vec::new(),
        facts: Vec::new(),
        limitations: Vec::new(),
        docstring_line_ranges: Vec::new(),
        import_line_ranges: Vec::new(),
    };
    let module = match parse_module_result(file, source) {
        Ok(Mod::Module(module)) => module,
        Ok(_) => {
            snapshot.limitations.push(PythonSourceLimitation {
                kind: StaticLimitKind::UnsupportedSyntax,
                evidence: "source_fact_parse_error: parse_error: expected Python module"
                    .to_string(),
                missing: "Static limit `unsupported_syntax`: malformed Python prevented source-fact extraction.".to_string(),
            });
            return snapshot;
        }
        Err(parse_reason) => {
            snapshot.limitations.push(PythonSourceLimitation {
                kind: StaticLimitKind::UnsupportedSyntax,
                evidence: format!("source_fact_parse_error: {parse_reason}"),
                missing: "Static limit `unsupported_syntax`: malformed Python prevented source-fact extraction.".to_string(),
            });
            return snapshot;
        }
    };
    // One line-start index per file; every line lookup below is a binary
    // search against it rather than a rescan from byte 0 (#4495).
    let source = &SourceText::new(source);

    let module_range = TextRange::new(
        TextSize::from(0),
        TextSize::from(u32::try_from(source.len()).unwrap_or(u32::MAX)),
    );
    push_source_fact(
        &mut snapshot.facts,
        file,
        source,
        PythonSourceFactKind::Module,
        None,
        module_range,
    );

    let imports = collect_imports_from_statements(file, &module.body);
    let module_constants = module_literal_constants(source, &module.body);
    collect_owners_from_statements(
        file,
        source,
        &module.body,
        None,
        &imports,
        &module_constants,
        &mut snapshot.owners,
    );
    mark_rebound_decorators(source, &mut snapshot.owners);
    snapshot
        .owners
        .push(module_owner(file, source, module_range, &imports));
    collect_tests_from_statements(
        file,
        source,
        &module.body,
        None,
        false,
        TestImportContext {
            body: &imports,
            definition: &[],
            standard_unittest_list: false,
            admission: &PythonAdmissionContext::of_module(
                &module.body,
                &imports,
                &dotted_dir(file),
            ),
        },
        &mut snapshot.tests,
    );
    snapshot.fixtures = super::fixtures::collect_fixture_defs(file, source, &module.body, &imports);
    with_module_rebinding(source, &module.body, &mut snapshot.tests);
    super::oracles::unittest_list::withhold_modified_oracles(&mut snapshot.tests);
    collect_source_facts_from_statements(
        file,
        source,
        &module.body,
        None,
        None,
        &mut snapshot.facts,
    );
    collect_docstring_line_ranges(source, &module.body, &mut snapshot.docstring_line_ranges);
    collect_import_line_ranges(source, &module.body, &mut snapshot.import_line_ranges);
    snapshot
}

/// Collects the line spans of `import` and `from ... import` statements in
/// every scope, including function-local and `if TYPE_CHECKING:` imports.
fn collect_import_line_ranges(
    source: &SourceText<'_>,
    statements: &[Stmt],
    out: &mut Vec<RangeInclusive<usize>>,
) {
    for statement in statements {
        match statement {
            Stmt::Import(_) | Stmt::ImportFrom(_) => {
                let range = statement.range();
                out.push(line_for_range_start(source, range)..=line_for_range_end(source, range));
            }
            Stmt::FunctionDef(function) => collect_import_line_ranges(source, &function.body, out),
            Stmt::AsyncFunctionDef(function) => {
                collect_import_line_ranges(source, &function.body, out);
            }
            Stmt::ClassDef(class) => collect_import_line_ranges(source, &class.body, out),
            Stmt::If(statement) => {
                collect_import_line_ranges(source, &statement.body, out);
                collect_import_line_ranges(source, &statement.orelse, out);
            }
            Stmt::For(statement) => {
                collect_import_line_ranges(source, &statement.body, out);
                collect_import_line_ranges(source, &statement.orelse, out);
            }
            Stmt::AsyncFor(statement) => {
                collect_import_line_ranges(source, &statement.body, out);
                collect_import_line_ranges(source, &statement.orelse, out);
            }
            Stmt::While(statement) => {
                collect_import_line_ranges(source, &statement.body, out);
                collect_import_line_ranges(source, &statement.orelse, out);
            }
            Stmt::With(statement) => collect_import_line_ranges(source, &statement.body, out),
            Stmt::AsyncWith(statement) => collect_import_line_ranges(source, &statement.body, out),
            Stmt::Try(statement) => {
                collect_import_line_ranges(source, &statement.body, out);
                for handler in &statement.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect_import_line_ranges(source, &handler.body, out);
                }
                collect_import_line_ranges(source, &statement.orelse, out);
                collect_import_line_ranges(source, &statement.finalbody, out);
            }
            Stmt::TryStar(statement) => {
                collect_import_line_ranges(source, &statement.body, out);
                for handler in &statement.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect_import_line_ranges(source, &handler.body, out);
                }
                collect_import_line_ranges(source, &statement.orelse, out);
                collect_import_line_ranges(source, &statement.finalbody, out);
            }
            Stmt::Match(statement) => {
                for case in &statement.cases {
                    collect_import_line_ranges(source, &case.body, out);
                }
            }
            _ => {}
        }
    }
}

/// Collects the line spans of real Python docstrings from parsed scope bodies.
///
/// A docstring is the first statement of a module, function, async function, or
/// class body when that statement is a plain string constant. Assigned strings,
/// f-strings, and string expressions inside control-flow blocks are deliberately
/// excluded: they are not docstrings, even when they use triple quotes.
fn collect_docstring_line_ranges(
    source: &SourceText<'_>,
    scope_body: &[Stmt],
    out: &mut Vec<RangeInclusive<usize>>,
) {
    if let Some(Stmt::Expr(expr_stmt)) = scope_body.first()
        && matches!(
            expr_stmt.value.as_ref(),
            Expr::Constant(constant) if matches!(&constant.value, ast::Constant::Str(_))
        )
    {
        push_docstring_only_line_ranges(source, expr_stmt.value.range(), out);
    }
    collect_nested_docstring_scopes(source, scope_body, out);
}

/// Adds only physical lines whose non-whitespace content is wholly inside the
/// docstring token. A docstring may share a line with behavioral code through a
/// semicolon; such boundary lines must remain analyzable.
fn push_docstring_only_line_ranges(
    source: &SourceText<'_>,
    range: TextRange,
    out: &mut Vec<RangeInclusive<usize>>,
) {
    let start_offset = usize::from(range.start());
    let end_offset = usize::from(range.end());
    let start_line = line_for_range_start(source, range);
    let end_line = line_for_range_end(source, range);
    let start_line_offset = source[..start_offset]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let end_line_offset = source[end_offset..]
        .find('\n')
        .map_or(source.len(), |offset| end_offset + offset);
    let prefix_is_whitespace = source[start_line_offset..start_offset].trim().is_empty();
    let suffix_is_whitespace = source[end_offset..end_line_offset].trim().is_empty();

    if start_line == end_line {
        if prefix_is_whitespace && suffix_is_whitespace {
            out.push(start_line..=end_line);
        }
        return;
    }

    let first_safe_line = if prefix_is_whitespace {
        start_line
    } else {
        start_line + 1
    };
    let last_safe_line = if suffix_is_whitespace {
        end_line
    } else {
        end_line.saturating_sub(1)
    };
    if first_safe_line <= last_safe_line {
        out.push(first_safe_line..=last_safe_line);
    }
}

fn collect_nested_docstring_scopes(
    source: &SourceText<'_>,
    statements: &[Stmt],
    out: &mut Vec<RangeInclusive<usize>>,
) {
    for statement in statements {
        match statement {
            Stmt::FunctionDef(function) => {
                collect_docstring_line_ranges(source, &function.body, out);
            }
            Stmt::AsyncFunctionDef(function) => {
                collect_docstring_line_ranges(source, &function.body, out);
            }
            Stmt::ClassDef(class) => {
                collect_docstring_line_ranges(source, &class.body, out);
            }
            Stmt::If(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
                collect_nested_docstring_scopes(source, &statement.orelse, out);
            }
            Stmt::For(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
                collect_nested_docstring_scopes(source, &statement.orelse, out);
            }
            Stmt::AsyncFor(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
                collect_nested_docstring_scopes(source, &statement.orelse, out);
            }
            Stmt::While(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
                collect_nested_docstring_scopes(source, &statement.orelse, out);
            }
            Stmt::With(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
            }
            Stmt::AsyncWith(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
            }
            Stmt::Try(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
                for handler in &statement.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect_nested_docstring_scopes(source, &handler.body, out);
                }
                collect_nested_docstring_scopes(source, &statement.orelse, out);
                collect_nested_docstring_scopes(source, &statement.finalbody, out);
            }
            Stmt::TryStar(statement) => {
                collect_nested_docstring_scopes(source, &statement.body, out);
                for handler in &statement.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    collect_nested_docstring_scopes(source, &handler.body, out);
                }
                collect_nested_docstring_scopes(source, &statement.orelse, out);
                collect_nested_docstring_scopes(source, &statement.finalbody, out);
            }
            Stmt::Match(statement) => {
                for case in &statement.cases {
                    collect_nested_docstring_scopes(source, &case.body, out);
                }
            }
            _ => {}
        }
    }
}

pub(super) fn source_fact_snapshot_observation(facts: &PythonSourceFacts) -> usize {
    let mut score = facts.file.components().count() + facts.language.len();
    score = score.saturating_add(facts.owners.len());
    score = score.saturating_add(facts.tests.len());
    score = score.saturating_add(facts.docstring_line_ranges.len());
    for fact in &facts.facts {
        score = score.saturating_add(fact.kind.as_str().len());
        score = score.saturating_add(fact.file.components().count());
        score = score.saturating_add(fact.owner.as_deref().unwrap_or_default().len());
        score = score.saturating_add(fact.start_line);
        score = score.saturating_add(fact.end_line);
        score = score.saturating_add(fact.start_byte);
        score = score.saturating_add(fact.end_byte);
        score = score.saturating_add(fact.text.len());
    }
    for limitation in &facts.limitations {
        score = score.saturating_add(limitation.kind.as_str().len());
        score = score.saturating_add(limitation.evidence.len());
        score = score.saturating_add(limitation.missing.len());
    }
    score
}

fn push_source_fact(
    out: &mut Vec<PythonSourceFact>,
    file: &Path,
    source: &SourceText<'_>,
    kind: PythonSourceFactKind,
    owner: Option<&str>,
    range: TextRange,
) {
    out.push(PythonSourceFact {
        kind,
        file: file.to_path_buf(),
        owner: owner.map(str::to_string),
        start_line: line_for_range_start(source, range),
        end_line: line_for_range_end(source, range),
        start_byte: usize::from(range.start()),
        end_byte: usize::from(range.end()),
        text: text_for_range(source, range).trim().to_string(),
    });
}

fn collect_source_facts_from_statements(
    file: &Path,
    source: &SourceText<'_>,
    statements: &[Stmt],
    class_context: Option<&str>,
    current_owner: Option<&str>,
    out: &mut Vec<PythonSourceFact>,
) {
    for stmt in statements {
        match stmt {
            Stmt::FunctionDef(function) => {
                collect_source_facts_from_function(
                    PythonFunctionSourceContext {
                        file,
                        source,
                        class_context,
                        name: function.name.as_str(),
                        range: function.range,
                        args: &function.args,
                        decorators: &function.decorator_list,
                        body: &function.body,
                    },
                    out,
                );
            }
            Stmt::AsyncFunctionDef(function) => {
                collect_source_facts_from_function(
                    PythonFunctionSourceContext {
                        file,
                        source,
                        class_context,
                        name: function.name.as_str(),
                        range: function.range,
                        args: &function.args,
                        decorators: &function.decorator_list,
                        body: &function.body,
                    },
                    out,
                );
            }
            Stmt::ClassDef(class) => {
                let owner = current_owner.unwrap_or(class.name.as_str());
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Class,
                    Some(owner),
                    class.range,
                );
                for decorator in &class.decorator_list {
                    collect_decorator_fact(file, source, decorator, Some(owner), out);
                }
                collect_source_facts_from_statements(
                    file,
                    source,
                    &class.body,
                    Some(class.name.as_str()),
                    Some(class.name.as_str()),
                    out,
                );
            }
            Stmt::Return(return_stmt) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Return,
                    current_owner,
                    return_stmt.range,
                );
                if let Some(value) = &return_stmt.value {
                    collect_source_facts_from_expr(file, source, value, current_owner, out);
                }
            }
            Stmt::Raise(raise_stmt) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Raise,
                    current_owner,
                    raise_stmt.range,
                );
                if let Some(exc) = &raise_stmt.exc {
                    collect_source_facts_from_expr(file, source, exc, current_owner, out);
                }
                if let Some(cause) = &raise_stmt.cause {
                    collect_source_facts_from_expr(file, source, cause, current_owner, out);
                }
            }
            Stmt::If(if_stmt) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Predicate,
                    current_owner,
                    if_stmt.test.range(),
                );
                collect_source_facts_from_expr(file, source, &if_stmt.test, current_owner, out);
                collect_source_facts_from_statements(
                    file,
                    source,
                    &if_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &if_stmt.orelse,
                    class_context,
                    current_owner,
                    out,
                );
            }
            Stmt::While(while_stmt) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Predicate,
                    current_owner,
                    while_stmt.test.range(),
                );
                collect_source_facts_from_expr(file, source, &while_stmt.test, current_owner, out);
                collect_source_facts_from_statements(
                    file,
                    source,
                    &while_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &while_stmt.orelse,
                    class_context,
                    current_owner,
                    out,
                );
            }
            Stmt::For(for_stmt) => {
                collect_source_facts_from_expr(file, source, &for_stmt.target, current_owner, out);
                collect_source_facts_from_expr(file, source, &for_stmt.iter, current_owner, out);
                collect_source_facts_from_statements(
                    file,
                    source,
                    &for_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &for_stmt.orelse,
                    class_context,
                    current_owner,
                    out,
                );
            }
            Stmt::AsyncFor(for_stmt) => {
                collect_source_facts_from_expr(file, source, &for_stmt.target, current_owner, out);
                collect_source_facts_from_expr(file, source, &for_stmt.iter, current_owner, out);
                collect_source_facts_from_statements(
                    file,
                    source,
                    &for_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &for_stmt.orelse,
                    class_context,
                    current_owner,
                    out,
                );
            }
            Stmt::Match(match_stmt) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Predicate,
                    current_owner,
                    match_stmt.subject.range(),
                );
                collect_source_facts_from_expr(
                    file,
                    source,
                    &match_stmt.subject,
                    current_owner,
                    out,
                );
                for case in &match_stmt.cases {
                    if let Some(guard) = &case.guard {
                        collect_source_facts_from_expr(file, source, guard, current_owner, out);
                    }
                    collect_source_facts_from_statements(
                        file,
                        source,
                        &case.body,
                        class_context,
                        current_owner,
                        out,
                    );
                }
            }
            Stmt::Assign(assign) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Assignment,
                    current_owner,
                    assign.range,
                );
                for target in &assign.targets {
                    collect_assignment_target_facts(file, source, target, current_owner, out);
                }
                collect_source_facts_from_expr(file, source, &assign.value, current_owner, out);
            }
            Stmt::AnnAssign(assign) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Assignment,
                    current_owner,
                    assign.range,
                );
                collect_assignment_target_facts(file, source, &assign.target, current_owner, out);
                collect_source_facts_from_expr(
                    file,
                    source,
                    &assign.annotation,
                    current_owner,
                    out,
                );
                if let Some(value) = &assign.value {
                    collect_source_facts_from_expr(file, source, value, current_owner, out);
                }
            }
            Stmt::AugAssign(assign) => {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::Assignment,
                    current_owner,
                    assign.range,
                );
                collect_assignment_target_facts(file, source, &assign.target, current_owner, out);
                collect_source_facts_from_expr(file, source, &assign.value, current_owner, out);
            }
            Stmt::Expr(expr_stmt) => {
                collect_source_facts_from_expr(file, source, &expr_stmt.value, current_owner, out);
            }
            Stmt::With(with_stmt) => {
                for item in &with_stmt.items {
                    collect_source_facts_from_expr(
                        file,
                        source,
                        &item.context_expr,
                        current_owner,
                        out,
                    );
                    if let Some(optional_vars) = &item.optional_vars {
                        collect_assignment_target_facts(
                            file,
                            source,
                            optional_vars,
                            current_owner,
                            out,
                        );
                    }
                }
                collect_source_facts_from_statements(
                    file,
                    source,
                    &with_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
            }
            Stmt::AsyncWith(with_stmt) => {
                for item in &with_stmt.items {
                    collect_source_facts_from_expr(
                        file,
                        source,
                        &item.context_expr,
                        current_owner,
                        out,
                    );
                    if let Some(optional_vars) = &item.optional_vars {
                        collect_assignment_target_facts(
                            file,
                            source,
                            optional_vars,
                            current_owner,
                            out,
                        );
                    }
                }
                collect_source_facts_from_statements(
                    file,
                    source,
                    &with_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
            }
            Stmt::Try(try_stmt) => {
                collect_source_facts_from_statements(
                    file,
                    source,
                    &try_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_except_handlers(
                    file,
                    source,
                    &try_stmt.handlers,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &try_stmt.orelse,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &try_stmt.finalbody,
                    class_context,
                    current_owner,
                    out,
                );
            }
            Stmt::TryStar(try_stmt) => {
                collect_source_facts_from_statements(
                    file,
                    source,
                    &try_stmt.body,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_except_handlers(
                    file,
                    source,
                    &try_stmt.handlers,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &try_stmt.orelse,
                    class_context,
                    current_owner,
                    out,
                );
                collect_source_facts_from_statements(
                    file,
                    source,
                    &try_stmt.finalbody,
                    class_context,
                    current_owner,
                    out,
                );
            }
            _ => {}
        }
    }
}

struct PythonFunctionSourceContext<'a> {
    file: &'a Path,
    source: &'a SourceText<'a>,
    class_context: Option<&'a str>,
    name: &'a str,
    range: TextRange,
    args: &'a ast::Arguments,
    decorators: &'a [Expr],
    body: &'a [Stmt],
}

fn collect_source_facts_from_function(
    context: PythonFunctionSourceContext<'_>,
    out: &mut Vec<PythonSourceFact>,
) {
    let qualified_name = context
        .class_context
        .map(|class| format!("{class}.{}", context.name))
        .unwrap_or_else(|| context.name.to_string());
    let kind = if context.class_context.is_some() {
        PythonSourceFactKind::Method
    } else {
        PythonSourceFactKind::Function
    };
    push_source_fact(
        out,
        context.file,
        context.source,
        kind,
        Some(&qualified_name),
        context.range,
    );
    for decorator in context.decorators {
        collect_decorator_fact(
            context.file,
            context.source,
            decorator,
            Some(&qualified_name),
            out,
        );
    }
    collect_parameter_facts(
        context.file,
        context.source,
        context.args,
        Some(&qualified_name),
        out,
    );
    collect_source_facts_from_statements(
        context.file,
        context.source,
        context.body,
        context.class_context,
        Some(&qualified_name),
        out,
    );
}

fn collect_decorator_fact(
    file: &Path,
    source: &SourceText<'_>,
    decorator: &Expr,
    owner: Option<&str>,
    out: &mut Vec<PythonSourceFact>,
) {
    push_source_fact(
        out,
        file,
        source,
        PythonSourceFactKind::Decorator,
        owner,
        decorator.range(),
    );
    collect_source_facts_from_expr(file, source, decorator, owner, out);
}

fn collect_parameter_facts(
    file: &Path,
    source: &SourceText<'_>,
    args: &ast::Arguments,
    owner: Option<&str>,
    out: &mut Vec<PythonSourceFact>,
) {
    for arg in args
        .posonlyargs
        .iter()
        .chain(args.args.iter())
        .chain(args.kwonlyargs.iter())
    {
        push_source_fact(
            out,
            file,
            source,
            PythonSourceFactKind::Parameter,
            owner,
            arg.def.range,
        );
        if let Some(annotation) = &arg.def.annotation {
            collect_source_facts_from_expr(file, source, annotation, owner, out);
        }
        if let Some(default) = &arg.default {
            collect_source_facts_from_expr(file, source, default, owner, out);
        }
    }
    if let Some(arg) = &args.vararg {
        push_source_fact(
            out,
            file,
            source,
            PythonSourceFactKind::Parameter,
            owner,
            arg.range,
        );
    }
    if let Some(arg) = &args.kwarg {
        push_source_fact(
            out,
            file,
            source,
            PythonSourceFactKind::Parameter,
            owner,
            arg.range,
        );
    }
}

fn collect_assignment_target_facts(
    file: &Path,
    source: &SourceText<'_>,
    target: &Expr,
    owner: Option<&str>,
    out: &mut Vec<PythonSourceFact>,
) {
    if let Expr::Attribute(attribute) = target {
        push_source_fact(
            out,
            file,
            source,
            PythonSourceFactKind::AttributeWrite,
            owner,
            attribute.range,
        );
    }
    collect_source_facts_from_expr(file, source, target, owner, out);
}

fn collect_source_facts_from_except_handlers(
    file: &Path,
    source: &SourceText<'_>,
    handlers: &[ast::ExceptHandler],
    class_context: Option<&str>,
    current_owner: Option<&str>,
    out: &mut Vec<PythonSourceFact>,
) {
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(type_expr) = &handler.type_ {
            collect_source_facts_from_expr(file, source, type_expr, current_owner, out);
        }
        collect_source_facts_from_statements(
            file,
            source,
            &handler.body,
            class_context,
            current_owner,
            out,
        );
    }
}

fn collect_source_facts_from_expr(
    file: &Path,
    source: &SourceText<'_>,
    expr: &Expr,
    owner: Option<&str>,
    out: &mut Vec<PythonSourceFact>,
) {
    match expr {
        Expr::BoolOp(bool_op) => {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::BooleanExpression,
                owner,
                bool_op.range,
            );
            for value in &bool_op.values {
                collect_source_facts_from_expr(file, source, value, owner, out);
            }
        }
        Expr::NamedExpr(named) => {
            collect_assignment_target_facts(file, source, &named.target, owner, out);
            collect_source_facts_from_expr(file, source, &named.value, owner, out);
        }
        Expr::BinOp(bin_op) => {
            collect_source_facts_from_expr(file, source, &bin_op.left, owner, out);
            collect_source_facts_from_expr(file, source, &bin_op.right, owner, out);
        }
        Expr::UnaryOp(unary) => {
            collect_source_facts_from_expr(file, source, &unary.operand, owner, out);
        }
        Expr::Lambda(lambda) => {
            collect_parameter_facts(file, source, &lambda.args, owner, out);
            collect_source_facts_from_expr(file, source, &lambda.body, owner, out);
        }
        Expr::IfExp(if_exp) => {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::Predicate,
                owner,
                if_exp.test.range(),
            );
            collect_source_facts_from_expr(file, source, &if_exp.test, owner, out);
            collect_source_facts_from_expr(file, source, &if_exp.body, owner, out);
            collect_source_facts_from_expr(file, source, &if_exp.orelse, owner, out);
        }
        Expr::Dict(dict) => {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::DictLiteral,
                owner,
                dict.range,
            );
            for key in dict.keys.iter().flatten() {
                collect_source_facts_from_expr(file, source, key, owner, out);
            }
            for value in &dict.values {
                collect_source_facts_from_expr(file, source, value, owner, out);
            }
        }
        Expr::Set(set) => {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::SetLiteral,
                owner,
                set.range,
            );
            for value in &set.elts {
                collect_source_facts_from_expr(file, source, value, owner, out);
            }
        }
        Expr::ListComp(list_comp) => {
            collect_source_facts_from_expr(file, source, &list_comp.elt, owner, out);
            collect_source_facts_from_comprehensions(
                file,
                source,
                &list_comp.generators,
                owner,
                out,
            );
        }
        Expr::SetComp(set_comp) => {
            collect_source_facts_from_expr(file, source, &set_comp.elt, owner, out);
            collect_source_facts_from_comprehensions(
                file,
                source,
                &set_comp.generators,
                owner,
                out,
            );
        }
        Expr::DictComp(dict_comp) => {
            collect_source_facts_from_expr(file, source, &dict_comp.key, owner, out);
            collect_source_facts_from_expr(file, source, &dict_comp.value, owner, out);
            collect_source_facts_from_comprehensions(
                file,
                source,
                &dict_comp.generators,
                owner,
                out,
            );
        }
        Expr::GeneratorExp(generator) => {
            collect_source_facts_from_expr(file, source, &generator.elt, owner, out);
            collect_source_facts_from_comprehensions(
                file,
                source,
                &generator.generators,
                owner,
                out,
            );
        }
        Expr::Await(await_expr) => {
            collect_source_facts_from_expr(file, source, &await_expr.value, owner, out);
        }
        Expr::Yield(yield_expr) => {
            if let Some(value) = &yield_expr.value {
                collect_source_facts_from_expr(file, source, value, owner, out);
            }
        }
        Expr::YieldFrom(yield_expr) => {
            collect_source_facts_from_expr(file, source, &yield_expr.value, owner, out);
        }
        Expr::Compare(compare) => {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::Comparison,
                owner,
                compare.range,
            );
            collect_source_facts_from_expr(file, source, &compare.left, owner, out);
            for comparator in &compare.comparators {
                collect_source_facts_from_expr(file, source, comparator, owner, out);
            }
        }
        Expr::Call(call) => {
            let call_name = expr_full_name(call.func.as_ref());
            if call_name.as_deref() == Some("print") {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::PrintCall,
                    owner,
                    call.range,
                );
            }
            if call_name.as_deref().is_some_and(is_log_call_name) {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::LogCall,
                    owner,
                    call.range,
                );
            }
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::Call,
                owner,
                call.range,
            );
            collect_source_facts_from_expr(file, source, &call.func, owner, out);
            for arg in &call.args {
                collect_source_facts_from_expr(file, source, arg, owner, out);
            }
            for keyword in &call.keywords {
                collect_source_facts_from_expr(file, source, &keyword.value, owner, out);
            }
        }
        Expr::FormattedValue(formatted) => {
            collect_source_facts_from_expr(file, source, &formatted.value, owner, out);
            if let Some(format_spec) = &formatted.format_spec {
                collect_source_facts_from_expr(file, source, format_spec, owner, out);
            }
        }
        Expr::JoinedStr(joined) => {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::StringLiteral,
                owner,
                joined.range,
            );
            for value in &joined.values {
                collect_source_facts_from_expr(file, source, value, owner, out);
            }
        }
        Expr::Constant(constant) => {
            if matches!(&constant.value, ast::Constant::Str(_)) {
                push_source_fact(
                    out,
                    file,
                    source,
                    PythonSourceFactKind::StringLiteral,
                    owner,
                    constant.range,
                );
            }
        }
        Expr::Attribute(attribute) => {
            collect_source_facts_from_expr(file, source, &attribute.value, owner, out);
        }
        Expr::Subscript(subscript) => {
            collect_source_facts_from_expr(file, source, &subscript.value, owner, out);
            collect_source_facts_from_expr(file, source, &subscript.slice, owner, out);
        }
        Expr::Starred(starred) => {
            collect_source_facts_from_expr(file, source, &starred.value, owner, out);
        }
        Expr::List(list) => {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::ListLiteral,
                owner,
                list.range,
            );
            for value in &list.elts {
                collect_source_facts_from_expr(file, source, value, owner, out);
            }
        }
        Expr::Tuple(tuple) => {
            for value in &tuple.elts {
                collect_source_facts_from_expr(file, source, value, owner, out);
            }
        }
        Expr::Slice(slice) => {
            if let Some(lower) = &slice.lower {
                collect_source_facts_from_expr(file, source, lower, owner, out);
            }
            if let Some(upper) = &slice.upper {
                collect_source_facts_from_expr(file, source, upper, owner, out);
            }
            if let Some(step) = &slice.step {
                collect_source_facts_from_expr(file, source, step, owner, out);
            }
        }
        Expr::Name(_) => {}
    }
}

fn collect_source_facts_from_comprehensions(
    file: &Path,
    source: &SourceText<'_>,
    comprehensions: &[ast::Comprehension],
    owner: Option<&str>,
    out: &mut Vec<PythonSourceFact>,
) {
    for comprehension in comprehensions {
        collect_assignment_target_facts(file, source, &comprehension.target, owner, out);
        collect_source_facts_from_expr(file, source, &comprehension.iter, owner, out);
        for guard in &comprehension.ifs {
            push_source_fact(
                out,
                file,
                source,
                PythonSourceFactKind::Predicate,
                owner,
                guard.range(),
            );
            collect_source_facts_from_expr(file, source, guard, owner, out);
        }
    }
}

fn is_log_call_name(name: &str) -> bool {
    matches!(
        name.rsplit('.').next(),
        Some("debug" | "info" | "warning" | "warn" | "error" | "exception" | "critical" | "log")
    ) && (name.starts_with("logging.") || name.starts_with("logger."))
}

/// The dotted directory of a source file: `e2e/api` for `e2e/api/test_m.py`
/// reads `e2e.api`; a file at the root reads "".
fn dotted_dir(file: &Path) -> String {
    file.parent()
        .map(|dir| {
            dir.components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(part) => part.to_str(),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(".")
        })
        .unwrap_or_default()
}

/// Records, per owner, the decorators whose first name segment the file binds
/// anywhere other than the one typing import, so an import match is not
/// trusted for them (`def final`, `final = ...`, a nested or class-scope
/// rebind, a tuple or loop target, a `try`/`except` import fallback).
///
/// This is deliberately a lexical, fail-closed screen rather than a scope
/// analysis: the name must occur only on `@name...` decorator lines and on
/// `typing`/`typing_extensions` import statements. A star import from another
/// module, or any decorator the screen cannot read as a plain dotted name
/// (`@registry["x"]`, `@(lambda f: g)`), withholds trust for the whole file.
fn mark_rebound_decorators(source: &str, owners: &mut [PythonOwner]) {
    let distrust_all = typing_markers_distrusted(source);
    for owner in owners {
        let rebound: Vec<String> = owner
            .decorators
            .iter()
            .filter(|decorator| {
                let first = decorator.split('.').next().unwrap_or_default();
                distrust_all || name_bound_outside_typing_import(source, first)
            })
            .cloned()
            .collect();
        owner.unplain_decorators.extend(rebound);
    }
}

/// Whether a decorator line or star import makes any marker unprovable.
fn typing_markers_distrusted(source: &str) -> bool {
    // A lone carriage return is a Python line break `str::lines` does not see.
    if source
        .match_indices('\r')
        .any(|(at, _)| source.as_bytes().get(at + 1) != Some(&b'\n'))
    {
        return true;
    }
    source.lines().any(|line| {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix('@') {
            return !is_plain_decorator(rest.split('#').next().unwrap_or_default().trim());
        }
        // A star import anywhere on the line (`import os; from m import *`,
        // `if x: from m import *`) may shadow a marker; so may one split by a
        // backslash continuation.
        (contains_word(line, "import")
            && (line.contains('*') || line.trim_end().ends_with('\\'))
            && !is_typing_import_line(line))
            || line.starts_with('*')
    })
}

/// A decorator that is a dotted name, optionally followed by one call whose
/// parentheses close at the end of the line (`name`, `a.b`, `name(...)`).
/// Anything else (`name(x)[0]`, `name(x) if c else y`, a multi-line call, a
/// subscript, a lambda) cannot be read, so the file is not trusted.
fn is_plain_decorator(text: &str) -> bool {
    let head_end = text.find('(').unwrap_or(text.len());
    let head = &text[..head_end];
    let plain_head = !head.is_empty()
        && !head.starts_with('.')
        && !head.ends_with('.')
        && head
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    if !plain_head {
        return false;
    }
    let call = &text[head_end..];
    if call.is_empty() {
        return true;
    }
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let chars: Vec<(usize, char)> = call.char_indices().collect();
    for (at, &(index, c)) in chars.iter().enumerate() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == open {
                quote = None;
            }
            continue;
        }
        match c {
            // Triple-quoted strings are not parsed: the line is unreadable.
            '\'' | '"' => {
                if chars.get(at + 1).is_some_and(|&(_, next)| next == c)
                    && chars.get(at + 2).is_some_and(|&(_, next)| next == c)
                {
                    return false;
                }
                quote = Some(c);
            }
            '(' => depth += 1,
            ')' => {
                depth = match depth.checked_sub(1) {
                    Some(depth) => depth,
                    None => return false,
                };
                if depth == 0 && index + 1 != call.len() {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0 && quote.is_none()
}

fn is_typing_import_line(line: &str) -> bool {
    let module_import = ["import typing", "import typing_extensions"]
        .iter()
        .any(|prefix| {
            line.strip_prefix(prefix)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        });
    (line.starts_with("from typing import")
        || line.starts_with("from typing_extensions import")
        || module_import)
        && !line.contains(';')
}

fn name_bound_outside_typing_import(source: &str, name: &str) -> bool {
    let mut in_typing_block = false;
    for line in source.lines() {
        let trimmed = line.trim_start();
        // Parentheses in a trailing comment must not open or close the block.
        let code = trimmed.split('#').next().unwrap_or_default();
        let mut scanned = line;
        let allowed = if in_typing_block {
            if let Some(close) = code.find(')') {
                // Only text before the closing parenthesis belongs to the import.
                in_typing_block = false;
                scanned = &code[close + 1..];
                false
            } else {
                true
            }
        } else if is_typing_import_line(trimmed) {
            if code.contains('(') && !code.contains(')') {
                in_typing_block = true;
            }
            true
        } else if let Some(rest) = trimmed.strip_prefix('@') {
            let head = rest.split('(').next().unwrap_or_default();
            // The call arguments are scanned too: `@final(final := wrapper)`
            // rebinds the name on its own decorator line.
            head.split('.').next() == Some(name) && !contains_word(&rest[head.len()..], name)
        } else {
            false
        };
        if !allowed && contains_word(scanned, name) {
            return true;
        }
    }
    false
}

fn contains_word(line: &str, word: &str) -> bool {
    line.match_indices(word).any(|(start, _)| {
        let before = line[..start].chars().next_back();
        let after = line[start + word.len()..].chars().next();
        let is_ident = |c: char| c.is_alphanumeric() || c == '_';
        !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
    })
}
