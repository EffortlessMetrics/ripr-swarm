//! #5500: whole-tail header observation never supplies individual-arm selection.
#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "#[derive(Debug)]
struct Lease { timeout_sec: u64, settled_elapsed_ms: Option<u128> }
fn new_lease(timeout_sec: u64) -> Lease {
    Lease {
        timeout_sec,
        settled_elapsed_ms: None,
    }
}
fn charge(lease: &Lease) -> u64 {
    match lease.settled_elapsed_ms {
        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),
        None => lease.timeout_sec,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Lease { new_lease(600) }
    #[test]
    fn charge_uses_elapsed_and_retains_legacy_reservation() {
        let mut lease = fixture();
        assert_eq!(charge(&lease), 600);
        lease.settled_elapsed_ms = Some(1001);
        assert_eq!(charge(&lease), 2);
    }
    #[test]
    fn unrelated_owner_with_same_none_literal() {
        let value: Option<u32> = None;
        assert_eq!(value, None);
    }
}
";

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str, diff: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-whole-tail-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        )));
        std::fs::create_dir_all(fixture.0.join("src"))?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname='field_selector'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        )?;
        std::fs::write(fixture.0.join("src/lib.rs"), source)?;
        std::fs::write(fixture.0.join("input.diff"), diff)?;
        Ok(fixture)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("input.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn added_diff(source: &str) -> String {
    format!(
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -8,0 +9,{} @@\n{}",
        source.lines().count() - 8,
        source
            .lines()
            .skip(8)
            .map(|line| format!("+{line}\n"))
            .collect::<String>()
    )
}

#[test]
fn added_documented_owner_header_observes_its_whole_return()
-> Result<(), Box<dyn std::error::Error>> {
    for visibility in ["", "pub ", "pub(crate) "] {
        let source = SOURCE.replace(
            "fn charge(",
            &format!("/// Charge elapsed time.\n/// Retain reservations.\n{visibility}fn charge("),
        );
        let output = Fixture::new(&source, &added_diff(&source))?.check()?;
        let headers = output
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ripr::ProbeFamily::MatchArm
                    && finding.probe.expression == "match lease.settled_elapsed_ms"
            })
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 1, "one exact header subject: {visibility}");
        assert_eq!(
            headers[0].class,
            ExposureClass::Exposed,
            "{visibility}: {}",
            headers[0].id
        );
    }
    Ok(())
}

fn with_test(body: &str) -> String {
    let production = SOURCE.split("#[cfg(test)]").next().unwrap_or_default();
    format!(
        "{production}#[cfg(test)]\nmod tests {{ use super::*; #[test] fn observes() {{ {body} }} }}\n"
    )
}

fn header_class(source: &str, diff: &str) -> Result<ExposureClass, String> {
    let output = Fixture::new(source, diff)
        .map_err(|e| e.to_string())?
        .check()?;
    let headers = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ripr::ProbeFamily::MatchArm
                && finding.probe.expression == "match lease.settled_elapsed_ms"
        })
        .collect::<Vec<_>>();
    if headers.len() != 1 {
        return Err(format!("expected one header, got {}", headers.len()));
    }
    Ok(headers[0].class.clone())
}

