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
    /// Scoping is off: a bundler alias or a cross-language consumer exists.
    disabled: bool,
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
        let package_json = std::fs::read_to_string(root.join("package.json")).ok();
        let package_names = package_json
            .as_deref()
            .map(package_json_name)
            .into_iter()
            .flatten()
            .collect();
        // A bundler, transpiler or test-runner alias is invisible to tsconfig.
        // Any config mentioning one makes every bare specifier suspect, so keep
        // the workspace-wide disclosure.
        let bundler_alias = resolution_config_hides_imports(root, alias_map);
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
            disabled: bundler_alias || cross_language_consumer,
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
            if seen.len() > CLOSURE_FILE_BUDGET || self.changed.contains(&path) {
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
        if self
            .package_names
            .iter()
            .any(|name| specifier == name || specifier.starts_with(&format!("{name}/")))
        {
            return Follow::MayReach;
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

/// Config that lets a bare or relative specifier resolve somewhere the import
/// scan cannot see. Checked over every `tsconfig*` / `jsconfig*` / `package.json`
/// within a few levels of the root, since a package-level config is not loaded
/// by the alias map.
fn resolution_config_hides_imports(root: &Path, alias_map: Option<&TsAliasMap>) -> bool {
    const ALWAYS: [&str; 8] = [
        "rootDirs",
        "moduleNameMapper",
        "moduleDirectories",
        "modulePaths",
        "alias",
        "module-resolver",
        "_moduleAliases",
        "tsconfig-paths",
    ];
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
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
                return true;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                return true;
            };
            if ALWAYS.iter().any(|marker| text.contains(marker)) {
                return true;
            }
            if is_ts_config {
                // `baseUrl` resolves any bare specifier into the workspace, with
                // or without `paths`, and the alias map does not model that.
                if text.contains("\"baseUrl\"") {
                    return true;
                }
                let resolves = text.contains("\"paths\"") || extends_a_workspace_config(&text);
                // Only the root config feeds the alias map, and only when it
                // loaded. Anything else that remaps resolution is invisible.
                let loaded_root = depth == 0 && alias_map.is_some_and(TsAliasMap::has_tsconfig);
                if resolves && !loaded_root {
                    return true;
                }
            }
        }
    }
    false
}

/// `"extends"` naming a config in the workspace (`./base.json`, `../tsconfig`).
/// A package base config (`@sindresorhus/tsconfig`) is shared compiler
/// settings, not a path map for this workspace; the alias map ignores it too.
fn extends_a_workspace_config(text: &str) -> bool {
    text.match_indices("\"extends\"").any(|(at, key)| {
        let rest = &text[at + key.len()..];
        let window: String = rest.chars().take(120).collect();
        window.contains("\"./") || window.contains("\"../")
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
            match quoted_literal(skip_trivia(arguments)) {
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
                false,
            ),
            (
                "rel-extends",
                "tsconfig.json",
                r#"{"extends": "./base.json"}"#,
                true,
            ),
            (
                "paths",
                "packages/a/tsconfig.json",
                r#"{"compilerOptions": {"paths": {"x": ["./y"]}}}"#,
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
                resolution_config_hides_imports(&root, None),
                hides,
                "{label}"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
        Ok(())
    }
}
