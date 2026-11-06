//! End-to-end pin for the bitflags-shaped trait-default `.contains` false
//! link (#11113 remainder).
//!
//! `classify/*` is fenced on this leaf, so the tests record the production
//! path as it stands: a `macro_rules!` that spells `impl Flags for TestFlags`
//! still fail-opens the std-receiver demotion from #11208, while the same
//! `Vec<(&str, TestFlags)>` receiver without that macro is not
//! `direct_owner_call`. Relating a tuple-element `.contains` to the trait
//! owner is out of scope.

#![cfg(feature = "lang-rust")]

use ripr::domain::RelationReason;
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const MANIFEST: &str = "[package]\nname = \"bitflags-trait-receiver\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n[workspace]\n";

const TRAITS: &str = r#"pub trait Flags: Sized + Copy {
    fn bits(&self) -> u8;

    fn intersects(&self, other: Self) -> bool {
        self.bits() & other.bits() != 0
    }

    fn contains(&self, other: Self) -> bool {
        self.bits() & other.bits() != other.bits()
    }
}
"#;

const TESTS_MOD: &str = "mod contains;\n";

const MACRO_LIB: &str = r#"pub mod traits;
pub use traits::Flags;

#[macro_export]
macro_rules! test_flags {
    () => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        struct TestFlags {
            bits: u8,
        }
        impl TestFlags {
            const A: Self = Self { bits: 0b01 };
            const B: Self = Self { bits: 0b10 };
        }
        impl $crate::Flags for TestFlags {
            fn bits(&self) -> u8 {
                self.bits
            }
        }
    };
}

#[cfg(test)]
mod tests;
"#;

const MACRO_CONTAINS: &str = r#"crate::test_flags!();

#[test]
fn cases() {
    let value: Vec<(&str, TestFlags)> = vec![("A", TestFlags::A), ("B", TestFlags::B)];
    for (name, _) in &value {
        let _ = name;
    }
    assert!(value.contains(&("A", TestFlags::A)));
}
"#;

const PLAIN_LIB: &str = r#"pub mod traits;
pub use traits::Flags;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TestFlags {
    bits: u8,
}

impl TestFlags {
    pub const A: Self = Self { bits: 0b01 };
    pub const B: Self = Self { bits: 0b10 };
}

impl Flags for TestFlags {
    fn bits(&self) -> u8 {
        self.bits
    }
}

#[cfg(test)]
mod tests;
"#;

const PLAIN_CONTAINS: &str = r#"use crate::TestFlags;

#[test]
fn cases() {
    let value: Vec<(&str, TestFlags)> = vec![("A", TestFlags::A), ("B", TestFlags::B)];
    for (name, _) in &value {
        let _ = name;
    }
    assert!(value.contains(&("A", TestFlags::A)));
}
"#;

const OLD_CONTAINS_BODY: &str = "        self.bits() & other.bits() == other.bits()";
const NEW_CONTAINS_BODY: &str = "        self.bits() & other.bits() != other.bits()";

struct Fixture(PathBuf);

impl Fixture {
    fn create(lib: &str, contains: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-bitflags-trait-receiver-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src/tests")).map_err(|error| error.to_string())?;
        let fixture = Self(root);
        fixture.write("Cargo.toml", MANIFEST)?;
        fixture.write("src/lib.rs", lib)?;
        fixture.write("src/traits.rs", TRAITS)?;
        fixture.write("src/tests/mod.rs", TESTS_MOD)?;
        fixture.write("src/tests/contains.rs", contains)?;
        let diff = one_line_replace_diff(
            "src/traits.rs",
            TRAITS,
            OLD_CONTAINS_BODY,
            NEW_CONTAINS_BODY,
        )?;
        fixture.write("change.diff", &diff)?;
        Ok(fixture)
    }

    fn write(&self, relative: &str, contents: &str) -> Result<(), String> {
        std::fs::write(self.0.join(relative), contents)
            .map_err(|error| format!("write {relative}: {error}"))
    }

