//! Module mocks registered from a runner setup file.
//!
//! A runner loads its setup files before every test file it governs:
//!
//! ```ts
//! // vitest.config.ts
//! export default { test: { setupFiles: ['./test/setup.ts'] } };
//! // test/setup.ts
//! vi.mock('../src/discount');
//! ```
//!
//! Every test under that config then calls the mock, not the changed module,
//! exactly as if its own file held the `vi.mock(...)`. Test extraction is a
//! pure function of one file, so it cannot see this. This pass runs once per
//! analysis, after extraction, and adds each setup-file mock to
//! `TypeScriptTest::mocks_in_file` of the tests the config governs. The
//! owner-module mock guard and the `mocked_module` static limit then read it
//! like any in-file mock.
//!
//! Bounded and syntax-only:
//!
//! - configs: `vitest.config.*`, `vite.config.*`, `vitest.workspace.*` and
//!   `jest.config.*` source files in the workspace, `jest.config.json`, and
//!   the `jest` key of a `package.json`;
//! - keys: `setupFiles`, `setupFilesAfterEnv` and the legacy
//!   `setupTestFrameworkScriptFile`, at any depth (a `test` block, a
//!   `projects` entry). `globalSetup` runs in another process and registers
//!   no mock for a test;
//! - entries: every string literal inside the value (`'./setup.ts'`,
//!   `resolve(__dirname, 'test/setup.ts')`), read against the config's
//!   directory and against a literal `rootDir` / `root` beside it. An entry
//!   that names no workspace file (a package such as `jest-extended/all`, a
//!   variable) contributes nothing;
//! - setup files: the file's own mocks plus those of files it loads through
//!   relative static imports, [`MAX_SETUP_IMPORT_HOPS`] hops and
//!   [`MAX_SETUP_FILES_PER_CONFIG`] files at most.
//!
//! A config governs every test file under its directory. A nearer config of
//! another project may in fact replace it; applying both can only withhold a
//! relation, never grant one.

use super::bounded_read::read_config_capped;
use super::owners::extract_imports_for_test_file;
use super::parse::parse_on_worker;
use super::paths::{is_relative_specifier, normalized_path};
use super::source_type_for;
use super::tests_extract::{UNRESOLVED_MOCK_SPECIFIER, extract_mocks_from_statements};
use super::types::{Shared, TypeScriptTest};
use oxc_ast::ast::{Expression, ObjectProperty, Statement, StringLiteral, TemplateLiteral};
use oxc_parser::Parser;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

/// Relative-import hops followed from a setup file.
const MAX_SETUP_IMPORT_HOPS: usize = 2;
/// Files read per config: its setup files and what they import.
const MAX_SETUP_FILES_PER_CONFIG: usize = 32;

const SETUP_KEYS: [&str; 3] = [
    "setupFiles",
    "setupFilesAfterEnv",
    "setupTestFrameworkScriptFile",
];
const ROOT_KEYS: [&str; 2] = ["rootDir", "root"];
const CONFIG_STEMS: [&str; 4] = [
    "vitest.config",
    "vite.config",
    "vitest.workspace",
    "jest.config",
];
const MODULE_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// One module mock a setup file registers.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SetupMock {
    /// A relative specifier, resolved to a workspace-relative module path
    /// (no leading `./`, extension kept as written).
    WorkspacePath(String),
    /// A bare, aliased or root-relative specifier, or the unresolved marker:
    /// it reads the same from any importer.
    Verbatim(String),
}

/// What a config file names: setup-file entries and literal root directories.
#[derive(Default)]
struct ConfigEntries {
    setup: Vec<String>,
    roots: Vec<String>,
}

