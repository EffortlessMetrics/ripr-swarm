//! End-to-end witness for #7858 (RIPR-SPEC-0225 decision 9).
//!
//! A field constructed inside an array the owner returns reads `exposed`
//! when a test pins that element alone: by its literal index, or through an
//! `.any(|r| r.field == literal)` membership that no other element can
//! satisfy. The `total: 33` decoy, where another element already holds the
//! literal, passes whatever the changed element holds and keeps the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with the returned array on line 6, built from `first` and
/// the changed `total: a + b + 0`, then one test with `body` after `items`
/// declared in the test module.
fn source_with(first: &str, items: &str, body: &str) -> String {
    format!(
        "pub struct Row {{
    pub total: u32,
}}

pub fn rows(a: u32, b: u32) -> [Row; 2] {{
    [Row {{ total: {first} }}, Row {{ total: a + b + 0 }}]
}}

#[cfg(test)]
mod tests {{
    use super::*;
{items}
    #[test]
    fn rows_carry_the_sum() {{
{body}
    }}
}}
"
    )
}

/// Line 6 changed from `a + b` to `a + b + 0` in the second element.
fn diff(first: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5,3 +5,3 @@
 pub fn rows(a: u32, b: u32) -> [Row; 2] {{
-    [Row {{ total: {first} }}, Row {{ total: a + b }}]
+    [Row {{ total: {first} }}, Row {{ total: a + b + 0 }}]
 }}
"
    )
}

const ANY: &str = "        let rows = rows(30, 3);
        assert!(rows.iter().any(|r| r.total == 33));";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        Self::create_with(lib, diff, "", &[])
    }

    /// `dependencies` is appended to the manifest after `[workspace]`;
    /// `files` are written under `src/`.
    fn create_with(
        lib: &str,
        diff: &str,
        dependencies: &str,
        files: &[(&str, &str)],
    ) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-returned-element-field-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"element-field\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n{dependencies}"
            ),
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        for (path, text) in files {
            let path = root.join("src").join(path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {parent:?} failed: {error}"))?;
            }
            std::fs::write(&path, text)
                .map_err(|error| format!("write {path:?} failed: {error}"))?;
        }
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The class of the `field_construction` finding on line 6. The premise is
/// that the finding exists and the related test reaches the owner.
fn field_class(first: &str, body: &str) -> Result<ExposureClass, String> {
    field_class_with(first, "", body)
}

fn field_class_with(first: &str, items: &str, body: &str) -> Result<ExposureClass, String> {
    field_class_of(&source_with(first, items, body), first)
}

fn field_class_of(lib: &str, first: &str) -> Result<ExposureClass, String> {
    field_class_in(lib, &diff(first))
}

fn field_class_in(lib: &str, diff: &str) -> Result<ExposureClass, String> {
    field_class_manifest(lib, diff, "", &[])
}

fn field_class_manifest(
    lib: &str,
    diff: &str,
    dependencies: &str,
    files: &[(&str, &str)],
) -> Result<ExposureClass, String> {
    let findings = TempRepo::create_with(lib, diff, dependencies, files)?.findings()?;
    let field = findings
        .iter()
        .find(|finding| {
            finding.probe.location.line == 6
                && finding.probe.family == ProbeFamily::FieldConstruction
        })
        .ok_or_else(|| {
            format!("premise: a field_construction finding on line 6, got {findings:?}")
        })?;
    if field.ripr.reach.summary.contains("rows_carry_the_sum") {
        Ok(field.class.clone())
    } else {
        Err(format!(
            "premise: the test reaches the owner, got {:?}",
            field.ripr.reach
        ))
    }
}