    fn contains_finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("change.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        select_contains_finding(&output.findings)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn one_line_replace_diff(
    path: &str,
    source: &str,
    old_line: &str,
    new_line: &str,
) -> Result<String, String> {
    if source.matches(new_line).count() != 1 {
        return Err(format!("setup: unique new line missing in {path}"));
    }
    if source.contains(old_line) {
        return Err(format!("setup: old line must not remain in {path}"));
    }
    let line = source
        .lines()
        .position(|line| line == new_line)
        .ok_or_else(|| format!("setup: new line is not a whole line in {path}"))?
        + 1;
    Ok(format!(
        "diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n@@ -{line},1 +{line},1 @@\n-{old_line}\n+{new_line}\n"
    ))
}

fn select_contains_finding(findings: &[Finding]) -> Result<Finding, String> {
    let on_traits: Vec<&Finding> = findings
        .iter()
        .filter(|finding| path_ends_with(&finding.probe.location.file, "src/traits.rs"))
        .filter(|finding| {
            finding.probe.family == ProbeFamily::Predicate
                && finding.probe.expression.contains("other.bits()")
        })
        .collect();
    match on_traits.as_slice() {
        [finding] => Ok((*finding).clone()),
        [] => Err(format!(
            "setup: expected a Flags::contains predicate finding, got {}",
            summarize_findings(findings)
        )),
        _ => Err(format!(
            "setup: unique Flags::contains predicate finding, got {}",
            summarize_findings(findings)
        )),
    }
}

fn path_ends_with(path: &Path, suffix: &str) -> bool {
    path.to_string_lossy().replace('\\', "/").ends_with(suffix)
}

fn summarize_findings(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|finding| {
            format!(
                "{}:{} class={} family={} expr={} related={}",
                finding.probe.location.file.display(),
                finding.probe.location.line,
                finding.class.as_str(),
                finding.probe.family.as_str(),
                finding.probe.expression,
                related_summary(finding)
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn related_summary(finding: &Finding) -> String {
    finding
        .related_tests
        .iter()
        .map(|test| {
            format!(
                "{}:{}",
                test.name,
                test.relation_reason
                    .map(RelationReason::as_str)
                    .unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn cases_reason(finding: &Finding) -> Option<RelationReason> {
    finding.related_tests.iter().find_map(|test| {
        (test.name == "cases" && path_ends_with(&test.file, "src/tests/contains.rs"))
            .then_some(test.relation_reason)
            .flatten()
    })
}

/// Bitflags-shaped crate: `macro_rules!` generates `TestFlags` and `impl Flags
/// for TestFlags`. Today's production still fail-opens the #11208 demotion, so
/// `cases` is `direct_owner_call` and the finding reads `infection_unknown`
/// rather than `no_static_path`.
#[test]
fn bitflags_macro_shape_still_credits_vec_contains_as_direct_owner_call() -> Result<(), String> {
    let fixture = Fixture::create(MACRO_LIB, MACRO_CONTAINS)?;
    let finding = fixture.contains_finding()?;
    let dump = format!(
        "class={} reach={} related={}",
        finding.class.as_str(),
        finding.ripr.reach.state.as_str(),
        related_summary(&finding)
    );
    if cases_reason(&finding) != Some(RelationReason::DirectOwnerCall) {
        return Err(format!(
            "setup drifted: expected today's fail-open direct_owner_call, got {dump}"
        ));
    }
    if finding.class != ExposureClass::InfectionUnknown {
        return Err(format!(
            "setup drifted: expected today's infection_unknown class (not no_static_path), got {dump}"
        ));
    }
    if finding.ripr.reach.state.as_str() != "yes" {
        return Err(format!(
            "today's fail-open still claims reach yes, got {dump}"
        ));
    }
    let next = finding
        .recommended_next_step
        .as_deref()
        .ok_or_else(|| format!("missing next step: {dump}"))?;
    if !next.contains("Open the related test") {
        return Err(format!(
            "today's next step still sends the user into the Vec test: {next}"
        ));
    }
    Ok(())
}

/// Control: the same `Vec<(&str, TestFlags)>` receiver without a crate-level
/// `macro_rules!` that spells `impl Flags for` is not `direct_owner_call`.
/// That is the #11208 production path this leaf can prove without touching
/// `classify/*`.
#[test]
fn source_level_testflags_vec_receiver_is_not_direct_owner_call() -> Result<(), String> {
    let fixture = Fixture::create(PLAIN_LIB, PLAIN_CONTAINS)?;
    let finding = fixture.contains_finding()?;
    let dump = format!(
        "class={} reach={} related={}",
        finding.class.as_str(),
        finding.ripr.reach.state.as_str(),
        related_summary(&finding)
    );
    if cases_reason(&finding) == Some(RelationReason::DirectOwnerCall) {
        return Err(format!(
            "plain TestFlags impl must not credit Vec::contains as Flags::contains: {dump}"
        ));
    }
    match cases_reason(&finding) {
        Some(RelationReason::WeakTokenSubstring) => {}
        other => {
            return Err(format!(
                "plain Vec receiver should demote to weak_token_substring, got {other:?} ({dump})"
            ));
        }
    }
    if finding.class == ExposureClass::InfectionUnknown
        && finding.ripr.reach.state.as_str() == "yes"
    {
        return Err(format!(
            "plain Vec receiver must not keep the fail-open infection_unknown/reach-yes reading: {dump}"
        ));
    }
    if finding.ripr.reach.state.as_str() == "yes" {
        return Err(format!(
            "plain Vec receiver must not keep reach yes after the #11208 demotion: {dump}"
        ));
    }
    Ok(())
}