/// Add the mocks of every governing runner setup file to each test's
/// `mocks_in_file`.
pub(crate) fn apply_setup_file_mocks(
    root: &Path,
    workspace_files: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
    tests: &mut [TypeScriptTest],
) {
    if tests.is_empty() {
        return;
    }
    let configs = runner_setup_mocks(root, workspace_files, sources, tests);
    if configs.is_empty() {
        return;
    }
    // Tests of one file are contiguous and share one list.
    let mut start = 0;
    while start < tests.len() {
        let file = tests[start].file.clone();
        let end = tests[start..]
            .iter()
            .position(|test| test.file != file)
            .map_or(tests.len(), |offset| start + offset);
        let test_file = normalized_path(&file);
        let test_dir = parent_dir(&test_file);
        let mut merged: Vec<String> = tests[start].mocks_in_file.to_vec();
        let before = merged.len();
        for (config_dir, mocks) in &configs {
            if !dir_contains(config_dir, &test_file) {
                continue;
            }
            for mock in mocks {
                let specifier = match mock {
                    SetupMock::Verbatim(specifier) => specifier.clone(),
                    SetupMock::WorkspacePath(path) => relative_specifier(test_dir, path),
                };
                if !merged.contains(&specifier) {
                    merged.push(specifier);
                }
            }
        }
        if merged.len() != before {
            let shared: Shared<String> = merged.into();
            for test in &mut tests[start..end] {
                test.mocks_in_file = shared.clone();
            }
        }
        start = end;
    }
}

/// Setup-file mocks per config directory (workspace-relative, `""` for the
/// root). Directories without a mock are left out.
fn runner_setup_mocks(
    root: &Path,
    workspace_files: &[PathBuf],
    sources: &HashMap<PathBuf, String>,
    tests: &[TypeScriptTest],
) -> BTreeMap<String, BTreeSet<SetupMock>> {
    let by_normalized: HashMap<String, &String> = sources
        .iter()
        .map(|(path, source)| (normalized_path(path), source))
        .collect();
    let mut entries: Vec<(String, ConfigEntries)> = Vec::new();
    for file in workspace_files {
        if !is_runner_config_source(file) {
            continue;
        }
        let normalized = normalized_path(file);
        let Some(source) = by_normalized.get(&normalized) else {
            continue;
        };
        if !SETUP_KEYS.iter().any(|key| source.contains(key)) {
            continue;
        }
        if let Some(found) = config_source_entries(file, source) {
            entries.push((parent_dir(&normalized).to_string(), found));
        }
    }
    // JSON configs are not in the source walk. A `jest.config.json` needs no
    // manifest beside it (`jest --config tests/jest.config.json`), so every
    // directory from a test file up to the root is asked once.
    let mut config_dirs: BTreeSet<String> = BTreeSet::new();
    let mut last_file: Option<&Path> = None;
    for test in tests {
        if last_file == Some(test.file.as_path()) {
            continue;
        }
        last_file = Some(test.file.as_path());
        let test_file = normalized_path(&test.file);
        let mut dir = parent_dir(&test_file);
        while config_dirs.insert(dir.to_string()) && !dir.is_empty() {
            dir = parent_dir(dir);
        }
    }
    // A root manifest can govern nested packages (`projects`).
    config_dirs.insert(String::new());
    for config_dir in config_dirs {
        for (name, key) in [("package.json", Some("jest")), ("jest.config.json", None)] {
            let Ok(text) = read_config_capped(&root.join(&config_dir).join(name)) else {
                continue;
            };
            if !SETUP_KEYS.iter().any(|key| text.contains(key)) {
                continue;
            }
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            let Some(value) = key.map_or(Some(&value), |key| value.get(key)) else {
                continue;
            };
            let mut found = ConfigEntries::default();
            json_entries(value, &mut found);
            entries.push((config_dir.clone(), found));
        }
    }

    let mut out: BTreeMap<String, BTreeSet<SetupMock>> = BTreeMap::new();
    for (config_dir, found) in entries {
        let mut bases = vec![config_dir.clone()];
        for literal in &found.roots {
            let base = match config_path(root, literal) {
                ConfigPath::Relative(literal) => join_normalized(&config_dir, literal),
                ConfigPath::Workspace(path) => join_normalized("", &path),
                ConfigPath::Outside => None,
            };
            if let Some(base) = base
                && !bases.contains(&base)
            {
                bases.push(base);
            }
        }
        let mut setup_files: Vec<String> = Vec::new();
        for entry in &found.setup {
            let candidates: Vec<String> = match config_path(root, entry) {
                ConfigPath::Relative(entry) => bases
                    .iter()
                    .filter_map(|base| join_normalized(base, entry))
                    .collect(),
                ConfigPath::Workspace(path) => join_normalized("", &path).into_iter().collect(),
                ConfigPath::Outside => Vec::new(),
            };
            for candidate in candidates {
                if setup_files.len() >= MAX_SETUP_FILES_PER_CONFIG {
                    break;
                }
                if let Some(file) = resolve_module_file(root, &by_normalized, &candidate)
                    && !setup_files.contains(&file)
                {
                    setup_files.push(file);
                }
            }
        }
        let mocks = setup_files_mocks(root, &by_normalized, setup_files);
        if !mocks.is_empty() {
            out.entry(config_dir).or_default().extend(mocks);
        }
    }
    out
}

