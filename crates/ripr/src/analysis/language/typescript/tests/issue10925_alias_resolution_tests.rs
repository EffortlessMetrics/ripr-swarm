//! Issue #10925: a non-relative import specifier resolves against one
//! immutable alias map for every owner x test x import combination (about
//! 65k calls over ~200 distinct specifiers on zod). Inside an analysis run the
//! answer is computed once per `(alias map, specifier)`; it must equal the
//! uncached answer for every shape and never outlive the run.

use super::related_tests::{
    ALIAS_RESOLUTIONS_COMPUTED, DirectoryModuleCacheScope, normalized_relative_import_module,
};
use super::tsconfig::{TsAliasMap, load_alias_map};
use super::workspace_packages::WorkspacePackages;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn computed() -> usize {
    ALIAS_RESOLUTIONS_COMPUTED.with(std::cell::Cell::get)
}

fn temp_dir(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("ripr-alias-memo-{label}-{stamp}"));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn write(dir: &Path, name: &str, content: &str) {
    if let Some(parent) = PathBuf::from(name).parent() {
        let _ = fs::create_dir_all(dir.join(parent));
    }
    let _ = fs::write(dir.join(name), content);
}

/// A workspace with a `paths` alias, a workspace package and plain files.
fn fixture(label: &str) -> Result<(PathBuf, TsAliasMap), String> {
    let root = temp_dir(label);
    write(
        &root,
        "tsconfig.json",
        r#"{"compilerOptions":{"baseUrl":".","paths":{"@/*":["src/*"],"exact":["src/a.ts"]}}}"#,
    );
    write(&root, "src/a.ts", "export const a = 1;\n");
    write(&root, "src/b.ts", "export const b = 1;\n");
    write(&root, "src/b.tsx", "export const b = 2;\n");
    write(
        &root,
        "pkgs/core/package.json",
        r#"{"name":"core-pkg","main":"src/index.ts"}"#,
    );
    write(&root, "pkgs/core/src/index.ts", "export const c = 1;\n");
    let files = [
        PathBuf::from("pkgs/core/src/index.ts"),
        PathBuf::from("src/a.ts"),
    ];
    let packages = WorkspacePackages::discover(&root, &files);
    let map = load_alias_map(&root)
        .ok_or("tsconfig must load")?
        .with_workspace_packages(packages);
    Ok((root, map))
}

const SPECIFIERS: &[&str] = &[
    "@/a",          // paths glob, unique file
    "@/b",          // paths glob, ambiguous (b.ts and b.tsx): unresolved
    "@/missing",    // paths glob, no file
    "exact",        // literal paths key
    "core-pkg",     // workspace package entry
    "core-pkg/src", // workspace package subpath
    "lodash",       // not an alias at all
    "./relative",   // relative: never consults the alias map
];

fn resolve_all(root: &Path, map: &TsAliasMap) -> Vec<Option<String>> {
    SPECIFIERS
        .iter()
        .map(|specifier| {
            normalized_relative_import_module(
                Path::new("tests/x.test.ts"),
                specifier,
                Some(map),
                Some(root),
            )
        })
        .collect()
}

#[test]
fn cached_answers_equal_uncached_answers_for_every_specifier() -> Result<(), String> {
    let (root, map) = fixture("differential")?;
    let uncached = resolve_all(&root, &map);
    let scoped = {
        let _scope = DirectoryModuleCacheScope::open();
        // Twice: the second pass is served from the memo.
        let first = resolve_all(&root, &map);
        let second = resolve_all(&root, &map);
        assert_eq!(first, second, "memo hit must equal the computed answer");
        second
    };
    let _ = fs::remove_dir_all(&root);
    assert_eq!(scoped, uncached);
    // Discriminate: the fixture must produce both resolved and unresolved
    // answers, otherwise equality proves nothing.
    assert!(
        uncached.iter().filter(|answer| answer.is_some()).count() >= 5,
        "fixture must resolve several specifiers: {uncached:?}"
    );
    assert_eq!(uncached[1], None, "ambiguous @/b stays unresolved");
    assert_eq!(uncached[2], None, "missing file stays unresolved");
    assert_eq!(uncached[6], None, "unknown package stays unresolved");
    Ok(())
}

#[test]
fn a_run_computes_each_specifier_once() -> Result<(), String> {
    let (root, map) = fixture("counter")?;
    let before = computed();
    {
        let _scope = DirectoryModuleCacheScope::open();
        for _ in 0..50 {
            resolve_all(&root, &map);
        }
    }
    let in_scope = computed() - before;
    let before = computed();
    for _ in 0..3 {
        resolve_all(&root, &map);
    }
    let outside = computed() - before;
    let _ = fs::remove_dir_all(&root);
    // Seven non-relative specifiers; `./relative` never reaches the alias map.
    assert_eq!(in_scope, 7, "one computation per distinct specifier");
    assert_eq!(outside, 21, "outside a run every call resolves afresh");
    Ok(())
}

#[test]
fn the_memo_does_not_outlive_the_run() -> Result<(), String> {
    let (root, map) = fixture("lifetime")?;
    let first = {
        let _scope = DirectoryModuleCacheScope::open();
        resolve_all(&root, &map)
    };
    // The filesystem changes between runs: `@/missing` now exists.
    write(&root, "src/missing.ts", "export const m = 1;\n");
    let second = {
        let _scope = DirectoryModuleCacheScope::open();
        resolve_all(&root, &map)
    };
    let _ = fs::remove_dir_all(&root);
    assert_eq!(first[2], None);
    assert_eq!(second[2].as_deref(), Some("src/missing"));
    Ok(())
}

#[test]
fn two_alias_maps_in_one_run_never_answer_for_each_other() -> Result<(), String> {
    let (root_a, map_a) = fixture("map-a")?;
    let (root_b, map_b) = fixture("map-b")?;
    // Same specifiers, different roots/tables: only the digest tells them apart.
    write(&root_b, "src/missing.ts", "export const m = 1;\n");
    let _scope = DirectoryModuleCacheScope::open();
    let a = resolve_all(&root_a, &map_a);
    let b = resolve_all(&root_b, &map_b);
    let a_again = resolve_all(&root_a, &map_a);
    drop(_scope);
    let _ = fs::remove_dir_all(&root_a);
    let _ = fs::remove_dir_all(&root_b);
    assert_eq!(a[2], None);
    assert_eq!(b[2].as_deref(), Some("src/missing"));
    assert_eq!(a, a_again);
    Ok(())
}
