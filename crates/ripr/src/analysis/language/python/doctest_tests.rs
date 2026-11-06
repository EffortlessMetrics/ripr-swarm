//! Doctest examples pytest collects, read as the tests they are
//! (RIPR-SPEC-0238 rule 26, #9619).
//!
//! With `--doctest-modules` in the pytest configuration, the `>>>` examples in
//! a module's function docstrings are tests: `slugify("Hello World")` followed
//! by `'hello-world'` fails when the function returns anything else. A module
//! whose only oracle is such an example has no test file for the relation
//! index to read, so its changed lines read `no_static_path`.
//!
//! This pass turns an example into the assertion it states and hands it to the
//! existing test extraction, so owner identity, shadowing, module-mock and
//! oracle-strength gates apply to it exactly as to a hand-written test:
//!
//! ```text
//! >>> slugify("Hello World")      assert slugify("Hello World") == 'hello-world'
//! 'hello-world'
//! ```
//!
//! An exact-value oracle alone does not say the example notices the edit
//! (`truncate("abc", 5)` returns `'abc'` with or without an off-by-one cut), so
//! an example is kept only when running it distinguishes the old function from
//! the new one ([`super::doctest_eval`]); an example the evaluator cannot run
//! is dropped. An example is read at all only when it is certain to be that
//! assertion:
//!
//! - the docstring belongs to an undecorated top-level `def` and every example
//!   in it is one `>>>` line, a call of a bare name with literal arguments,
//!   followed by one expected line equal to the `repr` of a string, integer or
//!   boolean. Any other example (a statement, a continuation, a trace, `...`,
//!   `<BLANKLINE>`) may change the namespace the later examples run in, so the
//!   docstring is not read at all;
//! - no `# doctest:` directive and no backslash appears in the docstring, so
//!   the text pytest matches is the text read here;
//! - the example exists unchanged in the old source (the diff does not write
//!   its own oracle), and the module is changed by the diff;
//! - pytest collects the module's doctests ([`collects_doctests`]).
//!
//! The synthesized test carries a path no file has, so nothing reads it from
//! disk and no rule that treats the owner's own file as an edited test applies
//! to it.

use super::PythonTest;
use super::approx_band::parse_expression;
use super::doctest_eval::{Function, Value, literal};
use super::module_constants::mentions_identifier;
use super::source_facts::{extract_source_facts, parse_module_result};
use super::source_utils::SourceText;
use rustpython_parser::ast::{self, Constant, Expr, Mod, Ranged, Stmt};
use std::path::{Component, Path, PathBuf};

/// The most examples read from one module, so a generated file of thousands of
/// docstrings cannot dominate the run.
const MAX_EXAMPLES_PER_MODULE: usize = 64;

/// The most bytes read from one configuration file or `conftest.py`.
const MAX_CONFIG_BYTES: u64 = 64 * 1024;

