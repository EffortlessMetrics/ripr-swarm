//! Diff-reach admission for Python workspaces past the walk cap (#10785).
//!
//! The walk cap keeps a bounded set of context files around the diff. It used
//! to keep the earliest paths, so on a workspace larger than the cap the kept
//! files had little to do with the diff: on home-assistant (about 20,000
//! files) a run kept `homeassistant/` and dropped the `tests/` that exercise a
//! changed integration, finding 3 of the 6 related tests an uncapped run finds.
//!
//! The adapter relates a test to a changed owner only when the test's own text
//! names it (RIPR-SPEC-0028): the owner's name, a method name, a module-level
//! name, a route segment, a pytest fixture whose body names one, or, for a
//! module-level or `__dunder__` change, an import of the module or a package
//! above it. Fixtures come from the test's module and its `conftest.py` chain,
//! and helper callers from the owner's own module. So a file reaches the diff
//! when its text names something a changed file defines, or a fixture built
//! over it. What else the
//! analysis reads around those files sits in the modules a reaching test or a
//! changed file imports (assertion helpers, base classes) and in the
//! `conftest.py` and package `__init__.py` files. This module admits that set
//! first and only then counts what the cap refuses.
//!
//! Every uncertain step answers "may reach": an unreadable or oversized file,
//! a changed route whose literal has no fixed word to look for, or a fixture
//! closure that does not settle. A wrong "cannot
//! reach" would hide a related test behind a complete result; a wrong "may
//! reach" only spends cap on a file that did not need it, or keeps the named
//! `diff_scope_oversized` disclosure.

use super::bounded_read::{ScanSource, scan_source_for};
use super::source_utils::{is_test_file, normalized_path};
use crate::analysis::diff::ChangedFile;
use rayon::prelude::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Discovered-file count past which the scan is not attempted: the path list
/// is held in memory, so a pathological workspace keeps the plain cap.
pub(super) const MAX_SCANNED_FILES: usize = 200_000;
/// Fixture-closure rounds before the scan gives up and keeps the plain cap.
const MAX_FIXTURE_ROUNDS: usize = 6;

/// Files kept for one over-cap diff run, and what the cap still refused.
#[derive(Debug)]
pub(super) struct ReachAdmission {
    /// Files to read and analyze, in path order.
    pub(super) kept: Vec<PathBuf>,
    /// Every discovered file, in path order.
    pub(super) discovered: Vec<PathBuf>,
    /// Files that may reach the diff (or that a reaching file imports), kept
    /// or not, excluding the changed files.
    pub(super) reaching: usize,
    /// Refused files that may reach the diff. Non-zero keeps the
    /// `diff_scope_oversized` disclosure.
    pub(super) refused_reaching: usize,
    /// Refused files whose text names nothing a changed file defines.
    pub(super) refused_unreached: usize,
}

