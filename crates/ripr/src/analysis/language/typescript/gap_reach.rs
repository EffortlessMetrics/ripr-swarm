//! Diff scoping for the `typescript_test_extraction_partial` disclosure (#10436).
//!
//! The test index is workspace-wide, so one registration the extractor drops
//! (a loop over non-literal data, a wrapper-passed `it`) anywhere in a large
//! repository made every analysis of it partial, however unrelated the diff.
//! A gap only matters when a test in that file could be related to code the
//! diff changed. This answers that question conservatively: the gap file
//! matters unless its whole local import closure is resolved and provably
//! never reaches a changed file or names a changed owner.
//!
//! Every uncertain step answers "may reach": an import that does not resolve to
//! a workspace file, a non-literal `import()` / `require()`, a bare specifier
//! the alias map or a workspace package could resolve, a bundler alias, or a
//! closure past the file budget. A wrong "cannot reach" would hide a real
//! partial result; a wrong "may reach" only keeps today's disclosure.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::TsAliasMap;
use super::paths::normalized_path;
use super::tsconfig::TsAliasUnresolveCause;

/// Files followed from one gap file before giving up and keeping the gap.
const CLOSURE_FILE_BUDGET: usize = 400;

const SOURCE_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Specifier extensions that are assets, not code with imports of their own.
const ASSET_EXTENSIONS: [&str; 9] = [
    "json", "css", "scss", "less", "svg", "png", "txt", "md", "wasm",
];

/// Calls whose first literal argument is a module specifier. The opening
/// parenthesis may follow whitespace or comments (`require ('x')`).
const SPECIFIER_CALLS: [&str; 6] = [
    "require",
    "import",
    "mock",
    "doMock",
    "importActual",
    "requireActual",
];

/// Constructs that load modules by a pattern or a computed name.
const OPAQUE_LOADERS: [&str; 5] = [
    "import.meta.glob",
    "require.context",
    "createRequire",
    "module.require",
    "import.meta.resolve",
];

pub(super) struct GapReach<'a> {
    /// Workspace sources keyed by normalized path.
    sources: &'a HashMap<String, &'a String>,
    changed: HashSet<String>,
    needles: Vec<String>,
    alias_map: Option<&'a TsAliasMap>,
    package_names: Vec<String>,
    /// The root `package.json` `exports` map, for the package's own name.
    package_exports: Option<serde_json::Value>,
    /// Scoping is off: a bundler alias or a cross-language consumer exists.
    disabled: bool,
    /// Directories whose own tsconfig remaps resolution for files beneath them.
    remapped_dirs: Vec<String>,
}

impl<'a> GapReach<'a> {
    pub(super) fn new(
        sources: &'a HashMap<String, &'a String>,
        changed_paths: &[String],
        owner_names: impl IntoIterator<Item = String>,
        alias_map: Option<&'a TsAliasMap>,
        root: &Path,
        cross_language_consumer: bool,
    ) -> Self {
        let package_json = super::bounded_read::read_config_capped(&root.join("package.json")).ok();
        let package_names = package_json
            .as_deref()
            .map(package_json_name)
            .into_iter()
            .flatten()
            .collect();
        let package_exports = package_json
            .as_deref()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
            .and_then(|manifest| manifest.get("exports").cloned());
        // A bundler, transpiler or test-runner alias is invisible to tsconfig.
        // Any config mentioning one makes every bare specifier suspect, so keep
        // the workspace-wide disclosure.
        let hiding = resolution_config_hides_imports(root, alias_map);
        let mut needles: Vec<String> = owner_names
            .into_iter()
            .filter(|name| !name.is_empty())
            .collect();
        needles.sort();
        needles.dedup();
        Self {
            sources,
            changed: changed_paths.iter().cloned().collect(),
            needles,
            alias_map,
            package_names,
            package_exports,
            disabled: hiding.everywhere || cross_language_consumer,
            remapped_dirs: hiding.dirs,
        }
    }

    /// Whether a test in `gap_file` could be related to the changed code.
    pub(super) fn may_reach(&self, gap_file: &Path) -> bool {
        if self.disabled {
            return true;
        }
        let start = normalized_path(gap_file);
        let mut seen: HashSet<String> = HashSet::new();
        let mut queue = vec![start];
        while let Some(path) = queue.pop() {
            if !seen.insert(path.clone()) {
                continue;
            }
            if seen.len() > CLOSURE_FILE_BUDGET
                || self.changed.contains(&path)
                || self.is_under_remapped_dir(&path)
            {
                return true;
            }
            let Some(source) = self.sources.get(&path) else {
                return true;
            };
            if self
                .needles
                .iter()
                .any(|needle| source.contains(needle.as_str()))
            {
                return true;
            }
            let Some(specifiers) = module_specifiers(source) else {
                return true;
            };
            for specifier in specifiers {
                match self.follow(&path, &specifier) {
                    Follow::External => {}
                    Follow::Files(files) => queue.extend(files),
                    Follow::MayReach => return true,
                }
            }
        }
        false
    }

