//! Public-API controls for tests compiled only into a target `cargo test`
//! skips (#14962).
//!
//! `[lib] test = false` keeps the library's unit-test binary out of `cargo
//! test` (`running 0 tests`), so a `#[cfg(test)]` module in `src/lib.rs` is
//! never a discriminator. An integration test under `tests/` of the same
//! crate still runs, a doc example still runs under rustdoc, and a library
//! that leaves `test` on keeps its in-crate credit. The skip is read from
//! cargo metadata, so these controls need the `cargo` that runs them.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn f(a: i32) -> i32 {\n    a + 2\n}\n";

const UNIT_TESTS: &str = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn t() {\n        assert_eq!(f(5), 7);\n    }\n}\n";

const INTEGRATION_TEST: &str =
    "#[test]\nfn integration() {\n    assert_eq!(lib_test_false::f(5), 7);\n}\n";

const LIB_TEST_FALSE: &str = "\n[lib]\ntest = false\n";

/// The unified diff that turns `a + 1` into `a + 2` in `file`, where `fn f`
/// opens on line `fn_line`.
fn diff(file: &str, fn_line: usize) -> String {
    format!(
        "diff --git a/{file} b/{file}\nindex 1111111..2222222 100644\n--- a/{file}\n+++ b/{file}\n@@ -{fn_line},3 +{fn_line},3 @@\n pub fn f(a: i32) -> i32 {{\n-    a + 1\n+    a + 2\n }}\n"
    )
}

struct TempCrate {
    root: PathBuf,
    changed_line: usize,
}

impl TempCrate {
    fn create(manifest_extra: &str, files: &[(&str, &str)]) -> Result<Self, String> {
        Self::create_changing(manifest_extra, files, "src/lib.rs", 1)
    }

    fn create_changing(
        manifest_extra: &str,
        files: &[(&str, &str)],
        changed_file: &str,
        fn_line: usize,
    ) -> Result<Self, String> {
        Self::create_in(
            &format!(
                "[package]\nname = \"lib-test-false\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[workspace]\n{manifest_extra}"
            ),
            files,
            changed_file,
            fn_line,
        )
    }

    /// A crate or workspace whose root manifest is exactly `root_manifest`.
    fn create_in(
        root_manifest: &str,
        files: &[(&str, &str)],
        changed_file: &str,
        fn_line: usize,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-lib-test-false-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let this = Self {
            root,
            changed_line: fn_line + 1,
        };
        let write = |relative: &str, text: &str| -> Result<(), String> {
            let path = this.root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {relative} parent failed: {error}"))?;
            }
            std::fs::write(&path, text).map_err(|error| format!("write {relative} failed: {error}"))
        };
        write("Cargo.toml", root_manifest)?;
        for (relative, text) in files {
            write(relative, text)?;
        }
        write("diff.patch", &diff(changed_file, fn_line))?;
        Ok(this)
    }

    /// The one return-value finding for the changed `a + 2`.
    fn finding(&self) -> Result<Finding, String> {
        self.finding_in(Mode::Ready)
    }

    fn finding_in(&self, mode: Mode) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut found = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue
                && finding.probe.location.line == self.changed_line
        });
        match (found.next(), found.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one return-value finding on line {}, got {other:?}",
                self.changed_line
            )),
        }
    }
}