/// Admit, beyond the changed files and up to `max_files`: every
/// `conftest.py` and package `__init__.py` above a changed file, every file
/// that names what a changed file defines, and the workspace modules those
/// files import. Returns `None` when the scan cannot be attempted (too many
/// files, or a changed file it cannot read), so the caller keeps the plain cap.
pub(super) fn admit_by_reach(
    root: &Path,
    mut discovered: Vec<PathBuf>,
    changed_files: &[ChangedFile],
    changed_paths: &BTreeSet<String>,
    max_files: usize,
    file_limit: u64,
) -> Option<ReachAdmission> {
    if discovered.len() > MAX_SCANNED_FILES {
        return None;
    }
    discovered.sort();
    let sources: Vec<ScanSource> = discovered
        .iter()
        .map(|file| scan_source_for(root, file, file_limit))
        .collect();
    let normalized: Vec<String> = discovered
        .iter()
        .map(|file| normalized_path(file))
        .collect();
    let is_changed: Vec<bool> = normalized
        .iter()
        .map(|file| changed_paths.contains(file))
        .collect();

    let mut needles = HashSet::new();
    let mut modules: Vec<ModuleNeedle> = Vec::new();
    for changed in changed_files {
        let changed_path = normalized_path(&changed.path);
        if !changed_paths.contains(&changed_path)
            || changed
                .path
                .extension()
                .is_none_or(|extension| extension != "py")
        {
            continue;
        }
        seed_lines(
            changed.removed_lines.iter().map(|line| line.text.as_str()),
            &mut needles,
        )?;
        let source = match normalized.iter().position(|file| *file == changed_path) {
            Some(index) => Some(sources[index].read(file_limit)?.ok()?),
            None => None,
        };
        seed_lines(
            source.iter().flat_map(|source| source.lines()),
            &mut needles,
        )?;
        // A module-level change has the module itself as an owner, which a
        // test relates to through an import of the module, so the module is
        // searched for on import lines.
        if changes_dunder(source.as_deref(), changed) {
            modules.push(ModuleNeedle::with_ancestors(&changed.path)?);
        } else if changes_module_level(source.as_deref(), changed)
            && let Some(module) = ModuleNeedle::of(&changed.path)
        {
            modules.push(module);
        }
    }
    // Whether each file reaches. A test can also reach a changed owner only
    // through a pytest fixture it names (`fixture_owner_call`), so the
    // fixtures a reaching test module or `conftest.py` defines over the diff's
    // names join the needles and the unreached files are scanned again, up to
    // a fixed number of rounds; a closure that does not settle keeps the
    // plain cap.
    let token = crate::analysis::cancellation::current_token();
    let mut reaching = vec![false; discovered.len()];
    let mut settled = false;
    // A cancelled run stops scanning; the caller's own checkpoints then
    // abort the analysis.
    let cancelled = || {
        token
            .as_ref()
            .is_some_and(|token| token.checkpoint().is_err())
    };
    for _ in 0..MAX_FIXTURE_ROUNDS {
        if cancelled() {
            return None;
        }
        let found: Vec<(usize, Vec<String>)> = (0..discovered.len())
            .into_par_iter()
            .filter_map(|index| {
                // Test modules and conftests host fixtures. They are rescanned
                // every round, even once reaching, because a fixture can build
                // on another fixture that only joins the needles in a later
                // round.
                let is_fixture_host = is_test_file(&discovered[index])
                    || discovered[index]
                        .file_name()
                        .is_some_and(|name| name == "conftest.py");
                let already_reaching = reaching[index];
                if already_reaching && !is_fixture_host {
                    return None;
                }
                crate::analysis::cancellation::with_optional_token(token.as_ref(), || {
                    let Ok(source) = sources[index].read(file_limit)? else {
                        if already_reaching {
                            return None;
                        }
                        // Unreadable here is unreadable for the analysis too;
                        // admitting it lets the read path name the failure.
                        return Some((index, Vec::new()));
                    };
                    let reaches = already_reaching
                        || names_any(&source, &needles)
                        || modules
                            .iter()
                            .any(|module| module.imported_by(&discovered[index], &source));
                    if !reaches {
                        return None;
                    }
                    let fixtures = if is_fixture_host {
                        fixtures_over(&source, &needles)
                    } else {
                        Vec::new()
                    };
                    Some((index, fixtures))
                })
            })
            .collect();
        let mut grew = false;
        for (index, fixtures) in found {
            reaching[index] = true;
            for fixture in fixtures {
                grew |= needles.insert(fixture);
            }
        }
        if !grew {
            settled = true;
            break;
        }
    }
    if !settled || cancelled() {
        return None;
    }
    // A conftest shapes the tests below it (fixtures, autouse setup, and an
    // unread conftest withholds a clean no-assertion state), so the ones on
    // the directory chain of a changed file or a reaching test are kept
    // first. A conftest elsewhere matters only through a fixture over the
    // diff's names, which makes it reaching above.
    let mut chain_dirs: HashSet<String> = HashSet::new();
    for (index, file) in normalized.iter().enumerate() {
        if is_changed[index] || (reaching[index] && is_test_file(&discovered[index])) {
            let mut dir = file.as_str();
            while let Some((parent, _)) = dir.rsplit_once('/') {
                chain_dirs.insert(parent.to_string());
                dir = parent;
            }
            chain_dirs.insert(String::new());
        }
    }
    let structural: Vec<bool> = discovered
        .iter()
        .enumerate()
        .map(|(index, file)| {
            is_changed[index]
                || (file.file_name().is_some_and(|name| name == "conftest.py")
                    && chain_dirs.contains(&normalized_dir(&normalized[index])))
                || is_package_init_above_changed(file, changed_paths)
        })
        .collect();
    // The workspace modules a changed file, conftest, package init or
    // reaching test imports: a test's assertion helpers and a changed class's
    // bases. A reaching production module is read for its own names, not for
    // what it imports.
    let scans: Vec<(bool, Vec<ImportTarget>)> = (0..discovered.len())
        .into_par_iter()
        .map(|index| {
            let wants_imports =
                structural[index] || (reaching[index] && is_test_file(&discovered[index]));
            let imports = if wants_imports {
                crate::analysis::cancellation::with_optional_token(token.as_ref(), || match sources
                    [index]
                    .read(file_limit)
                {
                    Some(Ok(source)) => import_targets(&discovered[index], &source),
                    _ => Vec::new(),
                })
            } else {
                Vec::new()
            };
            (reaching[index], imports)
        })
        .collect();
    let by_suffix = suffix_index(&normalized);
    let by_path: HashMap<&str, usize> = normalized
        .iter()
        .enumerate()
        .map(|(index, file)| (file.as_str(), index))
        .collect();
    let mut imported = vec![false; discovered.len()];
    for (_, targets) in &scans {
        for target in targets {
            let found: &[usize] = match target {
                ImportTarget::Exact(path) => by_path
                    .get(path.as_str())
                    .map(std::slice::from_ref)
                    .unwrap_or_default(),
                ImportTarget::Suffix(suffix) => by_suffix
                    .get(suffix.as_str())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
            };
            for index in found {
                imported[*index] = true;
            }
        }
    }

    // When the reach set itself exceeds the cap, what is kept decides which
    // related tests the run finds: conftests and package inits, then reaching
    // files near a changed file, then reaching tests, then the modules tests
    // and changed files import, then reaching production modules.
    let near = near_components(changed_paths);
    let mut ranked: Vec<(u8, usize)> = (0..discovered.len())
        .filter(|index| !is_changed[*index])
        .filter_map(|index| {
            let reaches = scans[index].0;
            let tier = if structural[index] {
                0
            } else if reaches && is_near(&discovered[index], &near) {
                1
            } else if reaches && is_test_file(&discovered[index]) {
                2
            } else if imported[index] {
                3
            } else if reaches {
                4
            } else {
                return None;
            };
            Some((tier, index))
        })
        .collect();
    ranked.sort();
    let context_slots =
        max_files.saturating_sub(is_changed.iter().filter(|changed| **changed).count());
    let mut keep = is_changed.clone();
    for (_, index) in ranked.iter().take(context_slots) {
        keep[*index] = true;
    }
    let refused_reaching = ranked.len().saturating_sub(context_slots);
    let kept: Vec<PathBuf> = discovered
        .iter()
        .zip(&keep)
        .filter(|(_, keep)| **keep)
        .map(|(file, _)| file.clone())
        .collect();
    let refused_unreached = discovered.len() - kept.len() - refused_reaching;
    Some(ReachAdmission {
        kept,
        reaching: ranked.len(),
        refused_reaching,
        refused_unreached,
        discovered,
    })
}

/// The directory part of a normalized workspace path, empty at the root.
fn normalized_dir(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(String::new, |(dir, _)| dir.to_string())
}

/// A workspace file an import statement may load.
enum ImportTarget {
    /// A relative import, resolved against the importing file's directory.
    Exact(String),
    /// An absolute import, matched against any file whose path ends with it:
    /// the import root (`src/`, a test directory on `sys.path`) is unknown.
    Suffix(String),
}

/// The workspace modules `file`'s import statements may load: each imported
/// module, and for `from M import x` both `M` and the submodule `M.x`.
fn import_targets(file: &Path, source: &str) -> Vec<ImportTarget> {
    let dir = file.parent().map(normalized_path).unwrap_or_default();
    let mut targets = Vec::new();
    for statement in import_statements(source) {
        let (modules, names): (Vec<String>, Vec<&str>) =
            if let Some(rest) = statement.strip_prefix("from ") {
                let (from, imported) = rest.split_once(" import").unwrap_or((rest, ""));
                // `x as y` imports `x`; `y` is only a local name.
                let words: Vec<&str> = identifier_words(imported).collect();
                let names = words
                    .iter()
                    .enumerate()
                    .filter(|(at, word)| **word != "as" && (*at == 0 || words[at - 1] != "as"))
                    .map(|(_, word)| *word)
                    .collect();
                (vec![from.trim().to_string()], names)
            } else {
                let rest = statement.strip_prefix("import ").unwrap_or_default();
                let modules = rest
                    .split(',')
                    .filter_map(|part| part.split_whitespace().next())
                    .map(str::to_string)
                    .collect();
                (modules, Vec::new())
            };
        for module in modules {
            let dots = module.chars().take_while(|ch| *ch == '.').count();
            let dotted = &module[dots..];
            let mut bases: Vec<String> = Vec::new();
            let base = dotted.replace('.', "/");
            bases.push(base.clone());
            bases.extend(names.iter().map(|name| {
                if base.is_empty() {
                    (*name).to_string()
                } else {
                    format!("{base}/{name}")
                }
            }));
            if dots == 0 {
                for base in bases.into_iter().filter(|base| !base.is_empty()) {
                    targets.push(ImportTarget::Suffix(format!("{base}.py")));
                    targets.push(ImportTarget::Suffix(format!("{base}/__init__.py")));
                }
                continue;
            }
            let Some(anchor) = ancestor(&dir, dots - 1) else {
                continue;
            };
            for base in bases {
                let joined = [anchor.as_str(), base.as_str()]
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("/");
                if joined.is_empty() {
                    continue;
                }
                targets.push(ImportTarget::Exact(format!("{joined}.py")));
                targets.push(ImportTarget::Exact(format!("{joined}/__init__.py")));
            }
        }
    }
    targets
}

