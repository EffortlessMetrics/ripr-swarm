//! Collection rules pytest reads from outside the test file.
//!
//! A test pytest never collects does not run, so its assertion is no evidence
//! about the changed behavior. Two project-level sources decide that, and both
//! are evaluated here so a skipped-at-collection test gets the same
//! `test_activation_unestablished` limit as a `@pytest.mark.skip` one:
//!
//! - `conftest.py` `collect_ignore` / `collect_ignore_glob`, relative to the
//!   conftest's directory;
//! - the pytest configuration of a directory (`pytest.ini`, `.pytest.ini`,
//!   `tox.ini` `[pytest]`, `setup.cfg` `[tool:pytest]`, `pyproject.toml`
//!   `[tool.pytest.ini_options]` / `[tool.pytest]`, `pytest.toml`): the
//!   options `python_files`, `python_classes`, `python_functions`,
//!   `testpaths`, `norecursedirs` and `--ignore` / `--ignore-glob` in
//!   `addopts`.
//!
//! The reading is parse-everything-or-refuse. A form this module cannot
//! evaluate (an unterminated quote, a character class, a computed
//! `collect_ignore`, a collection hook, an unreadable or unparsable file,
//! `-o`/`-c`/`--rootdir` in `addopts`) marks the whole directory scope
//! `unevaluated`, which withholds credit. Credit is never kept by default when
//! configuration exists in a shape that is not understood.
//!
//! Within one directory, pytest selects a single inifile by precedence
//! (`pytest.toml` / `.pytest.toml` / `pytest.ini` / `.pytest.ini` even if
//! empty, then `pyproject.toml` with a pytest table, `tox.ini` `[pytest]`,
//! `setup.cfg` `[tool:pytest]`). Later files in that directory are ignored.
//! An unreadable or unparsable candidate, or `addopts` that rewrite the
//! config file (`-c`, `--rootdir`, `--config-file`), falls back to unioning
//! every present file so this pass only over-withholds. Every selected (or
//! unioned) config on a test file's directory chain still counts, because a
//! static read cannot know the invocation `rootdir`, and a test is excluded
//! when any of them excludes it.
//!
//! Not modelled (credit is unchanged): the default `norecursedirs`, `-k` /
//! `-m` / `--deselect` selection, `pytest_collection_modifyitems`, positional
//! arguments given on the command line, and plugins. Those are named limits of
//! this pass, not claims that the tests run.

use super::test_activation::{DeclarationSite, collection_control};
use super::{PythonFramework, PythonTest};
use crate::analysis::path_glob::{path_glob_matches, segment_glob_matches};
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Largest configuration file this pass reads; a bigger one is unevaluated.
const MAX_CONFIG_BYTES: u64 = 1 << 20;

const COLLECT_IGNORE: &str = "pytest.collection(collect_ignore)";
const UNEVALUATED_CONFTEST: &str = "pytest.collection(unevaluated conftest)";
const UNEVALUATED_CONFIG: &str = "pytest.collection(unevaluated config)";
const PYTHON_FILES: &str = "pytest.collection(python_files)";
const PYTHON_CLASSES: &str = "pytest.collection(python_classes)";
const PYTHON_FUNCTIONS: &str = "pytest.collection(python_functions)";
const TESTPATHS: &str = "pytest.collection(testpaths)";
const NORECURSEDIRS: &str = "pytest.collection(norecursedirs)";
const IGNORE: &str = "pytest.collection(--ignore)";

