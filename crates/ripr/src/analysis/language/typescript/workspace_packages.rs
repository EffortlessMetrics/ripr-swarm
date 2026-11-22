//! Workspace package-name resolution for the TypeScript preview adapter (#4554).
//!
//! In a pnpm/npm/yarn/bun monorepo one package's tests import another
//! package by name (`import { toArray } from '@vitest/utils/helpers'`). The
//! package manager links the name to the package directory and the target's
//! `package.json` picks the file. This resolver reproduces that for packages
//! that live inside the analyzed workspace, and only when the manifest names
//! a workspace source file unambiguously:
//!
//! - the package is found by its `name` among the `package.json` files that
//!   own indexed workspace sources; a name two manifests share resolves
//!   nothing;
//! - with an `exports` field, the subpath must match an exact key or a
//!   single-`*` pattern key (`"./*"`); every string target under any
//!   condition is a candidate, and without `exports` the root subpath reads
//!   `source`/`module`/`main`/`types` and a deep subpath probes the file below
//!   the package directory;
//! - a candidate counts only when it is an indexed workspace source file
//!   (build output such as `dist/` is not indexed, `.d.ts` is excluded), and
//!   the distinct counted candidates must be exactly one;
//! - when every candidate is build output, each is mirrored into `src/`
//!   (`./esm/middleware.mjs` or `./middleware.js` → `src/middleware`), the
//!   layout the package's own test runner aliases to (`resolve.alias`,
//!   `moduleNameMapper`); the mirrored sources must again be exactly one.
//!
//! The package's own tests importing it by name (#4769, zustand) resolve the
//! same way: the root `package.json` is one of the manifests.
//!
//! Anything else fails closed, so the import keeps no relation, as before.

use super::bounded_read::read_config_capped;
use super::related_tests::package_root_for_file_path;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const SOURCE_EXTENSIONS: [&str; 8] = [".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"];

/// One workspace package's resolution-relevant manifest facts.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkspacePackage {
    /// Package directory, workspace-relative (`.` for the root).
    dir: PathBuf,
    exports: Option<serde_json::Value>,
    /// `source`, `module`, `main`, `types` values, in that order.
    entry_fields: Vec<String>,
}

/// Published runtime entry whose contents are outside the workspace source index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BuildEntry {
    pub(crate) main: String,
    pub(crate) source: Option<String>,
}

/// Workspace packages by `name`; `None` marks a name two manifests share.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspacePackages {
    by_name: BTreeMap<String, Option<WorkspacePackage>>,
    /// An unread root build entry, and its source only when it is unambiguous.
    build_entries: BTreeMap<PathBuf, BuildEntry>,
    /// Normalized workspace-relative paths of the indexed source files.
    sources: BTreeSet<String>,
}

impl WorkspacePackages {
    /// Reads the `package.json` that owns each indexed workspace source.
    pub(crate) fn discover(root: &Path, workspace_files: &[PathBuf]) -> Self {
        let mut dirs: BTreeSet<PathBuf> = BTreeSet::new();
        for file in workspace_files {
            if let Some(dir) = package_root_for_file_path(file, root) {
                dirs.insert(dir);
            }
        }
        let mut packages = Self {
            sources: workspace_files.iter().map(|file| normalize(file)).collect(),
            ..Self::default()
        };
        for dir in dirs {
            let Ok(text) = read_config_capped(&root.join(&dir).join("package.json")) else {
                continue;
            };
            let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&text) else {
                continue;
            };
            if let Some(main) = manifest.get("main").and_then(serde_json::Value::as_str)
                && let Some(stem) = build_entry_stem(main)
                && !packages.sources.contains(&normalize(&dir.join(main)))
            {
                let source = packages.own_build_source(root, &dir, &manifest, stem);
                packages.build_entries.insert(
                    dir.clone(),
                    BuildEntry {
                        main: main.to_owned(),
                        source,
                    },
                );
            }
            let Some(name) = manifest.get("name").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let entry_fields = ["source", "module", "main", "types"]
                .iter()
                .filter_map(|field| manifest.get(*field).and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect();
            let package = WorkspacePackage {
                dir,
                exports: manifest.get("exports").cloned(),
                entry_fields,
            };
            packages
                .by_name
                .entry(name.to_string())
                .and_modify(|existing| *existing = None)
                .or_insert(Some(package));
        }
        packages.require_tracked_build_sources(root);
        packages
    }