    fn is_under_remapped_dir(&self, path: &str) -> bool {
        self.remapped_dirs.iter().any(|dir| {
            path.strip_prefix(dir.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        })
    }

    fn follow(&self, from: &str, specifier: &str) -> Follow {
        if specifier.starts_with('.') {
            return self.follow_relative(from, specifier);
        }
        if specifier.starts_with("node:") {
            return Follow::External;
        }
        // Path-like or conventional alias prefixes a bundler could map.
        if specifier.starts_with(['/', '~', '#', '$']) || specifier.starts_with("@/") {
            return Follow::MayReach;
        }
        if let Some(name) = self
            .package_names
            .iter()
            .find(|name| specifier == name.as_str() || specifier.starts_with(&format!("{name}/")))
        {
            return self.follow_self_import(name, specifier);
        }
        if let Some(map) = self.alias_map {
            let unresolved_by_design = matches!(
                map.unresolve_cause_for(specifier),
                TsAliasUnresolveCause::PatternUnmatched | TsAliasUnresolveCause::NoPatterns
            );
            if map.resolve(specifier).is_some()
                || map.workspace_package_dir_for(specifier).is_some()
                || !unresolved_by_design
            {
                return Follow::MayReach;
            }
        }
        Follow::External
    }

    /// A package importing its own name resolves through `exports`. The
    /// adapter's resolver is authoritative where it resolves (including a
    /// build-output export mirrored to `src/`); otherwise only targets that
    /// are workspace sources are followed, and an unscanned build output
    /// relates nothing. A missing or pattern entry keeps the gap.
    fn follow_self_import(&self, name: &str, specifier: &str) -> Follow {
        // The adapter's own resolver maps a build-output export back to its
        // `src/` counterpart; follow exactly the file it relates tests to.
        if let Some(map) = self.alias_map {
            if let Some(resolved) = map.resolve(specifier) {
                let resolved = normalized_path(&resolved);
                return if self.sources.contains_key(&resolved) {
                    Follow::Files(vec![resolved])
                } else {
                    Follow::MayReach
                };
            }
            if map.workspace_package_dir_for(specifier).is_some() {
                return Follow::MayReach;
            }
        }
        let Some(exports) = &self.package_exports else {
            return Follow::MayReach;
        };
        let subpath = match specifier.strip_prefix(name) {
            Some("") => ".".to_string(),
            Some(rest) => format!(".{rest}"),
            None => return Follow::MayReach,
        };
        let entry = match exports {
            serde_json::Value::Object(map) if map.keys().any(|key| key.starts_with('.')) => {
                match map.get(&subpath) {
                    Some(entry) => entry,
                    None => return Follow::MayReach,
                }
            }
            other if subpath == "." => other,
            _ => return Follow::MayReach,
        };
        let mut targets = Vec::new();
        let mut pending = vec![entry];
        while let Some(value) = pending.pop() {
            match value {
                serde_json::Value::String(target) => targets.push(target.as_str()),
                serde_json::Value::Object(map) => pending.extend(map.values()),
                serde_json::Value::Array(items) => pending.extend(items),
                _ => {}
            }
        }
        let mut found = Vec::new();
        for target in targets {
            let Some(relative) = target.strip_prefix("./") else {
                return Follow::MayReach;
            };
            if relative.contains('*') {
                return Follow::MayReach;
            }
            if self.sources.contains_key(relative) {
                found.push(relative.to_string());
            }
        }
        if found.is_empty() {
            Follow::External
        } else {
            Follow::Files(found)
        }
    }

    fn follow_relative(&self, from: &str, specifier: &str) -> Follow {
        let extension = Path::new(specifier)
            .extension()
            .and_then(|extension| extension.to_str());
        if extension.is_some_and(|extension| ASSET_EXTENSIONS.contains(&extension)) {
            return Follow::External;
        }
        let dir = from.rsplit_once('/').map_or("", |(dir, _)| dir);
        let Some(joined) = join_normalized(dir, specifier) else {
            return Follow::MayReach;
        };
        let mut candidates = vec![joined.clone()];
        // ESM TypeScript spells the emitted extension: `./x.js` is `./x.ts`.
        let stem = ["js", "mjs", "cjs", "jsx"]
            .iter()
            .find_map(|ext| joined.strip_suffix(&format!(".{ext}")));
        let bases = std::iter::once(joined.as_str()).chain(stem);
        for base in bases {
            for ext in SOURCE_EXTENSIONS {
                candidates.push(format!("{base}.{ext}"));
                candidates.push(format!("{base}/index.{ext}"));
            }
        }
        let found: Vec<String> = candidates
            .into_iter()
            .filter(|candidate| self.sources.contains_key(candidate))
            .collect();
        if found.is_empty() {
            Follow::MayReach
        } else {
            Follow::Files(found)
        }
    }
}

enum Follow {
    External,
    Files(Vec<String>),
    MayReach,
}

/// Where a resolution-hiding config applies.
#[derive(Default)]
struct ConfigHiding {
    /// Resolution is remapped somewhere the scan cannot localize.
    everywhere: bool,
    /// A nested package tsconfig that extends, references or sets `paths`
    /// remaps only files beneath its own directory (relative, `/`-separated).
    dirs: Vec<String>,
}

impl ConfigHiding {
    fn everywhere() -> Self {
        Self {
            everywhere: true,
            dirs: Vec::new(),
        }
    }
}

/// Config that lets a bare or relative specifier resolve somewhere the import
/// scan cannot see. Checked over every `tsconfig*` / `jsconfig*` / `package.json`
/// within a few levels of the root, since a package-level config is not loaded
/// by the alias map.
fn resolution_config_hides_imports(root: &Path, alias_map: Option<&TsAliasMap>) -> ConfigHiding {
    const ALWAYS: [&str; 8] = [
        "rootDirs",
        "moduleNameMapper",
        "moduleDirectories",
        "modulePaths",
        "plugin-alias",
        "module-resolver",
        "_moduleAliases",
        "tsconfig-paths",
    ];
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    let mut hiding = ConfigHiding::default();
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                let skipped = name.starts_with('.')
                    || matches!(name.as_str(), "node_modules" | "target" | "dist" | "build");
                if !skipped && depth < 4 {
                    pending.push((path, depth + 1));
                }
                continue;
            }
            let is_ts_config = (name.starts_with("tsconfig") || name.starts_with("jsconfig"))
                && name.ends_with(".json");
            if !is_ts_config && name != "package.json" && !is_tool_config(&name) {
                continue;
            }
            visited += 1;
            if visited > 200 {
                return ConfigHiding::everywhere();
            }
            let Ok(text) = super::bounded_read::read_config_capped(&path) else {
                return ConfigHiding::everywhere();
            };
            if ALWAYS.iter().any(|marker| text.contains(marker)) || mentions_alias_key(&text) {
                return ConfigHiding::everywhere();
            }
            if is_ts_config {
                // TypeScript decodes JSONC keys, including `\u` escapes. The
                // alias loader does too, so a raw spelling check could hide a
                // reaching extraction gap behind inherited paths (#10522).
                let Some(config) =
                    super::tsconfig::strip_jsonc(&text)
                        .ok()
                        .and_then(|normalized| {
                            serde_json::from_str::<serde_json::Value>(&normalized).ok()
                        })
                else {
                    return ConfigHiding::everywhere();
                };
                if !config.is_object() {
                    return ConfigHiding::everywhere();
                }
                let mut pending = vec![&config];
                let mut resolves = false;
                while let Some(value) = pending.pop() {
                    match value {
                        serde_json::Value::Object(entries) => {
                            for (key, child) in entries {
                                // `baseUrl` and `rootDirs` can redirect imports
                                // even when a root alias map loaded successfully.
                                if matches!(key.as_str(), "baseUrl" | "rootDirs") {
                                    return ConfigHiding::everywhere();
                                }
                                if key == "paths" {
                                    resolves = true;
                                }
                                pending.push(child);
                            }
                        }
                        serde_json::Value::Array(items) => pending.extend(items),
                        _ => {}
                    }
                }
                resolves |= extends_may_remap(&config, &dir)
                    || references_leave_the_scan(&config, &dir, root);
                // Only the root config feeds the alias map, and only when it
                // loaded. Anything else that remaps resolution is invisible.
                let loaded_root = depth == 0 && alias_map.is_some_and(TsAliasMap::has_tsconfig);
                if resolves && !loaded_root {
                    if depth == 0 {
                        return ConfigHiding::everywhere();
                    }
                    let relative = dir
                        .strip_prefix(root)
                        .ok()
                        .map(|rel| rel.to_string_lossy().replace('\\', "/"));
                    match relative {
                        Some(rel) if !rel.is_empty() => {
                            if !hiding.dirs.contains(&rel) {
                                hiding.dirs.push(rel);
                            }
                        }
                        _ => return ConfigHiding::everywhere(),
                    }
                }
            }
        }
    }
    hiding
}