/// Withhold credit from every test pytest's collection rules do not select,
/// or whose collection rules this pass cannot evaluate. `root` is the
/// workspace root `tests` paths are relative to.
pub(super) fn apply_collection_config(tests: &mut [PythonTest], root: &Path) {
    let mut dirs: BTreeMap<PathBuf, Arc<DirRules>> = BTreeMap::new();
    let mut chains: BTreeMap<PathBuf, Vec<(PathBuf, Arc<DirRules>)>> = BTreeMap::new();
    let mut file_reasons: BTreeMap<PathBuf, Option<&'static str>> = BTreeMap::new();
    for test in tests.iter_mut() {
        let chain = chains.entry(test.file.clone()).or_insert_with(|| {
            let mut found: Vec<(PathBuf, Arc<DirRules>)> = test
                .file
                .ancestors()
                .skip(1)
                .map(|dir| {
                    let rules = dirs
                        .entry(dir.to_path_buf())
                        .or_insert_with(|| Arc::new(DirRules::read(root, dir)))
                        .clone();
                    (dir.to_path_buf(), rules)
                })
                .collect();
            found.retain(|(_, rules)| !rules.is_empty());
            found
        });
        if chain.is_empty() {
            continue;
        }
        let file_reason = *file_reasons.entry(test.file.clone()).or_insert_with(|| {
            chain
                .iter()
                .find_map(|(dir, rules)| rules.file_exclusion(&relative_to(&test.file, dir)))
        });
        let reason = file_reason.or_else(|| {
            chain
                .iter()
                .find_map(|(_, rules)| rules.test_exclusion(test))
        });
        if let Some(reason) = reason
            && !test
                .activation_controls
                .iter()
                .any(|control| control == &collection_control(reason, DeclarationSite::Module))
        {
            test.activation_controls
                .push(collection_control(reason, DeclarationSite::Module));
        }
    }
}

fn relative_to(file: &Path, dir: &Path) -> String {
    let relative = file.strip_prefix(dir).unwrap_or(file);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Each cumulative path of `relative`: `a/b/c.py` gives `a`, `a/b`, `a/b/c.py`.
fn prefixes(relative: &str) -> Vec<&str> {
    let mut found: Vec<&str> = relative
        .match_indices('/')
        .map(|(index, _)| &relative[..index])
        .collect();
    found.push(relative);
    found
}

enum Source {
    Missing,
    Text(String),
    Unreadable,
}

fn read_source(root: &Path, relative: &Path) -> Source {
    let full = root.join(relative);
    match std::fs::metadata(&full) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Source::Missing,
        Err(_) => Source::Unreadable,
        Ok(meta) if !meta.is_file() => Source::Missing,
        Ok(meta) if meta.len() > MAX_CONFIG_BYTES => Source::Unreadable,
        Ok(_) => std::fs::read_to_string(&full).map_or(Source::Unreadable, Source::Text),
    }
}

/// The collection rules one directory contributes.
#[derive(Default)]
struct DirRules {
    conftest: Option<ConftestRules>,
    configs: Vec<IniRules>,
}

impl DirRules {
    fn read(root: &Path, dir: &Path) -> Self {
        let conftest = match read_source(root, &dir.join("conftest.py")) {
            Source::Missing => None,
            Source::Unreadable => Some(ConftestRules::unevaluated()),
            Source::Text(text) => Some(ConftestRules::parse(&text)),
        };
        Self {
            conftest,
            configs: read_directory_configs(root, dir),
        }
    }

    fn is_empty(&self) -> bool {
        self.configs.is_empty() && self.conftest.as_ref().is_none_or(ConftestRules::is_inert)
    }

    fn file_exclusion(&self, relative: &str) -> Option<&'static str> {
        self.conftest
            .as_ref()
            .and_then(|rules| rules.exclusion(relative))
            .or_else(|| {
                self.configs
                    .iter()
                    .find_map(|rules| rules.file_exclusion(relative))
            })
    }

    fn test_exclusion(&self, test: &PythonTest) -> Option<&'static str> {
        self.configs
            .iter()
            .find_map(|rules| rules.test_exclusion(test))
    }
}

/// `collect_ignore` / `collect_ignore_glob` of one `conftest.py`.
#[derive(Default)]
struct ConftestRules {
    ignore: Vec<String>,
    ignore_glob: Vec<String>,
    unevaluated: bool,
}

/// Names whose presence in a statement this pass does not evaluate marks the
/// conftest unevaluated.
const COLLECTION_NAMES: &[&str] = &[
    "collect_ignore",
    "pytest_ignore_collect",
    "pytest_collect_file",
    "pytest_pycollect_makeitem",
];

impl ConftestRules {
    fn unevaluated() -> Self {
        Self {
            unevaluated: true,
            ..Self::default()
        }
    }