/// The tests the doctest examples of changed module `file` state: examples of
/// the new source that also stand in the old source, that pytest collects and
/// whose result differs between the old and the new function.
pub(super) fn doctest_tests(
    file: &Path,
    new_source: &str,
    old_source: &str,
    root: &Path,
) -> Vec<PythonTest> {
    if !collects_doctests(root, file) {
        return Vec::new();
    }
    let (Ok(Mod::Module(new_module)), Ok(Mod::Module(old_module))) = (
        parse_module_result(file, new_source),
        parse_module_result(file, old_source),
    ) else {
        return Vec::new();
    };
    let Some(import_module) = dotted_module_path(file) else {
        return Vec::new();
    };
    let new_text = SourceText::new(new_source);
    let old_text = SourceText::new(old_source);
    // The evaluator reads `len` as the builtin; a module that binds the name
    // may run something else.
    if binds_len(&new_module.body, new_source) || binds_len(&old_module.body, old_source) {
        return Vec::new();
    }
    let new_names = defined_names(&new_module.body, new_source);
    let old_names = defined_names(&old_module.body, old_source);
    let mut tests = Vec::new();
    let mut count = 0usize;
    for statement in &new_module.body {
        let Stmt::FunctionDef(function) = statement else {
            continue;
        };
        let name = function.name.as_str();
        if !new_names.iter().any(|defined| defined == name)
            || !old_names.iter().any(|defined| defined == name)
        {
            continue;
        }
        let Some(old_function) = old_module.body.iter().find_map(|old| match old {
            Stmt::FunctionDef(candidate) if candidate.name.as_str() == name => Some(candidate),
            _ => None,
        }) else {
            continue;
        };
        let Some(new_examples) = examples_of(function, &new_text, new_source) else {
            continue;
        };
        let old_examples = examples_of(old_function, &old_text, old_source).unwrap_or_default();
        let (Some(new_body), Some(old_body)) = (Function::of(function), Function::of(old_function))
        else {
            continue;
        };
        for example in new_examples {
            // The example must have been there before the diff.
            if !old_examples
                .iter()
                .any(|old| old.call == example.call && old.expected == example.expected)
            {
                continue;
            }
            count += 1;
            if count > MAX_EXAMPLES_PER_MODULE {
                return tests;
            }
            if !example_notices_edit(&example, &old_body, &new_body, name) {
                continue;
            }
            if let Some(test) = example_test(file, &import_module, name, &example) {
                tests.push(test);
            }
        }
    }
    tests
}

/// Whether the example, run against the old and the new function, passes on
/// the old and fails on the new. Anything the evaluator cannot run is `false`.
fn example_notices_edit(
    example: &Example,
    old_function: &Function<'_>,
    new_function: &Function<'_>,
    name: &str,
) -> bool {
    let Some(arguments) = example.arguments(name) else {
        return false;
    };
    let Some(expected) = example.expected_value() else {
        return false;
    };
    let (Some(before), Some(after)) =
        (old_function.call(&arguments), new_function.call(&arguments))
    else {
        return false;
    };
    before == expected && after != expected
}

/// Whether pytest collects the doctests of `file` (RIPR-SPEC-0238 rule 26).
///
/// Only the configuration pytest itself reads is consulted, in pytest's
/// precedence: `pytest.ini`, then `pyproject.toml`'s `[tool.pytest.ini_options]`,
/// then `tox.ini`'s `[pytest]`, then `setup.cfg`'s `[tool:pytest]`. The
/// command line is not known, so collection means "a bare `pytest` run": the
/// configuration must carry `--doctest-modules` in `addopts`, and nothing in it
/// or in a `conftest.py` above the file may select, skip or reshape what is
/// collected. Anything unreadable, oversized or unrecognized is "not collected".
pub(super) fn collects_doctests(root: &Path, file: &Path) -> bool {
    let name = file.file_name().and_then(|name| name.to_str());
    if matches!(name, Some("setup.py" | "__main__.py"))
        || file.extension().is_none_or(|e| e != "py")
    {
        return false;
    }
    // pytest skips these directories while recursing, so a module under one is
    // not collected from a bare run.
    if file.components().any(|component| match component {
        Component::Normal(part) => part.to_str().is_none_or(|part| {
            part.starts_with('.')
                || part.ends_with(".egg")
                || matches!(
                    part,
                    "_darcs" | "build" | "CVS" | "dist" | "node_modules" | "venv" | "{arch}"
                )
        }),
        _ => true,
    }) {
        return false;
    }
    let Some(section) = pytest_section(root) else {
        return false;
    };
    if !section.collects(file) {
        return false;
    }
    conftests_allow_collection(root, file)
}

/// The pytest options of a configuration file, as raw tokens.
struct PytestSection {
    addopts: Vec<String>,
    testpaths: Option<Vec<String>>,
    /// Any option that selects, skips or reshapes collection or doctest
    /// matching, or that this reader does not model.
    reshapes: bool,
}

