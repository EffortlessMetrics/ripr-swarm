//! Names changed `trybuild` fixtures that ripr does not read as test oracles
//! (RIPR-SPEC-0347, #8999).
//!
//! A `trybuild::TestCases` runner compiles each `tests/ui/*.rs` file and
//! compares the compiler output with the sibling `.stderr` snapshot (a
//! compile-fail file) or runs the file (a pass file). Those files are
//! compile-time expectation fixtures, not `#[test]` functions with assertions
//! ripr can extract, so a diff that adds or edits them contributes no oracle
//! evidence. The disclosure is advisory text only: it never feeds
//! classification, the analysis outcome or any machine-readable output.
//!
//! A path under `tests/ui` alone is not enough; the owning crate's manifest
//! must also declare `trybuild`, so an unrelated `tests/ui` directory (a
//! browser UI suite, a snapshot folder) is never named.

use super::diff;
use std::collections::BTreeMap;
use std::path::{Component, Path};

const MAX_NAMED_PATHS: usize = 3;
/// Bound on manifest bytes read per owning crate. A larger manifest fails
/// closed (no disclosure) rather than being parsed.
const MAX_MANIFEST_BYTES: u64 = 1 << 20;
const FIXTURE_EXTENSIONS: &[&str] = &["rs", "stderr", "stdout"];
const DEPENDENCY_TABLES: &[&str] = &["dependencies", "dev-dependencies", "build-dependencies"];

/// The stderr line naming changed trybuild fixtures, or `None` when the diff
/// changed none. `root` is the workspace root the changed paths are relative
/// to. Pure over its inputs apart from reading owning-crate manifests.
pub(super) fn trybuild_fixture_disclosure_message(
    root: &Path,
    changed_files: &[diff::ChangedFile],
) -> Option<String> {
    let mut manifest_declares: BTreeMap<String, bool> = BTreeMap::new();
    let mut fixtures = Vec::new();
    for file in changed_files {
        let Some(crate_dir) = fixture_crate_dir(&file.path) else {
            continue;
        };
        let declares = *manifest_declares
            .entry(crate_dir.clone())
            .or_insert_with(|| {
                manifest_declares_trybuild(&root.join(&crate_dir).join("Cargo.toml"))
            });
        if declares {
            fixtures.push(file.path.to_string_lossy().replace('\\', "/"));
        }
    }
    if fixtures.is_empty() {
        return None;
    }
    let mut listed = fixtures[..fixtures.len().min(MAX_NAMED_PATHS)].join(", ");
    if fixtures.len() > MAX_NAMED_PATHS {
        listed.push_str(&format!(" and {} more", fixtures.len() - MAX_NAMED_PATHS));
    }
    Some(format!(
        "ripr: {} changed trybuild fixture file(s) under tests/ui were not analyzed as test \
         oracles: {listed}. trybuild fixtures are compile-time expectations (compiler output or \
         a pass-file run), not assertions ripr reads, so they add no oracle evidence and change \
         no finding; review them with `cargo test`.",
        fixtures.len()
    ))
}

