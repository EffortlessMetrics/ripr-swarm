//! Public-API controls for `debug_assert_eq!` (#8051, RIPR-SPEC-0240).
//!
//! A test that calls the owner and pins its whole result with
//! `debug_assert_eq!` discriminates under the default `cargo test` profile and
//! never under `--release`, so a return-value finding reads `static_unknown`
//! with `rust_debug_assertion_profile_unresolved`: neither a missing assertion
//! nor credit. A standard `assert_eq!` keeps its credit. Other `debug_` checks,
//! an assertion that does not pin the owner, an ignored test, and other probe
//! families keep the gap, and a `debug_` check never earns `exposed`.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with `one` on lines 1-3 and a test with `attrs` and `body`.
fn source(attrs: &str, body: &str) -> String {
    format!(
        "pub fn one() -> u32 {{
    2 - 1
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    {attrs}
    fn one_is_one() {{
        {body}
    }}
}}
"
    )
}

/// The returned expression on line 2 changed.
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn one() -> u32 {
-    1
+    2 - 1
 }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-debug-assertion-limit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"debug-assertion\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn check(repo: &TempRepo) -> Result<ripr::CheckOutput, String> {
    check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })
}

/// The `return_value` finding on line 2, which must be the only one.
fn return_finding(attrs: &str, body: &str) -> Result<Finding, String> {
    let repo = TempRepo::create(&source(attrs, body), DIFF)?;
    let output = check(&repo)?;
    let mut on_line = output.findings.iter().filter(|finding| {
        finding.probe.location.line == 2 && finding.probe.family == ripr::ProbeFamily::ReturnValue
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one return_value finding on line 2, got {other:?} in {:?}",
            output.findings
        )),
    }
}

#[test]
fn a_debug_assert_eq_owner_pin_reads_as_a_profile_limit() -> Result<(), String> {
    for body in [
        "debug_assert_eq!(one(), 1);",
        "debug_assert_eq!(1, one());",
        "std::debug_assert_eq!(one(), 1);",
        "debug_assert_eq!(one(), 1, \"one, as ever\");",
        "debug_assert_eq!(one(), 1, \"x(y\");",
    ] {
        let finding = return_finding("", body)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{body}");
        assert_eq!(
            finding.static_limit_kind.map(|kind| kind.as_str()),
            Some("rust_debug_assertion_profile_unresolved"),
            "{body}"
        );
        assert!(
            finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("`one_is_one`") && step.contains("assert_eq!")),
            "{body}: {:?}",
            finding.recommended_next_step
        );
    }
    Ok(())
}

#[test]
fn standard_assertions_and_other_debug_checks_keep_their_class() -> Result<(), String> {
    // The standard macro is credited.
    let finding = return_finding("", "assert_eq!(one(), 1);")?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None);
    for (attrs, body) in [
        // Passes for every value other than the one named.
        ("", "debug_assert_ne!(one(), 2);"),
        // Not an equality pin of the result.
        ("", "debug_assert!(one() == 1);"),
        ("", "debug_assert!(one() > 0);"),
        // The equality does not compare the owner's result.
        (
            "",
            "let v = one(); debug_assert_eq!(2 + 2, 4); assert!(v > 0);",
        ),
        ("", "debug_assert_eq!(one() + 1, 2);"),
        ("", "debug_assert_eq!(one(), one());"),
        // Another macro that merely ends in the name.
        ("", "let _ = one(); other::debug_assert_eq!(one(), 1);"),
        // An ignored test never runs by default.
        ("#[ignore]", "debug_assert_eq!(one(), 1);"),
        // Checks that never run: a closure never called, a dead branch, a
        // nested helper never called.
        ("", "let _ = one(); let _c = || debug_assert_eq!(one(), 1);"),
        (
            "",
            "let _ = one(); if false { debug_assert_eq!(one(), 1); }",
        ),
        (
            "",
            "let _ = one(); fn helper() { debug_assert_eq!(one(), 1); }",
        ),
    ] {
        let finding = return_finding(attrs, body)?;
        assert!(
            matches!(
                finding.class,
                ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
            ),
            "{attrs} {body}: {finding:?}"
        );
        assert_eq!(finding.static_limit_kind, None, "{attrs} {body}");
    }
    Ok(())
}

/// Predicate, match-arm and field-construction changes each checked only by
/// `debug_assert_eq!`: lines 2, 7 and 16 of [`FAMILIES_SOURCE`].
const FAMILIES_SOURCE: &str = "pub fn is_big(x: u32) -> bool {
    x > 10
}

pub fn label(v: Option<u32>) -> u32 {
    match v {
        Some(v) => v + 2,
        None => 0,
    }
}

pub struct C {
    pub retries: u32,
}