impl PytestSection {
    fn collects(&self, file: &Path) -> bool {
        const REFUSED_FLAGS: [&str; 18] = [
            "--ignore",
            "--ignore-glob",
            "--deselect",
            "-k",
            "-m",
            "--doctest-glob",
            "--doctest-ignore-import-errors",
            "--pyargs",
            "--collect-only",
            "--co",
            "--confcutdir",
            "--rootdir",
            "--import-mode",
            "--override-ini",
            "-o",
            "-c",
            "--config-file",
            "-p",
        ];
        // A positional token names a collection target, so a bare `pytest` run
        // collects that target and not the whole tree.
        if self.reshapes
            || self.addopts.iter().any(|token| !token.starts_with('-'))
            || self.addopts.iter().any(|token| {
                ["-k", "-m", "-o", "-c", "-p"]
                    .iter()
                    .any(|short| token.starts_with(short) && !token.starts_with("--"))
                    || matches!(
                        token.as_str(),
                        "--lf"
                            | "--ff"
                            | "--sw"
                            | "--nf"
                            | "--last-failed"
                            | "--failed-first"
                            | "--stepwise"
                            | "--new-first"
                    )
            })
            || !self
                .addopts
                .iter()
                .any(|token| token == "--doctest-modules")
            || self.addopts.iter().any(|token| {
                REFUSED_FLAGS.iter().any(|flag| {
                    token == flag
                        || token
                            .strip_prefix(flag)
                            .is_some_and(|rest| rest.starts_with('='))
                }) || token.contains("no:doctest")
            })
        {
            return false;
        }
        match &self.testpaths {
            None => true,
            Some(paths) => {
                !paths.is_empty()
                    && paths.iter().any(|path| {
                        let path = Path::new(path.trim_start_matches("./"));
                        file.starts_with(path)
                    })
            }
        }
    }
}

fn read_bounded(path: &Path) -> Option<String> {
    use std::io::Read;
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return None;
    }
    // One byte past the limit: a file that grew after the check is refused,
    // never read as a truncated configuration that hides a later flag.
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    (text.len() as u64 <= MAX_CONFIG_BYTES).then_some(text)
}

/// The first configuration file pytest would use, as a section.
fn pytest_section(root: &Path) -> Option<PytestSection> {
    // Newer pytest reads these before the files modeled below; their options
    // are not modeled, so their presence leaves collection unknown.
    if [".pytest.ini", "pytest.toml", ".pytest.toml"]
        .iter()
        .any(|name| root.join(name).exists())
    {
        return None;
    }
    if root.join("pytest.ini").exists() {
        return section_of(&read_bounded(&root.join("pytest.ini"))?, "[pytest]", false);
    }
    if let Some(text) = read_bounded(&root.join("pyproject.toml"))
        && text
            .lines()
            .any(|line| line.trim() == "[tool.pytest.ini_options]")
        && !text.lines().any(|line| line.trim() == "[tool.pytest]")
    {
        return section_of(&text, "[tool.pytest.ini_options]", true);
    }
    for (name, header) in [("tox.ini", "[pytest]"), ("setup.cfg", "[tool:pytest]")] {
        if let Some(text) = read_bounded(&root.join(name))
            && text.lines().any(|line| line.trim() == header)
        {
            return section_of(&text, header, false);
        }
    }
    tox_command_section(root)
}

/// The pytest options of a `tox.ini` `[testenv]` `commands` line (#4561), for a
/// project whose only doctest signal is the command it runs
/// (`pytest --doctest-modules`).
///
/// Read only when no pytest configuration of any kind exists: a
/// `pyproject.toml` pytest table, whichever form, may reshape what a bare
/// `pytest` collects (the `humanize` negative twin), so it leaves collection
/// unknown. A line is read only when it is exactly one pytest invocation
/// (`pytest`, `py.test` or `python -m pytest`) with no shell operator, factor
/// condition or `tox` substitution other than an empty `{posargs}`.
fn tox_command_section(root: &Path) -> Option<PytestSection> {
    if read_bounded(&root.join("pyproject.toml")).is_some_and(|text| {
        text.lines()
            .any(|line| line.trim().starts_with("[tool.pytest"))
    }) {
        return None;
    }
    let text = read_bounded(&root.join("tox.ini"))?;
    let mut in_testenv = false;
    let mut in_commands = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && !line.starts_with(char::is_whitespace) {
            in_testenv = trimmed == "[testenv]";
            in_commands = false;
            continue;
        }
        if !in_testenv || trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';')
        {
            continue;
        }
        let command = if line.starts_with(char::is_whitespace) {
            if !in_commands {
                continue;
            }
            trimmed
        } else if let Some((key, value)) = trimmed.split_once('=') {
            in_commands = key.trim() == "commands";
            if !in_commands {
                continue;
            }
            value.trim()
        } else {
            in_commands = false;
            continue;
        };
        if let Some(section) = pytest_command_section(command) {
            return Some(section);
        }
    }
    None
}