    fn require_tracked_build_sources(&mut self, root: &Path) {
        let mut wanted = BTreeSet::new();
        for (dir, entry) in &self.build_entries {
            if let Some(source) = &entry.source {
                wanted.insert(source.clone());
                wanted.insert(normalize(&dir.join("package.json")));
                wanted.extend(competing_package_modules(dir).map(|path| normalize(&path)));
            }
        }
        if !wanted.is_empty() {
            let tracked = crate::analysis::committed_source::tracked_files_matching(root, |file| {
                wanted.contains(file)
            });
            for (dir, entry) in &mut self.build_entries {
                if let Some(source) = &entry.source
                    && !tracked.as_ref().is_some_and(|files| {
                        files.contains(&normalize(&dir.join("package.json")))
                            && files.contains(source)
                            && !competing_package_modules(dir).any(|path| {
                                files.contains(&normalize(&path))
                                    || !matches!(std::fs::symlink_metadata(root.join(path)),
                                        Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                            })
                    })
                {
                    entry.source = None;
                }
            }
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_name.is_empty() && self.build_entries.is_empty()
    }

    pub(crate) fn build_source_in(&self, package: &Path) -> Option<&str> {
        self.build_entries.get(package)?.source.as_deref()
    }

    /// Resolve a relative directory import only for the package that owns the importer.
    pub(crate) fn own_build_entry(
        &self,
        root: &Path,
        importer: &Path,
        module: &str,
    ) -> Option<&BuildEntry> {
        let dir = Path::new(if module.is_empty() { "." } else { module });
        let entry = self.build_entries.get(dir)?;
        (package_root_for_file_path(importer, root).as_deref() == Some(dir)).then_some(entry)
    }

    fn own_build_source(
        &self,
        root: &Path,
        dir: &Path,
        manifest: &serde_json::Value,
        stem: &str,
    ) -> Option<String> {
        // A bundler can choose a different entry. Only one output stem across
        // main, module and all root export conditions can support this redirect.
        if let Some(module) = manifest.get("module")
            && build_entry_stem(module.as_str()?) != Some(stem)
        {
            return None;
        }
        if let Some(exports) = manifest.get("exports") {
            let targets = export_targets(exports, ".")?;
            let mut runtime = targets.iter().filter(|target| {
                !super::discovery::is_typescript_declaration_file(Path::new(target))
            });
            if build_entry_stem(runtime.next()?) != Some(stem)
                || runtime.any(|target| build_entry_stem(target) != Some(stem))
            {
                return None;
            }
        }
        let mirror = src_mirror(manifest.get("main")?.as_str()?)?;
        let declared = manifest.get("source");
        let target = match declared {
            Some(value) => value.as_str()?,
            None => &mirror,
        };
        let source = self.source_file_for(dir, target)?;
        // A filename supplied with its extension must match as written.
        // An absent src/entry.ts cannot designate src/entry.ts.ts.
        if declared.is_some()
            && Path::new(target).extension().is_some()
            && source != normalize(&dir.join(target))
        {
            return None;
        }
        let path = Path::new(&source);
        if super::discovery::is_test_file(path)
            || package_root_for_file_path(path, root).as_deref() != Some(dir)
            || crate::analysis::committed_source::snapshot_inputs_differ_in(
                root,
                Path::new(&normalize(dir)),
            )
        {
            return None;
        }
        // Module identity drops extensions. Two indexed sources with the same
        // stem would both get the relation even for a fully specified source.
        let stem = normalize(&path.with_extension(""));
        let siblings = SOURCE_EXTENSIONS
            .iter()
            .filter(|ext| self.sources.contains(&format!("{stem}{ext}")))
            .count();
        (siblings == 1).then_some(source)
    }

    /// Resolves a bare specifier to one workspace-relative source file.
    pub(crate) fn resolve(&self, specifier: &str) -> Option<PathBuf> {
        if specifier.starts_with('.') || specifier.starts_with('/') {
            return None;
        }
        let (package, subpath) = self.package_for(specifier)?;
        let candidates = match &package.exports {
            Some(exports) => export_targets(exports, &subpath)?,
            None if subpath == "." => package.entry_fields.clone(),
            None => vec![subpath.clone()],
        };
        let unique = |targets: &mut dyn Iterator<Item = String>| {
            let found: BTreeSet<String> = targets
                .filter_map(|target| self.source_file_for(&package.dir, &target))
                .collect();
            let mut found = found.into_iter();
            match (found.next(), found.next()) {
                (Some(only), None) => Some(PathBuf::from(only)),
                _ => None,
            }
        };
        let any_source = candidates
            .iter()
            .any(|target| self.source_file_for(&package.dir, target).is_some());
        if any_source {
            return unique(&mut candidates.into_iter());
        }
        unique(&mut candidates.iter().filter_map(|target| src_mirror(target)))
    }

    /// The directory of the workspace package `specifier` names, when it
    /// names exactly one (for unresolved-import advice).
    pub(crate) fn package_dir_for(&self, specifier: &str) -> Option<&Path> {
        self.package_for(specifier)
            .map(|(package, _)| package.dir.as_path())
    }

    /// The package whose name is the longest `/`-bounded prefix of
    /// `specifier`, with the remaining subpath in exports form (`.`, `./x`).
    fn package_for(&self, specifier: &str) -> Option<(&WorkspacePackage, String)> {
        let mut best: Option<(&str, &Option<WorkspacePackage>)> = None;
        for (name, package) in &self.by_name {
            let matches = specifier == name
                || specifier
                    .strip_prefix(name.as_str())
                    .is_some_and(|rest| rest.starts_with('/'));
            if matches && best.is_none_or(|(current, _)| name.len() > current.len()) {
                best = Some((name.as_str(), package));
            }
        }
        let (name, package) = best?;
        let rest = specifier.get(name.len()..)?;
        let subpath = if rest.is_empty() {
            ".".to_string()
        } else {
            format!(".{rest}")
        };
        Some((package.as_ref()?, subpath))
    }

    /// The indexed source file `target` names below `dir`, if any: the path
    /// itself, or the one file a source extension or `/index` suffix yields.
    fn source_file_for(&self, dir: &Path, target: &str) -> Option<String> {
        if target.ends_with(".d.ts") || target.ends_with(".d.mts") || target.ends_with(".d.cts") {
            return None;
        }
        let joined = dir.join(target.trim_start_matches("./"));
        if joined
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return None;
        }
        let base = normalize(&joined);
        if self.sources.contains(&base) {
            return Some(base);
        }
        // Extensionless: `x.ts` and `x/index.ts` (or `x.ts` and `x.tsx`) both
        // present is a resolver-order question this does not model, so it
        // needs exactly one probe to hit.
        let mut hits = SOURCE_EXTENSIONS
            .iter()
            .flat_map(|ext| [format!("{base}{ext}"), format!("{base}/index{ext}")])
            .filter(|probe| self.sources.contains(probe));
        match (hits.next(), hits.next()) {
            (Some(only), None) => Some(only),
            _ => None,
        }
    }
}

/// Build-output directory names a mirrored target drops before `src/`.
const BUILD_DIRS: [&str; 8] = ["dist", "esm", "cjs", "lib", "build", "out", "es", "module"];

/// A runner may prefer a sibling file or fall back to the directory index.
/// Include TypeScript loader extensions as well as Node's JSON and native
/// alternatives. At the workspace root we can inspect only its index.
fn competing_package_modules(dir: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    let module = normalize(dir);
    SOURCE_EXTENSIONS
        .into_iter()
        .chain([".json", ".node"])
        .flat_map(move |extension| {
            let index = dir.join(format!("index{extension}"));
            let sibling =
                (!module.is_empty()).then(|| PathBuf::from(format!("{module}{extension}")));
            std::iter::once(index).chain(sibling)
        })
}

/// Only ordinary package-local build paths have a source counterpart.
fn build_entry_stem(target: &str) -> Option<&str> {
    let path = target.strip_prefix("./").unwrap_or(target);
    let (dir, _) = path.split_once('/')?;
    if !BUILD_DIRS.contains(&dir)
        || path.contains(['\\', ':'])
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return None;
    }
    let (stem, ext) = path.rsplit_once('.')?;
    matches!(ext, "js" | "jsx" | "mjs" | "cjs").then_some(stem)
}

