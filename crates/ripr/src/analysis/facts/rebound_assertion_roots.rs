//! Unknown strength for a `std::` or `core::` assertion whose root the
//! test's own manifest binds to another package (#10150).
//!
//! The parser admits `std::assert_eq!` and `core::assert!` after file-local
//! rebinding checks only. Cargo can still bind the root name to a different
//! package: `std = { package = "fakestd" }` in a `#![no_std]` crate, or
//! `core = { package = .. }` in an ordinary crate, makes `std::assert_eq!` or
//! `core::assert_eq!` whatever that package exports, which may discard its
//! operands. Owner-pin admission already refuses such a root for the read
//! seam through the strict manifest proof; every other consumer of the
//! test's assertion facts (a constructor credited through a direct call in
//! the operand, for one) read the parsed oracle as a real assertion.
//!
//! This pass marks those oracles `unknown` after the index is built, and only
//! when the manifest of the test's compilation unit declares a dependency
//! under the root name. It does not drop them:
//! the substituted package may still assert (a facade over the real `std`),
//! so "no assertion here" would be as unsupported as "exact value". An
//! unread or unusual manifest keeps the parsed oracle, because withdrawing a
//! genuine assertion turns a gripped seam into an actionable gap, which is
//! the worse error.

use std::collections::BTreeMap;
use std::path::Path;

use super::drop_in::normalized;
use super::model::RustIndex;
use crate::domain::{OracleKind, OracleStrength};

/// Marks `unknown` each test assertion written through a crate-qualified
/// root that the manifest of the test's compilation unit binds to a package.
/// A test file mounted by a resolved literal `include!` is judged by the
/// including file's manifest, because the including crate compiles it.
pub(super) fn mark_rebound_root_assertions_unknown(index: &mut RustIndex) {
    let Some(root) = index.drop_in_manifests.root().map(Path::to_path_buf) else {
        return;
    };
    let mut verdicts = BTreeMap::new();
    for test in index.tests().iter() {
        for oracle in &test.assertions {
            let Some(krate) = qualified_root(&oracle.text) else {
                continue;
            };
            let unit =
                super::compilation_unit_path_from_parents(&index.include_parents, &test.file);
            verdicts
                .entry((test.file.clone(), krate))
                .or_insert_with(|| manifest_rebinds_root(&root, &unit, krate));
        }
    }
    if !verdicts.values().any(|rebound| *rebound) {
        return;
    }
    index.for_each_test_mut(|test| {
        let file = test.file.clone();
        for oracle in &mut test.assertions {
            let rebound = qualified_root(&oracle.text)
                .and_then(|krate| verdicts.get(&(file.clone(), krate)))
                .copied()
                .unwrap_or(false);
            if rebound {
                oracle.kind = OracleKind::Unknown;
                oracle.strength = OracleStrength::Unknown;
                oracle.observed_tokens.clear();
            }
        }
    });
}

/// The crate root of an assertion written `std::leaf!(..)`, `core::leaf!(..)`
/// or their `::`-rooted forms.
fn qualified_root(text: &str) -> Option<&'static str> {
    let text = text.trim_start();
    let text = text.strip_prefix("::").unwrap_or(text);
    let (root, rest) = text.split_once("::")?;
    let root = root.trim();
    let leaf_end = rest.find('!')?;
    let leaf = rest[..leaf_end].trim();
    if leaf.is_empty() || !leaf.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    ["std", "core"].into_iter().find(|known| *known == root)
}

/// Dependency tables whose crates a library, binary or test target can name.
/// `build-dependencies` reach only the build script, so they are left out.
const TARGET_DEPENDENCY_TABLES: &[&str] = &["dependencies", "dev-dependencies", "dev_dependencies"];