#[test]
fn a_test_pinning_the_changed_element_exposes_its_field() -> Result<(), String> {
    for body in [
        ANY,
        "        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33);",
        "        let rows = rows(30, 3);\n        let r = &rows[1];\n        assert_eq!(r.total, 33);",
    ] {
        assert_eq!(field_class("0", body)?, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_membership_or_read_another_element_may_satisfy_keeps_the_gap() -> Result<(), String> {
    for (first, body, label) in [
        ("33", ANY, "decoy: the first element already holds 33"),
        ("a", ANY, "the first element is computed"),
        (
            "0",
            "        let rows = rows(30, 3);\n        assert_eq!(rows[0].total, 0);",
            "index of the unchanged element",
        ),
        (
            "0",
            "        let rows = rows(30, 3);\n        let r = &rows[0];\n        assert_eq!(r.total, 0);",
            "alias of the unchanged element",
        ),
        (
            "0",
            "        let rows = rows(30, 3);\n        let rows = [Row { total: 33 }];\n        assert!(rows.iter().any(|r| r.total == 33));",
            "shadowed receiver",
        ),
    ] {
        assert_eq!(
            field_class(first, body)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

/// Each of these tests passes without running a check of the changed
/// element, so none observes it (#8059 review).
#[test]
fn a_test_that_may_skip_or_fake_the_assertion_keeps_the_gap() -> Result<(), String> {
    const INDEX: &str = "        assert_eq!(rows[1].total, 33);";
    for (items, body, label) in [
        (
            "    macro_rules! fake {\n        ($n:ident) => { let $n = [Row { total: 33 }, Row { total: 33 }]; };\n    }\n",
            format!("        let rows = rows(1, 1);\n        fake!(rows);\n{INDEX}"),
            "a macro statement rebinds the receiver",
        ),
        (
            "    macro_rules! assert_eq {\n        ($a:expr, $b:expr) => { let _ = (&$a, &$b); };\n    }\n",
            format!("        let rows = rows(1, 1);\n{INDEX}"),
            "a local assert_eq! that checks nothing",
        ),
        (
            "",
            format!("        let rows = rows(1, 1);\n        std::process::exit(0);\n{INDEX}"),
            "process::exit before the assertion",
        ),
    ] {
        assert_eq!(
            field_class_with("0", items, &body)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

/// A nested module's own `rows` takes the test's `super::rows(..)` call and
/// returns 33 in both elements, so the assertion passes whatever the owner's
/// changed element holds. A name shared by two indexed functions is not read
/// (#8059 review).
#[test]
fn a_call_another_function_of_the_owner_name_may_take_keeps_the_gap() -> Result<(), String> {
    let lib = "pub struct Row {
    pub total: u32,
}

pub fn rows(a: u32, b: u32) -> [Row; 2] {
    [Row { total: 0 }, Row { total: a + b + 0 }]
}

pub mod inner {
    use super::Row;

    pub fn rows(_a: u32, _b: u32) -> [Row; 2] {
        [Row { total: 33 }, Row { total: 33 }]
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn rows_carry_the_sum() {
            let rows = super::rows(30, 3);
            assert_eq!(rows[1].total, 33);
        }
    }
}
";
    assert_eq!(field_class_of(lib, "0")?, ExposureClass::WeaklyExposed);
    Ok(())
}

/// Each decoy lets the assertion pass through something other than the
/// changed element's integer field, so none observes it (#8059 review).
#[test]
fn a_decoy_the_shared_checks_catch_keeps_the_gap() -> Result<(), String> {
    for (items, body, label) in [
        (
            "    trait Decoy {\n        fn iter(&self) -> std::iter::Once<Row>;\n    }\n    impl Decoy for [Row; 2] {\n        fn iter(&self) -> std::iter::Once<Row> {\n            std::iter::once(Row { total: 33 })\n        }\n    }\n",
            ANY,
            "a trait supplies the array's iter",
        ),
        (
            "    fn decoy(_a: u32, _b: u32) -> [Row; 2] {\n        [Row { total: 33 }, Row { total: 33 }]\n    }\n    const rows: fn(u32, u32) -> [Row; 2] = decoy;\n",
            "        assert_eq!(rows(30, 3)[1].total, 33);",
            "a callable const takes the bare call",
        ),
    ] {
        assert_eq!(
            field_class_with("0", items, body)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn an_element_cfg_may_drop_or_a_custom_equality_keeps_the_gap() -> Result<(), String> {
    let cfg = TempRepo::create(
        "pub struct Row {
    pub total: u32,
}

pub fn rows(a: u32, b: u32) -> [Row; 3] {
    [#[cfg(feature = \"x\")] Row { total: 0 }, Row { total: a + b + 0 }, Row { total: 33 }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_carry_the_sum() {
        assert_eq!(rows(30, 3)[1].total, 33);
    }
}
",
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5,3 +5,3 @@
 pub fn rows(a: u32, b: u32) -> [Row; 3] {
-    [#[cfg(feature = \"x\")] Row { total: 0 }, Row { total: a + b }, Row { total: 33 }]
+    [#[cfg(feature = \"x\")] Row { total: 0 }, Row { total: a + b + 0 }, Row { total: 33 }]
 }
",
    )?;
    let findings = cfg.findings()?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.probe.location.line == 6
                && finding.probe.family == ProbeFamily::FieldConstruction
                && finding.class != ExposureClass::Exposed),
        "a cfg-dropped element shifts the index: {findings:?}"
    );
    let equality = "pub struct Value(pub u32);

pub struct Row {
    pub total: Value,
}

pub fn rows(a: Value, b: Value) -> [Row; 2] {
    [Row { total: Value(0) }, Row { total: a + b + 0 }]
}

impl PartialEq<i32> for Value {
    fn eq(&self, _: &i32) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Value = Value(30);
    const B: Value = Value(3);

    #[test]
    fn rows_carry_the_sum() {
        assert_eq!(rows(A, B)[1].total, 33);
    }
}
";
    let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -7,3 +7,3 @@
 pub fn rows(a: Value, b: Value) -> [Row; 2] {
-    [Row { total: Value(0) }, Row { total: a + b }]
+    [Row { total: Value(0) }, Row { total: a + b + 0 }]
 }
";
    let findings = TempRepo::create(equality, diff)?.findings()?;
    let field = findings
        .iter()
        .find(|finding| {
            finding.probe.location.line == 8
                && finding.probe.family == ProbeFamily::FieldConstruction
        })
        .ok_or_else(|| {
            format!("premise: a field_construction finding on line 8, got {findings:?}")
        })?;
    assert_ne!(
        field.class,
        ExposureClass::Exposed,
        "a PartialEq that ignores the value"
    );
    Ok(())
}

/// The membership needs a closed world: a trait method or macro the index
/// cannot read in full may supply `iter` or `any`, and a sibling literal out
/// of the field's range wraps onto the expected value (#8059 review).
#[test]
fn a_membership_outside_a_closed_world_keeps_the_gap() -> Result<(), String> {
    for (first, items, label) in [
        (
            "0",
            "    const DECOY: [Row; 1] = [Row { total: 33 }];\n    trait Hijack {\n        fn/**/iter(&self) -> std::slice::Iter<'_, Row>;\n    }\n    impl Hijack for [Row; 2] {\n        fn/**/iter(&self) -> std::slice::Iter<'_, Row> {\n            DECOY.iter()\n        }\n    }\n",
            "a comment hides a trait iter",
        ),
        (
            "0",
            "    macro_rules! hijack {\n        ($m:ident) => {\n            trait Hijack {\n                fn $m(&self) -> std::iter::Once<Row>;\n            }\n            impl Hijack for [Row; 2] {\n                fn $m(&self) -> std::iter::Once<Row> {\n                    std::iter::once(Row { total: 33 })\n                }\n            }\n        };\n    }\n    hijack!(iter);\n",
            "a macro names the trait method",
        ),
        (
            "0",
            "    use hackdep::AnyHack;\n",
            "an outside crate's trait",
        ),
        (
            "0",
            "    #[path = \"../target/h.rs\"]\n    mod h;\n    use self::h::Hijack;\n",
            "a module the index does not read",
        ),
        (
            "0",
            "    #[cfg_attr(all(), path = \"../target/h.rs\")]\n    mod h;\n    use self::h::Hijack;\n",
            "a conditional attribute names the module path",
        ),
        (
            "0",
            "    macro_rules! bring {\n        () => {\n            use hackdep::AnyHack;\n        };\n    }\n    bring!();\n",
            "a macro body imports an outside trait",
        ),
        (
            "0",
            "    std::include!(\"../target/h.rs\");\n",
            "a qualified include the index does not read",
        ),
        ("4294967329", "", "a sibling literal that wraps onto 33"),
    ] {
        assert_eq!(
            field_class_with(first, items, ANY)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

/// The struct the owner's literals name is a macro-generated `Row` whose
/// field compares through an always-true `PartialEq`; a plain decoy `Row`
/// in another module must not stand in for it (#8059 review).
#[test]
fn a_decoy_declaration_of_the_struct_keeps_the_gap() -> Result<(), String> {
    for decoy in [
        "mod decoy {\n    pub struct Row {\n        pub total: u32,\n    }\n}",
        "#[cfg(any())]\npub struct Row {\n    pub total: u32,\n}",
    ] {
        let lib = format!(
            "macro_rules! real {{ ($n:ident) => {{ pub struct $n {{ pub total: W }} }}; }}
real!(Row);
pub struct W(pub u32);

pub fn rows(a: u32, b: u32) -> [Row; 2] {{
    [Row {{ total: W(0) }}, Row {{ total: W(a + b + 0) }}]
}}

impl PartialEq<u32> for W {{
    fn eq(&self, _: &u32) -> bool {{
        true
    }}
}}

{decoy}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn rows_carry_the_sum() {{
        let rows = rows(30, 3);
        assert_eq!(rows[1].total, 33);
    }}
}}
"
        );
        let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -5,3 +5,3 @@
 pub fn rows(a: u32, b: u32) -> [Row; 2] {
-    [Row { total: W(0) }, Row { total: W(a + b) }]
+    [Row { total: W(0) }, Row { total: W(a + b + 0) }]
 }
";
        let findings = TempRepo::create(&lib, diff)?.findings()?;
        let field = findings
            .iter()
            .find(|finding| {
                finding.probe.location.line == 6
                    && finding.probe.family == ProbeFamily::FieldConstruction
            })
            .ok_or_else(|| {
                format!("premise: a field_construction finding on line 6, got {findings:?}")
            })?;
        assert_ne!(field.class, ExposureClass::Exposed, "{decoy}");
    }
    Ok(())
}

/// A macro may declare a type under a primitive's name, or write an
/// iteration method from tokens no scan reads as one, and a dependency may
/// bring a trait under a name the source reads as the standard library's;
/// each keeps the gap (#8059 re-review G1, G2, G5, G6).
#[test]
fn a_macro_or_outside_crate_that_may_supply_the_name_keeps_the_gap() -> Result<(), String> {
    const INDEX: &str = "        assert_eq!(rows(30, 3)[1].total, 33);";
    for (tail, label) in [
        (
            "macro_rules! shadow {\n    ($n:ident) => {\n        pub struct $n(pub i64);\n    };\n}\nshadow!(u32);\n",
            "a macro declares a local u32",
        ),
        (
            "macro_rules! pass {\n    ($k:tt $n:ident) => {\n        pub $k $n(pub i64);\n    };\n}\npass!(struct u32);\n",
            "a macro is handed the keyword and the name",
        ),
        (
            "outside::declare!();\n",
            "an item-level macro the workspace does not define",
        ),
    ] {
        let lib = format!("{}{tail}", source_with("0", "", INDEX));
        assert_eq!(
            field_class_of(&lib, "0")?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    assert_eq!(
        field_class_with(
            "0",
            "    const DECOY: [Row; 1] = [Row { total: 33 }];\n    macro_rules! hijack {\n        ($kw:tt) => {\n            trait Hijack {\n                $kw iter(&self) -> std::slice::Iter<'static, Row>;\n            }\n            impl Hijack for [Row; 2] {\n                $kw iter(&self) -> std::slice::Iter<'static, Row> {\n                    DECOY.iter()\n                }\n            }\n        };\n    }\n    hijack!(fn);\n",
            ANY,
        )?,
        ExposureClass::WeaklyExposed,
        "a macro is handed `fn`"
    );
    for (dependency, root) in [
        (
            "alloc = { package = \"hackdep\", path = \"../hackdep\" }",
            "alloc",
        ),
        (
            "clippy = { package = \"hackdep\", path = \"../hackdep\" }",
            "clippy",
        ),
        ("hackdep = \"1\"", "hackdep"),
    ] {
        let lib = source_with("0", &format!("    use {root}::AnyHack;\n"), ANY);
        assert_eq!(
            field_class_manifest(
                &lib,
                &diff("0"),
                &format!("\n[dependencies]\n{dependency}\n"),
                &[]
            )?,
            ExposureClass::WeaklyExposed,
            "{dependency}"
        );
    }
    Ok(())
}

/// A lib tail, test body, manifest dependencies, extra `src/` files and label.
type OutsideCase<'a> = (&'a str, &'a str, &'a str, &'a [(&'a str, &'a str)], &'a str);

/// The indexed form needs the closed workspace too: a macro alias, a module
/// in a directory the index skips, or an outside crate's macro may declare a
/// local `u32`, and a skipped module may bring the membership's trait
/// (#8059 re-review H1, H7, H8, H15, H16).
#[test]
fn a_name_from_outside_the_scanned_workspace_keeps_the_gap() -> Result<(), String> {
    const INDEX: &str = "        assert_eq!(rows(30, 3)[1].total, 33);";
    const SHADOW: &str = "pub struct u32(pub i64);\nimpl PartialEq<i32> for u32 {\n    fn eq(&self, _: &i32) -> bool {\n        true\n    }\n}\n";
    const DEP: &str = "\n[dependencies]\nshadowdep = { path = \"../shadowdep\" }\n";
    let cases: [OutsideCase<'_>; 7] = [
        (
            "mod w {\n    pub struct W(pub i64);\n}\nmacro_rules! alias {\n    ($n:ident) => {\n        use crate::w::W as $n;\n    };\n}\nalias!(u32);\n",
            INDEX,
            "",
            &[],
            "a macro aliases a type as u32",
        ),
        (
            "mod fixtures;\nuse self::fixtures::*;\n",
            INDEX,
            "",
            &[("fixtures/mod.rs", SHADOW)],
            "a skipped directory declares u32",
        ),
        (
            "mod fixtures;\n",
            ANY,
            "",
            &[(
                "fixtures/mod.rs",
                "pub trait Hijack {\n    fn iter(&self) -> std::slice::Iter<'_, crate::Row>;\n}\n",
            )],
            "a skipped directory supplies a trait iter",
        ),
        (
            "macro_rules! shadow {\n    () => {};\n}\nshadowdep::shadow!(u32);\n",
            INDEX,
            DEP,
            &[],
            "an outside macro shares a workspace macro's name",
        ),
        (
            "macro_rules! wrap {\n    () => {\n        shadowdep::shadow!(u32);\n    };\n}\nwrap!();\n",
            INDEX,
            DEP,
            &[],
            "a workspace macro calls an outside one",
        ),
        (
            "mod fixtures {\n    pub mod evil;\n}\nuse self::fixtures::evil::*;\n",
            INDEX,
            "",
            &[("fixtures/evil.rs", SHADOW)],
            "an inline module declares a module in a skipped directory",
        ),
        (
            "mod m;\nuse self::m::lib::fixtures::*;\n",
            INDEX,
            "",
            &[
                ("m.rs", "pub mod lib;\n"),
                ("m/lib.rs", "pub mod fixtures;\n"),
                ("m/lib/fixtures/mod.rs", SHADOW),
                ("m/fixtures.rs", ""),
            ],
            "a module named lib declares one in a skipped directory",
        ),
    ];
    for (tail, body, dependencies, files, label) in cases {
        let lib = format!("{}{tail}", source_with("0", "", body));
        assert_eq!(
            field_class_manifest(&lib, &diff("0"), dependencies, files)?,
            ExposureClass::WeaklyExposed,
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn a_non_plain_drop_on_the_element_type_keeps_the_gap() -> Result<(), String> {
    // #8084: when the assertion fails, unwinding drops the receiver; a
    // `Drop` body that exits the process turns the failure into a pass.
    let lib = |drop_body: &str| {
        format!(
            "pub struct Row {{\n    pub total: u32,\n}}\n\nimpl Drop for Row {{\n    fn drop(&mut self) {{{drop_body}\n    }}\n}}\n\npub fn rows(a: u32, b: u32) -> [Row; 2] {{\n    [Row {{ total: 0 }}, Row {{ total: a + b + 0 }}]\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn pins_second_row() {{\n        let rows = rows(30, 3);\n        assert_eq!(rows[1].total, 33);\n    }}\n}}\n"
        )
    };
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11,3 +11,3 @@\n pub fn rows(a: u32, b: u32) -> [Row; 2] {\n-    [Row { total: 0 }, Row { total: a + b }]\n+    [Row { total: 0 }, Row { total: a + b + 0 }]\n }\n";
    let class = |drop_body: &str| -> Result<ExposureClass, String> {
        let findings = TempRepo::create(&lib(drop_body), diff)?.findings()?;
        let field = findings
            .iter()
            .find(|finding| finding.probe.family == ProbeFamily::FieldConstruction)
            .ok_or_else(|| format!("premise: a field_construction finding, got {findings:?}"))?;
        Ok(field.class.clone())
    };
    assert_eq!(
        class("\n        // no-op")?,
        ExposureClass::Exposed,
        "an empty Drop body keeps the pin exposed"
    );
    assert_eq!(
        class("\n        std::process::exit(0);")?,
        ExposureClass::WeaklyExposed,
        "a Drop body that exits cannot leave the pin exposed"
    );
    Ok(())
}