/// The `src/` counterpart of a JavaScript build-output target:
/// `./esm/middleware.mjs` → `src/middleware`, `./index.js` → `src/index`.
/// Declaration files and non-JavaScript targets have none.
fn src_mirror(target: &str) -> Option<String> {
    let path = target.trim_start_matches("./");
    let stem = [".mjs", ".cjs", ".js", ".jsx"]
        .iter()
        .find_map(|ext| path.strip_suffix(ext))?;
    let stem = match stem.split_once('/') {
        Some((first, rest)) if BUILD_DIRS.contains(&first) => rest,
        _ => stem,
    };
    Some(format!("src/{stem}"))
}

/// Every string target an `exports` value gives `subpath`, across all
/// conditions. `None` when `exports` does not export the subpath.
fn export_targets(exports: &serde_json::Value, subpath: &str) -> Option<Vec<String>> {
    let object = match exports {
        serde_json::Value::Object(object) => object,
        // `"exports": "./src/index.ts"` or an array exports only the root.
        other => return (subpath == ".").then(|| string_leaves(other)),
    };
    let has_subpath_keys = object.keys().any(|key| key.starts_with('.'));
    if !has_subpath_keys {
        // A conditions object for the root entry.
        return (subpath == ".").then(|| string_leaves(exports));
    }
    if let Some(value) = object.get(subpath) {
        return Some(string_leaves(value));
    }
    // Single-`*` pattern keys, longest prefix first (Node's ordering).
    let mut best: Option<(&str, String, &serde_json::Value)> = None;
    for (key, value) in object {
        let Some((prefix, suffix)) = key.split_once('*') else {
            continue;
        };
        if suffix.contains('*') {
            continue;
        }
        let Some(captured) = subpath
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_suffix(suffix))
        else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(current, _, _)| prefix.len() > current.len())
        {
            best = Some((prefix, captured.to_string(), value));
        }
    }
    let (_, captured, value) = best?;
    Some(
        string_leaves(value)
            .into_iter()
            .map(|target| target.replacen('*', &captured, 1))
            .collect(),
    )
}