/// `dir` with `levels` trailing components removed, or `None` past the root.
fn ancestor(dir: &str, levels: usize) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    if levels > parts.len() {
        return None;
    }
    parts.truncate(parts.len() - levels);
    Some(parts.join("/"))
}

/// Every component-aligned path suffix of every file (`a/b/c.py`, `b/c.py`,
/// `c.py`), for resolving absolute imports under an unknown import root.
fn suffix_index(files: &[String]) -> HashMap<String, Vec<usize>> {
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for (at, file) in files.iter().enumerate() {
        let mut rest = file.as_str();
        loop {
            index.entry(rest.to_string()).or_default().push(at);
            let Some((_, tail)) = rest.split_once('/') else {
                break;
            };
            rest = tail;
        }
    }
    index
}

/// Whether a changed line is module-level code: outside every top-level
/// `def` / `class` block. Imports count, since a re-pointed import is module
/// behavior. An added line is placed by its block in the new source; a
/// removed line by its own column-0 text, else by its new-side position or
/// the line above it (a deletion at the end of a module-level block lands on
/// the `def` that follows). Blank and comment-only lines change nothing.
fn changes_module_level(source: Option<&str>, changed: &ChangedFile) -> bool {
    let kinds = block_kinds(source.unwrap_or_default());
    let module_level = |line: usize| {
        kinds
            .get(line.wrapping_sub(1))
            .copied()
            .unwrap_or(BlockKind::Other)
            != BlockKind::Definition
    };
    let code = |text: &str| {
        let trimmed = text.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    };
    changed
        .added_lines
        .iter()
        .any(|line| code(&line.text) && module_level(line.line))
        || changed.removed_lines.iter().any(|line| {
            let trimmed = line.text.trim_start();
            if !code(trimmed) {
                return false;
            }
            if trimmed.len() == line.text.len() {
                return !trimmed.starts_with('@') && defined_name(trimmed).is_none();
            }
            module_level(line.new_side_line) || module_level(line.new_side_line.wrapping_sub(1))
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockKind {
    Definition,
    Import,
    Other,
}

/// For each line of `source`, the kind of top-level block it belongs to.
fn block_kinds(source: &str) -> Vec<BlockKind> {
    let mut kind = BlockKind::Other;
    source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let starts_block = trimmed.len() == line.len()
                && !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && !trimmed.starts_with([')', ']', '}']);
            if starts_block {
                kind = if trimmed.starts_with('@') || defined_name(trimmed).is_some() {
                    BlockKind::Definition
                } else if is_import_statement(trimmed) {
                    BlockKind::Import
                } else {
                    BlockKind::Other
                };
            }
            kind
        })
        .collect()
}

fn is_import_statement(trimmed: &str) -> bool {
    ["import", "from"].iter().any(|keyword| {
        trimmed.strip_prefix(keyword).is_some_and(|rest| {
            rest.starts_with(|ch: char| ch.is_whitespace() || ch == '.' || ch == '\\')
        })
    })
}

/// One import statement in a single spelling: any run of whitespace or a
/// backslash continuation becomes one space, parentheses and commas stand
/// apart, and `from.x` becomes `from .x`, so `from pytest\timport(fixture
/// as fx)` reads `from pytest import ( fixture as fx )`. Python accepts
/// all of these spellings, so every parser here must see them.
fn normalize_import(statement: &str) -> String {
    let mut spaced = String::with_capacity(statement.len() + 8);
    for ch in statement.chars() {
        match ch {
            '(' | ')' | ',' => {
                spaced.push(' ');
                spaced.push(ch);
                spaced.push(' ');
            }
            '\\' => spaced.push(' '),
            _ => spaced.push(ch),
        }
    }
    let joined = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    match joined.strip_prefix("from.") {
        Some(rest) => format!("from .{rest}"),
        None => joined,
    }
}

/// A changed module, searched for on import lines.
struct ModuleNeedle {
    /// The names the module is imported by: its own name, or for a dunder
    /// change every package above it too.
    names: Vec<String>,
    /// The name of the package holding it, when it has one and only the
    /// module itself is searched for.
    package: Option<String>,
    /// The directory a relative import must be under to reach it.
    dir: PathBuf,
}

impl ModuleNeedle {
    fn of(path: &Path) -> Option<Self> {
        let name = module_name(path)?;
        let module_dir = path.parent().unwrap_or(Path::new(""));
        let package_dir = if path.file_stem().is_some_and(|stem| stem == "__init__") {
            module_dir.parent()
        } else {
            Some(module_dir)
        };
        let package = package_dir
            .and_then(|dir| dir.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string);
        Some(Self {
            names: vec![name],
            package,
            dir: module_dir.to_path_buf(),
        })
    }

    /// A changed dunder method is dispatched by operators, and the adapter
    /// lets any import from a package above the owner reach its class
    /// (`test_may_reach_owner_class`), so every enclosing package name is
    /// searched for, and a relative import anywhere under the top package.
    fn with_ancestors(path: &Path) -> Option<Self> {
        let mut names = vec![module_name(path)?];
        let dir = path.parent().unwrap_or(Path::new(""));
        names.extend(
            dir.components()
                .filter_map(|component| component.as_os_str().to_str())
                .map(str::to_string),
        );
        let top = dir
            .components()
            .next()
            .map_or_else(PathBuf::new, |first| PathBuf::from(first.as_os_str()));
        Some(Self {
            names,
            package: None,
            dir: top,
        })
    }

    /// Whether an import statement in `file` may bind this module or import
    /// from it. The adapter matches an imported name by its last segment alone
    /// (`from anything import coordinator`), and a `from` source by its full
    /// dotted path under some import root: that path names the package, or
    /// starts with the module itself when its directory is an import root
    /// (`src/shop/__init__.py` as `from shop import x`), unless the import is
    /// relative from inside the module's own directory.
    fn imported_by(&self, file: &Path, source: &str) -> bool {
        let inside = file.starts_with(&self.dir);
        // Splitting statements reads the whole file; a file that holds none of
        // the module's names and cannot import it relatively is skipped first.
        if !inside && !self.names.iter().any(|name| source.contains(name.as_str())) {
            return false;
        }
        let named = |word: &str| self.names.iter().any(|name| name == word);
        import_statements(source).any(|statement| {
            let Some(rest) = statement.strip_prefix("from ") else {
                return identifier_words(&statement).any(named);
            };
            if inside && rest.trim_start().starts_with('.') {
                return true;
            }
            let (from, imported) = rest.split_once(" import").unwrap_or((rest, ""));
            if identifier_words(imported).any(named) {
                return true;
            }
            let Some(package) = &self.package else {
                return identifier_words(from).any(named);
            };
            identifier_words(from).any(named)
                && (identifier_words(from).any(|word| word == package)
                    || (!from.trim_start().starts_with('.')
                        && identifier_words(from).next().is_some_and(named)))
        })
    }
}

/// Whether a changed line sits in the body of a `__dunder__` method.
fn changes_dunder(source: Option<&str>, changed: &ChangedFile) -> bool {
    let enclosing = enclosing_defs(source.unwrap_or_default());
    let in_dunder = |line: usize| {
        enclosing
            .get(line.wrapping_sub(1))
            .is_some_and(|name| name.as_deref().is_some_and(is_dunder))
    };
    changed.added_lines.iter().any(|line| in_dunder(line.line))
        || changed.removed_lines.iter().any(|line| {
            in_dunder(line.new_side_line) || in_dunder(line.new_side_line.wrapping_sub(1))
        })
}

/// For each line of `source`, the innermost `def` whose block holds it.
fn enclosing_defs(source: &str) -> Vec<Option<String>> {
    let mut stack: Vec<(usize, String)> = Vec::new();
    source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let indent = line.len() - trimmed.len();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                while stack.last().is_some_and(|(depth, _)| *depth >= indent) {
                    stack.pop();
                }
                if let Some(name) = defined_name(trimmed) {
                    let is_def = trimmed
                        .strip_prefix("async ")
                        .unwrap_or(trimmed)
                        .trim_start()
                        .starts_with("def ");
                    if is_def {
                        stack.push((indent, name.to_string()));
                    } else {
                        stack.push((indent, String::new()));
                    }
                    return stack
                        .last()
                        .map(|(_, name)| name.clone())
                        .filter(|name| !name.is_empty());
                }
            }
            stack
                .iter()
                .rev()
                .find(|(_, name)| !name.is_empty())
                .map(|(_, name)| name.clone())
        })
        .collect()
}

