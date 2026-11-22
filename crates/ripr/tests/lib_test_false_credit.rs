//! Public-API controls for tests compiled only into a target `cargo test`
//! skips (#14962).
//!
//! `[lib] test = false` keeps the library's unit-test binary out of `cargo
//! test` (`running 0 tests`), so a `#[cfg(test)]` module in `src/lib.rs` is
//! never a discriminator. An integration test under `tests/` of the same
//! crate still runs, a doc example still runs under rustdoc, and a library
//! that leaves `test` on keeps its in-crate credit. The skip is read from
//! cargo metadata, so these controls need the `cargo` that runs them.
//!
//! #14972 adds roots cargo never discovers (`autobins = false`, `autolib =
//! false`) and examples, which `cargo test` builds without their tests
//! unless `[[example]] test = true`. #15262 adds benches, which plain
//! `cargo test` does not build unless `[[bench]] test = true`.

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

    /// Every finding the run reports, in any file (#15877).
    fn finding_count_in(&self, mode: Mode) -> Result<usize, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings.len())
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
fn a_forwarded_module_may_mount_only_from_outside_the_skipped_chain() -> Result<(), String> {
    let passthrough =
        "macro_rules! passthrough { ($item:item) => { $item }; }\npassthrough! { mod runtime; }\n";
    let skipped_lib = format!("{OWNER}{passthrough}{UNIT_TESTS}");
    let skipped = TempCrate::create(
        LIB_TEST_FALSE,
        &[("src/lib.rs", &skipped_lib), ("src/runtime.rs", "")],
    )?
    .finding()?;
    assert!(!credits_a_discriminator(&skipped.class), "{skipped:?}");

    let remap = "macro_rules! remap { ($keyword:tt $input:ident;) => { $keyword lib; }; }\nremap! { mod unrelated; }\n";
    let running = TempCrate::create(
        &format!("{LIB_TEST_FALSE}\n[[test]]\nname = \"driver\"\npath = \"src/driver.rs\"\n"),
        &[
            ("src/lib.rs", &lib_source(UNIT_TESTS)),
            ("src/driver.rs", remap),
        ],
    )?
    .finding()?;
    assert!(credits_a_discriminator(&running.class), "{running:?}");
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

/// The exact withheld class (#14972): a skipped target's tests leave the
/// changed behavior with no static path to a running discriminator.
/// A root Cargo never builds is no production root either (#15877): its
/// change is not behavior any build runs, so no mode reports it, and no
/// finding can carry a discriminator its tests would supply.
fn assert_never_built_in_every_mode(fixture: &TempCrate, label: &str) -> Result<(), String> {
    for mode in MODES {
        let count = fixture.finding_count_in(mode.clone())?;
        assert_eq!(count, 0, "{label} in {mode:?}");
    }
    Ok(())
}

fn assert_no_static_path_in_every_mode(fixture: &TempCrate, label: &str) -> Result<(), String> {
    for mode in MODES {
        let finding = fixture.finding_in(mode.clone())?;
        assert_eq!(
            finding.class,
            ExposureClass::NoStaticPath,
            "{label} in {mode:?}: {finding:?}"
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

/// `b` mounts `a/src/shared.rs` through `b_files`, read beside a tests-on
/// control: the skip may not move the class in any mode.
fn assert_b_mount_keeps_credit(b_files: &[(&str, &str)], label: &str) -> Result<(), String> {
    let fixture = mounted_from_b(b_files)?;
    let control = mounted_from_b_with(MEMBER_A_TESTED, b_files)?;
    assert_exposed_like_control_in_every_mode(&fixture, &control, label)
}

/// The text of a non-`.rs` file in `b/src` that mounts the shared file.
const B_SRC_MOUNT: &str = "#[path = \"../../a/src/shared.rs\"]\npub mod shared;\n";

#[test]
fn an_include_of_a_non_rust_file_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review n5: the scan reads only `.rs` sources, so a mount
    // spelled in an included `gen.inc` is doubt.
    assert_b_mount_keeps_credit(
        &[
            ("b/src/lib.rs", "include!(\"gen.inc\");\n"),
            ("b/src/gen.inc", B_SRC_MOUNT),
        ],
        "an `include!` of `gen.inc`",
    )
}

#[test]
fn an_aliased_generated_include_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review n7: `inc!` is `include!` under another name, so the
    // generated mount must still be doubt.
    let manifest = format!("{MEMBER_B}build = \"build.rs\"\n");
    assert_b_mount_keeps_credit(
        &[
            ("b/Cargo.toml", manifest.as_str()),
            ("b/build.rs", NAMELESS_MOUNT_BUILD),
            (
                "b/src/lib.rs",
                "use std::include as inc;\ninc!(concat!(env!(\"OUT_DIR\"), \"/plain.rs\"));\n",
            ),
        ],
        "an aliased `include!`",
    )
}

#[test]
fn an_escaped_include_operand_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review (Devin): `\u{73}hared.rs` decodes to `shared.rs`, which
    // no substring scan sees, and a comment splits `include` from `!`.
    assert_b_mount_keeps_credit(
        &[(
            "b/src/lib.rs",
            "include /* c */ !(\"../../a/src/\\u{73}hared.rs\");\n",
        )],
        "an escaped `include!` operand",
    )
}

/// A `[[test]]` target rooted at `src/container.rs`, which the library also
/// declares as a module.
const CONTAINER_TEST_TARGET: &str =
    "\n[[test]]\nname = \"container\"\npath = \"src/container.rs\"\n";

#[test]
fn a_running_test_target_in_the_ancestry_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review (Devin): the leaf's only ancestry ends at the skipped
    // library, but its parent `src/container.rs` is also a `[[test]]` root,
    // so `cargo test --test container` runs the leaf's test.
    let leaf = lib_source(UNIT_TESTS);
    let files = [
        ("src/lib.rs", "pub mod container;\n"),
        (
            "src/container.rs",
            "#[path = \"container/leaf.rs\"]\npub mod leaf;\n",
        ),
        ("src/container/leaf.rs", leaf.as_str()),
    ];
    let fixture = TempCrate::create_changing(
        &format!("{LIB_TEST_FALSE}{CONTAINER_TEST_TARGET}"),
        &files,
        "src/container/leaf.rs",
        1,
    )?;
    let control =
        TempCrate::create_changing(CONTAINER_TEST_TARGET, &files, "src/container/leaf.rs", 1)?;
    assert_exposed_like_control_in_every_mode(&fixture, &control, "a `[[test]]` parent module")
}