/// Whether the nearest `Cargo.toml` at or above `file` (within `root`) declares
/// a dependency keyed by the standard crate name `krate`, in a table the
/// file's targets can see: `[dependencies]`, `[dev-dependencies]`, or their
/// `[target.'cfg(..)'.*]` forms. The real `std` and `core` are never Cargo
/// dependencies, so the key alone means the name may be some other package. A
/// platform or optional dependency may be inactive for a given build; the
/// assertion then reads `unknown`, which states that uncertainty without
/// making the seam actionable. An unparsable nearest manifest decides nothing.
fn manifest_rebinds_root(root: &Path, file: &Path, krate: &str) -> bool {
    let Some(directory) = root.join(file).parent().map(Path::to_path_buf) else {
        return false;
    };
    let Some(text) = directory
        .ancestors()
        .take_while(|dir| dir.starts_with(root))
        .find_map(|dir| std::fs::read_to_string(dir.join("Cargo.toml")).ok())
    else {
        return false;
    };
    let Ok(manifest) = text.parse::<toml::Table>() else {
        return false;
    };
    let declares = |table: &toml::Table| {
        TARGET_DEPENDENCY_TABLES.iter().any(|name| {
            table
                .get(*name)
                .and_then(toml::Value::as_table)
                .is_some_and(|deps| deps.keys().any(|key| normalized(key) == krate))
        })
    };
    declares(&manifest)
        || manifest
            .get("target")
            .and_then(toml::Value::as_table)
            .is_some_and(|targets| {
                targets
                    .values()
                    .filter_map(toml::Value::as_table)
                    .any(declares)
            })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_root_reads_only_admitted_crate_paths() {
        assert_eq!(
            qualified_root("std::assert_eq!(Widget::new().read(), 4)"),
            Some("std")
        );
        assert_eq!(qualified_root("  ::core::assert!(ok)"), Some("core"));
        assert_eq!(qualified_root("pretty_assertions::assert_eq!(a, b)"), None);
        assert_eq!(
            qualified_root("assert_eq!(std::mem::size_of::<u8>(), 1)"),
            None
        );
        assert_eq!(qualified_root("other::assert_eq!(a, b)"), None);
        assert_eq!(qualified_root("std::mem::take(&mut x)"), None);
    }

    fn workspace(label: &str, files: &[(&str, &str)]) -> Result<std::path::PathBuf, String> {
        super::super::drop_in::temp_workspace(label, files)
    }

    #[test]
    fn manifest_rebinding_needs_a_declared_rename() -> Result<(), String> {
        let plain = "[package]\nname = \"a\"\nversion = \"0.1.0\"\n";
        let cases: &[(&str, &str, &str, bool)] = &[
            (
                "std-renamed",
                "[dependencies]\nstd = { package = \"fakestd\", path = \"../f\" }\n",
                "std",
                true,
            ),
            (
                "core-renamed",
                "[dev-dependencies]\ncore = { package = \"fakecore\", path = \"../f\" }\n",
                "core",
                true,
            ),
            (
                "core-target",
                "[target.'cfg(unix)'.dependencies]\ncore = { path = \"../f\" }\n",
                "core",
                true,
            ),
            ("std-plain", "[dependencies]\nserde = \"1\"\n", "std", false),
            (
                "core-build-only",
                "[build-dependencies]\ncore = { package = \"fakecore\", path = \"../f\" }\n",
                "core",
                false,
            ),
            (
                "core-target-build-only",
                "[target.'cfg(unix)'.build-dependencies]\ncore = { path = \"../f\" }\n",
                "core",
                false,
            ),
            (
                "other-key-renamed",
                "[dependencies]\nother = { package = \"fakestd\", path = \"../f\" }\n",
                "std",
                false,
            ),
        ];
        for (label, extra, krate, expected) in cases {
            let manifest = format!("{plain}{extra}");
            let root = workspace(label, &[("Cargo.toml", &manifest), ("src/lib.rs", "")])?;
            let verdict = manifest_rebinds_root(&root, Path::new("src/lib.rs"), krate);
            let _ = std::fs::remove_dir_all(&root);
            if verdict != *expected {
                return Err(format!(
                    "{label}: {krate} rebound = {verdict}, expected {expected}"
                ));
            }
        }
        Ok(())
    }

    #[test]
    fn only_the_nearest_manifest_declares_the_root() -> Result<(), String> {
        let member = "[package]\nname = \"a\"\nversion = \"0.1.0\"\n";
        let cases: &[(&str, &str, &str, bool)] = &[
            (
                "patch-only",
                "[workspace]\nmembers = [\"a\"]\n[patch.crates-io]\nstd = { path = \"f\" }\n",
                member,
                false,
            ),
            (
                "workspace-dependency-unused",
                "[workspace]\nmembers = [\"a\"]\n[workspace.dependencies]\nstd = { package = \"f\", path = \"f\" }\n",
                member,
                false,
            ),
            (
                "member-declares",
                "[workspace]\nmembers = [\"a\"]\n",
                "[package]\nname = \"a\"\nversion = \"0.1.0\"\n[dev-dependencies]\nstd = { package = \"f\", path = \"../f\" }\n",
                true,
            ),
            (
                "member-unparsable",
                "[workspace]\nmembers = [\"a\"]\n[dependencies]\nstd = { path = \"f\" }\n",
                "[package\n",
                false,
            ),
        ];
        for (label, workspace_manifest, member_manifest, expected) in cases {
            let root = workspace(
                label,
                &[
                    ("Cargo.toml", workspace_manifest),
                    ("a/Cargo.toml", member_manifest),
                    ("a/tests/t.rs", ""),
                ],
            )?;
            let verdict = manifest_rebinds_root(&root, Path::new("a/tests/t.rs"), "std");
            let _ = std::fs::remove_dir_all(&root);
            if verdict != *expected {
                return Err(format!(
                    "{label}: std rebound = {verdict}, expected {expected}"
                ));
            }
        }
        Ok(())
    }
}