/// Import statements of a module, each in [`normalize_import`]'s spelling,
/// read from its [`simple_statements`] so a statement after `;`, inside a
/// compound statement's one-line body, or after a string holding `#` is
/// found like any other.
fn import_statements(source: &str) -> impl Iterator<Item = String> + '_ {
    simple_statements(source)
        .into_iter()
        .filter(|statement| is_import_statement(statement))
        .map(|statement| normalize_import(&statement))
}

/// Keywords whose header ends at a top-level `:` and may carry a one-line
/// body (`try: import x`, `if TYPE_CHECKING: from y import z`).
const COMPOUND_HEADERS: [&str; 14] = [
    "if", "elif", "else", "try", "except", "finally", "with", "for", "while", "def", "class",
    "async", "match", "case",
];

/// The simple statements of a Python module, split the way the tokenizer
/// does: at a newline outside brackets and strings, at `;`, and after a
/// compound statement's header colon. Comments are dropped, a string
/// literal becomes `""`, and a backslash continuation joins its lines.
fn simple_statements(source: &str) -> Vec<String> {
    // Python reads `\r\n` and a lone `\r` as a newline, so a backslash
    // continuation before either joins lines too.
    let source = if source.contains('\r') {
        std::borrow::Cow::Owned(source.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        std::borrow::Cow::Borrowed(source)
    };
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut chars = source.chars().peekable();
    let finish = |current: &mut String, statements: &mut Vec<String>| {
        let statement = current.trim();
        if !statement.is_empty() {
            statements.push(statement.to_string());
        }
        current.clear();
    };
    while let Some(ch) = chars.next() {
        match ch {
            '#' => {
                while chars.peek().is_some_and(|next| *next != '\n') {
                    chars.next();
                }
            }
            '\'' | '"' => {
                let triple = chars.peek() == Some(&ch) && {
                    let mut ahead = chars.clone();
                    ahead.next();
                    ahead.peek() == Some(&ch)
                };
                if triple {
                    chars.next();
                    chars.next();
                }
                let mut run = 0;
                while let Some(inner) = chars.next() {
                    if inner == '\\' {
                        chars.next();
                        run = 0;
                    } else if inner == ch {
                        run += 1;
                        if !triple || run == 3 {
                            break;
                        }
                    } else if inner == '\n' && !triple {
                        break;
                    } else {
                        run = 0;
                    }
                }
                current.push_str("\"\"");
            }
            '\\' if chars.peek() == Some(&'\n') => {
                chars.next();
                current.push(' ');
            }
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            '\n' if depth == 0 => finish(&mut current, &mut statements),
            ';' if depth == 0 => finish(&mut current, &mut statements),
            ':' if depth == 0
                && identifier_words(&current)
                    .next()
                    .is_some_and(|word| COMPOUND_HEADERS.contains(&word))
                && current
                    .trim_start()
                    .starts_with(|c: char| c.is_ascii_alphabetic()) =>
            {
                current.push(ch);
                finish(&mut current, &mut statements);
            }
            _ => current.push(if ch == '\n' { ' ' } else { ch }),
        }
    }
    finish(&mut current, &mut statements);
    statements
}

/// Local names `source` binds to a needle with `import ... as`: the `b` of
/// `from m import a as b` or `import pkg.a as b` when `a` is a needle.
fn imported_aliases(source: &str, needles: &HashSet<String>) -> Vec<String> {
    let mut local = Vec::new();
    for statement in import_statements(source) {
        let imported = match statement.strip_prefix("from ") {
            Some(rest) => rest
                .split_once(" import ")
                .map_or("", |(_, imported)| imported),
            None => statement.strip_prefix("import ").unwrap_or_default(),
        };
        for item in imported
            .trim_matches(|c: char| c == '(' || c == ')' || c.is_whitespace())
            .split(',')
        {
            let mut words = item.split_whitespace();
            if let (Some(name), Some("as"), Some(alias)) =
                (words.next(), words.next(), words.next())
                && name
                    .rsplit('.')
                    .next()
                    .is_some_and(|last| needles.contains(last))
            {
                local.push(alias.trim_end_matches(')').to_string());
            }
        }
    }
    local
}

/// Local names bound to pytest's fixture decorator by
/// `from pytest import fixture as fx` (also `pytest_asyncio`). A later
/// rebinding is not tracked: reading a shadowed alias as a fixture only
/// admits more files.
fn fixture_aliases(source: &str) -> Vec<String> {
    let mut aliases = Vec::new();
    for statement in import_statements(source) {
        let Some(rest) = statement.strip_prefix("from ") else {
            continue;
        };
        let Some((module, imported)) = rest.split_once(" import ") else {
            continue;
        };
        if !matches!(module.trim(), "pytest" | "pytest_asyncio") {
            continue;
        }
        for item in imported
            .trim_matches(|c: char| c == '(' || c == ')' || c.is_whitespace())
            .split(',')
        {
            let mut words = item.split_whitespace();
            if let (Some("fixture"), Some("as"), Some(alias)) =
                (words.next(), words.next(), words.next())
            {
                aliases.push(alias.trim_end_matches(')').to_string());
            }
        }
    }
    aliases
}

/// The name other files import a module by: its stem, or its package's name
/// for an `__init__.py`.
fn module_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if stem == "__init__" {
        return path.parent()?.file_name()?.to_str().map(str::to_string);
    }
    Some(stem.to_string())
}