pub fn build() -> C {
    C { retries: 4 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_is_not_big() {
        debug_assert_eq!(is_big(10), false);
    }

    #[test]
    fn some_one_labels_three() {
        debug_assert_eq!(label(Some(1)), 3);
    }

    #[test]
    fn build_retries_four_times() {
        debug_assert_eq!(build().retries, 4);
    }
}
";

const FAMILIES_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn is_big(x: u32) -> bool {
-    x >= 10
+    x > 10
 }
@@ -6,3 +6,3 @@ pub fn label(v: Option<u32>) -> u32 {
     match v {
-        Some(v) => v + 1,
+        Some(v) => v + 2,
         None => 0,
@@ -15,3 +15,3 @@ pub struct C {
 pub fn build() -> C {
-    C { retries: 3 }
+    C { retries: 4 }
 }
";

#[test]
fn a_debug_check_never_earns_exposed_in_any_family() -> Result<(), String> {
    let repo = TempRepo::create(FAMILIES_SOURCE, FAMILIES_DIFF)?;
    let output = check(&repo)?;
    let mut families = Vec::new();
    for finding in &output.findings {
        families.push(finding.probe.family.clone());
        assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
        if !matches!(
            finding.probe.family,
            ripr::ProbeFamily::ReturnValue | ripr::ProbeFamily::ErrorPath
        ) {
            assert_ne!(
                finding.static_limit_kind.map(|kind| kind.as_str()),
                Some("rust_debug_assertion_profile_unresolved"),
                "{finding:?}"
            );
        }
    }
    // Premise: the changed predicate, arm and field each produced a finding.
    for family in [
        ripr::ProbeFamily::Predicate,
        ripr::ProbeFamily::MatchArm,
        ripr::ProbeFamily::FieldConstruction,
    ] {
        assert!(
            families.contains(&family),
            "{family:?} missing from {families:?}"
        );
    }
    Ok(())
}

/// `check` on lines 3-8: an early `Err` exit (line 5) beside an `Ok` tail.
fn check_source(body: &str) -> String {
    format!(
        "pub enum E {{ Bad, Worse }}

pub fn check(x: u32) -> Result<u32, E> {{
    if x > 10 {{
        return Err(E::Worse);
    }}
    Ok(x)
}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn checks() {{
        {body}
    }}
}}
"
    )
}

/// The early exit's variant changed on line 5.
const CHECK_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -4,3 +4,3 @@ pub fn check(x: u32) -> Result<u32, E> {
     if x > 10 {
-        return Err(E::Bad);
+        return Err(E::Worse);
     }
";

fn line_five_limits(body: &str) -> Result<Vec<(ExposureClass, Option<&'static str>)>, String> {
    let repo = TempRepo::create(&check_source(body), CHECK_DIFF)?;
    let output = check(&repo)?;
    let on_line: Vec<_> = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.line == 5)
        .map(|finding| {
            (
                finding.class.clone(),
                finding.static_limit_kind.map(|kind| kind.as_str()),
            )
        })
        .collect();
    if on_line.is_empty() {
        return Err(format!(
            "premise: a finding on line 5, got {:?}",
            output.findings
        ));
    }
    Ok(on_line)
}

#[test]
fn an_error_path_pin_reads_as_a_profile_limit_only_when_it_names_the_variant() -> Result<(), String>
{
    let pinned = line_five_limits("debug_assert_eq!(check(11), Err(E::Worse));")?;
    assert!(
        pinned.contains(&(
            ExposureClass::StaticUnknown,
            Some("rust_debug_assertion_profile_unresolved")
        )),
        "{pinned:?}"
    );
    // The pinned value is the `Ok` tail: the changed exit never produces it,
    // and the same `assert_eq!` keeps its gap too.
    for body in [
        "debug_assert_eq!(check(1), Ok(1));",
        "assert_eq!(check(1), Ok(1));",
    ] {
        let kept = line_five_limits(body)?;
        assert!(
            kept.iter().all(|(class, limit)| {
                *class != ExposureClass::Exposed
                    && *limit != Some("rust_debug_assertion_profile_unresolved")
            }),
            "{body}: {kept:?}"
        );
    }
    Ok(())
}

/// `div` panics on 0 at line 3; the guard changed.
const PANIC_SOURCE: &str = "pub fn div(x: u32) -> u32 {
    if x < 1 {
        panic!(\"zero\");
    }
    100 / x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn zero_panics() {
        debug_assert_ne!(div(0), 1);
    }
}
";

const PANIC_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn div(x: u32) -> u32 {
-    if x == 0 {
+    if x < 1 {
         panic!(\"zero\");
";

#[test]
fn a_should_panic_test_with_a_debug_check_keeps_its_panic_outcome_limit() -> Result<(), String> {
    let repo = TempRepo::create(PANIC_SOURCE, PANIC_DIFF)?;
    let output = check(&repo)?;
    let predicate = output
        .findings
        .iter()
        .find(|finding| {
            finding.probe.location.line == 2 && finding.probe.family == ripr::ProbeFamily::Predicate
        })
        .ok_or_else(|| {
            format!(
                "premise: a predicate finding on line 2 in {:?}",
                output.findings
            )
        })?;
    assert_eq!(
        predicate.class,
        ExposureClass::StaticUnknown,
        "{predicate:?}"
    );
    assert_eq!(
        predicate.static_limit_kind.map(|kind| kind.as_str()),
        Some("rust_panic_outcome_unresolved")
    );
    Ok(())
}