impl Drop for TempCrate {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn lib_source(tests: &str) -> String {
    format!("{OWNER}{tests}")
}

/// Classes that credit a discriminator; a skipped target must read neither.
fn credits_a_discriminator(class: &ExposureClass) -> bool {
    matches!(class, ExposureClass::Exposed | ExposureClass::WeaklyExposed)
}

#[test]
fn lib_test_false_unit_tests_are_not_a_discriminator() -> Result<(), String> {
    // Control: the same unit test with `test` left on is a discriminator,
    // so the manifest flag alone separates the two readings.
    let live = TempCrate::create("", &[("src/lib.rs", &lib_source(UNIT_TESTS))])?.finding()?;
    assert_eq!(live.class, ExposureClass::Exposed, "{live:?}");

    let skipped =
        TempCrate::create(LIB_TEST_FALSE, &[("src/lib.rs", &lib_source(UNIT_TESTS))])?.finding()?;
    assert!(
        !credits_a_discriminator(&skipped.class),
        "`[lib] test = false` unit tests never run under cargo test: {skipped:?}"
    );
    Ok(())
}

#[test]
fn lib_test_false_unit_tests_in_a_child_module_are_not_a_discriminator() -> Result<(), String> {
    // The skip follows the library's module tree, not only its root file.
    let lib = format!("{OWNER}\n#[cfg(test)]\nmod checks;\n");
    let skipped = TempCrate::create(
        LIB_TEST_FALSE,
        &[
            ("src/lib.rs", &lib),
            (
                "src/checks.rs",
                "use super::*;\n#[test]\nfn t() {\n    assert_eq!(f(5), 7);\n}\n",
            ),
        ],
    )?
    .finding()?;
    assert!(
        !credits_a_discriminator(&skipped.class),
        "a child module of a skipped library never runs: {skipped:?}"
    );
    Ok(())
}

#[test]
fn lib_test_false_keeps_integration_test_credit() -> Result<(), String> {
    let finding = TempCrate::create(
        LIB_TEST_FALSE,
        &[
            ("src/lib.rs", &lib_source(UNIT_TESTS)),
            ("tests/it.rs", INTEGRATION_TEST),
        ],
    )?
    .finding()?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "an integration test still runs under cargo test: {finding:?}"
    );
    Ok(())
}

#[test]
fn lib_test_false_keeps_doctest_credit() -> Result<(), String> {
    // rustdoc runs doc examples whatever `[lib] test` says (`doctest` is
    // its own flag), so withholding the unit test must not take the doc
    // example's credit with it.
    let doc = "/// ```\n/// assert_eq!(lib_test_false::f(5), 7);\n/// ```\n";
    let lib = format!("{doc}{OWNER}{UNIT_TESTS}");
    let finding =
        TempCrate::create_changing(LIB_TEST_FALSE, &[("src/lib.rs", &lib)], "src/lib.rs", 4)?
            .finding()?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a doc example still runs under cargo test: {finding:?}"
    );
    assert!(
        !finding.ripr.reach.summary.ends_with(", t"),
        "the skipped unit test `t` is not a related test: {finding:?}"
    );
    Ok(())
}

#[test]
fn test_false_integration_target_is_not_a_discriminator() -> Result<(), String> {
    // `[[test]] test = false` is the integration-target spelling of the
    // same skip; the library itself stays tested and holds no tests.
    let finding = TempCrate::create(
        "\n[[test]]\nname = \"it\"\ntest = false\n",
        &[("src/lib.rs", OWNER), ("tests/it.rs", INTEGRATION_TEST)],
    )?
    .finding()?;
    assert!(
        !credits_a_discriminator(&finding.class),
        "a `[[test]] test = false` target never runs: {finding:?}"
    );
    Ok(())
}

#[test]
fn bin_test_false_unit_tests_are_not_a_discriminator() -> Result<(), String> {
    let main = format!("{OWNER}\nfn main() {{\n    println!(\"{{}}\", f(1));\n}}\n{UNIT_TESTS}");
    let live =
        TempCrate::create_changing("", &[("src/main.rs", &main)], "src/main.rs", 1)?.finding()?;
    assert_eq!(live.class, ExposureClass::Exposed, "{live:?}");
    let skipped = TempCrate::create_changing(
        "\n[[bin]]\nname = \"lib-test-false\"\npath = \"src/main.rs\"\ntest = false\n",
        &[("src/main.rs", &main)],
        "src/main.rs",
        1,
    )?
    .finding()?;
    assert!(
        !credits_a_discriminator(&skipped.class),
        "`[[bin]] test = false` unit tests never run under cargo test: {skipped:?}"
    );
    Ok(())
}

#[test]
fn lib_test_false_module_shared_with_a_tested_bin_keeps_credit() -> Result<(), String> {
    // The shared module also compiles into the bin's unit-test binary,
    // which `cargo test` runs.
    let finding = TempCrate::create_changing(
        LIB_TEST_FALSE,
        &[
            ("src/lib.rs", "pub mod util;\n"),
            (
                "src/main.rs",
                "mod util;\nfn main() {\n    println!(\"{}\", util::f(1));\n}\n",
            ),
            ("src/util.rs", &lib_source(UNIT_TESTS)),
        ],
        "src/util.rs",
        1,
    )?
    .finding()?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a module a tested bin also compiles keeps its tests: {finding:?}"
    );
    Ok(())
}