/// The options of one command line when it is a single bare pytest run.
fn pytest_command_section(command: &str) -> Option<PytestSection> {
    let command = command.replace("{posargs}", "");
    if command.contains([
        '&', '|', ';', '$', '`', '{', '}', '<', '>', ':', '#', '(', ')',
    ]) {
        return None;
    }
    let words: Vec<&str> = command.split_whitespace().collect();
    let rest = match words.as_slice() {
        ["pytest" | "py.test", rest @ ..] | ["python" | "python3", "-m", "pytest", rest @ ..] => {
            rest
        }
        _ => return None,
    };
    let addopts: Vec<String> = rest.iter().map(|word| (*word).to_string()).collect();
    addopts
        .iter()
        .any(|word| word == "--doctest-modules")
        .then_some(PytestSection {
            addopts,
            testpaths: None,
            reshapes: false,
        })
}

/// The options of one section. Values may continue on later lines (INI
/// indentation or an open TOML array); every quoted or bare word of the value
/// is a token.
fn section_of(text: &str, header: &str, toml: bool) -> Option<PytestSection> {
    let mut in_section = false;
    let mut depth = 0i32;
    let mut current: Option<String> = None;
    let mut values: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if depth == 0 && trimmed.starts_with('[') && !trimmed.contains('=') {
            in_section = trimmed == header;
            current = None;
            continue;
        }
        if !in_section || trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';')
        {
            continue;
        }
        let continuation = depth > 0 || (!toml && line.starts_with(char::is_whitespace));
        if !continuation {
            let (key, value) = trimmed.split_once('=')?;
            current = Some(key.trim().to_string());
            values.push((key.trim().to_string(), tokens(value)));
        } else if let Some(key) = &current {
            let entry = values.iter_mut().rev().find(|(name, _)| name == key)?;
            entry.1.extend(tokens(trimmed));
        }
        let opens = i32::try_from(trimmed.matches('[').count()).ok()?;
        let closes = i32::try_from(trimmed.matches(']').count()).ok()?;
        depth = (depth + opens - closes).max(0);
        if toml && trimmed.contains("\"\"\"") {
            return None;
        }
    }
    let option = |name: &str| values.iter().find(|(key, _)| key == name);
    let known = [
        "addopts",
        "testpaths",
        "minversion",
        "pythonpath",
        "python_files",
    ];
    let reshapes = values
        .iter()
        .any(|(key, _)| !known.contains(&key.as_str()) && key != "required_plugins")
        || option("python_files").is_some();
    Some(PytestSection {
        addopts: option("addopts").map(|(_, tokens)| tokens.clone())?,
        testpaths: option("testpaths").map(|(_, tokens)| tokens.clone()),
        reshapes,
    })
}

/// The words of one value fragment: quotes, brackets and commas separate.
fn tokens(value: &str) -> Vec<String> {
    value
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | '[' | ']' | ','))
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