#[test]
fn a_path_mount_of_a_non_rust_file_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review n5p: `#[path = "m.txt"]` mounts a file the scan does
    // not read.
    assert_b_mount_keeps_credit(
        &[
            ("b/src/lib.rs", "#[path = \"m.txt\"]\npub mod m;\n"),
            ("b/src/m.txt", B_SRC_MOUNT),
        ],
        "a `#[path]` mount of `m.txt`",
    )
}

#[test]
fn a_macro_mount_after_a_url_string_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review n2_urlcmt: a `//` inside a string must not hide the
    // commented `mod /* c */ $n` the macro expands into the tested bin.
    let shared = lib_source(UNIT_TESTS);
    let main = "macro_rules! m {\n    ($n:ident) => {\n        const U: &str = \"http://x\"; mod /* c */ $n;\n    };\n}\nm!(shared);\nfn main() {}\n";
    let files = [
        ("src/lib.rs", "pub mod shared;\n"),
        ("src/shared.rs", shared.as_str()),
        ("src/main.rs", main),
    ];
    let fixture = TempCrate::create_changing(LIB_TEST_FALSE, &files, "src/shared.rs", 1)?;
    let control = TempCrate::create_changing("", &files, "src/shared.rs", 1)?;
    assert_exposed_like_control_in_every_mode(&fixture, &control, "a macro mount after a URL")
}

/// Each mode reads the tests-on `control` `exposed` and `fixture` with no
/// credited discriminator.
fn assert_withheld_against_control_in_every_mode(
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
        assert!(
            !credits_a_discriminator(&skipped.class),
            "{label} in {mode:?}: {skipped:?}"
        );
    }
    Ok(())
}