    fn is_inert(&self) -> bool {
        !self.unevaluated && self.ignore.is_empty() && self.ignore_glob.is_empty()
    }

    fn parse(source: &str) -> Self {
        let mut rules = Self::default();
        if !COLLECTION_NAMES.iter().any(|name| source.contains(name)) {
            return rules;
        }
        // Through the shared nesting budget, so a deeply nested conftest
        // refuses instead of reaching the recursive parser.
        let Ok(Mod::Module(module)) =
            super::source_facts::parse_module_result(Path::new("conftest.py"), source)
        else {
            return Self::unevaluated();
        };
        for stmt in &module.body {
            if rules.evaluate(stmt) {
                continue;
            }
            let range = stmt.range();
            let text = source
                .get(usize::from(range.start())..usize::from(range.end()))
                .unwrap_or(source);
            if COLLECTION_NAMES.iter().any(|name| text.contains(name)) {
                rules.unevaluated = true;
            }
        }
        rules
    }

    /// Evaluate a plain top-level `collect_ignore` statement. False when the
    /// statement is not one of the understood forms.
    fn evaluate(&mut self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Assign(assign) if assign.targets.len() == 1 => {
                let (Some(list), Some(name)) =
                    (string_list(&assign.value), switch_name(&assign.targets[0]))
                else {
                    return false;
                };
                *self.slot(name) = list;
                true
            }
            Stmt::AugAssign(assign) if matches!(assign.op, ast::Operator::Add) => {
                let (Some(list), Some(name)) =
                    (string_list(&assign.value), switch_name(&assign.target))
                else {
                    return false;
                };
                self.slot(name).extend(list);
                true
            }
            Stmt::Expr(expr) => {
                let Expr::Call(call) = expr.value.as_ref() else {
                    return false;
                };
                let Expr::Attribute(attribute) = call.func.as_ref() else {
                    return false;
                };
                let (Some(name), true) = (
                    switch_name(&attribute.value),
                    call.keywords.is_empty() && call.args.len() == 1,
                ) else {
                    return false;
                };
                let added = match attribute.attr.as_str() {
                    "append" => string_literal(&call.args[0]).map(|value| vec![value]),
                    "extend" => string_list(&call.args[0]),
                    _ => None,
                };
                let Some(added) = added else {
                    return false;
                };
                self.slot(name).extend(added);
                true
            }
            _ => false,
        }
    }

    fn slot(&mut self, name: &str) -> &mut Vec<String> {
        if name == "collect_ignore" {
            &mut self.ignore
        } else {
            &mut self.ignore_glob
        }
    }

    fn exclusion(&self, relative: &str) -> Option<&'static str> {
        if self.unevaluated {
            return Some(UNEVALUATED_CONFTEST);
        }
        let mut excluded = false;
        for entry in &self.ignore {
            let Some(entry) = normalize_entry(entry) else {
                return Some(UNEVALUATED_CONFTEST);
            };
            excluded |= prefixes(relative).contains(&entry.as_str());
        }
        for glob in &self.ignore_glob {
            if glob.contains('[') || normalize_entry(glob).is_none() {
                return Some(UNEVALUATED_CONFTEST);
            }
            let glob = glob.trim_start_matches("./");
            excluded |= prefixes(relative)
                .into_iter()
                .any(|prefix| segment_glob_matches(glob, prefix));
        }
        excluded.then_some(COLLECT_IGNORE)
    }
}

fn switch_name(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(name)
            if matches!(name.id.as_str(), "collect_ignore" | "collect_ignore_glob") =>
        {
            Some(name.id.as_str())
        }
        _ => None,
    }
}

fn string_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Constant(constant) => match &constant.value {
            ast::Constant::Str(value) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    }
}

fn string_list(expr: &Expr) -> Option<Vec<String>> {
    let elements = match expr {
        Expr::List(list) => &list.elts,
        Expr::Tuple(tuple) => &tuple.elts,
        _ => return None,
    };
    elements.iter().map(string_literal).collect()
}