/// Every `conftest.py` from the root down to the file's directory, and the
/// ones directly under the root's subdirectories (a hook there reshapes every
/// collected item), must leave collection and the doctest namespace alone.
/// Substring matches are deliberate: `collect_ignore_glob`, a `doctest_namespace`
/// fixture or any `pytest_*` hook is refused without modeling it.
fn conftests_allow_collection(root: &Path, file: &Path) -> bool {
    // On the module's path a fixture or an ignore list reaches the module's
    // items; a conftest elsewhere reaches them only through a session-wide
    // hook or plugin list (its fixtures and ignore lists stay in its own tree).
    const ON_PATH: [&str; 5] = [
        "doctest",
        "collect_ignore",
        "def pytest_",
        "autouse",
        "pytest_plugins",
    ];
    const ELSEWHERE: [&str; 2] = ["def pytest_", "pytest_plugins"];
    const MAX_SIBLING_DIRECTORIES: usize = 256;
    let clean = |directory: &Path, refused: &[&str]| {
        let conftest = root.join(directory).join("conftest.py");
        !conftest.exists()
            || read_bounded(&conftest)
                .is_some_and(|text| !refused.iter().any(|word| text.contains(word)))
    };
    let mut directory = PathBuf::new();
    if !clean(&directory, &ON_PATH) {
        return false;
    }
    for component in file.parent().into_iter().flat_map(Path::components) {
        directory.push(component);
        if !clean(&directory, &ON_PATH) {
            return false;
        }
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return true;
    };
    // Directory order is arbitrary, so a cap that stopped early would pick
    // different directories per run: a root with more subdirectories than the
    // cap is unknown and refuses the read.
    let directories: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.path().strip_prefix(root).map(Path::to_path_buf).ok())
        .take(MAX_SIBLING_DIRECTORIES + 1)
        .collect();
    directories.len() <= MAX_SIBLING_DIRECTORIES
        && directories
            .iter()
            .all(|directory| clean(directory, &ELSEWHERE))
}

/// One `>>> call(...)` example and the literal line that follows it.
#[derive(Debug, PartialEq, Eq)]
struct Example {
    /// 1-indexed file line of the `>>>` line.
    line: usize,
    call: String,
    expected: String,
}

impl Example {
    /// The literal arguments of `name(...)`, in call order.
    fn arguments(&self, name: &str) -> Option<Vec<Value>> {
        let Expr::Call(call) = parse_expression(&self.call)? else {
            return None;
        };
        let Expr::Name(callee) = call.func.as_ref() else {
            return None;
        };
        if callee.id.as_str() != name || !call.keywords.is_empty() {
            return None;
        }
        call.args.iter().map(literal).collect()
    }

    /// The expected value, only when the line is exactly its `repr`.
    fn expected_value(&self) -> Option<Value> {
        let value = literal(&parse_expression(&self.expected)?)?;
        (value.repr()? == self.expected).then_some(value)
    }
}

/// Every example of a function's docstring, or `None` when the function has no
/// docstring or it holds anything that is not a plain call-and-literal example.
fn examples_of(
    function: &ast::StmtFunctionDef,
    text: &SourceText<'_>,
    source: &str,
) -> Option<Vec<Example>> {
    let Some(Stmt::Expr(expression)) = function.body.first() else {
        return None;
    };
    let Expr::Constant(constant) = expression.value.as_ref() else {
        return None;
    };
    let Constant::Str(value) = &constant.value else {
        return None;
    };
    // An escape in the source (`\n`, `\x0a`, a line continuation) decodes to
    // text whose line count differs from the file's, so the lines read here
    // would not be the file's lines.
    let range = expression.value.range();
    if source
        .get(usize::from(range.start())..usize::from(range.end()))
        .is_none_or(|raw| raw.contains('\\'))
    {
        return None;
    }
    let first_line = text.line_for_offset(usize::from(expression.value.range().start()));
    read_examples(value, first_line)
}

/// The examples of one docstring.
///
/// `first_line` is the file line of the docstring's opening quotes; line `k`
/// of the value is on file line `first_line + k`, which holds while the
/// docstring has no backslash (a continuation or escape would shift it).
fn read_examples(docstring: &str, first_line: usize) -> Option<Vec<Example>> {
    if docstring.contains('\\')
        || docstring.contains("# doctest")
        || docstring.contains("<BLANKLINE>")
    {
        return None;
    }
    let lines: Vec<&str> = docstring.lines().collect();
    let mut examples = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if trimmed.starts_with("...") {
            return None;
        }
        let Some(call) = trimmed.strip_prefix(">>> ") else {
            if trimmed.starts_with(">>>") {
                return None;
            }
            index += 1;
            continue;
        };
        let indent = lines[index].len() - trimmed.len();
        let want = lines.get(index + 1)?;
        let want_trimmed = want.trim_start();
        // The expected output is one non-blank line at the example's indent
        // that is neither a prompt nor a continuation, ended by a blank line,
        // the next prompt or the end of the docstring.
        if want_trimmed.is_empty()
            || want.len() - want_trimmed.len() != indent
            || want_trimmed.starts_with(">>>")
            || want_trimmed.starts_with("...")
        {
            return None;
        }
        if let Some(after) = lines.get(index + 2)
            && !after.trim().is_empty()
            && !after.trim_start().starts_with(">>>")
        {
            return None;
        }
        examples.push(Example {
            line: first_line + index,
            call: call.trim().to_string(),
            expected: want_trimmed.trim_end().to_string(),
        });
        index += 2;
    }
    (!examples.is_empty()).then_some(examples)
}