#[test]
fn a_skipped_library_with_a_mod_rs_layout_is_withheld_in_every_mode() -> Result<(), String> {
    // #14962 review round 8: an unmounted `src/foo/mod.rs`, as the holder's
    // parent or as the holder itself, is keyed on `foo` and is no doubt.
    let tests = lib_source(UNIT_TESTS);
    let layouts: [(&[(&str, &str)], &str); 2] = [
        (
            &[
                ("src/lib.rs", "pub mod foo;\n"),
                ("src/foo/mod.rs", "pub mod bar;\n"),
                ("src/foo/bar.rs", tests.as_str()),
            ],
            "src/foo/bar.rs",
        ),
        (
            &[
                ("src/lib.rs", "pub mod foo;\n"),
                ("src/foo/mod.rs", tests.as_str()),
            ],
            "src/foo/mod.rs",
        ),
    ];
    for (files, holder) in layouts {
        let fixture = TempCrate::create_changing(LIB_TEST_FALSE, files, holder, 1)?;
        let control = TempCrate::create_changing("", files, holder, 1)?;
        assert_withheld_against_control_in_every_mode(&fixture, &control, holder)?;
    }
    Ok(())
}

#[test]
fn a_member_path_mount_of_a_skipped_mod_rs_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14962 review round 8: `b` mounts `a/src/foo/mod.rs`, which declares
    // the holder `bar`, so `cargo test -p b` runs its test.
    let tests = lib_source(UNIT_TESTS);
    let files = |member_a: &'static str| -> Vec<(&'static str, String)> {
        vec![
            ("a/Cargo.toml", member_a.to_string()),
            ("a/src/lib.rs", "pub mod foo;\n".to_string()),
            ("a/src/foo/mod.rs", "pub mod bar;\n".to_string()),
            ("a/src/foo/bar.rs", tests.clone()),
            ("b/Cargo.toml", MEMBER_B.to_string()),
            (
                "b/src/lib.rs",
                "#[path = \"../../a/src/foo/mod.rs\"]\nmod foo;\n".to_string(),
            ),
        ]
    };
    let create = |member_a: &'static str| {
        let owned = files(member_a);
        let borrowed: Vec<(&str, &str)> = owned
            .iter()
            .map(|(path, text)| (*path, text.as_str()))
            .collect();
        TempCrate::create_in(WORKSPACE, &borrowed, "a/src/foo/bar.rs", 1)
    };
    let fixture = create(MEMBER_A)?;
    let control = create(MEMBER_A_TESTED)?;
    assert_exposed_like_control_in_every_mode(&fixture, &control, "a `#[path]` mount of `mod.rs`")
}

/// A single-package manifest with `package_keys` in `[package]` and `tail`
/// after `[workspace]` (#14972).
fn package_manifest(package_keys: &str, tail: &str) -> String {
    format!(
        "[package]\nname = \"auto-targets\"\nversion = \"0.0.0\"\nedition = \"2021\"\n{package_keys}\n[workspace]\n{tail}"
    )
}

/// `OWNER`, a `main`, and `UNIT_TESTS`: a crate root whose own `#[test]`
/// is the only candidate discriminator for the changed `a + 2`.
fn main_source() -> String {
    format!("{OWNER}\nfn main() {{\n    println!(\"{{}}\", f(1));\n}}\n{UNIT_TESTS}")
}

