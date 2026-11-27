//! A production module the crate root mounts through `#[path]` from outside
//! `src/` (#15264) seeds diff probes like the same module under `src/`. Before
//! the fix such a change read "no changed line is behavior ripr checks". A
//! file outside `src/` that no target mounts, and one only a test target
//! mounts, still seed nothing. Children of a `#[path]` file resolve beside
//! it, never under its stem directory (#15877).

use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str =
    "[package]\nname = \"price\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n";

const BULK: &str = "pub fn bulk(qty: u32) -> bool {\n    qty >= 10\n}\n";

const BULK_TEST: &str = "#[test]\nfn bulk_at_twenty() {\n    assert!(price::price::bulk(20));\n}\n";

fn predicate_diff(path: &str) -> String {
    format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -1,3 +1,3 @@\n pub fn bulk(qty: u32) -> bool {{\n-    qty > 10\n+    qty >= 10\n }}\n"
    )
}

struct Fixture(PathBuf);

impl Fixture {
    fn with_files(files: &[(&str, &str)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-path-module-outside-src-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        for (relative, text) in files {
            let path = fixture.0.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create directory {}: {error}", parent.display()))?;
            }
            std::fs::write(&path, text)
                .map_err(|error| format!("write file {}: {error}", path.display()))?;
        }
        Ok(fixture)
    }

    /// Changed files that produced at least one finding, root-relative.
    fn finding_files(&self) -> Result<Vec<String>, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            ..CheckInput::default()
        })?;
        let mut files = output
            .findings
            .iter()
            .map(|finding| {
                let file = &finding.probe.location.file;
                file.strip_prefix(&self.0)
                    .unwrap_or(file)
                    .to_string_lossy()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .to_string()
            })
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        Ok(files)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn seeds(files: &[String], path: &str) -> bool {
    files.iter().any(|file| Path::new(file) == Path::new(path))
}

#[test]
fn a_path_module_outside_src_seeds_like_one_under_src() -> Result<(), String> {
    // Control: the same module under `src/` already seeds.
    let under_src = Fixture::with_files(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#[path = \"impl/price.rs\"]\npub mod price;\n",
        ),
        ("src/impl/price.rs", BULK),
        ("tests/price.rs", BULK_TEST),
        ("diff.patch", &predicate_diff("src/impl/price.rs")),
    ])?;
    let files = under_src.finding_files()?;
    assert!(seeds(&files, "src/impl/price.rs"), "control: {files:?}");

    let outside_src = Fixture::with_files(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#[path = \"../impl/price.rs\"]\npub mod price;\n",
        ),
        ("impl/price.rs", BULK),
        ("tests/price.rs", BULK_TEST),
        ("diff.patch", &predicate_diff("impl/price.rs")),
    ])?;
    let files = outside_src.finding_files()?;
    assert!(
        seeds(&files, "impl/price.rs"),
        "the crate root mounts impl/price.rs, so its change must seed: {files:?}"
    );
    Ok(())
}

#[test]
fn an_unmounted_file_outside_src_still_seeds_nothing() -> Result<(), String> {
    // Discriminating alternate: the mounted sibling seeds, the file no
    // target mounts stays non-production evidence.
    let fixture = Fixture::with_files(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#[path = \"../impl/price.rs\"]\npub mod price;\n",
        ),
        ("impl/price.rs", BULK),
        ("impl/loose.rs", BULK),
        ("tests/price.rs", BULK_TEST),
        (
            "diff.patch",
            &format!(
                "{}{}",
                predicate_diff("impl/price.rs"),
                predicate_diff("impl/loose.rs")
            ),
        ),
    ])?;
    let files = fixture.finding_files()?;
    assert!(seeds(&files, "impl/price.rs"), "{files:?}");
    assert!(!seeds(&files, "impl/loose.rs"), "{files:?}");
    Ok(())
}

#[test]
fn a_file_only_a_test_target_mounts_is_not_production() -> Result<(), String> {
    let fixture = Fixture::with_files(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn unrelated() {}\n"),
        ("support/helper.rs", BULK),
        (
            "tests/helper.rs",
            "#[path = \"../support/helper.rs\"]\nmod helper;\n\n#[test]\nfn helper_bulk() {\n    assert!(helper::bulk(20));\n}\n",
        ),
        ("diff.patch", &predicate_diff("support/helper.rs")),
    ])?;
    let files = fixture.finding_files()?;
    assert!(!seeds(&files, "support/helper.rs"), "{files:?}");
    Ok(())
}

#[test]
fn a_path_module_child_resolves_beside_the_file_not_under_its_stem() -> Result<(), String> {
    // rustc makes a `#[path]` file a directory owner (#15877): `mod child;`
    // in `src/impl/price.rs` loaded through `#[path]` compiles
    // `src/impl/child.rs`. The same name under `src/impl/price/` is never
    // built, so a change there seeds nothing while the built sibling seeds.
    let fixture = Fixture::with_files(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "#[path = \"impl/price.rs\"]\npub mod price;\n",
        ),
        ("src/impl/price.rs", "pub mod child;\n"),
        ("src/impl/child.rs", BULK),
        ("src/impl/price/child.rs", BULK),
        (
            "diff.patch",
            &format!(
                "{}{}",
                predicate_diff("src/impl/child.rs"),
                predicate_diff("src/impl/price/child.rs")
            ),
        ),
    ])?;
    let files = fixture.finding_files()?;
    assert!(seeds(&files, "src/impl/child.rs"), "built child: {files:?}");
    assert!(
        !seeds(&files, "src/impl/price/child.rs"),
        "stem-directory file is never compiled: {files:?}"
    );
    Ok(())
}