/// How a config spells a path.
enum ConfigPath<'a> {
    /// Relative to the config's base directory (`./setup.ts`,
    /// `<rootDir>/setup.ts`, `setup.ts`).
    Relative(&'a str),
    /// An absolute path inside the analysis root, as a root-relative path.
    Workspace(String),
    /// An absolute path outside the analysis root: it names no workspace file.
    Outside,
}

fn config_path<'a>(root: &Path, literal: &'a str) -> ConfigPath<'a> {
    if let Some(rest) = literal.strip_prefix("<rootDir>") {
        return ConfigPath::Relative(rest.trim_start_matches('/'));
    }
    let path = Path::new(literal);
    // A rooted path without a drive (`/test/setup.js` on Windows) is not
    // relative to the config either.
    if !path.is_absolute() && !literal.starts_with(['/', '\\']) {
        return ConfigPath::Relative(literal);
    }
    let inside = path.strip_prefix(root).ok().or_else(|| {
        let canonical = root.canonicalize().ok()?;
        path.strip_prefix(&canonical).ok()
    });
    match inside {
        Some(relative) => ConfigPath::Workspace(normalized_path(relative)),
        None => ConfigPath::Outside,
    }
}

/// Mocks of `setup_files` and of the files they load through relative static
/// imports, within the hop and file bounds.
fn setup_files_mocks(
    root: &Path,
    sources: &HashMap<String, &String>,
    setup_files: Vec<String>,
) -> BTreeSet<SetupMock> {
    let mut mocks = BTreeSet::new();
    let mut seen: BTreeSet<String> = setup_files.iter().cloned().collect();
    let mut frontier = setup_files;
    for hop in 0..=MAX_SETUP_IMPORT_HOPS {
        let mut next = Vec::new();
        for file in frontier {
            let Some((file_mocks, imports)) = setup_file_facts(root, sources, &file) else {
                continue;
            };
            let dir = parent_dir(&file);
            for specifier in file_mocks {
                if specifier != UNRESOLVED_MOCK_SPECIFIER && is_relative_specifier(&specifier) {
                    // A relative mock that leaves the workspace names nothing in it.
                    if let Some(path) = join_normalized(dir, &specifier) {
                        mocks.insert(SetupMock::WorkspacePath(path));
                    }
                } else {
                    mocks.insert(SetupMock::Verbatim(specifier));
                }
            }
            if hop == MAX_SETUP_IMPORT_HOPS {
                continue;
            }
            for specifier in imports {
                if seen.len() >= MAX_SETUP_FILES_PER_CONFIG {
                    break;
                }
                if let Some(imported) = join_normalized(dir, &specifier)
                    .and_then(|path| resolve_module_file(root, sources, &path))
                    && seen.insert(imported.clone())
                {
                    next.push(imported);
                }
            }
        }
        frontier = next;
    }
    mocks
}