/// Whether a JSONC tsconfig declares `paths`, `baseUrl` or `rootDirs` anywhere.
fn declares_resolution_remap(config: &serde_json::Value) -> bool {
    let mut pending = vec![config];
    while let Some(value) = pending.pop() {
        match value {
            serde_json::Value::Object(entries) => {
                for (key, child) in entries {
                    if matches!(key.as_str(), "paths" | "baseUrl" | "rootDirs") {
                        return true;
                    }
                    pending.push(child);
                }
            }
            serde_json::Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    false
}

fn read_tsconfig(path: &Path) -> Option<serde_json::Value> {
    let text = super::bounded_read::read_config_capped(path).ok()?;
    let normalized = super::tsconfig::strip_jsonc(&text).ok()?;
    serde_json::from_str(&normalized).ok()
}

/// Top-level `"extends"` entries as specifiers; `None` when malformed.
fn extends_specifiers(config: &serde_json::Value) -> Option<Vec<&str>> {
    match config.get("extends") {
        None => Some(Vec::new()),
        Some(serde_json::Value::String(spec)) => Some(vec![spec.as_str()]),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str())
            .collect::<Option<Vec<_>>>(),
        Some(_) => None,
    }
}

/// Whether the `extends` chain of `config` (in `dir`) could remap resolution.
/// A package base config (`@scope/tsconfig`) can carry `paths`, so it keeps
/// the gap; a workspace-relative chain is followed and only a chain that
/// declares `paths`, `baseUrl` or `rootDirs`, or cannot be read, does.
fn extends_may_remap(config: &serde_json::Value, dir: &Path) -> bool {
    const MAX_CHAIN: usize = 8;
    let mut pending: Vec<(serde_json::Value, std::path::PathBuf, usize)> =
        vec![(config.clone(), dir.to_path_buf(), 0)];
    while let Some((current, current_dir, chain)) = pending.pop() {
        let Some(specs) = extends_specifiers(&current) else {
            return true;
        };
        for spec in specs {
            if !spec.starts_with('.') || chain >= MAX_CHAIN {
                return true;
            }
            let base = current_dir.join(spec);
            let mut with_json = base.clone().into_os_string();
            with_json.push(".json");
            let with_json = std::path::PathBuf::from(with_json);
            let Some(path) = [base, with_json].into_iter().find(|path| path.is_file()) else {
                return true;
            };
            let Some(parsed) = read_tsconfig(&path) else {
                return true;
            };
            if declares_resolution_remap(&parsed) {
                return true;
            }
            let next_dir = path.parent().map_or(current_dir.clone(), Path::to_path_buf);
            pending.push((parsed, next_dir, chain + 1));
        }
    }
    false
}

/// Whether a project `"references"` entry names a config this scan does not
/// visit itself (outside the root, missing, hidden or beyond the depth bound).
fn references_leave_the_scan(config: &serde_json::Value, dir: &Path, root: &Path) -> bool {
    let Some(references) = config.get("references") else {
        return false;
    };
    let Some(items) = references.as_array() else {
        return true;
    };
    items.iter().any(|item| {
        let Some(reference) = item.get("path").and_then(serde_json::Value::as_str) else {
            return true;
        };
        let target = dir.join(reference);
        let file = if target.is_dir() {
            target.join("tsconfig.json")
        } else {
            target
        };
        let Ok(relative) = file.strip_prefix(root) else {
            return true;
        };
        // Lexically resolve `..` (`packages/a/../b`); climbing above the root
        // leaves the scan.
        let mut components: Vec<String> = Vec::new();
        for part in relative.components() {
            match part {
                // The guard pops: `..` with nothing left climbs above the root.
                std::path::Component::ParentDir if components.pop().is_none() => return true,
                std::path::Component::ParentDir => {}
                std::path::Component::Normal(name) => {
                    components.push(name.to_string_lossy().into_owned());
                }
                _ => {}
            }
        }
        let skipped = components.iter().any(|part| {
            part.starts_with('.')
                || matches!(part.as_str(), "node_modules" | "target" | "dist" | "build")
        });
        !file.is_file() || skipped || components.len() > 5
    })
}

/// Whether `text` uses an `alias` key or identifier (`resolve.alias`, `alias:`,
/// `"alias"`, `alias({...})`). A hyphenated name such as the lint rule
/// `typescript/no-this-alias` is not a resolution alias.
fn mentions_alias_key(text: &str) -> bool {
    text.match_indices("alias").any(|(at, _)| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(|prev| prev.is_alphanumeric() || matches!(prev, '_' | '$' | '-'))
    })
}