/// Record what the lines of a changed file define: every `def` / `class`
/// name, top-level assignment targets, and the words of decorator string
/// literals (route paths). Returns `None` for a route literal with no word
/// in it (`@app.get("/")`): a test reaching that route by its path names
/// nothing the scan could look for.
fn seed_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    needles: &mut HashSet<String>,
) -> Option<()> {
    let mut in_decorator = false;
    for line in lines {
        let trimmed = line.trim_start();
        let name = defined_name(trimmed);
        if let Some(name) = name
            && !is_dunder(name)
        {
            needles.insert(name.to_string());
        }
        if trimmed.len() == line.len() {
            needles.extend(assignment_targets(line).map(str::to_string));
            // `RATE: dict[str, int] = {}`: the annotation's brackets stop the
            // assignment scan, so take the annotated name directly.
            if let Some((target, _)) = line.split_once(':')
                && starts_identifier(target.trim_end())
                && target.trim_end().chars().all(is_identifier_char)
                && !is_keyword(target.trim_end())
                && !matches!(target.trim_end(), "else" | "try" | "finally")
            {
                needles.insert(target.trim_end().to_string());
            }
        }
        if trimmed.starts_with('@') {
            in_decorator = true;
        } else if name.is_some() {
            in_decorator = false;
        }
        if in_decorator {
            for literal in string_literals(trimmed) {
                // A `{id}` / `<id>` segment matches any value, so a route of
                // placeholders alone (`/{item_id}`) names nothing a test's
                // request path must spell.
                let fixed = without_placeholders(literal);
                if literal.contains('/') && identifier_words(&fixed).next().is_none() {
                    return None;
                }
                needles.extend(identifier_words(literal).map(str::to_string));
            }
        }
    }
    Some(())
}

/// The name a `def`, `async def` or `class` statement binds.
fn defined_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed
        .strip_prefix("async ")
        .unwrap_or(trimmed)
        .trim_start();
    let rest = rest
        .strip_prefix("def ")
        .or_else(|| rest.strip_prefix("class "))?
        .trim_start();
    let end = rest
        .find(|ch: char| !is_identifier_char(ch))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// Identifier targets of a top-level assignment line (`A = B = 1`,
/// `A, B = pair`, `A: int = 1`); empty for comparisons and keyword lines.
fn assignment_targets(line: &str) -> impl Iterator<Item = &str> {
    let mut targets = Vec::new();
    let bytes = line.as_bytes();
    let mut start = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(byte, b'"' | b'\'' | b'#' | b'(' | b'[' | b'{') {
            break;
        }
        if *byte != b'=' {
            continue;
        }
        let previous = index.checked_sub(1).map(|at| bytes[at]);
        let next = bytes.get(index + 1).copied();
        if next == Some(b'=') || matches!(previous, Some(b'=' | b'!' | b'<' | b'>' | b':')) {
            break;
        }
        let target = &line[start..index];
        let target = target.split(':').next().unwrap_or_default();
        targets.extend(
            identifier_words(target).filter(|word| !is_keyword(word) && starts_identifier(word)),
        );
        start = index + 1;
    }
    targets.into_iter()
}

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "elif"
            | "while"
            | "for"
            | "return"
            | "assert"
            | "del"
            | "global"
            | "nonlocal"
            | "lambda"
            | "with"
            | "yield"
            | "not"
            | "and"
            | "or"
            | "in"
            | "is"
            | "print"
            | "type"
    )
}

/// Names of the pytest fixtures in `source` whose decorator, signature or
/// body names a needle: a test that takes one as a parameter reaches what it
/// calls. A `name=` override in the decorator is kept too (its words are).
fn fixtures_over(source: &str, needles: &HashSet<String>) -> Vec<String> {
    let source = if source.contains('\r') {
        std::borrow::Cow::Owned(source.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        std::borrow::Cow::Borrowed(source)
    };
    let lines: Vec<&str> = source.lines().collect();
    let continued = continuation_lines(&lines);
    let aliases = fixture_aliases(&source);
    // A fixture body calls an owner by the local name this file imports it
    // as (`from orders import make_order as build`).
    // The aliases are few, so they are checked beside the shared needles
    // instead of merged into a per-file copy: on a suite of thousands of
    // fixture hosts, that copy cost more than the scan itself.
    let local = imported_aliases(&source, needles);
    let names_needle = |line: &str| {
        identifier_words(line)
            .any(|word| needles.contains(word) || local.iter().any(|alias| alias == word))
    };
    let mut fixtures = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let trimmed = lines[at].trim_start();
        let is_fixture = trimmed.strip_prefix('@').is_some_and(|decorator| {
            decorator.contains("fixture")
                || identifier_words(decorator)
                    .next()
                    .is_some_and(|head| aliases.iter().any(|alias| alias == head))
        });
        if !is_fixture {
            at += 1;
            continue;
        }
        let start = at;
        // Skip the rest of the decorator stack, including a decorator's
        // bracketed arguments, to the `def`.
        while at < lines.len() && (continued[at] || defined_name(lines[at].trim_start()).is_none())
        {
            at += 1;
        }
        let Some(header) = lines.get(at) else {
            break;
        };
        let indent = header.len() - header.trim_start().len();
        let Some(name) = defined_name(header.trim_start()) else {
            break;
        };
        // A line that continues a bracket, string or backslash belongs to
        // the logical line before it: the rest of a multi-line header
        // (`def order(\n):`) or a column-0 line of a body docstring never ends
        // the body.
        let mut end = at + 1;
        while end < lines.len() {
            let line = lines[end];
            let body = line.trim_start();
            if !continued[end]
                && !body.is_empty()
                && !body.starts_with('#')
                && line.len() - body.len() <= indent
            {
                break;
            }
            end += 1;
        }
        let block = &lines[start..end];
        if block.iter().any(|line| names_needle(line)) {
            fixtures.push(name.to_string());
            for line in &lines[start..at] {
                for literal in string_literals(line.trim_start()) {
                    fixtures.extend(identifier_words(literal).map(str::to_string));
                }
            }
        }
        at = end;
    }
    fixtures
}

