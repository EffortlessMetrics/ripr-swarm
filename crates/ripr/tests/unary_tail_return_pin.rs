//! Public-API controls for a prefix `-`/`!` deleted from or added to a
//! returned name (#14815, cargo-mutants' UnaryOperator genre).
//!
//! When the owner's whole body is the one-line tail `a` (was `-a` or `!a`),
//! the change reaches the owner's return. An exact pin at an input where the
//! two versions differ reads `exposed` and a bare call reads
//! `reachable_unrevealed`. `-a` and `a` agree at `0`, so an assertion at `0`,
//! or at an argument ripr cannot fold to a literal (a local, a loop row, a
//! constant), is never `exposed`. Neither is a diff whose old owner held more
//! than the replaced line.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` holding `owner` and one test whose body is `test_body`.
fn source(owner: &str, test_body: &str) -> String {
    format!(
        "{owner}

#[cfg(test)]
mod tests {{
    use super::*;

    const ZERO: i32 = 0;

    #[test]
    fn checks_owner() {{
        {test_body}
    }}
}}
"
    )
}

/// The owner `fn f(a: TY) -> TY { TAIL }` on lines 1-3.
fn owner(ty: &str, tail: &str) -> String {
    format!("pub fn f(a: {ty}) -> {ty} {{\n    {tail}\n}}")
}

/// A diff replacing the tail line 2 of [`owner`].
fn tail_diff(ty: &str, removed: &str, added: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn f(a: {ty}) -> {ty} {{
-    {removed}
+    {added}
 }}
"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-unary-tail-return-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"unary-tail\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
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

/// The one return-value finding for the changed tail `tail` on `line`.
fn tail_finding(findings: &[Finding], line: usize, tail: &str) -> Result<Finding, String> {
    let mut matching = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.line == line
            && finding.probe.expression.trim() == tail
    });
    match (matching.next(), matching.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one return_value finding for `{tail}` on line {line}, got {other:?} in {findings:?}"
        )),
    }
}

/// The class of the changed tail's finding when the owner's tail changes from
/// `removed` to `added` and the test body is `test_body`.
fn tail_class(
    ty: &str,
    removed: &str,
    added: &str,
    test_body: &str,
) -> Result<(ExposureClass, Finding), String> {
    let findings = TempRepo::create(
        &source(&owner(ty, added), test_body),
        &tail_diff(ty, removed, added),
    )?
    .findings()?;
    let finding = tail_finding(&findings, 2, added)?;
    Ok((finding.class.clone(), finding))
}

#[test]
fn an_exact_pin_where_the_versions_differ_exposes_a_deleted_or_added_minus() -> Result<(), String> {
    for (removed, added, test_body) in [
        ("-a", "a", "assert_eq!(f(5), 5);"),
        ("a", "-a", "assert_eq!(f(5), -5);"),
    ] {
        let (class, finding) = tail_class("i32", removed, added, test_body)?;
        assert_eq!(
            class,
            ExposureClass::Exposed,
            "{removed} -> {added}: {finding:?}"
        );
        assert!(
            finding
                .flow_sinks
                .iter()
                .any(|sink| sink.kind == ripr::domain::FlowSinkKind::ReturnValue
                    && sink.text == added),
            "the sink is the owner's return, named by the raw tail: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn an_exact_pin_exposes_a_deleted_or_added_negation() -> Result<(), String> {
    for (removed, added, test_body) in [
        ("!a", "a", "assert_eq!(f(true), true);"),
        ("a", "!a", "assert_eq!(f(true), false);"),
        // `!a` differs from `a` at every input, so even an argument ripr
        // cannot fold reaches the change: the old version returns `false`.
        ("!a", "a", "let d = true;\n        assert!(f(d));"),
    ] {
        let (class, finding) = tail_class("bool", removed, added, test_body)?;
        assert_eq!(
            class,
            ExposureClass::Exposed,
            "{removed} -> {added} / {test_body}: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn a_bare_call_reads_reachable_unrevealed() -> Result<(), String> {
    for (ty, removed, added, test_body) in [
        ("i32", "-a", "a", "let _ = f(5);"),
        ("bool", "!a", "a", "let _ = f(true);"),
    ] {
        let (class, finding) = tail_class(ty, removed, added, test_body)?;
        assert_eq!(
            class,
            ExposureClass::ReachableUnrevealed,
            "{removed} -> {added}: {finding:?}"
        );
    }
    Ok(())
}

/// `-a` and `a` are equal at `0`: a pin there cannot tell them apart.
#[test]
fn a_pin_where_minus_and_plain_agree_is_weakly_exposed() -> Result<(), String> {
    for (removed, added) in [("-a", "a"), ("a", "-a")] {
        let (class, finding) = tail_class("i32", removed, added, "assert_eq!(f(0), 0);")?;
        assert_eq!(
            class,
            ExposureClass::WeaklyExposed,
            "{removed} -> {added}: {finding:?}"
        );
    }
    Ok(())
}

/// The fold reads only literal arguments, so a local, a loop row or a
/// constant holding `0` must not let the pin read as separating.
#[test]
fn an_unfolded_argument_holding_zero_is_not_exposed() -> Result<(), String> {
    for test_body in [
        "let d = 0;\n        assert_eq!(f(d), 0);",
        "for (x, want) in [(0, 0)] {\n            assert_eq!(f(x), want);\n        }",
        "assert_eq!(f(ZERO), 0);",
    ] {
        let (class, finding) = tail_class("i32", "-a", "a", test_body)?;
        assert!(
            !matches!(class, ExposureClass::Exposed),
            "{test_body}: {finding:?}"
        );
    }
    Ok(())
}

/// The removed `-a` is not the whole old owner: the old owner also returned
/// early at `a == 5`, so `f(5) == 5` holds on both versions. Pairing the
/// removed tail with the added one must not stand in for the old function.
#[test]
fn an_old_owner_with_more_than_the_replaced_line_is_not_exposed() -> Result<(), String> {
    let lib = source(
        "pub fn f(a: i32) -> i32 {\n    // c\n    a\n}",
        "assert_eq!(f(5), 5);",
    );
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,4 @@
 pub fn f(a: i32) -> i32 {
-    if a == 5 { return 5; }
-    -a
+    // c
+    a
 }
";
    let findings = TempRepo::create(&lib, patch)?.findings()?;
    let finding = tail_finding(&findings, 3, "a")?;
    assert!(
        !matches!(finding.class, ExposureClass::Exposed),
        "{finding:?}"
    );
    // The same old owner with the comment kept as its own unchanged line:
    // the tail pairs positionally with `-a`, but the owner also lost a line.
    let lib = source(
        "pub fn f(a: i32) -> i32 {\n    a\n}",
        "assert_eq!(f(5), 5);",
    );
    let patch = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,3 @@
 pub fn f(a: i32) -> i32 {
-    if a == 5 { return 5; }
-    -a
+    a
 }
";
    let findings = TempRepo::create(&lib, patch)?.findings()?;
    let finding = tail_finding(&findings, 2, "a")?;
    assert!(
        !matches!(finding.class, ExposureClass::Exposed),
        "{finding:?}"
    );
    Ok(())
}

/// A float's `-0.0 == 0.0` and `NaN` are outside the integer fold, so a
/// float tail keeps the ordinary reading and is never credited here.
#[test]
fn a_float_tail_is_not_exposed() -> Result<(), String> {
    let (class, finding) = tail_class("f64", "-a", "a", "assert_eq!(f(5.0), 5.0);")?;
    assert!(!matches!(class, ExposureClass::Exposed), "{finding:?}");
    Ok(())
}