/// Build, transpile and test-runner configs that can alias module names.
fn is_tool_config(name: &str) -> bool {
    [
        "babel.config",
        ".babelrc",
        ".swcrc",
        "vite.config",
        "vitest.config",
        "jest.config",
        "webpack.config",
        "rollup.config",
        "rspack.config",
        "tsup.config",
        "nuxt.config",
        "next.config",
        "svelte.config",
        "ava.config",
        ".mocharc",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

/// `dir` joined with a relative `specifier`, with `.` and `..` resolved.
/// `None` when the specifier climbs above the workspace root.
fn join_normalized(dir: &str, specifier: &str) -> Option<String> {
    let mut parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    for part in specifier.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

fn package_json_name(text: &str) -> Option<String> {
    let at = text.find("\"name\"")?;
    let rest = text[at + "\"name\"".len()..]
        .trim_start()
        .strip_prefix(':')?;
    let rest = rest.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

/// Literal module specifiers of one file, scanned from text so a file need not
/// parse. Over-collecting (a specifier inside a comment) only widens the
/// closure. `None` when the file loads modules in a way a scan cannot follow.
fn module_specifiers(source: &str) -> Option<Vec<String>> {
    if OPAQUE_LOADERS.iter().any(|loader| source.contains(loader)) {
        return None;
    }
    let mut specifiers = Vec::new();
    let bytes = source.as_bytes();
    // `from '...'`, `import '...'`: a quoted literal after the keyword.
    for keyword in ["from", "import"] {
        for (at, _) in source.match_indices(keyword) {
            if at > 0 && is_ident_byte(bytes[at - 1]) {
                continue;
            }
            let after = at + keyword.len();
            if bytes.get(after).copied().is_some_and(is_ident_byte) {
                continue;
            }
            // A static `from` / `import` specifier is never a template
            // literal; this also skips prose such as "the `from`/`to` options".
            let rest = skip_trivia(&source[after..]);
            if rest.starts_with(['\'', '"'])
                && let Some(literal) = quoted_literal(rest)
            {
                specifiers.push(literal);
            }
        }
    }
    for call in SPECIFIER_CALLS {
        for (at, _) in source.match_indices(call) {
            if at > 0 && is_call_name_byte(bytes[at - 1]) {
                continue;
            }
            let after = at + call.len();
            if bytes.get(after).copied().is_some_and(is_call_name_byte) {
                continue;
            }
            let rest = skip_trivia(&source[after..]);
            let Some(arguments) = rest.strip_prefix('(') else {
                continue;
            };
            match closed_call_literal(skip_trivia(arguments)) {
                Some(literal) => specifiers.push(literal),
                // A computed `import(name)` / `require(name)` reaches anything.
                None if matches!(call, "require" | "import") => return None,
                None => {}
            }
        }
    }
    Some(specifiers)
}

fn is_call_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

/// `text` without leading whitespace and `/* ... */` or `// ...` comments.
fn skip_trivia(mut text: &str) -> &str {
    loop {
        text = text.trim_start();
        if let Some(rest) = text.strip_prefix("/*") {
            let Some(end) = rest.find("*/") else {
                return "";
            };
            text = &rest[end + 2..];
        } else if let Some(rest) = text.strip_prefix("//") {
            text = rest.split_once('\n').map_or("", |(_, after)| after);
        } else {
            return text;
        }
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$' || byte == b'.'
}

/// The literal that is the whole first argument: its closing quote must be
/// followed by `)` or `,`. `'./dir/' + name` is computed, not a fixed path.
fn closed_call_literal(text: &str) -> Option<String> {
    let literal = quoted_literal(text)?;
    let tail = skip_trivia(&text[literal.len() + 2..]);
    (tail.starts_with(')') || tail.starts_with(',')).then_some(literal)
}

fn quoted_literal(text: &str) -> Option<String> {
    let quote = text
        .chars()
        .next()
        .filter(|c| matches!(c, '\'' | '"' | '`'))?;
    let rest = &text[1..];
    let end = rest.find(quote)?;
    let literal = &rest[..end];
    // A template with substitutions is computed, not a fixed specifier.
    (!literal.contains("${")).then(|| literal.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reach<'a>(
        sources: &'a HashMap<String, &'a String>,
        changed: &[&str],
        names: &[&str],
    ) -> GapReach<'a> {
        GapReach::new(
            sources,
            &changed
                .iter()
                .map(|path| path.to_string())
                .collect::<Vec<_>>(),
            names.iter().map(|name| name.to_string()),
            None,
            Path::new("/nonexistent-root"),
            false,
        )
    }

    fn files(entries: &[(&str, &str)]) -> (Vec<(String, String)>, ()) {
        (
            entries
                .iter()
                .map(|(path, source)| (path.to_string(), source.to_string()))
                .collect(),
            (),
        )
    }

    fn check(entries: &[(&str, &str)], changed: &[&str], names: &[&str], gap: &str) -> bool {
        let (owned, ()) = files(entries);
        let sources: HashMap<String, &String> = owned
            .iter()
            .map(|(path, source)| (path.clone(), source))
            .collect();
        reach(&sources, changed, names).may_reach(Path::new(gap))
    }

    #[test]
    fn an_unrelated_gap_file_cannot_reach_the_diff() {
        let entries = [
            (
                "test/a.test.ts",
                "import { x } from '../src/x';\nimport vitest from 'vitest';\n",
            ),
            ("src/x.ts", "export const x = 1;\n"),
            ("src/changed.ts", "export function changedFn() {}\n"),
        ];
        assert!(!check(
            &entries,
            &["src/changed.ts"],
            &["changedFn"],
            "test/a.test.ts"
        ));
    }

    #[test]
    fn a_direct_or_transitive_import_of_a_changed_file_reaches() {
        let entries = [
            ("test/a.test.ts", "import { h } from './helper';\n"),
            (
                "test/helper.ts",
                "export { changedFn as h } from '../src/changed';\n",
            ),
            ("src/changed.ts", "export function changedFn() {}\n"),
        ];
        assert!(check(&entries, &["src/changed.ts"], &[], "test/a.test.ts"));
    }

    #[test]
    fn a_barrel_reaches_the_changed_file_it_re_exports() {
        let entries = [
            ("test/a.test.ts", "import { x } from '../src';\n"),
            ("src/index.ts", "export * from './changed';\n"),
            ("src/changed.ts", "export function changedFn() {}\n"),
        ];
        assert!(check(&entries, &["src/changed.ts"], &[], "test/a.test.ts"));
    }

    #[test]
    fn an_owner_name_in_the_closure_reaches_without_a_resolved_import() {
        let entries = [("test/a.test.ts", "globalThis.changedFn();\n")];
        assert!(check(
            &entries,
            &["src/changed.ts"],
            &["changedFn"],
            "test/a.test.ts"
        ));
    }

    #[test]
    fn unresolved_computed_and_opaque_loading_keeps_the_gap() {
        let unresolved = [("test/a.test.ts", "import x from './missing';\n")];
        assert!(check(&unresolved, &["src/c.ts"], &[], "test/a.test.ts"));
        let computed = [("test/a.test.ts", "const m = await import(name);\n")];
        assert!(check(&computed, &["src/c.ts"], &[], "test/a.test.ts"));
        let glob = [(
            "test/a.test.ts",
            "const m = import.meta.glob('../src/*.ts');\n",
        )];
        assert!(check(&glob, &["src/c.ts"], &[], "test/a.test.ts"));
        let aliased = [("test/a.test.ts", "import x from '~/thing';\n")];
        assert!(check(&aliased, &["src/c.ts"], &[], "test/a.test.ts"));
    }

    #[test]
    fn spaced_and_commented_loader_calls_are_followed() {
        let entries = [
            ("test/a.test.ts", "const m = require ('../src/changed');\n"),
            (
                "test/b.test.ts",
                "const m = await import /* lazy */ ('../src/changed');\n",
            ),
            ("test/c.test.ts", "const m = require\n(name);\n"),
            (
                "test/d.test.ts",
                "import x from /* c */ '../src/changed';\n",
            ),
            ("src/changed.ts", "export default function tier() {}\n"),
        ];
        for gap in [
            "test/a.test.ts",
            "test/b.test.ts",
            "test/c.test.ts",
            "test/d.test.ts",
        ] {
            assert!(check(&entries, &["src/changed.ts"], &[], gap), "{gap}");
        }
    }

    #[test]
    fn a_concatenated_specifier_is_computed_not_its_leading_literal() {
        for source in [
            "const m = require('../src/' + name);\n",
            "const m = await import(\"./fixtures/\" /* c */ + n);\n",
        ] {
            assert_eq!(module_specifiers(source), None, "{source}");
        }
        assert_eq!(
            module_specifiers("require('../src/a', opts); import('./b')\n"),
            Some(vec!["../src/a".to_string(), "./b".to_string()]),
        );
        let entries = [
            ("test/a.test.ts", "const m = require('../src/' + name);\n"),
            ("src/index.ts", "export {};\n"),
            ("src/changed.ts", "export default function tier() {}\n"),
        ];
        assert!(check(&entries, &["src/changed.ts"], &[], "test/a.test.ts"));
    }

    #[test]
    fn esm_emitted_extensions_resolve_to_the_typescript_source() {
        let entries = [
            ("test/a.test.ts", "import { x } from '../src/changed.js';\n"),
            ("src/changed.ts", "export const x = 1;\n"),
        ];
        assert!(check(&entries, &["src/changed.ts"], &[], "test/a.test.ts"));
    }

    #[test]
    fn a_cross_language_consumer_disables_scoping() {
        let (owned, ()) = files(&[("test/a.test.ts", "import vitest from 'vitest';\n")]);
        let sources: HashMap<String, &String> = owned
            .iter()
            .map(|(path, source)| (path.clone(), source))
            .collect();
        let scoped = GapReach::new(
            &sources,
            &["src/c.ts".to_string()],
            std::iter::empty(),
            None,
            Path::new("/nonexistent-root"),
            true,
        );
        assert!(scoped.may_reach(Path::new("test/a.test.ts")));
    }

    fn temp_root(label: &str) -> Result<std::path::PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-gap-reach-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|err| format!("temp dir: {err}"))?;
        Ok(dir)
    }

    #[test]
    fn resolution_config_that_remaps_imports_disables_scoping() -> Result<(), String> {
        for (label, file, text, hides) in [
            (
                "pkg-extends",
                "tsconfig.json",
                r#"{"extends": "@scope/tsconfig", "include": ["src"]}"#,
                true,
            ),
            (
                "rel-extends",
                "tsconfig.json",
                r#"{"extends": "./base.json"}"#,
                true,
            ),
            (
                "escaped-extends",
                "tsconfig.json",
                r#"{"\u0065xtends": "@scope/tsconfig"}"#,
                true,
            ),
            (
                "array-extends",
                "tsconfig.json",
                r#"{"extends": ["@scope/tsconfig"]}"#,
                true,
            ),
            (
                "commented-extends",
                "tsconfig.json",
                r#"{"extends" /* comment */ : "@scope/tsconfig",}"#,
                true,
            ),
            (
                "paths",
                "packages/a/tsconfig.json",
                r#"{"compilerOptions": {"paths": {"x": ["./y"]}}}"#,
                true,
            ),
            (
                "escaped-paths",
                "tsconfig.json",
                r#"{"compilerOptions": {"\u0070aths": {"x": ["./y"]}}}"#,
                true,
            ),
            (
                "escaped-unrelated-key",
                "tsconfig.json",
                r#"{"\u0069nclude": ["src"]}"#,
                false,
            ),
            (
                "unparseable-config",
                "tsconfig.json",
                r#"{"compilerOptions": {"paths": "#,
                true,
            ),
            (
                "base-url",
                "tsconfig.json",
                r#"{"compilerOptions": {"baseUrl": "."}}"#,
                true,
            ),
            (
                "root-dirs",
                "tsconfig.json",
                r#"{"compilerOptions": {"rootDirs": ["a", "b"]}}"#,
                true,
            ),
            (
                "jest-dirs",
                "package.json",
                r#"{"jest": {"moduleDirectories": ["src"]}}"#,
                true,
            ),
            (
                "plain",
                "package.json",
                r#"{"name": "x", "scripts": {}}"#,
                false,
            ),
            (
                "vitest-alias",
                "vitest.config.ts",
                "export default { resolve: { alias: { lib: './src' } } };",
                true,
            ),
            (
                "babel-resolver",
                "babel.config.js",
                "module.exports = { plugins: [['module-resolver', { root: ['./src'] }]] };",
                true,
            ),
            (
                "babelrc",
                ".babelrc",
                r#"{"plugins": [["module-resolver", {"alias": {"u": "./src"}}]]}"#,
                true,
            ),
            (
                "plugin-alias-import",
                "vite.config.ts",
                "import a from '@rollup/plugin-alias';\nexport default { plugins: [a({})] };",
                true,
            ),
            (
                "lint-rule-named-alias",
                "vite.config.ts",
                "export default { lint: { rules: { 'typescript/no-this-alias': 'error' } } };",
                false,
            ),
            (
                "alias-key-after-lint-rule",
                "vite.config.ts",
                "export default { lint: { 'no-this-alias': 'error' }, resolve: { alias: { a: './b' } } };",
                true,
            ),
            (
                "plain-tsconfig",
                "tsconfig.json",
                r#"{"compilerOptions": {"strict": true}}"#,
                false,
            ),
            (
                "plain-vitest",
                "vitest.config.ts",
                "export default { test: { globals: true } };",
                false,
            ),
        ] {
            let root = temp_root(label)?;
            let path = root.join(file);
            let parent = path.parent().ok_or("config path has a parent")?;
            std::fs::create_dir_all(parent).map_err(|err| format!("dirs: {err}"))?;
            std::fs::write(&path, text).map_err(|err| format!("write config: {err}"))?;
            assert_eq!(
                {
                    let hiding = resolution_config_hides_imports(&root, None);
                    hiding.everywhere || !hiding.dirs.is_empty()
                },
                hides,
                "{label}"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
        Ok(())
    }

    #[test]
    fn a_nested_package_tsconfig_remaps_only_its_own_directory() -> Result<(), String> {
        let root = temp_root("nested-extends")?;
        let nested = root.join("runtime-tests/node");
        std::fs::create_dir_all(&nested).map_err(|err| format!("dirs: {err}"))?;
        std::fs::write(
            nested.join("tsconfig.json"),
            r#"{"extends": "../../tsconfig.base.json"}"#,
        )
        .map_err(|err| format!("write config: {err}"))?;
        let hiding = resolution_config_hides_imports(&root, None);
        assert!(!hiding.everywhere);
        assert_eq!(hiding.dirs, vec!["runtime-tests/node".to_string()]);

        let entries = [
            ("test/a.test.ts", "import { x } from '../src/x';\n"),
            ("src/x.ts", "export const x = 1;\n"),
            ("src/changed.ts", "export function changedFn() {}\n"),
            ("runtime-tests/node/b.test.ts", "export const y = 1;\n"),
        ];
        let (owned, ()) = files(&entries);
        let sources: HashMap<String, &String> = owned
            .iter()
            .map(|(path, source)| (path.clone(), source))
            .collect();
        let reach = GapReach::new(
            &sources,
            &["src/changed.ts".to_string()],
            std::iter::empty(),
            None,
            &root,
            false,
        );
        assert!(!reach.may_reach(Path::new("test/a.test.ts")));
        assert!(reach.may_reach(Path::new("runtime-tests/node/b.test.ts")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }

    fn check_in(root: &Path, entries: &[(&str, &str)], changed: &[&str], gap: &str) -> bool {
        let (owned, ()) = files(entries);
        let sources: HashMap<String, &String> = owned
            .iter()
            .map(|(path, source)| (path.clone(), source))
            .collect();
        GapReach::new(
            &sources,
            &changed
                .iter()
                .map(|path| path.to_string())
                .collect::<Vec<_>>(),
            std::iter::empty(),
            None,
            root,
            false,
        )
        .may_reach(Path::new(gap))
    }

    #[test]
    fn a_self_import_follows_exports_only_into_workspace_sources() -> Result<(), String> {
        let entries = [
            ("test/a.test.ts", "import { j } from 'pkg/jsx';\n"),
            ("src/changed.ts", "export function changedFn() {}\n"),
        ];
        let write = |label: &str, manifest: &str| -> Result<std::path::PathBuf, String> {
            let root = temp_root(label)?;
            std::fs::write(root.join("package.json"), manifest)
                .map_err(|err| format!("write manifest: {err}"))?;
            Ok(root)
        };
        // Build output is not a scanned source: the import relates nothing.
        let built = write(
            "self-built",
            r#"{"name": "pkg", "exports": {"./jsx": {"import": "./dist/jsx.js"}}}"#,
        )?;
        assert!(!check_in(
            &built,
            &entries,
            &["src/changed.ts"],
            "test/a.test.ts"
        ));
        // An exports target that is a changed source reaches it.
        let source = write(
            "self-source",
            r#"{"name": "pkg", "exports": {"./jsx": "./src/changed.ts"}}"#,
        )?;
        assert!(check_in(
            &source,
            &entries,
            &["src/changed.ts"],
            "test/a.test.ts"
        ));
        // No exports map, no entry for the subpath, or a pattern keeps the gap.
        for (label, manifest) in [
            ("self-none", r#"{"name": "pkg", "main": "dist/index.js"}"#),
            (
                "self-missing",
                r#"{"name": "pkg", "exports": {".": "./dist/index.js"}}"#,
            ),
            (
                "self-pattern",
                r#"{"name": "pkg", "exports": {"./*": "./dist/*.js"}}"#,
            ),
        ] {
            let root = write(label, manifest)?;
            assert!(
                check_in(&root, &entries, &["src/changed.ts"], "test/a.test.ts"),
                "{label}"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
        let _ = std::fs::remove_dir_all(&built);
        let _ = std::fs::remove_dir_all(&source);
        Ok(())
    }

    /// Label, files written under the root, whether imports are hidden.
    type ChainCase<'a> = (&'a str, &'a [(&'a str, &'a str)], bool);

    #[test]
    fn a_workspace_extends_chain_hides_imports_only_when_it_remaps() -> Result<(), String> {
        // (label, files, scanned tsconfig, hides)
        let cases: [ChainCase; 10] = [
            (
                "plain-base",
                &[
                    (
                        "tsconfig.spec.json",
                        r#"{"extends": "./tsconfig.base.json"}"#,
                    ),
                    (
                        "tsconfig.base.json",
                        r#"{"compilerOptions": {"strict": true}}"#,
                    ),
                ],
                false,
            ),
            (
                "base-paths",
                &[
                    (
                        "tsconfig.spec.json",
                        r#"{"extends": "./tsconfig.base.json"}"#,
                    ),
                    (
                        "tsconfig.base.json",
                        r#"{"compilerOptions": {"paths": {"x": ["./y"]}}}"#,
                    ),
                ],
                true,
            ),
            (
                "second-link-base-url",
                &[
                    ("tsconfig.spec.json", r#"{"extends": "./mid"}"#),
                    ("mid.json", r#"{"extends": "./tsconfig.base.json"}"#),
                    (
                        "tsconfig.base.json",
                        r#"{"compilerOptions": {"baseUrl": "."}}"#,
                    ),
                ],
                true,
            ),
            (
                "missing-base",
                &[("tsconfig.spec.json", r#"{"extends": "./gone.json"}"#)],
                true,
            ),
            (
                "unreadable-base",
                &[
                    (
                        "tsconfig.spec.json",
                        r#"{"extends": "./tsconfig.base.json"}"#,
                    ),
                    ("tsconfig.base.json", "{ not json"),
                ],
                true,
            ),
            (
                "package-base",
                &[("tsconfig.spec.json", r#"{"extends": "@scope/tsconfig"}"#)],
                true,
            ),
            (
                "scanned-reference",
                &[
                    ("tsconfig.json", r#"{"references": [{"path": "./lib"}]}"#),
                    (
                        "lib/tsconfig.json",
                        r#"{"compilerOptions": {"strict": true}}"#,
                    ),
                ],
                false,
            ),
            (
                "sibling-reference-through-parent",
                &[
                    (
                        "packages/a/tsconfig.json",
                        r#"{"references": [{"path": "../b"}]}"#,
                    ),
                    (
                        "packages/b/tsconfig.json",
                        r#"{"compilerOptions": {"strict": true}}"#,
                    ),
                ],
                false,
            ),
            (
                "reference-above-root",
                &[(
                    "tsconfig.json",
                    r#"{"references": [{"path": "../outside"}]}"#,
                )],
                true,
            ),
            (
                "missing-reference",
                &[("tsconfig.json", r#"{"references": [{"path": "./gone"}]}"#)],
                true,
            ),
        ];
        for (label, entries, hides) in cases {
            let root = temp_root(label)?;
            for (file, text) in entries {
                let path = root.join(file);
                let parent = path.parent().ok_or("config path has a parent")?;
                std::fs::create_dir_all(parent).map_err(|err| format!("dirs: {err}"))?;
                std::fs::write(&path, text).map_err(|err| format!("write config: {err}"))?;
            }
            let hiding = resolution_config_hides_imports(&root, None);
            assert_eq!(
                hiding.everywhere || !hiding.dirs.is_empty(),
                hides,
                "{label}"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
        Ok(())
    }

    #[test]
    fn a_build_output_export_follows_the_source_the_adapter_resolves() -> Result<(), String> {
        let root = temp_root("self-mirror")?;
        std::fs::create_dir_all(root.join("src")).map_err(|err| format!("dirs: {err}"))?;
        for (file, text) in [
            (
                "package.json",
                r#"{"name": "pkg", "exports": {".": "./dist/index.js"}}"#,
            ),
            ("tsconfig.json", r#"{"compilerOptions": {"strict": true}}"#),
            ("src/index.ts", "export function changedFn() {}\n"),
        ] {
            std::fs::write(root.join(file), text).map_err(|err| format!("write: {err}"))?;
        }
        let entries = [
            ("test/a.test.ts", "import { changedFn } from 'pkg';\n"),
            ("src/index.ts", "export function changedFn() {}\n"),
        ];
        let (owned, ()) = files(&entries);
        let sources: HashMap<String, &String> = owned
            .iter()
            .map(|(path, source)| (path.clone(), source))
            .collect();
        let workspace: Vec<std::path::PathBuf> = entries
            .iter()
            .map(|(path, _)| std::path::PathBuf::from(path))
            .collect();
        let packages =
            super::super::workspace_packages::WorkspacePackages::discover(&root, &workspace);
        let map = TsAliasMap::workspace_packages_only(&root, packages);
        let reach = GapReach::new(
            &sources,
            &["src/index.ts".to_string()],
            std::iter::empty(),
            Some(&map),
            &root,
            false,
        );
        assert!(reach.may_reach(Path::new("test/a.test.ts")));
        let _ = std::fs::remove_dir_all(&root);
        Ok(())
    }
}
