//! A `matches!` whose pattern is an or-pattern of values accepts each of
//! them, so it is a disjunction like `a == x || a == y` and earns no exact
//! credit (#16286, RIPR-SPEC-0231 rules 1 and 3). The changed boundary moves
//! `discount(100)` from 100 to 90, so `matches!(discount(100), 90 | 100)`
//! passes on both versions and must not read `exposed`.

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
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "ripr-matches-or-pattern-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
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
fn an_or_pattern_of_values_reads_like_a_disjunction_not_an_exact_pin() -> Result<(), String> {
    // Controls: one pinned value is exact, and the same disjunction spelled
    // with `||` is already weak.
    for body in [
        "assert!(matches!(demo::discount(100), 90));",
        "assert!(matches!(Some(demo::discount(100)), Some(90)));",
        "assert_eq!(demo::discount(100), 90);",
    ] {
        assert_eq!(class_with(body)?, ExposureClass::Exposed, "{body}");
    }
    assert_eq!(
        class_with("assert!(demo::discount(100) == 90 || demo::discount(100) == 100);")?,
        ExposureClass::WeaklyExposed
    );
    for body in [
        "assert!(matches!(demo::discount(100), 90 | 100));",
        "assert!(matches!(demo::discount(100), 100 | 90));",
        "let value = demo::discount(100);\n    assert!(matches!(value, 90 | 100));",
        "assert!(matches!(Some(demo::discount(100)), Some(90 | 100)));",
        "assert!(matches!(Some(demo::discount(100)), Some(90) | Some(100)));",
        "assert!(matches!(demo::discount(100), found @ (90 | 100) if found > 0));",
    ] {
        assert_eq!(
            class_with(body)?,
            ExposureClass::WeaklyExposed,
            "an or-pattern accepts both versions: {body}"
        );
    }
    Ok(())
}
