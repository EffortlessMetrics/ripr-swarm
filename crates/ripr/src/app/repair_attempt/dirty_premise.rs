//! Policy-free conservative preflight for the before-phase dirty-production
//! premise refusal (#11458).
//!
//! The exact check ([`super::trusted_head_surface_violations`]) needs the
//! packet-derived `EditCagePolicy`, and the packet needs the full classified
//! seam inventory. When the premise is already unrepairable, that inventory
//! is minutes of wasted analysis. This module answers one narrower question
//! before the inventory: is every tracked path that differs from HEAD
//! certainly outside any seam's repair target and outside every operational
//! write? Any doubt returns `None` and the exact check remains the backstop.

use std::path::Path;

use super::git_paths;

/// Tracked paths that differ from `head`, when every one of them is
/// certainly a violation of the trusted surface for every possible seam.
///
/// All-or-nothing on purpose: the refusal text lists the exact check's
/// violation list verbatim, so it is only byte-identical when each dirty
/// path is a certain violation. One uncertain path (a possible test target,
/// an operational write, an unreadable file) or any git failure returns
/// `None` and defers the whole decision to the exact check.
pub(crate) fn conservative_dirty_premise_violations(
    root: &Path,
    store: Option<&Path>,
    head: &str,
) -> Option<Vec<String>> {
    let tracked = git_paths(
        root,
        &[
            "diff",
            "--no-renames",
            "--no-ext-diff",
            "--name-only",
            "-z",
            head,
            "--",
        ],
    )
    .ok()?;
    if tracked.is_empty() {
        return None;
    }
    let store_prefix = store.map(|store| {
        store
            .strip_prefix(root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| store.to_path_buf())
    });
    tracked
        .iter()
        .all(|path| certainly_not_a_repair_target(root, store_prefix.as_deref(), path))
        .then_some(tracked)
}

fn certainly_not_a_repair_target(root: &Path, store_prefix: Option<&Path>, path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    // Operational writes the exact policy admits: the ripr workflow subtree,
    // Cargo build output and the workspace lockfile (Rust repairs), and an
    // explicit store.
    if normalized == "target"
        || normalized.starts_with("target/")
        || normalized == "Cargo.lock"
        || store_prefix.is_some_and(|prefix| Path::new(&normalized).starts_with(prefix))
    {
        return false;
    }
    if crate::analysis::is_test_surface_path(&normalized)
        || test_shaped_by_other_convention(&normalized)
    {
        return false;
    }
    // A Rust file may still be the selected target as its one governed inline
    // `#[cfg(test)]` module (#5210). Only a file readable as text and carrying
    // no `cfg(...test...)` marker can be excluded; unreadable means unsure.
    if Path::new(&normalized).extension() == Some(std::ffi::OsStr::new("rs")) {
        return match std::fs::read_to_string(root.join(&normalized)) {
            Ok(source) => !mentions_cfg_test(&source),
            Err(_) => false,
        };
    }
    true
}

/// Test-file conventions the shared surface recognizer does not (or, with a
/// feature off, may not) cover. The preflight never early-refuses these, so
/// it cannot over-credit a path the exact policy would allow.
fn test_shaped_by_other_convention(normalized: &str) -> bool {
    let file_name = normalized.rsplit('/').next().unwrap_or_default();
    file_name == "conftest.py"
        || file_name.contains(".spec.")
        || file_name.contains(".test.")
        || file_name.contains(".cy.")
        || normalized.split('/').any(|part| part == "__tests__")
}

