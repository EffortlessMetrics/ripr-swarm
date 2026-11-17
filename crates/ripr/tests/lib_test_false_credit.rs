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
    // Distinct names, so the credit is attributed: the integration test is
    // the related discriminator and the disabled unit test is not revived by
    // the integration root's presence.
    let unit = "\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn disabled_unit_discriminator() {\n        assert_eq!(f(5), 7);\n    }\n}\n";
    let integration =
        "#[test]\nfn integration_discriminator() {\n    assert_eq!(lib_test_false::f(5), 7);\n}\n";
    let lib = lib_source(unit);
    let with_integration = TempCrate::create(
        LIB_TEST_FALSE,
        &[("src/lib.rs", &lib), ("tests/it.rs", integration)],
    )?
    .finding()?;
    assert_eq!(
        with_integration.class,
        ExposureClass::Exposed,
        "an integration test still runs under cargo test: {with_integration:?}"
    );
    let names = related_names(&with_integration);
    assert!(
        names.iter().any(|name| name == "integration_discriminator"),
        "the integration test is the related discriminator: {names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|name| name == "disabled_unit_discriminator"),
        "the disabled unit test is not related: {names:?}"
    );

    // The same unit source without the integration target loses the credit.
    let without_integration =
        TempCrate::create(LIB_TEST_FALSE, &[("src/lib.rs", &lib)])?.finding()?;
    assert!(
        !credits_a_discriminator(&without_integration.class),
        "only the integration target carried the credit: {without_integration:?}"
    );
    let names = related_names(&without_integration);
    assert!(
        !names
            .iter()
            .any(|name| name == "disabled_unit_discriminator"),
        "the disabled unit test is not related: {names:?}"
    );
    Ok(())
}