const MEMBER_A: &str =
    "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\ntest = false\n";

const MEMBER_B: &str = "[package]\nname = \"b\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";

const WORKSPACE: &str = "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n";

/// Every mode, so no mode's narrower index can drop a credit (#14962 review).
const MODES: [Mode; 5] = [
    Mode::Instant,
    Mode::Draft,
    Mode::Fast,
    Mode::Deep,
    Mode::Ready,
];

fn assert_exposed_in_every_mode(fixture: &TempCrate, label: &str) -> Result<(), String> {
    for mode in MODES {
        let finding = fixture.finding_in(mode.clone())?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "{label} in {mode:?}: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn a_member_path_mount_of_a_skipped_library_file_keeps_credit_in_every_mode() -> Result<(), String>
{
    // #14962 review B3: `b` compiles `a/src/shared.rs` through `#[path]`
    // without depending on `a`, so `cargo test -p b` runs `t`. A draft run
    // that does not index `b` must not withhold it.
    let fixture = TempCrate::create_in(
        WORKSPACE,
        &[
            ("a/Cargo.toml", MEMBER_A),
            ("a/src/lib.rs", "pub mod shared;\n"),
            ("a/src/shared.rs", &lib_source(UNIT_TESTS)),
            ("b/Cargo.toml", MEMBER_B),
            (
                "b/src/lib.rs",
                "#[path = \"../../a/src/shared.rs\"]\npub mod shared;\n",
            ),
        ],
        "a/src/shared.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&fixture, "a `#[path]` mount in another member")
}

#[test]
fn an_integration_target_path_mount_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review B3: `tests/it.rs` mounts `src/util.rs`, so the
    // integration binary runs `t` even though the library's does not.
    let fixture = TempCrate::create_changing(
        LIB_TEST_FALSE,
        &[
            ("src/lib.rs", "pub mod util;\n"),
            ("src/util.rs", &lib_source(UNIT_TESTS)),
            ("tests/it.rs", "#[path = \"../src/util.rs\"]\nmod util;\n"),
        ],
        "src/util.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&fixture, "a `#[path]` mount from `tests/`")
}

#[test]
fn a_build_script_generated_mount_keeps_credit() -> Result<(), String> {
    // #14962 review B1: `b`'s build script writes `#[path = ...] pub mod
    // shared;` into `OUT_DIR`, and `b` includes it, so `cargo test -p b`
    // runs `t`. The dynamic `include!` is doubt, not a skip.
    let build = "use std::path::PathBuf;\nfn main() {\n    let out = PathBuf::from(std::env::var(\"OUT_DIR\").unwrap());\n    let shared = PathBuf::from(std::env::var(\"CARGO_MANIFEST_DIR\").unwrap()).join(\"../a/src/shared.rs\");\n    std::fs::write(out.join(\"plain.rs\"), format!(\"#[path = {:?}] pub mod shared;\\n\", shared.display().to_string())).unwrap();\n}\n";
    let fixture = TempCrate::create_in(
        WORKSPACE,
        &[
            ("a/Cargo.toml", MEMBER_A),
            ("a/src/lib.rs", "pub mod shared;\n"),
            ("a/src/shared.rs", &lib_source(UNIT_TESTS)),
            ("b/Cargo.toml", &format!("{MEMBER_B}build = \"build.rs\"\n")),
            ("b/build.rs", build),
            (
                "b/src/lib.rs",
                "include!(concat!(env!(\"OUT_DIR\"), \"/plain.rs\"));\n",
            ),
        ],
        "a/src/shared.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&fixture, "a build-script-generated mount")
}

#[test]
fn a_skipped_library_is_withheld_in_every_mode() -> Result<(), String> {
    // The skip itself does not depend on the mode either.
    let fixture = TempCrate::create(LIB_TEST_FALSE, &[("src/lib.rs", &lib_source(UNIT_TESTS))])?;
    for mode in MODES {
        let finding = fixture.finding_in(mode.clone())?;
        assert!(
            !credits_a_discriminator(&finding.class),
            "{mode:?}: {finding:?}"
        );
    }
    Ok(())
}
