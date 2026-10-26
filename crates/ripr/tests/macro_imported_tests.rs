//! Public-API controls for `macro_rules!` test generators defined in
//! another file of the same crate (#6925).
//!
//! A table invoked in `tests/cases.rs` reads like the hand-written test
//! when its generator reaches the file through textual scope: a
//! `#[macro_use] mod` of the invoking file, or a parent module that defines
//! it before the `mod` declaration. A module the resolver cannot name
//! exactly adds no test.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const GATE: &str = "pub fn gate(input: u32) -> bool {\n    input >= 10\n}\n";

const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn gate(input: u32) -> bool {
-    input > 10
+    input >= 10
 }
";

const USE_GATE: &str = "use macro_imported_tests::gate;\n";

const TABLE: &str = "macro_rules! gate_table {\n    ($($name:ident($input:expr, $want:expr))*) => {\n        $(\n            #[test]\n            fn $name() {\n                assert_eq!(gate($input), $want);\n            }\n        )*\n    };\n}\n";

const BOUNDARY_ROWS: &str =
    "gate_table! {\n    nine_is_closed(9, false)\n    ten_is_open(10, true)\n}\n";
const FAR_ROWS: &str = "gate_table! {\n    zero(0, false)\n    far(50, true)\n}\n";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(files: &[(&str, String)]) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-macro-imported-tests-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let write = |relative: &str, text: &str| -> Result<(), String> {
            let path = root.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {} failed: {error}", parent.display()))?;
            }
            std::fs::write(&path, text)
                .map_err(|error| format!("write {} failed: {error}", path.display()))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"macro-imported-tests\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", GATE)?;
        write("diff.patch", DIFF)?;
        for (relative, text) in files {
            write(relative, text)?;
        }
        Ok(Self { root })
    }

    /// The finding for the changed `input >= 10` predicate.
    fn predicate(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut predicates = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::Predicate && finding.probe.location.line == 2
        });
        match (predicates.next(), predicates.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one predicate finding on line 2, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn state(stage: &impl std::fmt::Debug) -> String {
    format!("{stage:?}")
}

/// `tests/cases.rs` invokes `rows`; `tests/macros.rs` defines the table.
fn layout(main: &str, macros: Option<&str>, cases: &str) -> Vec<(&'static str, String)> {
    let mut files = vec![
        ("tests/main.rs", main.to_string()),
        ("tests/cases.rs", format!("{USE_GATE}{cases}")),
    ];
    if let Some(macros) = macros {
        files.push(("tests/macros.rs", macros.to_string()));
    }
    files
}

fn related_names(finding: &Finding) -> Vec<&str> {
    let mut names: Vec<_> = finding
        .related_tests
        .iter()
        .map(|test| test.name.as_str())
        .collect();
    names.sort_unstable();
    names
}

#[test]
fn a_table_from_a_macro_use_module_reads_like_the_hand_written_test() -> Result<(), String> {
    // The generator lives in `tests/macros.rs`; the invoking file imports
    // it with `#[macro_use] mod macros;`.
    let invoked_here = TempRepo::create(&[
        (
            "tests/main.rs",
            format!("#[macro_use]\nmod macros;\n{USE_GATE}{BOUNDARY_ROWS}"),
        ),
        ("tests/macros.rs", TABLE.to_string()),
    ])?
    .predicate()?;
    assert_eq!(
        state(&invoked_here.ripr.reach.state),
        "Yes",
        "{invoked_here:?}"
    );
    assert_eq!(
        invoked_here.class,
        ExposureClass::Exposed,
        "{invoked_here:?}"
    );
    assert_eq!(
        related_names(&invoked_here),
        vec!["nine_is_closed", "ten_is_open"],
        "{invoked_here:?}"
    );

    // Rows that never reach the edge read like the same hand-written tests.
    let far = TempRepo::create(&[
        (
            "tests/main.rs",
            format!("#[macro_use]\nmod macros;\n{USE_GATE}{FAR_ROWS}"),
        ),
        ("tests/macros.rs", TABLE.to_string()),
    ])?
    .predicate()?;
    assert_eq!(state(&far.ripr.reach.state), "Yes", "{far:?}");
    assert_ne!(far.class, ExposureClass::Exposed, "{far:?}");
    Ok(())
}

#[test]
fn a_table_in_a_child_module_sees_the_macros_its_parent_defines_first() -> Result<(), String> {
    let defined_in_parent = TempRepo::create(&layout(
        &format!("{TABLE}mod cases;\n"),
        None,
        BOUNDARY_ROWS,
    ))?
    .predicate()?;
    assert_eq!(
        defined_in_parent.class,
        ExposureClass::Exposed,
        "{defined_in_parent:?}"
    );
    assert_eq!(
        related_names(&defined_in_parent),
        vec!["nine_is_closed", "ten_is_open"],
        "{defined_in_parent:?}"
    );

    // The usual layout: the parent imports the generator with
    // `#[macro_use]`, then declares the module that invokes it.
    let through_macro_use = TempRepo::create(&layout(
        "#[macro_use]\nmod macros;\nmod cases;\n",
        Some(TABLE),
        BOUNDARY_ROWS,
    ))?
    .predicate()?;
    assert_eq!(
        through_macro_use.class,
        ExposureClass::Exposed,
        "{through_macro_use:?}"
    );
    Ok(())
}

#[test]
fn a_module_the_resolver_cannot_name_exactly_adds_no_test() -> Result<(), String> {
    for (label, main, macros) in [
        // The module is declared before the macro exists: rustc rejects
        // the invocation, and no definition is in scope to credit.
        (
            "declared before the definition",
            format!("mod cases;\n{TABLE}"),
            None,
        ),
        // `mod macros;` has no `#[macro_use]`: its macros stay inside it.
        (
            "no macro_use",
            "mod macros;\nmod cases;\n".to_string(),
            Some(TABLE),
        ),
        // A `#[path]` module may be any file.
        (
            "path attribute",
            "#[macro_use]\n#[path = \"macros.rs\"]\nmod macros;\nmod cases;\n".to_string(),
            Some(TABLE),
        ),
        // `macros.rs` and `macros/mod.rs` both exist: no single file.
        (
            "two candidate files",
            "#[macro_use]\nmod macros;\nmod cases;\n".to_string(),
            Some(TABLE),
        ),
    ] {
        let mut files = layout(&main, macros, BOUNDARY_ROWS);
        if label == "two candidate files" {
            files.push(("tests/macros/mod.rs", TABLE.to_string()));
        }
        let refused = TempRepo::create(&files)?.predicate()?;
        assert!(refused.related_tests.is_empty(), "{label}: {refused:?}");
        assert_ne!(
            state(&refused.ripr.reach.state),
            "Yes",
            "{label}: {refused:?}"
        );
    }

    // Two test crates declare the same `mod cases;`: which macros the file
    // sees depends on the crate, so it inherits none.
    let mut shared = layout(&format!("{TABLE}mod cases;\n"), None, BOUNDARY_ROWS);
    shared.push(("tests/other.rs", format!("{TABLE}mod cases;\n")));
    let ambiguous = TempRepo::create(&shared)?.predicate()?;
    assert!(ambiguous.related_tests.is_empty(), "{ambiguous:?}");

    // The invoking file defines a generator of the same name as the
    // imported one: rustc picks the later definition, so neither is chosen.
    let shadowed = TempRepo::create(&layout(
        "#[macro_use]\nmod macros;\nmod cases;\n",
        Some(TABLE),
        &format!("{TABLE}{BOUNDARY_ROWS}"),
    ))?
    .predicate()?;
    assert!(shadowed.related_tests.is_empty(), "{shadowed:?}");
    Ok(())
}

fn exported_table() -> String {
    format!("#[macro_export]\n{TABLE}")
}

#[test]
fn an_exported_generator_named_by_path_reads_like_the_hand_written_test() -> Result<(), String> {
    // `tests/macros.rs` exports the table; `tests/cases.rs` names it with
    // `use crate::gate_table;` and no `#[macro_use]` anywhere.
    let by_path = TempRepo::create(&layout(
        "mod macros;\nmod cases;\n",
        Some(&exported_table()),
        &format!("use crate::gate_table;\n{BOUNDARY_ROWS}"),
    ))?
    .predicate()?;
    assert_eq!(by_path.class, ExposureClass::Exposed, "{by_path:?}");
    assert_eq!(
        related_names(&by_path),
        vec!["nine_is_closed", "ten_is_open"],
        "{by_path:?}"
    );

    // Without the import the name is not in scope for rustc either.
    let unimported = TempRepo::create(&layout(
        "mod macros;\nmod cases;\n",
        Some(&exported_table()),
        BOUNDARY_ROWS,
    ))?
    .predicate()?;
    assert!(unimported.related_tests.is_empty(), "{unimported:?}");
    Ok(())
}

#[test]
fn an_exported_generator_called_by_crate_path_needs_no_import() -> Result<(), String> {
    let prefixed = BOUNDARY_ROWS.replace("gate_table!", "crate::gate_table!");
    assert_ne!(prefixed, BOUNDARY_ROWS, "premise: the rows call the table");
    let by_path = TempRepo::create(&layout(
        "mod macros;\nmod cases;\n",
        Some(&exported_table()),
        &prefixed,
    ))?
    .predicate()?;
    assert_eq!(by_path.class, ExposureClass::Exposed, "{by_path:?}");
    assert_eq!(
        related_names(&by_path),
        vec!["nine_is_closed", "ten_is_open"],
        "{by_path:?}"
    );

    // A bare call with no import could be a glob or extern-crate macro.
    let bare_and_prefixed = format!("{prefixed}{BOUNDARY_ROWS}");
    let mixed = TempRepo::create(&layout(
        "mod macros;\nmod cases;\n",
        Some(&exported_table()),
        &bare_and_prefixed,
    ))?
    .predicate()?;
    assert!(mixed.related_tests.is_empty(), "{mixed:?}");
    Ok(())
}

#[test]
fn an_exported_generator_that_is_not_the_only_one_adds_no_test() -> Result<(), String> {
    let invoking = format!("use crate::gate_table;\n{BOUNDARY_ROWS}");
    for (label, extra) in [
        ("second export", exported_table()),
        ("local definition of the name", TABLE.to_string()),
        (
            "export the scope cannot list",
            format!("mod inner {{\n{}}}\n", exported_table()),
        ),
    ] {
        let mut files = layout("mod macros;\nmod other;\nmod cases;\n", None, &invoking);
        files.push(("tests/macros.rs", exported_table()));
        files.push(("tests/other.rs", extra));
        let refused = TempRepo::create(&files)?.predicate()?;
        assert!(refused.related_tests.is_empty(), "{label}: {refused:?}");
    }

    // A macro that is not exported cannot be named by `crate::`.
    let private = TempRepo::create(&layout("mod macros;\nmod cases;\n", Some(TABLE), &invoking))?
        .predicate()?;
    assert!(private.related_tests.is_empty(), "{private:?}");
    Ok(())
}