/// The names of the finding's related tests.
fn related_names(finding: &Finding) -> Vec<String> {
    finding
        .related_tests
        .iter()
        .map(|test| test.name.clone())
        .collect()
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
    assert!(
        !related_names(&finding).iter().any(|name| name == "t"),
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

/// A build script that writes `#[path = ".../a/src/shared.rs"] pub mod
/// shared;` into `OUT_DIR/plain.rs` for `b` to include.
const GENERATED_MOUNT_BUILD: &str = "use std::path::PathBuf;\nfn main() {\n    let out = PathBuf::from(std::env::var(\"OUT_DIR\").unwrap());\n    let shared = PathBuf::from(std::env::var(\"CARGO_MANIFEST_DIR\").unwrap()).join(\"../a/src/shared.rs\");\n    std::fs::write(out.join(\"plain.rs\"), format!(\"#[path = {:?}] pub mod shared;\\n\", shared.display().to_string())).unwrap();\n}\n";

#[test]
fn a_build_script_generated_mount_keeps_credit() -> Result<(), String> {
    // #14962 review B1: `b`'s build script writes `#[path = ...] pub mod
    // shared;` into `OUT_DIR`, and `b` includes it, so `cargo test -p b`
    // runs `t`. The dynamic `include!` is doubt, not a skip.
    let build = GENERATED_MOUNT_BUILD;
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
    assert_withheld_in_every_mode(&fixture)
}

fn assert_withheld_in_every_mode(fixture: &TempCrate) -> Result<(), String> {
    for mode in MODES {
        let finding = fixture.finding_in(mode.clone())?;
        assert!(
            !credits_a_discriminator(&finding.class),
            "{mode:?}: {finding:?}"
        );
    }
    Ok(())
}

/// Member `a` skips its unit tests and member `b` mounts `a/src/shared.rs`
/// through `extra`, so `cargo test -p b --lib` runs `t` (#14962 review).
fn mounted_from_b(extra: &[(&str, &str)]) -> Result<TempCrate, String> {
    mounted_from_b_with(MEMBER_A, extra)
}

fn mounted_from_b_with(member_a: &str, extra: &[(&str, &str)]) -> Result<TempCrate, String> {
    let shared = lib_source(UNIT_TESTS);
    let mut files: Vec<(&str, &str)> = vec![
        ("a/Cargo.toml", member_a),
        ("a/src/lib.rs", "pub mod shared;\n"),
        ("a/src/shared.rs", &shared),
        ("b/Cargo.toml", MEMBER_B),
    ];
    files.extend_from_slice(extra);
    TempCrate::create_in(WORKSPACE, &files, "a/src/shared.rs", 1)
}

#[test]
fn a_mount_inside_a_source_directory_named_target_keeps_credit_in_every_mode() -> Result<(), String>
{
    // #14962 review u1: `b/src/target` is a module, not Cargo's build
    // directory, so the scan must read it.
    let fixture = mounted_from_b(&[
        ("b/src/lib.rs", "pub mod target;\n"),
        (
            "b/src/target/mod.rs",
            "#[path = \"../../../a/src/shared.rs\"]\npub mod shared;\n",
        ),
    ])?;
    assert_exposed_in_every_mode(&fixture, "a mount under `src/target`")
}

#[test]
fn a_mount_inside_a_hidden_source_directory_keeps_its_tests_in_every_mode() -> Result<(), String> {
    // #14962 review u2: `#[path = ".gen/m.rs"]` mounts a file in a dot
    // directory, which mounts the skipped library's file in turn.
    // This layout reads `static_unknown` with `t` related even with `a`'s
    // tests left on (the assertion binding through the hidden mount is
    // unresolved), so each mode must read exactly what that control reads:
    // the skip may not move the class, and `t` stays a related test.
    let b_files = [
        ("b/src/lib.rs", "#[path = \".gen/m.rs\"]\npub mod m;\n"),
        (
            "b/src/.gen/m.rs",
            "#[path = \"../../../a/src/shared.rs\"]\npub mod shared;\n",
        ),
    ];
    let fixture = mounted_from_b(&b_files)?;
    let control = mounted_from_b_with(MEMBER_B.replace("\"b\"", "\"a\"").as_str(), &b_files)?;
    for mode in MODES {
        let skipped = fixture.finding_in(mode.clone())?;
        let tested = control.finding_in(mode.clone())?;
        assert_eq!(
            skipped.class, tested.class,
            "a mount under `src/.gen` in {mode:?}: {skipped:?}"
        );
    }
    let ready = fixture.finding()?;
    assert!(
        related_names(&ready).contains(&"t".to_string()),
        "the mounted `t` stays a related test: {ready:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_symlinked_alias_of_a_skipped_library_file_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review u3: `b/src/alias.rs` is a symlink to `a/src/shared.rs`,
    // so `cargo test -p b` compiles `t` under a name no text mentions.
    let fixture = mounted_from_b(&[("b/src/lib.rs", "pub mod alias;\n")])?;
    let alias = fixture.root.join("b/src/alias.rs");
    std::os::unix::fs::symlink("../../a/src/shared.rs", &alias)
        .map_err(|error| format!("symlink alias.rs failed: {error}"))?;
    let text = std::fs::read_to_string(&alias)
        .map_err(|error| format!("premise: alias.rs resolves: {error}"))?;
    assert!(text.contains("fn t()"), "premise: alias reads shared.rs");
    assert_exposed_in_every_mode(&fixture, "a symlinked alias")
}

#[test]
fn an_ordinary_macro_does_not_restore_skipped_unit_test_credit() -> Result<(), String> {
    // #14962 review a1c: a `macro_rules!` whose metavariables build no `mod`
    // name, `#[path]` value or `include!` operand is no mount doubt.
    let lib = format!(
        "{OWNER}\nmacro_rules! twice {{\n    ($e:expr) => {{\n        $e * 2\n    }};\n}}\n{UNIT_TESTS}"
    );
    let fixture = TempCrate::create(LIB_TEST_FALSE, &[("src/lib.rs", &lib)])?;
    assert_withheld_in_every_mode(&fixture)
}

/// Each mode reads `fixture` exactly as `control` (the same tree with the
/// tests left on) reads it, and both credit the discriminator.
fn assert_exposed_like_control_in_every_mode(
    fixture: &TempCrate,
    control: &TempCrate,
    label: &str,
) -> Result<(), String> {
    for mode in MODES {
        let tested = control.finding_in(mode.clone())?;
        assert_eq!(
            tested.class,
            ExposureClass::Exposed,
            "premise: tests-on {label} in {mode:?}: {tested:?}"
        );
        let skipped = fixture.finding_in(mode.clone())?;
        assert_eq!(
            skipped.class, tested.class,
            "{label} in {mode:?}: {skipped:?}"
        );
    }
    Ok(())
}

/// The issue's member `a` with its unit tests on, for a tests-on control.
const MEMBER_A_TESTED: &str = "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";

/// [`GENERATED_MOUNT_BUILD`] without spelling `shared.rs` or `mod shared`,
/// so only the dynamic `include!` in `b` can tie the mount to the file.
const NAMELESS_MOUNT_BUILD: &str = "use std::path::PathBuf;\nfn main() {\n    let out = PathBuf::from(std::env::var(\"OUT_DIR\").unwrap());\n    let stem = \"shared\";\n    let file = PathBuf::from(std::env::var(\"CARGO_MANIFEST_DIR\").unwrap()).join(\"../a/src\").join(format!(\"{stem}.{}\", \"rs\"));\n    std::fs::write(out.join(\"plain.rs\"), format!(\"#[path = {:?}] pub mod {stem};\", file.display().to_string())).unwrap();\n}\n";

#[test]
fn a_brace_or_bracket_generated_include_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review n1: `include!{..}` and `include![..]` are the same
    // dynamic mount as `include!(..)`.
    for include in [
        "include!(concat!(env!(\"OUT_DIR\"), \"/plain.rs\"));\n",
        "include!{concat!(env!(\"OUT_DIR\"), \"/plain.rs\")}\n",
        "include![concat!(env!(\"OUT_DIR\"), \"/plain.rs\")];\n",
    ] {
        let manifest = format!("{MEMBER_B}build = \"build.rs\"\n");
        let b_files = [
            ("b/Cargo.toml", manifest.as_str()),
            ("b/build.rs", NAMELESS_MOUNT_BUILD),
            ("b/src/lib.rs", include),
        ];
        let fixture = mounted_from_b(&b_files)?;
        let control = mounted_from_b_with(MEMBER_A_TESTED, &b_files)?;
        assert_exposed_like_control_in_every_mode(&fixture, &control, include.trim())?;
    }
    Ok(())
}

#[test]
fn a_commented_macro_mount_in_a_tested_bin_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review n2: a comment between `mod` and the metavariable must
    // not hide the macro-built mount of `src/shared.rs` into the tested bin.
    // (`path = /* x */ $p` is pinned by the operand unit test.)
    let shared = lib_source(UNIT_TESTS);
    let main = "macro_rules! m {\n    ($n:ident) => {\n        mod /* m */ $n;\n    };\n}\nm!(shared);\nfn main() {}\n";
    let files = [
        ("src/lib.rs", "pub mod shared;\n"),
        ("src/shared.rs", shared.as_str()),
        ("src/main.rs", main),
    ];
    let fixture = TempCrate::create_changing(LIB_TEST_FALSE, &files, "src/shared.rs", 1)?;
    let control = TempCrate::create_changing("", &files, "src/shared.rs", 1)?;
    assert_exposed_like_control_in_every_mode(&fixture, &control, "a commented macro mount")
}

#[cfg(unix)]
#[test]
fn a_symlinked_directory_without_sources_does_not_restore_credit() -> Result<(), String> {
    // #14962 review n3: a directory link holding no Rust source aliases
    // nothing, so the skip still holds.
    let fixture = TempCrate::create(
        LIB_TEST_FALSE,
        &[
            ("src/lib.rs", &lib_source(UNIT_TESTS)),
            ("assets/readme.txt", "no sources\n"),
        ],
    )?;
    std::os::unix::fs::symlink("assets", fixture.root.join("linked-assets"))
        .map_err(|error| format!("symlink assets failed: {error}"))?;
    assert_withheld_in_every_mode(&fixture)
}