#[test]
fn saturated_opaque_imported_input_observes_the_whole_result()
-> Result<(), Box<dyn std::error::Error>> {
    let source = with_test("let lease = new_lease(600); let mut lease = lease; lease.settled_elapsed_ms = Some(u128::MAX); assert_eq!(charge(&lease), u64::MAX);")
        .replace("#[derive(Debug)]\nstruct Lease { timeout_sec: u64, settled_elapsed_ms: Option<u128> }",
            "mod imported { #[derive(Debug)] pub struct Lease { pub timeout_sec: u64, pub settled_elapsed_ms: Option<u128> } }\nuse imported::Lease;");
    assert_eq!(
        header_class(&source, &added_diff(&source))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn whole_result_refuses_unobserved_or_ambiguous_assertions()
-> Result<(), Box<dyn std::error::Error>> {
    for (case, body) in [
        (
            "wrong owner",
            "fn unrelated(_: &Lease) -> u64 { 600 } assert_eq!(unrelated(&lease), 600);",
        ),
        (
            "wrong result",
            "let _ = charge(&lease); assert_eq!(lease.timeout_sec, 600);",
        ),
        ("discarded", "let _ = charge(&lease);"),
        ("deferred", "if false { assert_eq!(charge(&lease), 600); }"),
        (
            "uncalled",
            "let _later = || { assert_eq!(charge(&lease), 600); };",
        ),
        (
            "macro shadow",
            "macro_rules! assert_eq { ($($t:tt)*) => {} } assert_eq!(charge(&lease), 600);",
        ),
        (
            "callee shadow",
            "fn charge(_: &Lease) -> u64 { 600 } assert_eq!(charge(&lease), 600);",
        ),
        (
            "self expected",
            "let expected = charge(&lease); assert_eq!(charge(&lease), expected);",
        ),
        (
            "overwritten",
            "let mut value = charge(&lease); value = 600; assert_eq!(value, 600);",
        ),
    ] {
        let source = with_test(&format!("let lease = new_lease(600); {body}"));
        assert_ne!(
            header_class(&source, &added_diff(&source))?,
            ExposureClass::Exposed,
            "{case}"
        );
    }
    Ok(())
}

#[test]
fn whole_tail_receipt_never_selects_an_unobserved_sibling_arm()
-> Result<(), Box<dyn std::error::Error>> {
    let source = with_test("assert_eq!(charge(&new_lease(600)), 600);");
    let output = Fixture::new(&source, &added_diff(&source))?.check()?;
    let header = output
        .findings
        .iter()
        .find(|f| f.probe.expression == "match lease.settled_elapsed_ms")
        .ok_or("header subject")?;
    let some = output
        .findings
        .iter()
        .find(|f| {
            f.probe.family == ripr::ProbeFamily::MatchArm
                && f.probe.expression.contains("Some(ms) =>")
        })
        .ok_or("Some arm subject")?;
    assert_eq!(header.class, ExposureClass::Exposed);
    assert_ne!(
        some.class,
        ExposureClass::Exposed,
        "a whole result does not select the Some arm"
    );
    Ok(())
}

#[test]
fn existing_nested_and_mismatched_owners_never_receive_the_receipt()
-> Result<(), Box<dyn std::error::Error>> {
    let source = with_test("let lease = new_lease(600); assert_eq!(charge(&lease), 600);");
    let existing = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10 +10,4 @@\n-    lease.timeout_sec\n+    match lease.settled_elapsed_ms {\n+        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),\n+        None => lease.timeout_sec,\n+    }\n";
    assert_ne!(
        header_class(&source, existing)?,
        ExposureClass::Exposed,
        "existing changed owner"
    );
    let wrong_diff = added_diff(&source).replace("+fn charge(", "+fn other(");
    assert_ne!(
        header_class(&source, &wrong_diff)?,
        ExposureClass::Exposed,
        "mismatched current source"
    );
    for (case, changed) in [
        (
            "statement before tail",
            source.replace("    match lease.", "    let _ = 1;\n    match lease."),
        ),
        (
            "nested tail",
            source
                .replace("    match lease.", "    if true { match lease.")
                .replace(
                    "    }\n}\n#[cfg(test)]",
                    "    } } else { 0 }\n}\n#[cfg(test)]",
                ),
        ),
        (
            "discarded match",
            source.replace("    }\n}\n#[cfg(test)]", "    };\n    600\n}\n#[cfg(test)]"),
        ),
        (
            "attribute",
            source.replace("fn charge(", "#[inline]\nfn charge("),
        ),
        (
            "async owner",
            source.replace("fn charge(", "async fn charge("),
        ),
    ] {
        assert_ne!(
            header_class(&changed, &added_diff(&changed))?,
            ExposureClass::Exposed,
            "{case}"
        );
    }
    Ok(())
}

#[test]
fn immutable_whole_result_binding_preserves_observation() -> Result<(), Box<dyn std::error::Error>>
{
    let source = with_test(
        "let lease = new_lease(600);\nlet value = charge(&lease);\nassert_eq!(value, 600);",
    );
    assert_eq!(
        header_class(&source, &added_diff(&source))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn opaque_or_rebound_expected_values_never_gain_new_credit()
-> Result<(), Box<dyn std::error::Error>> {
    for (case, body) in [
        (
            "macro expected",
            "macro_rules! expected { () => { charge(&lease) } } assert_eq!(charge(&lease), expected!());",
        ),
        (
            "static expected",
            "static EXPECTED: u64 = 600; assert_eq!(charge(&lease), EXPECTED);",
        ),
        (
            "rewritten expected",
            "let mut expected = 600; macro_rules! rewrite { () => { expected = charge(&lease); } } rewrite!(); assert_eq!(charge(&lease), expected);",
        ),
        (
            "u64 MAX rebound",
            "mod u64 { pub const MAX: u64 = 600; } assert_eq!(charge(&lease), u64::MAX);",
        ),
        (
            "std root rebound",
            "mod std { pub mod primitive { pub mod u64 { pub const MAX: u64 = 600; } } } assert_eq!(charge(&lease), std::primitive::u64::MAX);",
        ),
        (
            "core root rebound",
            "mod core { pub mod primitive { pub mod u64 { pub const MAX: u64 = 600; } } } assert_eq!(charge(&lease), core::primitive::u64::MAX);",
        ),
    ] {
        let source = with_test(&format!("let lease = new_lease(600); {body}"));
        assert_ne!(
            header_class(&source, &added_diff(&source))?,
            ExposureClass::Exposed,
            "{case}"
        );
    }
    Ok(())
}

#[test]
fn nonlocal_or_macro_exit_in_the_tail_cannot_bypass_header_observation()
-> Result<(), Box<dyn std::error::Error>> {
    for (case, source) in [
        (
            "scrutinee return",
            SOURCE.replace(
                "match lease.settled_elapsed_ms",
                "match { return 600; lease.settled_elapsed_ms }",
            ),
        ),
        (
            "arm return",
            SOURCE.replace(
                "None => lease.timeout_sec",
                "None => return lease.timeout_sec",
            ),
        ),
        (
            "macro scrutinee",
            SOURCE
                .replace(
                    "    match lease.settled_elapsed_ms",
                    "    match early!(lease.settled_elapsed_ms)",
                )
                .replace(
                    "fn charge(",
                    "macro_rules! early { ($e:expr) => {{ return 600; $e }} }\nfn charge(",
                ),
        ),
        (
            "shadowed bounded macro",
            SOURCE
                .replace(
                    "    match lease.settled_elapsed_ms",
                    "    match assert!(lease.settled_elapsed_ms)",
                )
                .replace(
                    "fn charge(",
                    "macro_rules! assert { ($e:expr) => {{ return 600; $e }} }\nfn charge(",
                ),
        ),
    ] {
        let output = Fixture::new(&source, &added_diff(&source))?.check()?;
        let headers = output
            .findings
            .iter()
            .filter(|f| {
                f.probe.family == ripr::ProbeFamily::MatchArm
                    && f.probe.expression.starts_with("match ")
            })
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 1, "{case} header must survive analysis");
        assert_ne!(headers[0].class, ExposureClass::Exposed, "{case}");
    }
    Ok(())
}

#[test]
fn neighboring_old_text_does_not_replace_validated_new_owner_custody()
-> Result<(), Box<dyn std::error::Error>> {
    let source = SOURCE.replace(
        "fn charge(",
        "fn prior(lease: &Lease) -> u64 { lease.timeout_sec }\nfn charge(",
    );
    let diff = format!(
        "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -9 +9,{} @@\n-fn prior(lease: &Lease) -> u64 {{ 0 }}\n{}",
        source.lines().count() - 8,
        source
            .lines()
            .skip(8)
            .map(|line| format!("+{line}\n"))
            .collect::<String>()
    );
    let output = Fixture::new(&source, &diff)?.check()?;
    let header = output
        .findings
        .iter()
        .find(|f| f.probe.expression == "match lease.settled_elapsed_ms")
        .ok_or("header subject")?;
    assert!(
        header.probe.before.is_some(),
        "the fixture must exercise neighboring removed-before text"
    );
    assert_eq!(
        header.class,
        ExposureClass::Exposed,
        "validated old source has no charge owner despite heuristic before text"
    );
    Ok(())
}

#[test]
fn external_item_macro_cannot_supply_primitive_return_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("../../../fixtures/whole_tail_match_macro_scalar/input/src/lib.rs");
    let foreign =
        include_str!("../../../fixtures/whole_tail_match_macro_scalar/input/src/foreign.rs");
    let direct = source
        .replace("fn input() -> bool { true }\n", "")
        .replace("charge(input())", "charge(true)");
    for (case, source) in [
        ("direct original", direct.clone()),
        (
            "direct selected arm zero",
            direct.replace("true => u64(600)", "true => u64(0)"),
        ),
        (
            "renamed item macro",
            direct.replace(
                "crate::define_scalar!();",
                "use crate::define_scalar as opaque_scalar;\nopaque_scalar!();",
            ),
        ),
        ("original", source.to_string()),
        (
            "selected arm zero",
            source.replace("true => u64(600)", "true => u64(0)"),
        ),
        (
            "other arm zero",
            source.replace("false => u64(2)", "false => u64(0)"),
        ),
        (
            "both arms zero",
            source
                .replace("u64(600)", "u64(0)")
                .replace("u64(2)", "u64(0)"),
        ),
    ] {
        let diff = format!(
            "--- /dev/null\n+++ b/src/lib.rs\n@@ -0,0 +1,{} @@\n{}",
            source.lines().count(),
            source
                .lines()
                .map(|line| format!("+{line}\n"))
                .collect::<String>()
        );
        let fixture = Fixture::new(&source, &diff)?;
        std::fs::write(fixture.0.join("src/foreign.rs"), foreign)?;
        let output = fixture.check()?;
        let headers = output
            .findings
            .iter()
            .filter(|f| {
                f.probe.family == ripr::ProbeFamily::MatchArm && f.probe.expression == "match input"
            })
            .collect::<Vec<_>>();
        assert_eq!(headers.len(), 1, "retained header: {case}");
        assert_eq!(
            headers[0].class,
            ExposureClass::WeaklyExposed,
            "{case}: {}",
            headers[0].id
        );
        assert!(
            !headers[0]
                .evidence_path
                .iter()
                .any(|e| e.contains("whole_tail_match_return_observation")),
            "{case}"
        );
    }
    Ok(())
}
