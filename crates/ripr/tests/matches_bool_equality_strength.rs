//! `assert_eq!(matches!(x, PATTERN), true)` asserts the pattern exactly as
//! `assert!(matches!(x, PATTERN))` does, so it is only as strong as its
//! pattern (#16290, RIPR-SPEC-0231 rules 1 and 3). The changed boundary moves
//! `discount(100)` from 100 to 90, so an unpinned pattern passes on both
//! versions and must not read `exposed`.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str =
    "[package]\nname = \"demo\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n";

const LIB: &str = "pub fn discount(amount: i32) -> i32 {\n    if amount >= 100 { amount - 10 } else { amount }\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(amount: i32) -> i32 {\n-    if amount > 100 { amount - 10 } else { amount }\n+    if amount >= 100 { amount - 10 } else { amount }\n }\n";

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The class of the changed predicate when the one boundary test in
/// `tests/pin.rs` has `body`.
fn class_with(body: &str) -> Result<ExposureClass, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-matches-bool-equality-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    // `create_dir` refuses a path that already exists, a symlink included,
    // so the fixture never writes through a directory it did not create.
    std::fs::create_dir(&root)
        .map_err(|error| format!("create fixture root {}: {error}", root.display()))?;
    let fixture = Fixture(root);
    let test = format!("#[test]\nfn discount_boundary() {{\n    {body}\n}}\n");
    for (relative, text) in [
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", LIB),
        ("tests/pin.rs", test.as_str()),
        ("diff.patch", DIFF),
    ] {
        let path = fixture.0.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create directory {}: {error}", parent.display()))?;
        }
        std::fs::write(&path, text)
            .map_err(|error| format!("write file {}: {error}", path.display()))?;
    }
    let output = check_workspace(CheckInput {
        root: fixture.0.clone(),
        diff_file: Some(fixture.0.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let finding = output
        .findings
        .into_iter()
        .find(|finding| finding.probe.family == ProbeFamily::Predicate)
        .ok_or("missing changed predicate probe")?;
    assert!(
        finding
            .related_tests
            .iter()
            .any(|related| related.name == "discount_boundary"),
        "the boundary test must be the related test: {finding:?}"
    );
    Ok(finding.class)
}

#[test]
fn a_matches_compared_with_true_is_only_as_strong_as_its_pattern() -> Result<(), String> {
    // Controls: a pinned pattern stays exact in both operand orders, as the
    // `assert!` spelling does.
    for body in [
        "assert!(matches!(demo::discount(100), 90));",
        "assert_eq!(matches!(demo::discount(100), 90), true);",
        "assert_eq!(true, matches!(demo::discount(100), 90));",
        "assert_eq!({ matches!(demo::discount(100), 90) } /* pinned */, true);",
    ] {
        assert_eq!(class_with(body)?, ExposureClass::Exposed, "{body}");
    }
    // The `assert!` spelling of an unpinned pattern is already weak.
    assert_eq!(
        class_with("assert!(matches!(demo::discount(100), _));")?,
        ExposureClass::WeaklyExposed
    );
    for body in [
        "assert_eq!(matches!(demo::discount(100), _), true);",
        "assert_eq!(matches!(Some(demo::discount(100)), Some(_)), true);",
        "assert_eq!(matches!(demo::discount(100), 90 | 100), true);",
        "assert_eq!(true, matches!(demo::discount(100), 90 | 100));",
        "assert_eq!((matches!(demo::discount(100), _)), (true));",
        "assert_ne!(matches!(demo::discount(100), _), false);",
        // Comments, groups and a block holding only the operand are
        // transparent.
        "assert_eq!((matches!(demo::discount(100), 90 | 100)) /* either */, true);",
        "assert_eq!({ matches!(demo::discount(100), 90 | 100) }, true);",
        // `assert_ne!` stays an inequality macro whatever the literal, so
        // even a pinned pattern is capped there (rule 1).
        "assert_ne!(matches!(demo::discount(100), 90), false);",
        // A comparison that excludes the pattern stays capped (rule 1).
        "assert_eq!(matches!(demo::discount(100), 90), false);",
    ] {
        assert_eq!(
            class_with(body)?,
            ExposureClass::WeaklyExposed,
            "a compared pattern reads as its pattern: {body}"
        );
    }
    Ok(())
}