/// The crate directory (relative, `/`-joined, empty for the root crate) owning
/// a `tests/ui/**` fixture path, or `None` when the path is not a fixture
/// file. Only the first `tests/ui` component pair counts, and the file must
/// sit below `ui` with a fixture extension.
fn fixture_crate_dir(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?;
    if !FIXTURE_EXTENSIONS.contains(&extension) {
        return None;
    }
    let components = path
        .components()
        .map(|component| match component {
            Component::Normal(name) => name.to_str().map(str::to_string),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    let at = components
        .windows(3)
        .position(|window| window[0] == "tests" && window[1] == "ui")?;
    // `windows(3)` guarantees a third component, so the file is below `ui`.
    Some(components[..at].join("/"))
}

/// Whether the manifest at `path` declares `trybuild` in a dependency table,
/// including target-specific tables. Missing, oversized or unparseable
/// manifests answer `false`: the disclosure only ever under-names.
fn manifest_declares_trybuild(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if metadata.len() > MAX_MANIFEST_BYTES {
        return false;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(manifest) = text.parse::<toml::Table>() else {
        return false;
    };
    let in_tables = |table: &toml::Table| {
        DEPENDENCY_TABLES.iter().any(|name| {
            table
                .get(*name)
                .and_then(toml::Value::as_table)
                .is_some_and(|deps| deps.contains_key("trybuild"))
        })
    };
    if in_tables(&manifest) {
        return true;
    }
    manifest
        .get("target")
        .and_then(toml::Value::as_table)
        .is_some_and(|targets| {
            targets
                .values()
                .filter_map(toml::Value::as_table)
                .any(in_tables)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("ripr-trybuild-{name}-{stamp}"));
        fs::create_dir_all(&root).map_err(|err| format!("create temp root failed: {err}"))?;
        Ok(root)
    }

    fn write(path: &Path, text: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("create parent failed: {err}"))?;
        }
        fs::write(path, text).map_err(|err| format!("write {} failed: {err}", path.display()))
    }

    fn changed(path: &str) -> diff::ChangedFile {
        diff::ChangedFile {
            path: PathBuf::from(path),
            ..diff::ChangedFile::default()
        }
    }

    const TRYBUILD_MANIFEST: &str =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\ntrybuild = \"1\"\n";
    const PLAIN_MANIFEST: &str =
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[dev-dependencies]\ninsta = \"1\"\n";

    #[test]
    fn trybuild_crate_fixtures_are_named_with_their_paths() -> Result<(), String> {
        let root = temp_root("named")?;
        write(&root.join("Cargo.toml"), TRYBUILD_MANIFEST)?;
        let files = vec![
            changed("src/lib.rs"),
            changed("tests/ui/fail_len.rs"),
            changed("tests/ui/fail_len.stderr"),
        ];
        let message = trybuild_fixture_disclosure_message(&root, &files)
            .ok_or_else(|| "trybuild fixtures must be named".to_string())?;
        assert!(
            message.contains("2 changed trybuild fixture file(s)"),
            "{message}"
        );
        assert!(
            message.contains("tests/ui/fail_len.rs, tests/ui/fail_len.stderr"),
            "{message}"
        );
        assert!(!message.contains("src/lib.rs"), "{message}");
        Ok(())
    }

    #[test]
    fn member_crate_manifest_owns_its_fixtures() -> Result<(), String> {
        let root = temp_root("member")?;
        write(&root.join("Cargo.toml"), PLAIN_MANIFEST)?;
        write(&root.join("crates/macros/Cargo.toml"), TRYBUILD_MANIFEST)?;
        write(&root.join("crates/other/Cargo.toml"), PLAIN_MANIFEST)?;
        let files = vec![
            changed("crates/macros/tests/ui/pass.rs"),
            changed("crates/other/tests/ui/pass.rs"),
        ];
        let message = trybuild_fixture_disclosure_message(&root, &files)
            .ok_or_else(|| "the trybuild member must be named".to_string())?;
        assert!(
            message.contains("1 changed trybuild fixture file(s)"),
            "{message}"
        );
        assert!(
            message.contains("crates/macros/tests/ui/pass.rs"),
            "{message}"
        );
        assert!(!message.contains("crates/other"), "{message}");
        Ok(())
    }

    #[test]
    fn ui_directory_in_a_crate_without_trybuild_is_not_named() -> Result<(), String> {
        let root = temp_root("plain")?;
        write(&root.join("Cargo.toml"), PLAIN_MANIFEST)?;
        let files = vec![
            changed("tests/ui/snapshot.rs"),
            changed("tests/ui/snapshot.stderr"),
        ];
        assert_eq!(trybuild_fixture_disclosure_message(&root, &files), None);
        Ok(())
    }

    #[test]
    fn missing_or_unparseable_manifest_names_nothing() -> Result<(), String> {
        let root = temp_root("nomanifest")?;
        let files = vec![changed("tests/ui/a.rs")];
        assert_eq!(trybuild_fixture_disclosure_message(&root, &files), None);
        write(&root.join("Cargo.toml"), "this is = = not toml [")?;
        assert_eq!(trybuild_fixture_disclosure_message(&root, &files), None);
        Ok(())
    }

    #[test]
    fn target_specific_and_build_dependency_tables_count() -> Result<(), String> {
        let root = temp_root("target")?;
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[target.'cfg(unix)'.dev-dependencies]\ntrybuild = \"1\"\n",
        )?;
        let files = vec![changed("tests/ui/a.rs")];
        assert!(trybuild_fixture_disclosure_message(&root, &files).is_some());
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[build-dependencies]\ntrybuild = { workspace = true }\n",
        )?;
        assert!(trybuild_fixture_disclosure_message(&root, &files).is_some());
        Ok(())
    }

    #[test]
    fn trybuild_named_only_outside_dependency_tables_does_not_count() -> Result<(), String> {
        let root = temp_root("lookalike")?;
        write(
            &root.join("Cargo.toml"),
            "[package]\nname = \"trybuild\"\nversion = \"0.1.0\"\ndescription = \"trybuild\"\n\n[features]\ntrybuild = []\n",
        )?;
        let files = vec![changed("tests/ui/a.rs")];
        assert_eq!(trybuild_fixture_disclosure_message(&root, &files), None);
        Ok(())
    }

    #[test]
    fn only_fixture_files_below_tests_ui_count() -> Result<(), String> {
        let root = temp_root("shape")?;
        write(&root.join("Cargo.toml"), TRYBUILD_MANIFEST)?;
        for path in [
            "tests/ui.rs",
            "tests/ui/README.md",
            "tests/ui/notes.txt",
            "tests/uix/a.rs",
            "tests/other/ui/a.rs",
            "src/tests/ui/a.rs",
            "ui/a.rs",
        ] {
            assert_eq!(
                trybuild_fixture_disclosure_message(&root, &[changed(path)]),
                None,
                "{path}"
            );
        }
        let nested =
            trybuild_fixture_disclosure_message(&root, &[changed("tests/ui/sub/dir/a.stdout")]);
        assert!(
            nested.is_some(),
            "a nested fixture below tests/ui is a fixture"
        );
        Ok(())
    }

    #[test]
    fn long_fixture_lists_are_bounded() -> Result<(), String> {
        let root = temp_root("bounded")?;
        write(&root.join("Cargo.toml"), TRYBUILD_MANIFEST)?;
        let files = (0..6)
            .map(|index| changed(&format!("tests/ui/case_{index}.rs")))
            .collect::<Vec<_>>();
        let message = trybuild_fixture_disclosure_message(&root, &files)
            .ok_or_else(|| "fixtures must be named".to_string())?;
        assert!(
            message.contains("6 changed trybuild fixture file(s)"),
            "{message}"
        );
        assert!(message.contains("and 3 more"), "{message}");
        assert!(!message.contains("case_3.rs"), "{message}");
        Ok(())
    }
}
