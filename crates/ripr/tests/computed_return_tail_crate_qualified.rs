//! #13037 residual: an integration test that pins the owner through the
//! package's own library path with no `use` (`shipping::shipping_cost(..)`)
//! read `weakly_exposed` on a changed computed return tail, while the same
//! pin spelled bare after a `use` read `exposed`. An agent that appends one
//! test to an existing file without touching its imports writes the
//! qualified spelling, so a correct discriminator was reported as a gap.
//!
//! The reached-tail pin (RIPR-SPEC-0443, #13037 amendment) now reads the
//! qualified call as the bare owner call when the relation's own-crate
//! authority (`own_crate_qualified_entry_path`) establishes the path. Every
//! spelling that authority refuses keeps today's reading.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const LIB: &str = "pub fn shipping_cost(weight_g: u32, express: bool) -> u32 {
    let base = if weight_g <= 500 { 400 } else { 900 };
    if express {
        base * 2
    } else {
        base
    }
}
";

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,3 +3,3 @@ pub fn shipping_cost(weight_g: u32, express: bool) -> u32 {
     if express {
-        base * 3
+        base * 2
     } else {
";

const MODULE_PATCH: &str = "diff --git a/src/rates.rs b/src/rates.rs
index 1111111..2222222 100644
--- a/src/rates.rs
+++ b/src/rates.rs
@@ -3,3 +3,3 @@ pub fn shipping_cost(weight_g: u32, express: bool) -> u32 {
     if express {
-        base * 3
+        base * 2
     } else {
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(package: &str, files: &[(&str, &str)], patch: &str) -> Result<Self, String> {
        Self::with_manifest_tail(package, "", files, patch)
    }

    fn with_manifest_tail(
        package: &str,
        manifest_tail: &str,
        files: &[(&str, &str)],
        patch: &str,
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-qualified-tail-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = Self { root };
        repo.write(
            "Cargo.toml",
            &format!(
                "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n{manifest_tail}\n[workspace]\n"
            ),
        )?;
        for (path, content) in files {
            repo.write(path, content)?;
        }
        repo.write("diff.patch", patch)?;
        Ok(repo)
    }

    fn write(&self, relative: &str, content: &str) -> Result<(), String> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
        }
        std::fs::write(&path, content)
            .map_err(|error| format!("write {} failed: {error}", path.display()))
    }

    fn computed_finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut computed = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::ReturnValue
                && finding.probe.location.line == 4
                && finding.probe.expression == "base * 2"
        });
        match (computed.next(), computed.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one return_value finding for `base * 2` on line 4, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn test_file(prelude: &str, body: &str) -> String {
    format!("{prelude}#[test]\nfn pins() {{\n    {body}\n}}\n")
}

/// The class of the changed tail when `tests/shipping.rs` of package
/// `shipping` holds `prelude` and one test with `body`.
fn class_of(prelude: &str, body: &str) -> Result<ExposureClass, String> {
    Ok(TempRepo::create(
        "shipping",
        &[
            ("src/lib.rs", LIB),
            ("tests/shipping.rs", &test_file(prelude, body)),
        ],
        PATCH,
    )?
    .computed_finding()?
    .class)
}

const QUALIFIED_PIN: &str = "assert_eq!(shipping::shipping_cost(100, true), 800);";

#[test]
fn premise_the_patch_changes_only_the_computed_tail() {
    assert_eq!(LIB.lines().nth(3), Some("        base * 2"));
}

#[test]
fn a_crate_qualified_exact_pin_that_reaches_the_branch_is_exposed() -> Result<(), String> {
    assert_eq!(class_of("", QUALIFIED_PIN)?, ExposureClass::Exposed);
    // rustfmt wraps a long assertion: the call sits on its own line.
    assert_eq!(
        class_of(
            "",
            "assert_eq!(\n        shipping::shipping_cost(100, true),\n        800\n    );"
        )?,
        ExposureClass::Exposed
    );
    // The file already imports another item of the crate: the test an agent
    // appends without touching the `use` line.
    let lib = format!("{LIB}\npub fn flat_rate() -> u32 {{\n    400\n}}\n");
    let tests = test_file(
        "use shipping::flat_rate;\n\n#[test]\nfn flat() {\n    assert_eq!(flat_rate(), 400);\n}\n\n",
        QUALIFIED_PIN,
    );
    let appended = TempRepo::create(
        "shipping",
        &[("src/lib.rs", &lib), ("tests/shipping.rs", &tests)],
        PATCH,
    )?
    .computed_finding()?
    .class;
    assert_eq!(appended, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn the_bare_imported_spelling_keeps_its_credit() -> Result<(), String> {
    assert_eq!(
        class_of(
            "use shipping::shipping_cost;\n",
            "assert_eq!(shipping_cost(100, true), 800);"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_crate_qualified_pin_through_a_library_module_is_exposed() -> Result<(), String> {
    let class = |body: &str| -> Result<ExposureClass, String> {
        Ok(TempRepo::create(
            "shipping",
            &[
                ("src/lib.rs", "pub mod rates;\n"),
                ("src/rates.rs", LIB),
                ("tests/shipping.rs", &test_file("", body)),
            ],
            MODULE_PATCH,
        )?
        .computed_finding()?
        .class)
    };
    assert_eq!(
        class("assert_eq!(shipping::rates::shipping_cost(100, true), 800);")?,
        ExposureClass::Exposed
    );
    // The path skips the module: it names no item of the library.
    assert_ne!(class(QUALIFIED_PIN)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn the_library_target_name_is_the_spelling_that_counts() -> Result<(), String> {
    let class = |body: &str| -> Result<ExposureClass, String> {
        Ok(TempRepo::create(
            "ship-cost",
            &[
                ("src/lib.rs", LIB),
                ("tests/shipping.rs", &test_file("", body)),
            ],
            PATCH,
        )?
        .computed_finding()?
        .class)
    };
    assert_eq!(
        class("assert_eq!(ship_cost::shipping_cost(100, true), 800);")?,
        ExposureClass::Exposed
    );
    assert_ne!(class(QUALIFIED_PIN)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_explicit_library_name_is_the_spelling_that_counts() -> Result<(), String> {
    let class = |body: &str| -> Result<ExposureClass, String> {
        Ok(TempRepo::with_manifest_tail(
            "shipping",
            "\n[lib]\nname = \"shipcalc\"\npath = \"src/lib.rs\"\n",
            &[
                ("src/lib.rs", LIB),
                ("tests/shipping.rs", &test_file("", body)),
            ],
            PATCH,
        )?
        .computed_finding()?
        .class)
    };
    assert_eq!(
        class("assert_eq!(shipcalc::shipping_cost(100, true), 800);")?,
        ExposureClass::Exposed
    );
    // The package name is not a second spelling when `[lib] name` differs.
    assert_ne!(class(QUALIFIED_PIN)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_expected_name_bound_twice_resolves_to_the_binding_in_scope() -> Result<(), String> {
    // The assertion's own position selects the `let` it reads, so the
    // qualified spelling must keep that position.
    let body = |call: &str, standard: &str| {
        format!(
            "let expected = 400;\n    assert_eq!({standard}(100, false), expected);\n    let expected = 800;\n    assert_eq!({call}(100, true), expected);"
        )
    };
    assert_eq!(
        class_of(
            "",
            &body("shipping::shipping_cost", "shipping::shipping_cost")
        )?,
        class_of(
            "use shipping::shipping_cost;\n",
            &body("shipping_cost", "shipping_cost")
        )?,
        "the qualified spelling reads like the bare one"
    );
    assert_eq!(
        class_of(
            "",
            &body("shipping::shipping_cost", "shipping::shipping_cost")
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_let_bound_qualified_call_reads_like_the_bare_let_bound_call() -> Result<(), String> {
    let qualified = class_of(
        "",
        "let got = shipping::shipping_cost(100, true);\n    assert_eq!(got, 800);",
    )?;
    let bare = class_of(
        "use shipping::shipping_cost;\n",
        "let got = shipping_cost(100, true);\n    assert_eq!(got, 800);",
    )?;
    assert_eq!(qualified, bare);
    // The other branch's value through the same `let` is credited by neither.
    let other = class_of(
        "",
        "let got = shipping::shipping_cost(100, false);\n    assert_eq!(got, 400);",
    )?;
    assert_ne!(other, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_qualified_pin_from_the_other_branch_only_is_not_credited() -> Result<(), String> {
    let class = class_of("", "assert_eq!(shipping::shipping_cost(100, false), 400);")?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_rebound_crate_name_is_not_credited() -> Result<(), String> {
    for prelude in [
        // A local module takes the name.
        "mod shipping {\n    pub fn shipping_cost(_w: u32, _e: bool) -> u32 {\n        800\n    }\n}\n",
        // An import renames something else to it.
        "use std as shipping;\n",
        "extern crate std as shipping;\n",
        // A glob may bring a module of that name into scope.
        "use helpers::*;\nmod helpers {\n    pub mod shipping {\n        pub fn shipping_cost(_w: u32, _e: bool) -> u32 {\n            800\n        }\n    }\n}\n",
    ] {
        let class = class_of(prelude, QUALIFIED_PIN)?;
        assert_ne!(class, ExposureClass::Exposed, "{prelude}");
    }
    Ok(())
}

#[test]
fn another_path_ending_in_the_owner_is_not_credited() -> Result<(), String> {
    for (prelude, body) in [
        // A longer path names another crate's item.
        (
            "mod other {\n    pub mod shipping {\n        pub fn shipping_cost(_w: u32, _e: bool) -> u32 {\n            800\n        }\n    }\n}\n",
            "assert_eq!(other::shipping::shipping_cost(100, true), 800);",
        ),
        // A different item whose name the owner's name prefixes.
        (
            "",
            "assert_eq!(shipping::shipping_cost_v2(100, true), 800);",
        ),
    ] {
        let class = class_of(prelude, body)?;
        assert_ne!(class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_transformed_or_wrapped_qualified_call_is_not_a_pin() -> Result<(), String> {
    for body in [
        "assert_eq!(shipping::shipping_cost(100, true) + 0, 800);",
        "assert_eq!(u32::from(shipping::shipping_cost(100, true) > 0), 1);",
        "assert_ne!(shipping::shipping_cost(100, true), 1200);",
        // Two owner calls: neither side is a value the test fixes.
        "assert_eq!(shipping::shipping_cost(100, true), shipping::shipping_cost(100, true));",
    ] {
        let class = class_of("", body)?;
        assert_ne!(class, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_test_outside_the_test_crate_root_is_not_credited() -> Result<(), String> {
    // A module child resolves paths from a parent file this scan does not
    // read, so the crate name is not established there.
    let class = TempRepo::create(
        "shipping",
        &[
            ("src/lib.rs", LIB),
            ("tests/suite/main.rs", "mod cases;\n"),
            ("tests/suite/cases.rs", &test_file("", QUALIFIED_PIN)),
        ],
        PATCH,
    )?
    .computed_finding()?
    .class;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}