/// The mock specifiers and relative static import specifiers of one setup
/// file. `None` when it cannot be read or parsed.
fn setup_file_facts(
    root: &Path,
    sources: &HashMap<String, &String>,
    file: &str,
) -> Option<(Vec<String>, Vec<String>)> {
    let owned;
    let source: &str = match sources.get(file) {
        Some(source) => source,
        None => {
            owned = read_config_capped(&root.join(file)).ok()?;
            &owned
        }
    };
    // Cheap text gate: a file with neither a mock call nor an import to
    // follow has nothing to report.
    if !["mock", "Mock", "import", "require", "from"]
        .iter()
        .any(|word| source.contains(word))
    {
        return None;
    }
    parse_on_worker(Path::new(file), source, |file, source, allocator| {
        let ret = Parser::new(allocator, source, source_type_for(file)).parse();
        if !ret.errors.is_empty() {
            return None;
        }
        let imports = extract_imports_for_test_file(&ret.program.body);
        let mocks = extract_mocks_from_statements(&ret.program.body, &imports);
        let mut loads: Vec<String> = Vec::new();
        for statement in &ret.program.body {
            let specifier = match statement {
                Statement::ImportDeclaration(import) if !import.import_kind.is_type() => {
                    Some(import.source.value.as_str())
                }
                // `export type ... from` is erased: the helper never loads.
                Statement::ExportAllDeclaration(export) if !export.export_kind.is_type() => {
                    Some(export.source.value.as_str())
                }
                Statement::ExportNamedDeclaration(export) if !export.export_kind.is_type() => {
                    export.source.as_ref().map(|source| source.value.as_str())
                }
                // A bare `require('./mocks');` loads the helper without a
                // binding, so the import records below do not hold it.
                Statement::ExpressionStatement(statement) => bare_require(&statement.expression),
                _ => None,
            };
            if let Some(specifier) = specifier
                && is_relative_specifier(specifier)
                && !loads.iter().any(|seen| seen == specifier)
            {
                loads.push(specifier.to_string());
            }
        }
        // `require('./mocks')` binds through the import records.
        for import in &imports {
            if is_relative_specifier(&import.source) && !loads.contains(&import.source) {
                loads.push(import.source.clone());
            }
        }
        Some((mocks, loads))
    })
    .ok()
    .flatten()
}

/// The literal specifier of a `require('<specifier>')` call expression.
fn bare_require<'a>(expression: &'a Expression<'_>) -> Option<&'a str> {
    let Expression::CallExpression(call) = expression.get_inner_expression() else {
        return None;
    };
    let Expression::Identifier(callee) = call.callee.get_inner_expression() else {
        return None;
    };
    if callee.name != "require" || call.arguments.len() != 1 {
        return None;
    }
    match call
        .arguments
        .first()?
        .as_expression()?
        .get_inner_expression()
    {
        Expression::StringLiteral(literal) => Some(literal.value.as_str()),
        _ => None,
    }
}

fn is_runner_config_source(file: &Path) -> bool {
    let Some(name) = file.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    CONFIG_STEMS.iter().any(|stem| {
        name.strip_prefix(stem)
            .and_then(|rest| rest.strip_prefix('.'))
            .is_some_and(|extension| MODULE_EXTENSIONS.contains(&extension))
    })
}

/// Setup entries and root literals of a config source file. `None` when it
/// does not parse.
fn config_source_entries(file: &Path, source: &str) -> Option<ConfigEntries> {
    parse_on_worker(file, source, |file, source, allocator| {
        use oxc_ast_visit::Visit;
        let ret = Parser::new(allocator, source, source_type_for(file)).parse();
        if !ret.errors.is_empty() {
            return None;
        }
        let mut reader = ConfigReader::default();
        reader.visit_program(&ret.program);
        Some(reader.entries)
    })
    .ok()
    .flatten()
}

#[derive(Default)]
struct ConfigReader {
    entries: ConfigEntries,
    /// Depth of enclosing setup-key values; literals are setup entries
    /// while it is positive.
    in_setup_value: usize,
}

