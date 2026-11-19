//! #15332: a `[[test]]` target whose manifest `path` sits outside `tests/`
//! is an integration-test crate, so its `my_pkg::owner(..)` call names the
//! package's own library just as a `tests/` file's does.
//!
//! The changed owner is `clamp_score` (`x > 100` to `x >= 100`), pinned by
//! `assert_eq!(my_pkg::clamp_score(100), 100)` in `checks/score_check.rs`.
//! Each case changes only the manifest, and asserts its generated fixture
//! (the finding sits on the changed line of `src/lib.rs`) before any claim.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const PACKAGE: &str = "[package]\nname = \"my-pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const OWNER: &str = "pub fn clamp_score(x: i32) -> i32 {\n    if x >= 100 { 100 } else { x }\n}\n";
const TEST: &str = "#[test]\nfn clamps_at_limit() {\n    assert_eq!(my_pkg::clamp_score(100), 100);\n    assert_eq!(my_pkg::clamp_score(101), 100);\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\nindex 0000000..1111111 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn clamp_score(x: i32) -> i32 {\n-    if x > 100 { 100 } else { x }\n+    if x >= 100 { 100 } else { x }\n }\n";

struct TempRepo {
    root: PathBuf,
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The finding for the changed `x >= 100` with `targets` appended to the
/// manifest and the test written at `test_path`.
fn finding(targets: &str, test_path: &str) -> Result<Finding, String> {
    finding_with(targets, test_path, &[])
}

/// [`finding`] with `extra` files written after the test, so one can
/// replace the file at `test_path`.
fn finding_with(targets: &str, test_path: &str, extra: &[(&str, &str)]) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo {
        root: std::env::temp_dir().join(format!(
            "ripr-declared-test-target-crate-name-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )),
    };
    let root = &repo.root;
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        let parent = path
            .parent()
            .ok_or_else(|| format!("no parent: {}", path.display()))?;
        std::fs::create_dir_all(parent).map_err(|error| format!("dir {path:?}: {error}"))?;
        std::fs::write(&path, text).map_err(|error| format!("write {path:?}: {error}"))
    };
    write("Cargo.toml", &format!("{PACKAGE}{targets}"))?;
    write("src/lib.rs", OWNER)?;
    write(test_path, TEST)?;
    for (path, text) in extra {
        write(path, text)?;
    }
    write("diff.patch", DIFF)?;
    let output = check_workspace(CheckInput {
        root: root.clone(),
        base: None,
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Draft,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
    .map_err(|error| format!("check: {error}"))?;
    let found = output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.location.line == 2 && finding.probe.expression.contains("x >= 100")
        })
        .ok_or_else(|| "no finding for the changed `x >= 100`".to_string())?;
    let file = normalized(&found.probe.location.file);
    if !(file == "src/lib.rs" || file.ends_with("/src/lib.rs")) {
        return Err(format!("finding sits in {file}, not src/lib.rs"));
    }
    Ok(found)
}

fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

const DECLARED: &str = "[[test]]\nname = \"score\"\npath = \"checks/score_check.rs\"\n";

#[test]
fn a_declared_test_target_outside_tests_pairs_its_crate_named_call() -> Result<(), String> {
    let found = finding(DECLARED, "checks/score_check.rs")?;
    assert_eq!(
        found.class,
        ExposureClass::Exposed,
        "evidence: {:?}",
        found.evidence
    );
    // The control: the same test under `tests/` reads the same.
    let control = finding("", "tests/score.rs")?;
    assert_eq!(control.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_file_no_test_target_declares_stays_uncredited() -> Result<(), String> {
    // Cargo never builds `checks/score_check.rs` without a `[[test]]` entry.
    let undeclared = finding("", "checks/score_check.rs")?;
    assert_ne!(undeclared.class, ExposureClass::Exposed);
    // A `[[bin]]` target is not an integration-test crate.
    let binary = finding(
        "[[bin]]\nname = \"score\"\npath = \"checks/score_check.rs\"\n",
        "checks/score_check.rs",
    )?;
    assert_ne!(binary.class, ExposureClass::Exposed);
    // `test = false`: `cargo test` does not build it.
    let skipped = finding(
        &format!("{DECLARED}test = false\n"),
        "checks/score_check.rs",
    )?;
    assert_ne!(skipped.class, ExposureClass::Exposed);
    // `harness = false`: libtest never collects its `#[test]` items.
    let custom = finding(
        &format!("{DECLARED}harness = false\n"),
        "checks/score_check.rs",
    )?;
    assert_ne!(custom.class, ExposureClass::Exposed);
    // Unmet `required-features` keeps the target out of a default run.
    let gated = finding(
        &format!("{DECLARED}required-features = [\"slow\"]\n[features]\nslow = []\n"),
        "checks/score_check.rs",
    )?;
    assert_ne!(gated.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_member_package_declared_test_never_reads_as_the_root_package() -> Result<(), String> {
    // The root package owns the changed `clamp_score`; member `b` declares
    // `checks/t.rs` and spells the root's `my_pkg::clamp_score` with no
    // dependency edge on it. Test discovery already drops that test (the
    // owner reads no_static_path even with `root_package_only` forced off),
    // so this is a regression guard on the end-to-end reading; the
    // root-package check in `is_declared_test_target_entry` is defense in
    // depth behind it.
    let member_test = "#[test]\nfn clamps_at_limit() {\n    assert_eq!(my_pkg::clamp_score(100), 100);\n    assert_eq!(my_pkg::clamp_score(101), 100);\n}\n";
    let found = finding_with(
        "[workspace]\nmembers = [\"crates/b\"]\n",
        "crates/b/src/lib.rs",
        &[
            (
                "crates/b/Cargo.toml",
                "[package]\nname = \"b\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[[test]]\nname = \"t\"\npath = \"checks/t.rs\"\n",
            ),
            // Overwrites the `TEST` text: `b`'s library is its own owner.
            ("crates/b/src/lib.rs", OWNER),
            ("crates/b/checks/t.rs", member_test),
        ],
    )?;
    assert_ne!(
        found.class,
        ExposureClass::Exposed,
        "evidence: {:?}",
        found.evidence
    );
    Ok(())
}
