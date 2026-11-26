//! `let ok = value > 0; assert_eq!(ok, true);` asserts `value > 0`, so it is
//! only as strong as that expression written in place (#16296,
//! RIPR-SPEC-0231 rules 1 and 3). The changed boundary moves `discount(100)`
//! from 100 to 90, so a boolean that holds for both values passes on both
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
        "ripr-bool-local-equality-{}-{}",
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
fn a_compared_bool_local_is_only_as_strong_as_its_initializer() -> Result<(), String> {
    // Controls: an initializer that pins one value stays exact, and so does
    // an ordinary local pin.
    for body in [
        "let ok = demo::discount(100) == 90;\n    assert_eq!(ok, true);",
        "let ok = matches!(demo::discount(100), 90);\n    assert_eq!(ok, true);",
        "let value = demo::discount(100);\n    assert_eq!(value, 90);",
        "let state_ok = demo::discount(100) == 90;\n    assert_eq!(state_ok, true); // pinned",
        // An upper-case initial is still a local the binding table knows.
        "let Ok_state = demo::discount(100) == 90;\n    assert_eq!(Ok_state, true);",
    ] {
        assert_eq!(class_with(body)?, ExposureClass::Exposed, "{body}");
    }
    // The same checks written in place are already weak.
    assert_eq!(
        class_with("assert_eq!(demo::discount(100) > 0, true);")?,
        ExposureClass::WeaklyExposed
    );
    for body in [
        "let ok = demo::discount(100) > 0;\n    assert_eq!(ok, true);",
        "let ok = demo::discount(100) != 95;\n    assert_eq!(true, ok);",
        "let ok: bool = demo::discount(100) >= 1;\n    assert_eq!(ok, true);",
        "let ok = matches!(demo::discount(100), 90 | 100);\n    assert_eq!(ok, true);",
        "let ok = matches!(demo::discount(100), _);\n    assert_eq!(ok, true);",
        "let ok = matches!(Some(demo::discount(100)), Some(_));\n    assert_eq!(ok, true);",
        // A comment around an operand, or after the statement, changes
        // nothing.
        "let ok = demo::discount(100) > 0;\n    assert_eq!(ok, /* expected */ true);",
        "let state_ok = demo::discount(100) > 0;\n    assert_eq!(state_ok, true); // boundary check",
        // An upper-case initial resolves through the binding table too. Raw
        // and non-ASCII spellings are covered by the unit rows: the
        // test-to-call pairing check already reads those workspaces weak
        // with an exact initializer, so they cannot discriminate here.
        "let Ok_state = demo::discount(100) > 0;\n    assert_eq!(Ok_state, true);",
    ] {
        assert_eq!(
            class_with(body)?,
            ExposureClass::WeaklyExposed,
            "a compared bool local reads as its initializer: {body}"
        );
    }
    Ok(())
}