impl<'a> oxc_ast_visit::Visit<'a> for ConfigReader {
    fn visit_object_property(&mut self, property: &ObjectProperty<'a>) {
        let key = property.key.static_name();
        let key = key.as_deref();
        if !property.computed && key.is_some_and(|key| SETUP_KEYS.contains(&key)) {
            self.in_setup_value += 1;
            self.visit_expression(&property.value);
            self.in_setup_value -= 1;
            return;
        }
        if !property.computed
            && key.is_some_and(|key| ROOT_KEYS.contains(&key))
            && let Some(literal) = literal_text(&property.value)
        {
            self.entries.roots.push(literal);
        }
        oxc_ast_visit::walk::walk_object_property(self, property);
    }

    fn visit_string_literal(&mut self, literal: &StringLiteral<'a>) {
        if self.in_setup_value > 0 {
            self.entries.setup.push(literal.value.to_string());
        }
    }

    fn visit_template_literal(&mut self, template: &TemplateLiteral<'a>) {
        if self.in_setup_value > 0
            && let Some(quasi) = template.single_quasi()
        {
            self.entries.setup.push(quasi.to_string());
        }
        oxc_ast_visit::walk::walk_template_literal(self, template);
    }
}

fn literal_text(expression: &Expression<'_>) -> Option<String> {
    match expression.get_inner_expression() {
        Expression::StringLiteral(literal) => Some(literal.value.to_string()),
        Expression::TemplateLiteral(template) => {
            template.single_quasi().map(|quasi| quasi.to_string())
        }
        _ => None,
    }
}

/// Setup entries and root literals of a JSON config value, at any depth.
fn json_entries(value: &serde_json::Value, out: &mut ConfigEntries) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if SETUP_KEYS.contains(&key.as_str()) {
                    json_strings(value, &mut out.setup);
                } else if ROOT_KEYS.contains(&key.as_str()) {
                    if let Some(literal) = value.as_str() {
                        out.roots.push(literal.to_string());
                    }
                } else {
                    json_entries(value, out);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                json_entries(item, out);
            }
        }
        _ => {}
    }
}

fn json_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                json_strings(item, out);
            }
        }
        _ => {}
    }
}

/// The workspace file a module path names: itself, with a module extension,
/// or its `index` module. A source the run already read wins; otherwise the
/// file must exist on disk.
fn resolve_module_file(
    root: &Path,
    sources: &HashMap<String, &String>,
    path: &str,
) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    let mut candidates = vec![path.to_string()];
    // `./setup.js` in a TypeScript project names `setup.ts`.
    let stems = [
        Some(path),
        path.rsplit_once('.')
            .filter(|(_, extension)| MODULE_EXTENSIONS.contains(extension))
            .map(|(stem, _)| stem),
    ];
    for stem in stems.into_iter().flatten() {
        for extension in MODULE_EXTENSIONS {
            candidates.push(format!("{stem}.{extension}"));
        }
    }
    for extension in MODULE_EXTENSIONS {
        candidates.push(format!("{path}/index.{extension}"));
    }
    candidates
        .into_iter()
        .find(|candidate| sources.contains_key(candidate) || root.join(candidate).is_file())
}

/// The directory part of a normalized workspace-relative path (`""` at the
/// root).
fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Whether the normalized `file` sits under `dir` (`""` holds every file).
fn dir_contains(dir: &str, file: &str) -> bool {
    dir.is_empty()
        || file
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// `dir/relative` with `.` and `..` folded. `None` when the path leaves the
/// workspace.
fn join_normalized(dir: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<String> = dir
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    for part in relative.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part.to_string()),
        }
    }
    Some(parts.join("/"))
}

/// The relative specifier that names workspace path `target` from a file in
/// `from_dir`.
fn relative_specifier(from_dir: &str, target: &str) -> String {
    let from: Vec<&str> = from_dir
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').filter(|part| !part.is_empty()).collect();
    let common = from
        .iter()
        .zip(to.iter())
        .take_while(|(left, right)| left == right)
        .count();
    let mut out = String::new();
    if from.len() == common {
        out.push_str("./");
    }
    for _ in common..from.len() {
        out.push_str("../");
    }
    out.push_str(&to[common..].join("/"));
    out
}
