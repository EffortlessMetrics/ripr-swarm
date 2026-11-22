//! A test that reads its build environment with `env!` or `option_env!`
//! (`env!("CARGO_MANIFEST_DIR")` is the common case) keeps its exact
//! assertion's credit (#14116). Both macros expand to a literal, so neither
//! can return or skip a later assertion. A rebinding of the name in reach
//! still refuses the pin, as for every other trusted macro (RIPR-SPEC-0197).
//! `include_str!` and `include_bytes!` expand to a literal read from a file
//! at build time, so a test that reads a fixture with them, or sits beside
//! a helper whose `include_str!(concat!(env!(..), ..))` mentions `env`,
//! keeps the same credit.

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

/// The class of the changed predicate when `tests/pin.rs` is `prelude`
/// followed by one boundary test whose body opens with `lead`.
fn class_with(prelude: &str, lead: &str) -> Result<ExposureClass, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "ripr-build-env-macro-in-test-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    let test = format!(
        "{prelude}#[test]\nfn discount_boundary() {{\n    {lead}\n    assert_eq!(demo::discount(101), 91);\n    assert_eq!(demo::discount(100), 90);\n}}\n"
    );
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
fn a_test_that_reads_its_build_environment_pins_like_one_that_does_not() -> Result<(), String> {
    assert_eq!(
        class_with("", "let _ = 0;")?,
        ExposureClass::Exposed,
        "the test without a build-environment read is the control"
    );
    for (prelude, lead) in [
        ("", "let _dir = env!(\"CARGO_MANIFEST_DIR\");"),
        ("", "let _flag = option_env!(\"RIPR_UNSET\");"),
        ("", "let _ = env!(\"CARGO_PKG_NAME\", \"set by cargo\");"),
        // `use std::env;` names the module and the standard macro alike.
        (
            "use std::env;\n",
            "let _dir = env!(\"CARGO_MANIFEST_DIR\");",
        ),
        (
            "use std::{env, fs};\n",
            "let _dir = env!(\"CARGO_MANIFEST_DIR\"); let _ = fs::metadata(\".\");",
        ),
    ] {
        assert_eq!(
            class_with(prelude, lead)?,
            ExposureClass::Exposed,
            "`env!` cannot leave the test: {prelude:?} {lead:?}"
        );
    }
    Ok(())
}

#[test]
fn a_test_that_reads_an_included_file_pins_like_one_that_does_not() -> Result<(), String> {
    let helper = "fn manifest() -> &'static str {\n    include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/Cargo.toml\"))\n}\n";
    assert!(helper.contains("include_str!(concat!(env!("));
    for (prelude, lead) in [
        (
            "",
            "let _spec = include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/Cargo.toml\"));",
        ),
        (
            "",
            "let _bytes = include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/Cargo.toml\"));",
        ),
        ("", "let _spec = include_str!(\"../Cargo.toml\");"),
        (
            "use std::include_str;\n",
            "let _spec = include_str!(\"../Cargo.toml\");",
        ),
        // The helper's `include_str!` mentions `env`, which the test also
        // calls; the mention defines nothing.
        (
            helper,
            "let _dir = env!(\"CARGO_MANIFEST_DIR\"); let _ = manifest();",
        ),
    ] {
        assert_eq!(
            class_with(prelude, lead)?,
            ExposureClass::Exposed,
            "an included file cannot leave the test: {prelude:?} {lead:?}"
        );
    }
    Ok(())
}

#[test]
fn a_rebound_build_environment_macro_keeps_the_assertion_uncredited() -> Result<(), String> {
    for (prelude, lead) in [
        // A local definition may hide a `return` before the assertions.
        (
            "macro_rules! env { ($name:literal) => { return }; }\n",
            "env!(\"CARGO_MANIFEST_DIR\");",
        ),
        (
            "macro_rules! option_env { ($name:literal) => { return }; }\n",
            "option_env!(\"RIPR_UNSET\");",
        ),
        // An import from a crate other than `std`/`core` is another macro.
        ("use demo::env;\n", "env!(\"CARGO_MANIFEST_DIR\");"),
        // A plain import that takes over the `std` or `core` name makes
        // `std::env` that crate's macro.
        (
            "use demo::std;\nuse std::env;\n",
            "env!(\"CARGO_MANIFEST_DIR\");",
        ),
        (
            "use demo::core;\nuse core::option_env;\n",
            "option_env!(\"RIPR_UNSET\");",
        ),
        (
            "macro_rules! include_str { ($path:expr) => { return }; }\n",
            "include_str!(\"../Cargo.toml\");",
        ),
        (
            "use demo::include_bytes;\n",
            "include_bytes!(\"../Cargo.toml\");",
        ),
        // A nested macro in a trusted macro's operand is a literal only when
        // it is a standard built-in that nothing rebinds. `format!` takes
        // any expression, so both controls compile and return early.
        (
            "macro_rules! concat { ($($t:tt)*) => {{ return; \"\" }}; }\n",
            "let _ = format!(\"{}\", concat!(\"Cargo\", \".toml\"));",
        ),
        (
            "macro_rules! leave { () => {{ return; \"\" }}; }\n",
            "let _ = format!(\"{}\", leave!());",
        ),
        // An untrusted macro whose arguments mention `include_str` may
        // define it.
        (
            "wrap! { macro_rules! include_str { ($path:expr) => { return }; } }\n",
            "include_str!(\"../Cargo.toml\");",
        ),
        // An unrelated macro called in the test stays opaque.
        ("macro_rules! leave { () => { return }; }\n", "leave!();"),
    ] {
        assert_ne!(
            class_with(prelude, lead)?,
            ExposureClass::Exposed,
            "the macro in the test may leave it early: {prelude:?} {lead:?}"
        );
    }
    Ok(())
}