/// For each physical line, whether it continues the logical line before it:
/// it starts inside an open bracket or triple-quoted string, or the line
/// before ended with a backslash.
fn continuation_lines(lines: &[&str]) -> Vec<bool> {
    let mut continued = Vec::with_capacity(lines.len());
    let mut depth = 0usize;
    // The quote character and width of a string still open at line end.
    let mut open: Option<(char, bool)> = None;
    let mut joined = false;
    for line in lines {
        continued.push(depth > 0 || open.is_some() || joined);
        joined = false;
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if let Some((quote, triple)) = open {
                if ch == '\\' {
                    joined = chars.next().is_none();
                } else if ch == quote
                    && (!triple || {
                        let mut ahead = chars.clone();
                        ahead.next() == Some(quote) && ahead.next() == Some(quote)
                    })
                {
                    if triple {
                        chars.next();
                        chars.next();
                    }
                    open = None;
                }
                continue;
            }
            match ch {
                '#' => break,
                '\'' | '"' => {
                    let mut ahead = chars.clone();
                    let triple = ahead.next() == Some(ch) && ahead.next() == Some(ch);
                    if triple {
                        chars.next();
                        chars.next();
                    }
                    open = Some((ch, triple));
                }
                '\\' if chars.peek().is_none() => joined = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        // A single-quoted string cannot cross a newline unescaped.
        if open.is_some_and(|(_, triple)| !triple) && !joined {
            open = None;
        }
    }
    continued
}

/// A route literal with its `{...}` and `<...>` placeholder segments removed.
fn without_placeholders(literal: &str) -> String {
    let mut fixed = String::with_capacity(literal.len());
    let mut depth = 0usize;
    for ch in literal.chars() {
        match ch {
            '{' | '<' => depth += 1,
            '}' | '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => fixed.push(ch),
            _ => {}
        }
    }
    fixed
}

/// Contents of the quoted string literals on one line.
fn string_literals(line: &str) -> Vec<&str> {
    let mut literals = Vec::new();
    let mut rest = line;
    while let Some(open) = rest.find(['"', '\'']) {
        let quote = rest.as_bytes()[open];
        let after = &rest[open + 1..];
        let Some(close) = after.find(char::from(quote)) else {
            break;
        };
        literals.push(&after[..close]);
        rest = &after[close + 1..];
    }
    literals
}

/// Whether `source` contains any needle as a whole identifier word. Words
/// inside strings and comments count: `patch("pkg.mod.owner")` and route
/// literals name owners from inside a string.
fn names_any(source: &str, needles: &HashSet<String>) -> bool {
    identifier_words(source).any(|word| needles.contains(word))
}

fn identifier_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|ch: char| !is_identifier_char(ch))
        .filter(|word| !word.is_empty())
}

/// Python identifier characters, with every non-ASCII character treated as
/// one so a Unicode identifier stays a single word.
fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || !ch.is_ascii()
}

fn starts_identifier(word: &str) -> bool {
    word.chars().next().is_some_and(|ch| !ch.is_ascii_digit())
}

fn is_dunder(name: &str) -> bool {
    name.len() > 4 && name.starts_with("__") && name.ends_with("__")
}

/// Whether `file` is a package `__init__.py` whose directory holds a changed
/// file: package re-exports and module paths read these.
fn is_package_init_above_changed(file: &Path, changed_paths: &BTreeSet<String>) -> bool {
    if file.file_name().is_none_or(|name| name != "__init__.py") {
        return false;
    }
    let Some(dir) = file.parent().map(normalized_path) else {
        return false;
    };
    changed_paths
        .iter()
        .any(|changed| dir.is_empty() || changed.starts_with(&format!("{dir}/")))
}