#[test]
fn autobins_false_main_unit_tests_are_not_a_discriminator_in_every_mode() -> Result<(), String> {
    // #14972 c04: `autobins = false` and no `[[bin]]`: cargo builds no bin
    // from `src/main.rs`, so `cargo test` runs 0 tests from it, and since
    // #15877 its change is not production behavior either.
    let lib = ("src/lib.rs", "pub fn g() {}\n");
    let main = main_source();
    let skipped = TempCrate::create_in(
        &package_manifest("autobins = false\n", ""),
        &[lib, ("src/main.rs", &main)],
        "src/main.rs",
        1,
    )?;
    assert_never_built_in_every_mode(&skipped, "an undiscovered bin")?;
    // Controls: auto-discovery on, and `autobins = false` with the bin
    // declared explicitly, both run `t` under `cargo test`.
    let discovered = TempCrate::create_in(
        &package_manifest("", ""),
        &[lib, ("src/main.rs", &main)],
        "src/main.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&discovered, "an autodiscovered bin")?;
    let declared = TempCrate::create_in(
        &package_manifest(
            "autobins = false\n",
            "\n[[bin]]\nname = \"auto-targets\"\npath = \"src/main.rs\"\n",
        ),
        &[lib, ("src/main.rs", &main)],
        "src/main.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&declared, "a declared bin under autobins = false")
}

#[test]
fn autolib_false_lib_unit_tests_are_not_a_discriminator_in_every_mode() -> Result<(), String> {
    // #14972 c18: `autolib = false` and no `[lib]`: `src/lib.rs` is no
    // target, so `cargo test` runs 0 tests from it, and since #15877 its
    // change is not production behavior either.
    let lib = lib_source(UNIT_TESTS);
    let main = ("src/main.rs", "fn main() {}\n");
    let skipped = TempCrate::create_in(
        &package_manifest("autolib = false\n", ""),
        &[("src/lib.rs", &lib), main],
        "src/lib.rs",
        1,
    )?;
    assert_never_built_in_every_mode(&skipped, "an undiscovered lib")?;
    let discovered = TempCrate::create_in(
        &package_manifest("", ""),
        &[("src/lib.rs", &lib), main],
        "src/lib.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&discovered, "an autodiscovered lib")?;
    let declared = TempCrate::create_in(
        &package_manifest("autolib = false\n", "\n[lib]\npath = \"src/lib.rs\"\n"),
        &[("src/lib.rs", &lib), main],
        "src/lib.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&declared, "a declared lib under autolib = false")
}

/// An example whose own `#[test]` calls the library's changed `f`.
const EXAMPLE_TESTS: &str = "fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(auto_targets::f(5), 7);\n    }\n}\n";

#[test]
fn example_unit_tests_are_not_a_discriminator_unless_the_example_is_tested() -> Result<(), String> {
    // #14972 c25: `cargo test` builds `examples/demo.rs` as a plain
    // executable (examples default to `test = false`), so `t` never runs
    // and the library's changed `f` has no running discriminator.
    let lib = lib_source("");
    let files = [
        ("src/lib.rs", lib.as_str()),
        ("examples/demo.rs", EXAMPLE_TESTS),
    ];
    let skipped = TempCrate::create_in(&package_manifest("", ""), &files, "src/lib.rs", 1)?;
    assert_no_static_path_in_every_mode(&skipped, "a default example")?;
    // Control: `test = true` makes `cargo test` run the example's tests.
    // `--mode instant` does not index `examples/` at all, so it finds no
    // static path there with or without this change; every other mode
    // credits the running example test.
    let tested = TempCrate::create_in(
        &package_manifest("", "\n[[example]]\nname = \"demo\"\ntest = true\n"),
        &files,
        "src/lib.rs",
        1,
    )?;
    for mode in [Mode::Draft, Mode::Fast, Mode::Deep, Mode::Ready] {
        let finding = tested.finding_in(mode.clone())?;
        assert_eq!(
            finding.class,
            ExposureClass::Exposed,
            "a `test = true` example in {mode:?}: {finding:?}"
        );
    }
    Ok(())
}

/// A bench whose own `#[test]` calls the library's changed `f`.
const BENCH_TESTS: &str = "#[test]\nfn t() {\n    assert_eq!(auto_targets::f(5), 7);\n}\n";

#[test]
fn bench_unit_tests_are_not_a_discriminator_unless_the_bench_is_tested() -> Result<(), String> {
    // #15262: a bench defaults to `test = false`, so plain `cargo test`
    // does not build `benches/load.rs` and `t` never runs; the library's
    // changed `f` has no running discriminator.
    let lib = lib_source("");
    let files = [
        ("src/lib.rs", lib.as_str()),
        ("benches/load.rs", BENCH_TESTS),
    ];
    let skipped = TempCrate::create_in(&package_manifest("", ""), &files, "src/lib.rs", 1)?;
    assert_no_static_path_in_every_mode(&skipped, "a default bench")?;
    // Control: `test = true` makes `cargo test` build and run the bench's
    // tests. `--mode instant` does not index `benches/`, so every other
    // mode credits the running bench test.
    let tested = TempCrate::create_in(
        &package_manifest("", "\n[[bench]]\nname = \"load\"\ntest = true\n"),
        &files,
        "src/lib.rs",
        1,
    )?;
    assert_credited_beyond_instant(&tested, "a `test = true` bench")
}

/// Cargo rejects a bare package nested in a workspace until it is admitted
/// as a member. Check the same file, diff and API options in both layouts.
#[test]
fn bare_bench_requires_a_runnable_target() -> Result<(), String> {
    let manifest = "[package]\nname = 'auto-targets'\nversion = '0.0.0'\nedition = '2021'\n";
    let outer = TempCrate::create_in(
        "[workspace]\nmembers = []\n",
        &[
            ("billing/Cargo.toml", manifest),
            ("billing/src/lib.rs", OWNER),
            ("billing/benches/load.rs", BENCH_TESTS),
        ],
        "src/lib.rs",
        1,
    )?;
    let fixture = TempCrate {
        root: outer.root.join("billing"),
        changed_line: 2,
    };
    std::fs::rename(
        outer.root.join("diff.patch"),
        fixture.root.join("diff.patch"),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        fixture.finding_in(Mode::Fast)?.class,
        ExposureClass::NoStaticPath
    );
    std::fs::write(
        outer.root.join("Cargo.toml"),
        "[workspace]\nmembers = ['billing']\n",
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        fixture.finding_in(Mode::Fast)?.class,
        ExposureClass::NoStaticPath
    );
    let tests = fixture.root.join("tests");
    std::fs::create_dir(&tests).map_err(|error| error.to_string())?;
    let sibling = tests.join("fee.rs");
    std::fs::write(
        &sibling,
        BENCH_TESTS.replace("fn t()", "fn integration_pin()"),
    )
    .map_err(|error| error.to_string())?;
    let finding = fixture.finding_in(Mode::Fast)?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(related_names(&finding), ["integration_pin"]);
    std::fs::remove_file(sibling).map_err(|error| error.to_string())?;
    std::fs::write(
        fixture.root.join("Cargo.toml"),
        format!("{manifest}\n[[bench]]\nname = 'load'\ntest = true\n"),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(
        fixture.finding_in(Mode::Fast)?.class,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_bench_included_into_an_integration_test_keeps_credit() -> Result<(), String> {
    // #15262: `tests/it.rs` compiles `benches/load.rs` through `include!`
    // inside a module body, so `cargo test` runs `t` although the bench
    // target itself is skipped; the mount scan sees the running target.
    let lib = lib_source("");
    let fixture = TempCrate::create_in(
        &package_manifest("", ""),
        &[
            ("src/lib.rs", lib.as_str()),
            ("benches/load.rs", BENCH_TESTS),
            (
                "tests/it.rs",
                "mod load {\n    include!(\"../benches/load.rs\");\n}\n",
            ),
        ],
        "src/lib.rs",
        1,
    )?;
    assert_credited_beyond_instant(&fixture, "a bench included by an integration test")
}

#[test]
fn a_member_path_mount_of_an_undiscovered_root_keeps_credit_in_every_mode() -> Result<(), String> {
    // #14972 merge with #14962 round 7/8: `a` turns `autobins` off, so
    // cargo builds nothing from `a/src/main.rs`, but `b` compiles it through
    // `#[path]` and `cargo test -p b` runs `t`. An undiscovered root is no
    // listed target, so the mount scan asks about it like any other
    // ancestry member; a run whose index omits `b` must not withhold it.
    let main = main_source();
    let fixture = TempCrate::create_in(
        WORKSPACE,
        &[
            (
                "a/Cargo.toml",
                "[package]\nname = \"a\"\nversion = \"0.0.0\"\nedition = \"2021\"\nautobins = false\n",
            ),
            ("a/src/lib.rs", "pub fn g() {}\n"),
            ("a/src/main.rs", &main),
            ("b/Cargo.toml", MEMBER_B),
            (
                "b/src/lib.rs",
                "#[path = \"../../a/src/main.rs\"]\npub mod cli;\n",
            ),
        ],
        "a/src/main.rs",
        1,
    )?;
    assert_exposed_in_every_mode(&fixture, "a `#[path]` mount of an undiscovered root")
}

#[test]
fn skipped_example_shapes_are_withheld_in_every_mode() -> Result<(), String> {
    // #14972 review: putting example roots under the mount scan must not
    // restore credit where nothing mounts the example. A multi-file example
    // directory, a `test = true` example whose required feature is off, and
    // an example whose required feature is on by default but which leaves
    // `test` at its `false` default all run no `t` under `cargo test`.
    let lib = lib_source("");
    let multi = TempCrate::create_in(
        &package_manifest("", ""),
        &[
            ("src/lib.rs", lib.as_str()),
            ("examples/demo/main.rs", "mod h;\nfn main() {}\n"),
            (
                "examples/demo/h.rs",
                "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(auto_targets::f(5), 7);\n    }\n}\n",
            ),
        ],
        "src/lib.rs",
        1,
    )?;
    assert_no_static_path_in_every_mode(&multi, "a multi-file example directory")?;
    let files = [
        ("src/lib.rs", lib.as_str()),
        ("examples/demo.rs", EXAMPLE_TESTS),
    ];
    let unmet = TempCrate::create_in(
        &package_manifest(
            "",
            "\n[features]\nx = []\n\n[[example]]\nname = \"demo\"\ntest = true\nrequired-features = [\"x\"]\n",
        ),
        &files,
        "src/lib.rs",
        1,
    )?;
    assert_no_static_path_in_every_mode(&unmet, "a `test = true` example with an unmet feature")?;
    let skipped_example = TempCrate::create_in(
        &package_manifest(
            "",
            "\n[features]\ndefault = [\"x\"]\nx = []\n\n[[example]]\nname = \"demo\"\nrequired-features = [\"x\"]\n",
        ),
        &files,
        "src/lib.rs",
        1,
    )?;
    assert_no_static_path_in_every_mode(
        &skipped_example,
        "a default-feature example without `test = true`",
    )
}

/// Every mode that indexes `examples/` credits the running example test;
/// `--mode instant` does not index `examples/`, so it reads no static path
/// on `main` and here alike.
fn assert_credited_beyond_instant(fixture: &TempCrate, label: &str) -> Result<(), String> {
    for mode in [Mode::Draft, Mode::Fast, Mode::Deep, Mode::Ready] {
        let finding = fixture.finding_in(mode.clone())?;
        assert!(
            credits_a_discriminator(&finding.class),
            "{label} in {mode:?}: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn an_example_included_into_the_library_test_tree_keeps_credit() -> Result<(), String> {
    // #14972 review x5: `src/lib.rs` compiles `examples/demo.rs` through
    // `include!` inside `mod ex`, so `cargo test` runs `ex::tests::t`. The
    // index records no edge for an `include!` in a module body, so only the
    // mount scan sees the running target that mounts the example root.
    let lib = lib_source("\n#[cfg(test)]\nmod ex {\n    include!(\"../examples/demo.rs\");\n}\n");
    let fixture = TempCrate::create_in(
        &package_manifest("", ""),
        &[
            ("src/lib.rs", lib.as_str()),
            (
                "examples/demo.rs",
                "fn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(crate::f(5), 7);\n    }\n}\n",
            ),
        ],
        "src/lib.rs",
        1,
    )?;
    assert_credited_beyond_instant(&fixture, "an example included by the library")
}

#[test]
fn an_example_included_into_an_integration_test_keeps_credit() -> Result<(), String> {
    // #14972 review x2: `tests/it.rs` compiles `examples/demo.rs` through
    // `include!` inside `mod demo`, so `cargo test` runs `demo::tests::t`
    // in the integration binary.
    let lib = lib_source("");
    let fixture = TempCrate::create_in(
        &package_manifest("", ""),
        &[
            ("src/lib.rs", lib.as_str()),
            ("examples/demo.rs", EXAMPLE_TESTS),
            (
                "tests/it.rs",
                "mod demo {\n    include!(\"../examples/demo.rs\");\n}\n",
            ),
        ],
        "src/lib.rs",
        1,
    )?;
    assert_credited_beyond_instant(&fixture, "an example included by an integration test")
}

/// An edition 2015 manifest (no `edition` key) that declares one `[[test]]`
/// leaves `autotests` off unless it says otherwise (#15877): Cargo builds
/// only the declared test, so `tests/integration.rs` never runs and credits
/// no discriminator. Edition 2021 and an explicit `autotests = true` build
/// it and keep the credit.
#[test]
fn edition_2015_declared_test_turns_off_test_discovery() -> Result<(), String> {
    let manifest = |package_keys: &str| {
        format!(
            "[package]\nname = \"lib-test-false\"\nversion = \"0.0.0\"\n{package_keys}\n[[test]]\nname = \"declared\"\npath = \"tests/declared.rs\"\n\n[workspace]\n"
        )
    };
    // Edition 2015 integration tests name the library through `extern
    // crate`, so the opt-in control is a test Cargo can build and run.
    let integration = format!("extern crate lib_test_false;\n\n{INTEGRATION_TEST}");
    let files = [
        ("src/lib.rs", OWNER),
        ("tests/declared.rs", "#[test]\nfn unrelated() {}\n"),
        ("tests/integration.rs", integration.as_str()),
    ];
    let legacy = TempCrate::create_in(&manifest(""), &files, "src/lib.rs", 1)?;
    let finding = legacy.finding()?;
    assert!(
        !credits_a_discriminator(&finding.class),
        "edition 2015 never builds tests/integration.rs: {finding:?}"
    );
    for (keys, label) in [
        ("edition = \"2021\"\n", "edition 2021"),
        ("autotests = true\n", "edition 2015 with autotests = true"),
    ] {
        let built = TempCrate::create_in(&manifest(keys), &files, "src/lib.rs", 1)?;
        let finding = built.finding()?;
        assert!(
            credits_a_discriminator(&finding.class),
            "{label} builds tests/integration.rs: {finding:?}"
        );
    }
    Ok(())
}

/// A unit test inside a binary root Cargo never builds is no discriminator
/// (#16051 review): `autobins = false` with no `[[bin]]`, or edition 2015
/// declaring a different `[[bin]]`, leaves `src/bin/extra.rs` undiscovered,
/// so its test cannot run. The discovered binary keeps the credit.
#[test]
fn a_test_in_an_undiscovered_bin_is_not_a_discriminator() -> Result<(), String> {
    let manifest = |package_keys: &str, targets: &str| {
        format!(
            "[package]\nname = \"lib-test-false\"\nversion = \"0.0.0\"\n{package_keys}\n{targets}\n[workspace]\n"
        )
    };
    let bin = "extern crate lib_test_false;\n\nfn main() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        assert_eq!(lib_test_false::f(5), 7);\n    }\n}\n";
    let files = [
        ("src/lib.rs", OWNER),
        ("src/bin/extra.rs", bin),
        ("src/declared.rs", "fn main() {}\n"),
    ];
    let discovered = TempCrate::create_in(
        &manifest("edition = \"2021\"\n", ""),
        &files,
        "src/lib.rs",
        1,
    )?;
    let finding = discovered.finding()?;
    assert!(
        credits_a_discriminator(&finding.class),
        "control: the discovered bin's test runs: {finding:?}"
    );
    for (keys, targets, label) in [
        (
            "edition = \"2021\"\nautobins = false\n",
            "",
            "autobins = false",
        ),
        (
            "",
            "[[bin]]\nname = \"declared\"\npath = \"src/declared.rs\"\n",
            "edition 2015 with a declared bin",
        ),
    ] {
        let undiscovered = TempCrate::create_in(&manifest(keys, targets), &files, "src/lib.rs", 1)?;
        let finding = undiscovered.finding()?;
        assert!(
            !credits_a_discriminator(&finding.class),
            "{label} never builds src/bin/extra.rs: {finding:?}"
        );
    }
    Ok(())
}