/// A workspace-relative path entry with `.` segments and a trailing `/`
/// removed. None for an absolute path, a `..` segment, a backslash or an
/// empty entry, none of which this pass resolves.
fn normalize_entry(entry: &str) -> Option<String> {
    if entry.starts_with('/') || entry.contains('\\') {
        return None;
    }
    let segments: Vec<&str> = entry
        .split('/')
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect();
    if segments.is_empty() || segments.contains(&"..") {
        return None;
    }
    Some(segments.join("/"))
}

#[derive(Clone, Copy)]
enum ConfigFormat {
    Ini(&'static str),
    Toml(&'static [&'static [&'static str]]),
}

/// Outcome of asking whether one present file is pytest's inifile.
enum ConfigCandidate {
    /// Present but not a pytest config (no required section or table).
    Absent,
    /// Unparsable: which file pytest would pick is not established.
    Unsure,
    /// This file is the inifile; `Vec` is empty when it matches by presence
    /// with no pytest section (for example an empty `pytest.ini`).
    Selected(Vec<IniRules>),
}

/// Files pytest may use as the inifile, in pytest's precedence order.
/// The first match in a directory wins; an unreadable or unparsable
/// candidate, or `addopts` that rewrite the config file, falls back to
/// unioning every present file so this pass only over-withholds.
const CONFIG_FILES: [(&str, ConfigFormat); 7] = [
    ("pytest.toml", ConfigFormat::Toml(&[&["pytest"]])),
    (".pytest.toml", ConfigFormat::Toml(&[&["pytest"]])),
    ("pytest.ini", ConfigFormat::Ini("pytest")),
    (".pytest.ini", ConfigFormat::Ini("pytest")),
    (
        "pyproject.toml",
        ConfigFormat::Toml(&[&["tool", "pytest", "ini_options"], &["tool", "pytest"]]),
    ),
    ("tox.ini", ConfigFormat::Ini("pytest")),
    ("setup.cfg", ConfigFormat::Ini("tool:pytest")),
];

/// `pytest.toml` / `.pytest.toml` / `pytest.ini` / `.pytest.ini` match by
/// presence, including when empty.
fn inifile_matches_by_presence(name: &str) -> bool {
    matches!(
        name,
        "pytest.toml" | ".pytest.toml" | "pytest.ini" | ".pytest.ini"
    )
}

fn read_directory_configs(root: &Path, dir: &Path) -> Vec<IniRules> {
    match select_directory_inifile(root, dir) {
        Some(configs) => configs,
        None => union_directory_configs(root, dir),
    }
}

/// The inifile pytest would pick in `dir`, or `None` when that choice is not
/// established and the caller should union every present file.
fn select_directory_inifile(root: &Path, dir: &Path) -> Option<Vec<IniRules>> {
    for (name, format) in CONFIG_FILES {
        match read_source(root, &dir.join(name)) {
            Source::Missing => {}
            Source::Unreadable => return None,
            Source::Text(text) => match format.consider(name, &text) {
                ConfigCandidate::Absent => {}
                ConfigCandidate::Unsure => return None,
                ConfigCandidate::Selected(rules) => {
                    if rules.iter().any(IniRules::rewrites_config_file) {
                        return None;
                    }
                    return Some(rules);
                }
            },
        }
    }
    Some(Vec::new())
}

fn union_directory_configs(root: &Path, dir: &Path) -> Vec<IniRules> {
    let mut configs = Vec::new();
    for (name, format) in CONFIG_FILES {
        match read_source(root, &dir.join(name)) {
            Source::Missing => {}
            Source::Unreadable => configs.push(IniRules::unevaluated()),
            Source::Text(text) => configs.extend(format.rules(&text)),
        }
    }
    configs
}

impl ConfigFormat {
    fn consider(self, name: &str, text: &str) -> ConfigCandidate {
        match self {
            Self::Ini(section) => match ini_section(text, section) {
                Err(()) => ConfigCandidate::Unsure,
                Ok(None) if inifile_matches_by_presence(name) => {
                    ConfigCandidate::Selected(Vec::new())
                }
                Ok(None) => ConfigCandidate::Absent,
                Ok(Some(options)) => ConfigCandidate::Selected(vec![IniRules::from_options(
                    options
                        .into_iter()
                        .map(|(key, value)| (key, split_args(&value)))
                        .collect(),
                )]),
            },
            Self::Toml(tables) => {
                let Ok(document) = toml::from_str::<toml::Table>(text) else {
                    return ConfigCandidate::Unsure;
                };
                let rules: Vec<IniRules> = tables
                    .iter()
                    .filter_map(|path| toml_table(&document, path))
                    .map(|table| {
                        IniRules::from_options(
                            table
                                .iter()
                                .map(|(key, value)| (key.clone(), toml_words(value)))
                                .collect(),
                        )
                    })
                    .collect();
                if rules.is_empty() && !inifile_matches_by_presence(name) {
                    ConfigCandidate::Absent
                } else {
                    ConfigCandidate::Selected(rules)
                }
            }
        }
    }

    fn rules(self, text: &str) -> Vec<IniRules> {
        match self {
            Self::Ini(section) => match ini_section(text, section) {
                Err(()) => vec![IniRules::unevaluated()],
                Ok(None) => Vec::new(),
                Ok(Some(options)) => vec![IniRules::from_options(
                    options
                        .into_iter()
                        .map(|(key, value)| (key, split_args(&value)))
                        .collect(),
                )],
            },
            Self::Toml(tables) => {
                let Ok(document) = toml::from_str::<toml::Table>(text) else {
                    return vec![IniRules::unevaluated()];
                };
                tables
                    .iter()
                    .filter_map(|path| toml_table(&document, path))
                    .map(|table| {
                        IniRules::from_options(
                            table
                                .iter()
                                .map(|(key, value)| (key.clone(), toml_words(value)))
                                .collect(),
                        )
                    })
                    .collect()
            }
        }
    }
}

fn toml_table<'a>(document: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Table> {
    let mut table = document;
    for key in path {
        table = table.get(*key)?.as_table()?;
    }
    Some(table)
}

fn toml_words(value: &toml::Value) -> Option<Vec<String>> {
    match value {
        toml::Value::String(text) => split_args(text),
        toml::Value::Array(items) => items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect(),
        _ => None,
    }
}

/// The `key = value` pairs of one ini section: Ok(None) when the section is
/// absent, Err when the file is not a shape this reader understands.
fn ini_section(text: &str, section: &str) -> Result<Option<BTreeMap<String, String>>, ()> {
    let mut found: Option<BTreeMap<String, String>> = None;
    let mut inside = false;
    let mut last_key: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
            continue;
        }
        let continuation = line.starts_with([' ', '\t']);
        if continuation && let Some(key) = &last_key {
            if let Some(options) = found.as_mut().filter(|_| inside)
                && let Some(value) = options.get_mut(key)
            {
                value.push('\n');
                value.push_str(trimmed);
            }
            continue;
        }
        if let Some(name) = trimmed.strip_prefix('[') {
            let Some(name) = name.strip_suffix(']') else {
                return Err(());
            };
            inside = name == section;
            last_key = None;
            if inside {
                if found.is_some() {
                    return Err(());
                }
                found = Some(BTreeMap::new());
            }
            continue;
        }
        if !inside {
            last_key = None;
            continue;
        }
        let Some(split) = trimmed.find(['=', ':']) else {
            return Err(());
        };
        let key = trimmed[..split].trim().to_string();
        let value = trimmed[split + 1..].trim().to_string();
        let Some(options) = found.as_mut() else {
            return Err(());
        };
        if key.is_empty() || options.insert(key.clone(), value).is_some() {
            return Err(());
        }
        last_key = Some(key);
    }
    Ok(found)
}