fn mentions_cfg_test(source: &str) -> bool {
    source
        .lines()
        .any(|line| line.contains("cfg(") && line.contains("test"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture_git::fixture_git_ok as run_git;

    fn dirty_repo(label: &str) -> Result<std::path::PathBuf, String> {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-dirty-premise-{label}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        std::fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
        for (path, text) in [
            ("src/lib.rs", "pub fn a() {}\n"),
            (
                "src/inline.rs",
                "pub fn b() {}\n#[cfg(test)]\nmod tests {}\n",
            ),
            ("tests/t.rs", "#[test]\nfn t() {}\n"),
            ("web/a.spec.ts", "test('a', () => {});\n"),
            ("py/conftest.py", "import pytest\n"),
            ("web/prod.ts", "export const a = 1;\n"),
            ("Cargo.lock", "# lock\n"),
        ] {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::write(&full, text).map_err(|error| error.to_string())?;
        }
        run_git(&root, &["init", "-q"])?;
        run_git(&root, &["add", "."])?;
        run_git(
            &root,
            &[
                "-c",
                "user.name=RIPR test",
                "-c",
                "user.email=ripr@example.invalid",
                "commit",
                "--no-gpg-sign",
                "-qm",
                "base",
            ],
        )?;
        Ok(root)
    }

    fn touch(root: &Path, path: &str) -> Result<(), String> {
        let full = root.join(path);
        let mut text = std::fs::read_to_string(&full).map_err(|error| error.to_string())?;
        text.push_str("\n// dirty\n");
        std::fs::write(&full, text).map_err(|error| error.to_string())
    }

    fn preflight(root: &Path) -> Result<Option<Vec<String>>, String> {
        let head = crate::agent::artifact::current_git_head(root)?;
        Ok(conservative_dirty_premise_violations(root, None, &head))
    }

    #[test]
    fn dirty_production_files_are_named_and_match_the_exact_check() -> Result<(), String> {
        let root = dirty_repo("positive")?;
        let result = (|| {
            touch(&root, "src/lib.rs")?;
            touch(&root, "web/prod.ts")?;
            let named = preflight(&root)?;
            let expected = Some(vec!["src/lib.rs".to_string(), "web/prod.ts".to_string()]);
            if named != expected {
                return Err(format!("preflight named {named:?}, wanted {expected:?}"));
            }
            // Subset property: the exact check refuses every named path.
            let head = crate::agent::artifact::current_git_head(&root)?;
            let policy = super::super::edit_cage_policy_from_packet(
                r#"{"seam_id":"s","allowed_edit_surface":["tests/t.rs"],"forbidden_files":[]}"#,
                "s",
            )?;
            let exact = super::super::trusted_head_surface_violations(&root, &policy, &[], &head)?;
            if Some(exact) != named {
                return Err("exact check disagrees with the preflight".to_string());
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn every_uncertain_dirty_path_defers_to_the_exact_check() -> Result<(), String> {
        for path in [
            "tests/t.rs",
            "src/inline.rs",
            "web/a.spec.ts",
            "py/conftest.py",
            "Cargo.lock",
        ] {
            let root = dirty_repo("negative")?;
            let result = (|| {
                touch(&root, path)?;
                if let Some(named) = preflight(&root)? {
                    return Err(format!("`{path}` alone must defer, got {named:?}"));
                }
                // A production edit beside an uncertain path still defers, so
                // the refusal text can never omit or add a path.
                touch(&root, "src/lib.rs")?;
                if let Some(named) = preflight(&root)? {
                    return Err(format!(
                        "`{path}` plus production must defer, got {named:?}"
                    ));
                }
                Ok(())
            })();
            let _ = std::fs::remove_dir_all(&root);
            result?;
        }
        Ok(())
    }

    #[test]
    fn clean_trees_git_failures_and_unreadable_files_defer() -> Result<(), String> {
        let root = dirty_repo("deferral")?;
        let result = (|| {
            if preflight(&root)?.is_some() {
                return Err("a clean tree has nothing to refuse".to_string());
            }
            // An unreadable (deleted) Rust file is not certain.
            std::fs::remove_file(root.join("src/lib.rs")).map_err(|error| error.to_string())?;
            if preflight(&root)?.is_some() {
                return Err("an unreadable file must defer".to_string());
            }
            // A bogus head is a git failure, never a refusal.
            if conservative_dirty_premise_violations(&root, None, "not-a-revision").is_some() {
                return Err("a git failure must defer".to_string());
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }

    #[test]
    fn an_explicit_store_path_is_an_operational_write() -> Result<(), String> {
        let root = dirty_repo("store")?;
        let result = (|| {
            std::fs::create_dir_all(root.join("store")).map_err(|error| error.to_string())?;
            std::fs::write(root.join("store/m.json"), "{}").map_err(|error| error.to_string())?;
            run_git(&root, &["add", "store/m.json"])?;
            run_git(
                &root,
                &[
                    "-c",
                    "user.name=RIPR test",
                    "-c",
                    "user.email=ripr@example.invalid",
                    "commit",
                    "--no-gpg-sign",
                    "-qm",
                    "store",
                ],
            )?;
            std::fs::write(root.join("store/m.json"), "{\"a\":1}")
                .map_err(|error| error.to_string())?;
            let head = crate::agent::artifact::current_git_head(&root)?;
            let with_store =
                conservative_dirty_premise_violations(&root, Some(&root.join("store")), &head);
            let without = conservative_dirty_premise_violations(&root, None, &head);
            if with_store.is_some() || without != Some(vec!["store/m.json".to_string()]) {
                return Err(format!(
                    "store handling wrong: {with_store:?} / {without:?}"
                ));
            }
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&root);
        result
    }
}