/// Directory names of the changed files' own directories, used to rank a
/// reaching file under a matching directory (`tests/components/<name>/`)
/// ahead of a distant one when the reach set itself exceeds the cap.
fn near_components(changed_paths: &BTreeSet<String>) -> BTreeSet<String> {
    changed_paths
        .iter()
        .filter_map(|changed| {
            Path::new(changed)
                .parent()
                .and_then(|dir| dir.file_name())
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .filter(|name| !matches!(name.as_str(), "tests" | "test" | "src"))
        .collect()
}

fn is_near(file: &Path, near: &BTreeSet<String>) -> bool {
    file.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| near.contains(name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::diff::ChangedLine;

    fn changed(path: &str, added: &[(usize, &str)]) -> ChangedFile {
        ChangedFile {
            path: PathBuf::from(path),
            added_lines: added
                .iter()
                .map(|(line, text)| ChangedLine {
                    line: *line,
                    text: (*text).to_string(),
                    new_side_line: *line,
                })
                .collect(),
            removed_lines: Vec::new(),
        }
    }

    #[test]
    fn module_level_change_searches_import_lines_for_the_module() -> Result<(), String> {
        let source = "import os\n\nRATE = 2\n\n\ndef total(amount):\n    return amount * RATE\n";
        let constant = changed("shop/settings.py", &[(3, "RATE = 2")]);
        let in_body = changed("shop/settings.py", &[(7, "    return amount * RATE")]);
        let import_only = changed("shop/settings.py", &[(1, "import os")]);
        assert!(changes_module_level(Some(source), &constant));
        assert!(!changes_module_level(Some(source), &in_body));
        // A re-pointed import is module behavior too.
        assert!(changes_module_level(Some(source), &import_only));

        let module = ModuleNeedle::of(Path::new("shop/settings.py"))
            .ok_or("a module file has a module needle")?;
        let file = Path::new("tests/test_checkout.py");
        // The adapter matches an imported module by its last segment alone.
        assert!(module.imported_by(file, "from other import settings\n"));
        assert!(module.imported_by(file, "import shop.settings as cfg\n"));
        assert!(module.imported_by(file, "from shop.settings import (\n    total,\n)\n"));
        // Another package's module of the same name, imported from.
        assert!(!module.imported_by(file, "from other.settings import total\n"));
        // A relative import names the package only from inside it.
        assert!(!module.imported_by(file, "from .settings import total\n"));
        assert!(module.imported_by(Path::new("shop/cart.py"), "from .settings import total\n"));
        assert!(!module.imported_by(file, "settings = load()\n"));
        Ok(())
    }

    #[test]
    fn a_route_literal_without_a_word_cannot_be_searched_for() {
        let mut needles = HashSet::new();
        let routed =
            "@app.get(\"/items/{item_id}\")\ndef read_item(item_id):\n    return item_id\n";
        assert!(seed_lines(routed.lines(), &mut needles).is_some());
        assert!(needles.contains("items") && needles.contains("read_item"));
        let root = "@app.get(\n    \"/\",\n)\ndef index():\n    return 1\n";
        assert!(seed_lines(root.lines(), &mut HashSet::new()).is_none());
        // An empty literal that is not a route is just a value.
        let parametrized =
            "@pytest.mark.parametrize(\"value\", [\"\", \"x\"])\ndef test_v(value):\n    pass\n";
        assert!(seed_lines(parametrized.lines(), &mut HashSet::new()).is_some());
    }

    #[test]
    fn src_layout_package_is_imported_by_its_own_name() -> Result<(), String> {
        let module = ModuleNeedle::of(Path::new("src/shop/__init__.py"))
            .ok_or("a package init has a module needle")?;
        let file = Path::new("tests/test_shop.py");
        assert!(module.imported_by(file, "from shop import Client\n"));
        assert!(module.imported_by(file, "from shop.api import Client\n"));
        assert!(!module.imported_by(file, "from other.shop import Client\n"));
        let single = ModuleNeedle::of(Path::new("src/shop.py")).ok_or("module needle")?;
        assert!(single.imported_by(file, "from shop import Client\n"));
        Ok(())
    }

    #[test]
    fn dunder_change_searches_every_enclosing_package() -> Result<(), String> {
        let source = "class Base:\n    def __get__(self, obj, owner):\n        return self\n\n    def name(self):\n        return 1\n";
        assert!(changes_dunder(
            Some(source),
            &changed("src/cachetools/_cached.py", &[(3, "        return self")])
        ));
        assert!(!changes_dunder(
            Some(source),
            &changed("src/cachetools/_cached.py", &[(6, "        return 1")])
        ));
        let module = ModuleNeedle::with_ancestors(Path::new("src/cachetools/_cached.py"))
            .ok_or("ancestor needle")?;
        assert!(module.imported_by(
            Path::new("tests/test_method.py"),
            "from cachetools import cachedmethod\n"
        ));
        Ok(())
    }

    #[test]
    fn a_fixture_over_a_changed_name_joins_the_needles() {
        let needles: HashSet<String> = ["make_order".to_string()].into_iter().collect();
        let conftest = "import pytest\n\n\n@pytest.fixture\ndef order():\n    return make_order(3)\n\n\n@pytest.fixture(name=\"cart\")\ndef _cart(order):\n    return [order]\n\n\n@pytest.fixture\ndef unrelated():\n    return 1\n";
        let first = fixtures_over(conftest, &needles);
        assert_eq!(first, vec!["order".to_string()]);
        // The next round sees `order` as a needle, so `cart` (by its
        // `name=` override) follows.
        let mut grown = needles.clone();
        grown.extend(first);
        let second = fixtures_over(conftest, &grown);
        assert!(second.contains(&"cart".to_string()) && !second.contains(&"unrelated".to_string()));
    }

    #[test]
    fn a_conftest_already_reaching_is_rescanned_for_fixtures_built_on_fixtures()
    -> Result<(), String> {
        let root =
            std::env::temp_dir().join(format!("ripr-reach-chained-fixture-{}", std::process::id()));
        let files = [
            (
                "orders.py",
                "def make_order(count):\n    if count > 2:\n        return count * 3\n    return count\n",
            ),
            (
                "tests/conftest.py",
                "import pytest\n\nfrom orders import make_order\n\n\n@pytest.fixture\ndef cart(order):\n    return [order]\n\n\n@pytest.fixture\ndef order():\n    return make_order(3)\n",
            ),
            (
                "tests/test_cart.py",
                "def test_cart(cart):\n    assert cart == [9]\n",
            ),
            ("filler/a.py", "def a(value):\n    return value\n"),
            ("filler/b.py", "def b(value):\n    return value\n"),
        ];
        for (path, text) in files {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|err| format!("mkdir: {err}"))?;
            }
            std::fs::write(&full, text).map_err(|err| format!("write: {err}"))?;
        }
        let changed_files = vec![changed("orders.py", &[(2, "    if count > 2:")])];
        let changed_paths: BTreeSet<String> = ["orders.py".to_string()].into_iter().collect();
        let discovered = files.iter().map(|(path, _)| PathBuf::from(path)).collect();
        let admission = admit_by_reach(
            &root,
            discovered,
            &changed_files,
            &changed_paths,
            3,
            1 << 20,
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove: {err}"))?;
        let admission = admission.ok_or("reach scan fell back")?;
        // `cart` joins the needles only after `order` does, in the conftest
        // that already reached in the first round.
        assert!(
            admission
                .kept
                .contains(&PathBuf::from("tests/test_cart.py")),
            "kept {:?}",
            admission.kept
        );
        assert_eq!(admission.refused_reaching, 0);
        assert_eq!(admission.refused_unreached, 2);
        Ok(())
    }

    #[test]
    fn an_aliased_fixture_decorator_still_marks_a_fixture() {
        let needles: HashSet<String> = ["make_order".to_string()].into_iter().collect();
        let conftest = "from pytest import (\n    fixture as fx,\n    mark,\n)\n\n\n@fx\ndef order():\n    return make_order(3)\n\n\n@mark.slow\ndef helper():\n    return make_order(1)\n";
        assert_eq!(fixtures_over(conftest, &needles), vec!["order".to_string()]);
        let unaliased =
            "from other import fixture as fx\n\n\n@fx\ndef order():\n    return make_order(3)\n";
        assert!(fixtures_over(unaliased, &needles).is_empty());
    }

    #[test]
    fn a_fixture_calling_an_owner_by_its_import_alias_joins_the_needles() {
        let needles: HashSet<String> = ["make_order".to_string()].into_iter().collect();
        let conftest = "import pytest\n\nfrom orders import make_order as build\n\n\n@pytest.fixture\ndef order():\n    return build(3)\n";
        assert_eq!(fixtures_over(conftest, &needles), vec!["order".to_string()]);
    }

    #[test]
    fn a_fixture_body_ends_at_its_logical_header_and_body() {
        let needles: HashSet<String> = ["make_order".to_string()].into_iter().collect();
        let multiline = "@pytest.fixture\ndef order(\nrequest,  # )\nlabel=\")\",\n):\n    \"\"\"Doc.\n\nColumn zero.\n    \"\"\"\n    return make_order(3)\n";
        assert_eq!(
            fixtures_over(multiline, &needles),
            vec!["order".to_string()]
        );
        // The body still ends at the next logical line at the def's indent:
        // a later plain function calling the owner is not the fixture's.
        let ended = "@pytest.fixture(\nscope=\"module\",\n)\ndef order(\n):\n    return 3\n\n\ndef helper():\n    return make_order(1)\n";
        assert!(fixtures_over(ended, &needles).is_empty());
        let lines = [
            "def f(a,  # (",
            "b='(',",
            "):",
            "    s = '''x",
            "y'''",
            "    t = 1 \\",
            "+ 2",
            "z = 0",
        ];
        assert_eq!(
            continuation_lines(&lines),
            vec![false, true, true, false, true, false, true, false]
        );
    }

    #[test]
    fn import_statements_read_every_spelling_python_accepts() {
        let source = "from pytest\timport fixture as fx\nfrom  pytest  import(fixture  as  fy)\nfrom.orders import make_order as build\nimport os; from pytest import fixture as fz\nfrom pytest import \\\n    fixture as fw\nfrom pytest import (  # fixtures (and more)\n    # decorator\n    fixture as fv,  # alias\n)\nfrom_x = 1\npass; from pytest import fixture as fu\nimport os; '#'; from pytest import fixture as ft\ntry: from pytest import fixture as fs\nexcept ImportError: pass\nNOTE = \"\"\"\nimport not_an_import\n\"\"\"\n";
        let statements: Vec<String> = import_statements(source).collect();
        assert_eq!(
            statements,
            vec![
                "from pytest import fixture as fx",
                "from pytest import ( fixture as fy )",
                "from .orders import make_order as build",
                "import os",
                "from pytest import fixture as fz",
                "from pytest import fixture as fw",
                "from pytest import ( fixture as fv , )",
                "from pytest import fixture as fu",
                "import os",
                "from pytest import fixture as ft",
                "from pytest import fixture as fs",
            ]
        );
        assert_eq!(
            fixture_aliases(source),
            vec!["fx", "fy", "fz", "fw", "fv", "fu", "ft", "fs"]
        );
        let needles: HashSet<String> = ["make_order".to_string()].into_iter().collect();
        assert_eq!(imported_aliases(source, &needles), vec!["build"]);
    }

    #[test]
    fn only_conftests_on_a_changed_file_or_reaching_test_chain_are_structural() -> Result<(), String>
    {
        let root =
            std::env::temp_dir().join(format!("ripr-reach-conftest-chain-{}", std::process::id()));
        let files = [
            (
                "orders.py",
                "def make_order(count):\n    if count > 2:\n        return count * 3\n    return count\n",
            ),
            ("tests/conftest.py", "import pytest\n"),
            (
                "tests/test_a.py",
                "from orders import make_order\n\n\ndef test_a():\n    assert make_order(3) == 9\n",
            ),
            (
                "other/conftest.py",
                "import pytest\nfrom other.helpers import h\n",
            ),
            ("other/helpers.py", "def h():\n    return 1\n"),
            ("other/test_b.py", "def test_b():\n    assert True\n"),
        ];
        for (path, text) in files {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|err| format!("mkdir: {err}"))?;
            }
            std::fs::write(&full, text).map_err(|err| format!("write: {err}"))?;
        }
        let changed_files = vec![changed("orders.py", &[(2, "    if count > 2:")])];
        let changed_paths: BTreeSet<String> = ["orders.py".to_string()].into_iter().collect();
        let discovered = files.iter().map(|(path, _)| PathBuf::from(path)).collect();
        let admission = admit_by_reach(
            &root,
            discovered,
            &changed_files,
            &changed_paths,
            3,
            1 << 20,
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove: {err}"))?;
        let admission = admission.ok_or("reach scan fell back")?;
        // The unrelated `other/conftest.py` neither takes a slot nor makes
        // what it imports count as reaching.
        assert_eq!(
            admission.kept,
            ["orders.py", "tests/conftest.py", "tests/test_a.py"].map(PathBuf::from)
        );
        assert_eq!(admission.refused_reaching, 0);
        assert_eq!(admission.refused_unreached, 3);
        Ok(())
    }

    #[test]
    fn an_unreadable_changed_file_keeps_the_plain_cap() -> Result<(), String> {
        // The changed file is missing from the worktree, so what it defines
        // is unknown and admission must not claim the rest cannot reach it.
        let root = std::env::temp_dir().join(format!(
            "ripr-reach-unreadable-changed-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).map_err(|err| format!("mkdir: {err}"))?;
        std::fs::write(root.join("other.py"), "def other():\n    return 1\n")
            .map_err(|err| format!("write: {err}"))?;
        let changed_files = vec![changed("orders.py", &[(2, "    if count > 2:")])];
        let changed_paths: BTreeSet<String> = ["orders.py".to_string()].into_iter().collect();
        let discovered = vec![PathBuf::from("orders.py"), PathBuf::from("other.py")];
        let admission = admit_by_reach(
            &root,
            discovered,
            &changed_files,
            &changed_paths,
            1,
            1 << 20,
        );
        std::fs::remove_dir_all(&root).map_err(|err| format!("remove: {err}"))?;
        assert!(admission.is_none());
        Ok(())
    }

    #[test]
    fn a_route_of_placeholders_alone_cannot_be_searched_for() {
        let routed = "@app.get(\"/{item_id}\")\ndef read_item(item_id):\n    return item_id\n";
        assert!(seed_lines(routed.lines(), &mut HashSet::new()).is_none());
        let flask = "@app.route(\"/<name>/<int:page>\")\ndef page(name, page):\n    return name\n";
        assert!(seed_lines(flask.lines(), &mut HashSet::new()).is_none());
        let annotated = "RATES: dict[str, int] = {}\n";
        let mut needles = HashSet::new();
        assert!(seed_lines(annotated.lines(), &mut needles).is_some());
        assert!(needles.contains("RATES"));
    }

    #[test]
    fn import_targets_resolve_relative_and_suffix_match_absolute_imports() {
        let source =
            "from . import helpers\nfrom ..common import assert_ok as ok\nimport pkg.util, other\n";
        let targets: Vec<String> = import_targets(Path::new("tests/unit/test_a.py"), source)
            .into_iter()
            .map(|target| match target {
                ImportTarget::Exact(path) => format!("exact:{path}"),
                ImportTarget::Suffix(path) => format!("suffix:{path}"),
            })
            .collect();
        for expected in [
            "exact:tests/unit/helpers.py",
            "exact:tests/common.py",
            "exact:tests/common/assert_ok.py",
            "suffix:pkg/util.py",
            "suffix:other/__init__.py",
        ] {
            assert!(
                targets.iter().any(|target| target == expected),
                "missing {expected} in {targets:?}"
            );
        }
        assert!(
            !targets.iter().any(|target| target.contains("/ok")),
            "an alias is not a module: {targets:?}"
        );
    }
}