/// `shlex.split` for the quoting forms that need no escape handling. None for
/// a backslash or an unterminated quote.
fn split_args(value: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    for ch in value.chars() {
        match quote {
            Some(open) if ch == open => quote = None,
            Some(_) => current.push(ch),
            None if ch == '\\' => return None,
            None if ch == '\'' || ch == '"' => {
                quote = Some(ch);
                started = true;
            }
            None if ch.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            None => {
                current.push(ch);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if started {
        words.push(current);
    }
    Some(words)
}

/// The pytest options of one configuration section that decide collection.
#[derive(Default)]
struct IniRules {
    python_files: Option<Vec<String>>,
    python_classes: Option<Vec<String>>,
    python_functions: Option<Vec<String>>,
    testpaths: Option<Vec<String>>,
    norecursedirs: Option<Vec<String>>,
    ignore: Vec<String>,
    ignore_glob: Vec<String>,
    unevaluated: bool,
    /// `addopts` contains `-c` / `--rootdir` / `--config-file`, so pytest may
    /// not be using this file as the inifile.
    config_file_override: bool,
}

impl IniRules {
    fn unevaluated() -> Self {
        Self {
            unevaluated: true,
            ..Self::default()
        }
    }

    fn rewrites_config_file(&self) -> bool {
        self.config_file_override
    }

    fn from_options(options: BTreeMap<String, Option<Vec<String>>>) -> Self {
        let mut rules = Self::default();
        for (key, words) in options {
            let slot = match key.as_str() {
                "python_files" => &mut rules.python_files,
                "python_classes" => &mut rules.python_classes,
                "python_functions" => &mut rules.python_functions,
                "testpaths" => &mut rules.testpaths,
                "norecursedirs" => &mut rules.norecursedirs,
                "addopts" => {
                    match words {
                        Some(words) => rules.scan_addopts(&words),
                        None => rules.unevaluated = true,
                    }
                    continue;
                }
                _ => continue,
            };
            // An option that is present but empty, quoted oddly, or built from
            // path separators or character classes this pass does not match
            // is not evaluated.
            let usable = words.filter(|words| {
                !words.is_empty()
                    && words.iter().all(|word| {
                        !word.is_empty()
                            && !word.contains('[')
                            && (key == "testpaths" || !word.contains('/'))
                    })
            });
            match usable {
                Some(words) => *slot = Some(words),
                None => rules.unevaluated = true,
            }
        }
        rules
    }

    fn scan_addopts(&mut self, words: &[String]) {
        let mut index = 0;
        while index < words.len() {
            let word = &words[index];
            index += 1;
            if let Some(long) = word.strip_prefix("--") {
                let (name, inline) = match long.split_once('=') {
                    Some((name, value)) => (format!("--{name}"), Some(value.to_string())),
                    None => (word.clone(), None),
                };
                if let Some(kind) = ignore_kind(&name) {
                    let separate = inline.is_none();
                    let value = inline.or_else(|| {
                        let value = words.get(index).cloned();
                        index += usize::from(value.is_some());
                        value
                    });
                    match value {
                        // pytest rejects `--ignore -q` (an option where the
                        // value belongs) before collecting anything.
                        Some(value) if separate && value.starts_with('-') => {
                            self.unevaluated = true;
                        }
                        Some(value) if kind == IgnoreKind::Path => self.ignore.push(value),
                        Some(value) => self.ignore_glob.push(value),
                        None => self.unevaluated = true,
                    }
                } else if inline.is_none() && VALUE_OPTIONS.contains(&name.as_str()) {
                    // The next token is this option's value (a report or log
                    // path), not a positional collection argument.
                    index += usize::from(index < words.len());
                } else if ["--ov", "--ro", "--confc", "--config"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
                {
                    // `--override-ini`, `--rootdir`, `--confcutdir`,
                    // `--config-file` change which rules apply.
                    self.unevaluated = true;
                    // `-c` / `--rootdir` / `--config-file` pick a different
                    // inifile; `--override-ini` and `--confcutdir` do not.
                    self.config_file_override |= ["--ro", "--config"]
                        .iter()
                        .any(|prefix| name.starts_with(prefix));
                }
            } else if word.starts_with('-') {
                // `-o name=value` and `-c file` rewrite configuration.
                if word.starts_with("-o") || word.starts_with("-c") {
                    self.unevaluated = true;
                    self.config_file_override |= word.starts_with("-c");
                }
            } else if word.contains('/') || word.ends_with(".py") || word.contains(".py::") {
                // A positional path restricts collection like a command-line
                // argument.
                self.unevaluated = true;
            }
        }
    }

    fn file_exclusion(&self, relative: &str) -> Option<&'static str> {
        if self.unevaluated {
            return Some(UNEVALUATED_CONFIG);
        }
        let all = prefixes(relative);
        for entry in &self.ignore {
            let Some(entry) = normalize_entry(entry) else {
                return Some(UNEVALUATED_CONFIG);
            };
            if all.contains(&entry.as_str()) {
                return Some(IGNORE);
            }
        }
        for glob in &self.ignore_glob {
            if glob.contains('[') || glob.contains('\\') {
                return Some(UNEVALUATED_CONFIG);
            }
            // pytest matches the pattern against the absolute path; the
            // relative spelling and an any-prefix spelling both count.
            let glob = glob.trim_start_matches("./");
            if all.iter().any(|prefix| {
                segment_glob_matches(glob, prefix)
                    || segment_glob_matches(glob, &format!("/{prefix}"))
            }) {
                return Some(IGNORE);
            }
        }
        if let Some(patterns) = &self.norecursedirs {
            let directories = &all[..all.len() - 1];
            if directories.iter().any(|directory| {
                let name = directory.rsplit('/').next().unwrap_or(directory);
                patterns
                    .iter()
                    .any(|pattern| segment_glob_matches(pattern, name))
            }) {
                return Some(NORECURSEDIRS);
            }
        }
        if let Some(patterns) = &self.python_files {
            let name = relative.rsplit('/').next().unwrap_or(relative);
            if !patterns
                .iter()
                .any(|pattern| segment_glob_matches(pattern, name))
            {
                return Some(PYTHON_FILES);
            }
        }
        if let Some(entries) = &self.testpaths {
            let mut selected = false;
            for entry in entries {
                if matches!(entry.as_str(), "." | "./") {
                    selected = true;
                    continue;
                }
                let Some(entry) = normalize_entry(entry) else {
                    return Some(UNEVALUATED_CONFIG);
                };
                selected |= all.iter().any(|prefix| path_glob_matches(&entry, prefix));
            }
            if !selected {
                return Some(TESTPATHS);
            }
        }
        None
    }

    fn test_exclusion(&self, test: &PythonTest) -> Option<&'static str> {
        if test.framework != PythonFramework::Pytest {
            return None;
        }
        let mut segments: Vec<&str> = test.qualified_name.split('.').collect();
        let function = segments.pop()?;
        if let Some(prefixes) = &self.python_classes
            && !segments.iter().all(|class| name_selected(class, prefixes))
        {
            return Some(PYTHON_CLASSES);
        }
        if let Some(prefixes) = &self.python_functions
            && !name_selected(function, prefixes)
        {
            return Some(PYTHON_FUNCTIONS);
        }
        None
    }
}

/// pytest's `python_classes` / `python_functions` rule: a prefix, or a glob
/// when the option holds `*` or `?`.
fn name_selected(name: &str, options: &[String]) -> bool {
    options.iter().any(|option| {
        name.starts_with(option.as_str())
            || (option.contains(['*', '?']) && segment_glob_matches(option, name))
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IgnoreKind {
    Path,
    Glob,
}

/// Common options that take a space-separated value which may look like a
/// path; the value is never a positional collection argument.
const VALUE_OPTIONS: &[&str] = &[
    "--junitxml",
    "--junit-xml",
    "--html",
    "--log-file",
    "--basetemp",
    "--report-log",
    "--resultlog",
    "--cov-report",
    "--cov-config",
    "--alluredir",
    "--cache-dir",
    "--json-report-file",
];

/// `--ignore` and `--ignore-glob`, including argparse's unambiguous prefixes.
fn ignore_kind(name: &str) -> Option<IgnoreKind> {
    if name == "--ignore" || (name.len() >= 4 && "--ignore".starts_with(name)) {
        Some(IgnoreKind::Path)
    } else if name == "--ignore-glob" || (name.len() > 8 && "--ignore-glob".starts_with(name)) {
        Some(IgnoreKind::Glob)
    } else {
        None
    }
}