fn string_leaves(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(target) => vec![target.clone()],
        serde_json::Value::Array(items) => items.iter().flat_map(string_leaves).collect(),
        serde_json::Value::Object(conditions) => {
            conditions.values().flat_map(string_leaves).collect()
        }
        _ => Vec::new(),
    }
}

fn normalize(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::language::typescript::tsconfig::TsAliasMap;

    fn packages(manifests: &[(&str, &str)], sources: &[&str]) -> Result<WorkspacePackages, String> {
        let root = super::super::tests::ts_unique_tempdir("workspace-packages")?;
        for (dir, manifest) in manifests {
            super::super::tests::ts_write_file(&root.join(dir).join("package.json"), manifest)?;
        }
        for source in sources {
            super::super::tests::ts_write_file(&root.join(source), "export {}\n")?;
        }
        let files: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
        Ok(WorkspacePackages::discover(&root, &files))
    }

    #[test]
    fn exports_subpath_with_source_condition_resolves_to_source() -> Result<(), String> {
        let packages = packages(
            &[(
                "packages/utils",
                r#"{"name":"@vitest/utils","exports":{
                    ".":{"__vitest_source__":"./src/index.ts","default":"./dist/index.js"},
                    "./helpers":{"__vitest_source__":"./src/helpers.ts","types":"./dist/helpers.d.ts","default":"./dist/helpers.js"}
                }}"#,
            )],
            &[
                "packages/utils/src/index.ts",
                "packages/utils/src/helpers.ts",
            ],
        )?;
        assert_eq!(
            packages.resolve("@vitest/utils/helpers"),
            Some(PathBuf::from("packages/utils/src/helpers.ts"))
        );
        assert_eq!(
            packages.resolve("@vitest/utils"),
            Some(PathBuf::from("packages/utils/src/index.ts"))
        );
        // Not exported: `exports` blocks deep imports.
        assert_eq!(packages.resolve("@vitest/utils/src/helpers"), None);
        // Another package's name that only shares a prefix.
        assert_eq!(packages.resolve("@vitest/utils-extra"), None);
        Ok(())
    }

    #[test]
    fn extensionless_target_with_two_candidate_files_fails_closed() -> Result<(), String> {
        let packages = packages(
            &[(
                "packages/ui",
                r#"{"name":"ui","exports":{"./*":"./src/*"}}"#,
            )],
            &[
                "packages/ui/src/foo.tsx",
                "packages/ui/src/foo/index.ts",
                "packages/ui/src/bar.ts",
            ],
        )?;
        assert_eq!(packages.resolve("ui/foo"), None);
        assert_eq!(
            packages.resolve("ui/bar"),
            Some(PathBuf::from("packages/ui/src/bar.ts"))
        );
        Ok(())
    }

    /// A specifier a tsconfig `paths` key owns never falls back to the
    /// package's own entry, even when the key's substitutions cannot be
    /// resolved (review #4611): the compiler would use `paths`. Here the
    /// first fallback target is a template this map does not support, so
    /// the walk never reaches the second.
    #[test]
    fn paths_key_owning_specifier_blocks_package_fallback() -> Result<(), String> {
        let root = super::super::tests::ts_unique_tempdir("workspace-packages-paths")?;
        super::super::tests::ts_write_file(
            &root.join("packages/utils/package.json"),
            r#"{"name":"@acme/utils","exports":{".":"./src/index.ts"}}"#,
        )?;
        super::super::tests::ts_write_file(
            &root.join("packages/utils/src/index.ts"),
            "export {}\n",
        )?;
        super::super::tests::ts_write_file(
            &root.join("packages/utils-v2/src/index.ts"),
            "export {}\n",
        )?;
        super::super::tests::ts_write_file(
            &root.join("tsconfig.json"),
            r#"{"compilerOptions":{"baseUrl":".","paths":{"@acme/utils":["packages/*/*/src/index.ts","packages/utils/src/index.ts"]}}}"#,
        )?;
        let files = vec![
            PathBuf::from("packages/utils/src/index.ts"),
            PathBuf::from("packages/utils-v2/src/index.ts"),
        ];
        let packages = WorkspacePackages::discover(&root, &files);
        assert_eq!(
            packages.resolve("@acme/utils"),
            Some(PathBuf::from("packages/utils/src/index.ts"))
        );
        let map = super::super::tsconfig::load_alias_map(&root)
            .ok_or("tsconfig.json should load")?
            .with_workspace_packages(packages.clone());
        assert_eq!(map.resolve("@acme/utils"), None);
        // Without a loaded tsconfig the package entry applies.
        let unrelated = TsAliasMap::workspace_packages_only(&root, packages);
        assert_eq!(
            unrelated.resolve("@acme/utils"),
            Some(PathBuf::from("packages/utils/src/index.ts"))
        );
        Ok(())
    }

    #[test]
    fn build_output_targets_mirror_into_src_or_fail_closed() -> Result<(), String> {
        let packages = packages(
            &[
                (
                    "packages/core",
                    r#"{"name":"core","main":"./dist/index.js","types":"./dist/index.d.ts"}"#,
                ),
                (
                    "packages/bundle",
                    r#"{"name":"bundle","main":"./dist/bundle.js"}"#,
                ),
            ],
            &["packages/core/src/index.ts", "packages/bundle/src/index.ts"],
        )?;
        assert_eq!(
            packages.resolve("core"),
            Some(PathBuf::from("packages/core/src/index.ts"))
        );
        // The mirrored file does not exist: no guess at another source.
        assert_eq!(packages.resolve("bundle"), None);
        Ok(())
    }

    /// #4769: zustand's own tests import `zustand/middleware`; the root
    /// manifest's `./*` export names only published build output, which
    /// mirrors to `src/middleware.ts`.
    #[test]
    fn self_import_through_build_output_pattern_resolves_to_src() -> Result<(), String> {
        let packages = packages(
            &[(
                ".",
                r#"{"name":"zustand","exports":{
                    "./package.json":"./package.json",
                    ".":{"import":{"types":"./esm/index.d.mts","default":"./esm/index.mjs"},"default":{"types":"./index.d.ts","default":"./index.js"}},
                    "./*":{"import":{"types":"./esm/*.d.mts","default":"./esm/*.mjs"},"default":{"types":"./*.d.ts","default":"./*.js"}}
                }}"#,
            )],
            &[
                "src/index.ts",
                "src/middleware.ts",
                "src/middleware/devtools.ts",
                "tests/devtools.test.tsx",
            ],
        )?;
        assert_eq!(
            packages.resolve("zustand/middleware"),
            Some(PathBuf::from("src/middleware.ts"))
        );
        assert_eq!(
            packages.resolve("zustand"),
            Some(PathBuf::from("src/index.ts"))
        );
        assert_eq!(
            packages.resolve("zustand/middleware/devtools"),
            Some(PathBuf::from("src/middleware/devtools.ts"))
        );
        assert_eq!(packages.resolve("zustand/missing"), None);
        Ok(())
    }

    #[test]
    fn main_and_pattern_exports_and_deep_paths_resolve() -> Result<(), String> {
        let packages = packages(
            &[
                ("packages/a", r#"{"name":"a","main":"src/index.ts"}"#),
                (
                    "packages/b",
                    r#"{"name":"b","exports":{"./*":"./src/*.ts"}}"#,
                ),
            ],
            &[
                "packages/a/src/index.ts",
                "packages/a/src/deep.ts",
                "packages/b/src/format.ts",
            ],
        )?;
        assert_eq!(
            packages.resolve("a"),
            Some(PathBuf::from("packages/a/src/index.ts"))
        );
        assert_eq!(
            packages.resolve("a/src/deep"),
            Some(PathBuf::from("packages/a/src/deep.ts"))
        );
        assert_eq!(
            packages.resolve("b/format"),
            Some(PathBuf::from("packages/b/src/format.ts"))
        );
        assert_eq!(packages.resolve("b/missing"), None);
        Ok(())
    }

    #[test]
    fn duplicate_names_and_divergent_targets_fail_closed() -> Result<(), String> {
        let duplicate = packages(
            &[
                ("packages/a", r#"{"name":"same","main":"src/index.ts"}"#),
                ("packages/b", r#"{"name":"same","main":"src/index.ts"}"#),
            ],
            &["packages/a/src/index.ts", "packages/b/src/index.ts"],
        )?;
        assert_eq!(duplicate.resolve("same"), None);

        let divergent = packages(
            &[(
                "packages/c",
                r#"{"name":"c","exports":{".":{"import":"./src/esm.ts","require":"./src/cjs.ts"}}}"#,
            )],
            &["packages/c/src/esm.ts", "packages/c/src/cjs.ts"],
        )?;
        assert_eq!(divergent.resolve("c"), None);

        let escaping = packages(
            &[("packages/d", r#"{"name":"d","main":"../a/src/index.ts"}"#)],
            &["packages/d/src/index.ts", "packages/a/src/index.ts"],
        )?;
        assert_eq!(escaping.resolve("d"), None);
        Ok(())
    }
}