/// The test the example states, extracted like any other test.
fn example_test(
    file: &Path,
    import_module: &str,
    name: &str,
    example: &Example,
) -> Option<PythonTest> {
    let test_name = format!("test_doctest_line_{}", example.line);
    let synthetic = format!(
        "from {import_module} import {name}\n\n\ndef {test_name}():\n    assert (\n        {}\n    ) == {}\n",
        example.call, example.expected
    );
    let path = synthetic_path(file, example.line);
    let mut tests = extract_source_facts(&path, &synthetic).tests;
    if tests.len() != 1 {
        return None;
    }
    let mut test = tests.remove(0);
    test.file = path;
    test.line = example.line;
    for assertion in &mut test.assertions {
        assertion.line = example.line;
    }
    Some(test)
}

/// A path no file has, under a `tests` directory so the test-file predicate
/// admits it.
fn synthetic_path(file: &Path, line: usize) -> PathBuf {
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("module");
    Path::new("tests")
        .join("__doctest__")
        .join(format!("test_{stem}_line_{line}.py"))
}

/// `idioms/texts.py` as `idioms.texts`, or `None` when a component is not an
/// identifier (a hyphenated directory cannot be imported by name).
fn dotted_module_path(file: &Path) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for component in file.with_extension("").components() {
        let text = component.as_os_str().to_str()?;
        if text.is_empty()
            || !text
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            || text.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        {
            return None;
        }
        parts.push(text.to_string());
    }
    if parts.last().map(String::as_str) == Some("__init__") {
        parts.pop();
    }
    (!parts.is_empty()).then(|| parts.join("."))
}

/// Names a top-level `def` or `class` binds exactly once, with no other
/// top-level statement mentioning the name (an assignment, an import or a
/// decorator application could replace the function the docstring describes),
/// and no `__test__` table that changes what doctest collects.
fn defined_names(body: &[Stmt], source: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut other_text = String::new();
    for statement in body {
        match statement {
            Stmt::FunctionDef(function) => names.push(function.name.to_string()),
            Stmt::ClassDef(class) => names.push(class.name.to_string()),
            other => {
                let range = other.range();
                if let Some(text) = source.get(usize::from(range.start())..usize::from(range.end()))
                {
                    other_text.push_str(text);
                    other_text.push('\n');
                }
            }
        }
    }
    if mentions_identifier(&other_text, "__test__") || other_text.contains("import *") {
        return Vec::new();
    }
    names
        .iter()
        .filter(|name| {
            names.iter().filter(|candidate| candidate == name).count() == 1
                && !mentions_identifier(&other_text, name)
        })
        .cloned()
        .collect()
}

/// Whether a top-level statement binds or mentions `len`, which could shadow
/// the builtin the evaluator assumes. A function that uses `len` itself is
/// the common case, so only statements outside function bodies count, plus a
/// `def len`.
fn binds_len(body: &[Stmt], source: &str) -> bool {
    body.iter().any(|statement| match statement {
        Stmt::FunctionDef(function) => function.name.as_str() == "len",
        Stmt::ClassDef(class) => class.name.as_str() == "len",
        other => {
            let range = other.range();
            source
                .get(usize::from(range.start())..usize::from(range.end()))
                .is_none_or(|text| mentions_identifier(text, "len"))
        }
    })
}
