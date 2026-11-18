//! End-to-end pin for the bitflags-shaped trait-default `.contains` false
//! link (#11113).
//!
//! A `Vec<(&str, TestFlags)>` receiver calls `slice::contains`, never the
//! trait default `Flags::contains`. The std-receiver demotion from #11208
//! must hold when a `macro_rules!` declares `TestFlags` and implements
//! `Flags` for it, as a bitflags-shaped crate does. Where the macro does not
//! establish the impl's self type (a `$metavariable`, or `Vec` itself), the
//! relation keeps today's credit. Relating a `TestFlags::contains` path
//! argument to the trait owner is out of scope.

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

const DIRECT_CONTAINS: &str = r#"crate::test_flags!();

#[test]
fn cases() {
    let value = TestFlags::A;
    assert!(value.contains(TestFlags::A));
}
"#;

/// The macro's impl self type is a metavariable: which type gets the impl is
/// decided at the call site, which this rule does not read.
const METAVAR_LIB: &str = r#"pub mod traits;
pub use traits::Flags;

#[macro_export]
macro_rules! test_flags {
    ($name:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        struct $name {
            bits: u8,
        }
        impl $name {
            const A: Self = Self { bits: 0b01 };
            const B: Self = Self { bits: 0b10 };
        }
        impl $crate::Flags for $name {
            fn bits(&self) -> u8 {
                self.bits
            }
        }
    };
}

#[cfg(test)]
mod tests;
"#;

const METAVAR_CONTAINS: &str = r#"crate::test_flags!(TestFlags);

#[test]
fn cases() {
    let value: Vec<(&str, TestFlags)> = vec![("A", TestFlags::A), ("B", TestFlags::B)];
    assert!(value.contains(&("A", TestFlags::A)));
}
"#;

/// The macro implements the trait for `Vec` as well as for its own type, so
/// `value.contains(..)` on a `Vec` can be the trait default method.
const VEC_IMPL_LIB: &str = r#"pub mod traits;
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
        impl $crate::Flags for Vec<u8> {
            fn bits(&self) -> u8 {
                self.len() as u8
            }
        }
    };
}

#[cfg(test)]
mod tests;
"#;

/// The reduced crate from the #11113 reproduction: the macro is invoked in
/// `src/tests/mod.rs`, `iter.rs` calls `Vec::contains`, and `contains.rs`
/// hands `TestFlags::contains` to a helper.
const SPLIT_TESTS_MOD: &str = "crate::test_flags!();\nmod contains;\nmod iter;\n";

const SPLIT_CONTAINS: &str = r#"use super::*;
use crate::Flags;

#[test]
fn cases() {
    case(TestFlags::A, &[(TestFlags::A, true), (TestFlags::B, false)], TestFlags::contains);
}

#[track_caller]
fn case<T: Flags + std::fmt::Debug>(value: T, inputs: &[(T, bool)], inherent: impl FnOnce(&T, T) -> bool + Copy) {
    for (input, expected) in inputs {
        assert_eq!(*expected, inherent(&value, *input), "{:?}.contains({:?})", value, input);
    }
}
"#;

const SPLIT_ITER: &str = r#"use super::*;

#[test]
fn test_defined_names() {
    let all_named: Vec<(&str, TestFlags)> = vec![("A", TestFlags::A), ("B", TestFlags::B)];
    let expected_flag = &("A", TestFlags::A);
    assert!(all_named.contains(expected_flag));
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

    /// The macro crate with the reproduction's `src/tests` layout.
    fn create_split_tests() -> Result<Self, String> {
        let fixture = Self::create(MACRO_LIB, SPLIT_CONTAINS)?;
        fixture.write("src/tests/mod.rs", SPLIT_TESTS_MOD)?;
        fixture.write("src/tests/iter.rs", SPLIT_ITER)?;
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
    test_reason(finding, "cases", "src/tests/contains.rs")
}

fn test_reason(finding: &Finding, name: &str, file: &str) -> Option<RelationReason> {
    finding.related_tests.iter().find_map(|test| {
        (test.name == name && path_ends_with(&test.file, file))
            .then_some(test.relation_reason)
            .flatten()
    })
}

fn dump(finding: &Finding) -> String {
    format!(
        "class={} reach={} related={}",
        finding.class.as_str(),
        finding.ripr.reach.state.as_str(),
        related_summary(finding)
    )
}

/// Bitflags-shaped crate: `macro_rules!` declares `TestFlags` and spells
/// `impl Flags for TestFlags`. The impl's self type is a type the macro
/// declares, so the `Vec` receiver cannot be the trait's receiver and `cases`
/// is a name-only relation that grants no reach.
#[test]
fn bitflags_macro_shape_does_not_credit_vec_contains_as_direct_owner_call() -> Result<(), String> {
    let fixture = Fixture::create(MACRO_LIB, MACRO_CONTAINS)?;
    let finding = fixture.contains_finding()?;
    let dump = dump(&finding);
    match cases_reason(&finding) {
        Some(RelationReason::WeakTokenSubstring) => {}
        other => {
            return Err(format!(
                "macro-declared TestFlags must demote Vec::contains to weak_token_substring, got {other:?} ({dump})"
            ));
        }
    }
    if finding.ripr.reach.state.as_str() == "yes" {
        return Err(format!(
            "Vec::contains must not grant reach to Flags::contains: {dump}"
        ));
    }
    // The macro spelling must read exactly as the source-level impl does:
    // same class, reach and next step as the #11208 control fixture.
    let plain = Fixture::create(PLAIN_LIB, PLAIN_CONTAINS)?.contains_finding()?;
    if finding.class != plain.class
        || finding.ripr.reach.state.as_str() != plain.ripr.reach.state.as_str()
        || finding.recommended_next_step != plain.recommended_next_step
    {
        return Err(format!(
            "macro-declared TestFlags must read as the source-level impl does: {dump} next={:?} vs {} next={:?}",
            finding.recommended_next_step,
            self::dump(&plain),
            plain.recommended_next_step
        ));
    }
    if matches!(
        finding.class,
        ExposureClass::Exposed | ExposureClass::WeaklyExposed
    ) {
        return Err(format!(
            "Vec::contains must not lend Flags::contains an exposed reading: {dump}"
        ));
    }
    Ok(())
}

/// The #11113 reproduction layout: `iter.rs::test_defined_names` calls
/// `Vec::contains` and must not be a direct call of `Flags::contains`, nor
/// lend it reach. `contains.rs::cases` names the trait method only as a path
/// argument; relating it is a separate rule and it is not asserted here.
#[test]
fn reproduction_layout_does_not_credit_iter_vec_contains() -> Result<(), String> {
    let fixture = Fixture::create_split_tests()?;
    let finding = fixture.contains_finding()?;
    let dump = dump(&finding);
    match test_reason(&finding, "test_defined_names", "src/tests/iter.rs") {
        None | Some(RelationReason::WeakTokenSubstring) => {}
        other => {
            return Err(format!(
                "test_defined_names calls Vec::contains and must be name-only at most, got {other:?} ({dump})"
            ));
        }
    }
    if finding.ripr.reach.state.as_str() == "yes" {
        return Err(format!(
            "Vec::contains must not grant reach to Flags::contains: {dump}"
        ));
    }
    if matches!(
        finding.class,
        ExposureClass::Exposed | ExposureClass::WeaklyExposed
    ) {
        return Err(format!(
            "Vec::contains must not lend Flags::contains an exposed reading: {dump}"
        ));
    }
    Ok(())
}

/// Positive control: in the same macro crate, a call whose receiver is a
/// `TestFlags` value is still a direct call of the trait default method.
#[test]
fn bitflags_macro_shape_keeps_a_testflags_receiver_as_direct_owner_call() -> Result<(), String> {
    let fixture = Fixture::create(MACRO_LIB, DIRECT_CONTAINS)?;
    let finding = fixture.contains_finding()?;
    if cases_reason(&finding) != Some(RelationReason::DirectOwnerCall) {
        return Err(format!(
            "a TestFlags receiver must stay direct_owner_call: {}",
            dump(&finding)
        ));
    }
    if finding.ripr.reach.state.as_str() != "yes" {
        return Err(format!(
            "a TestFlags receiver must keep reach yes: {}",
            dump(&finding)
        ));
    }
    Ok(())
}

/// Unresolved control: the impl's self type is a `$metavariable`, so syntax
/// does not establish which type implements `Flags`. The relation keeps
/// today's credit; reading the call site is not part of this rule.
#[test]
fn metavariable_self_type_keeps_todays_direct_owner_call() -> Result<(), String> {
    let fixture = Fixture::create(METAVAR_LIB, METAVAR_CONTAINS)?;
    let finding = fixture.contains_finding()?;
    if cases_reason(&finding) != Some(RelationReason::DirectOwnerCall) {
        return Err(format!(
            "setup drifted: a metavariable self type should keep direct_owner_call, got {}",
            dump(&finding)
        ));
    }
    Ok(())
}

/// Negative control: the macro also spells `impl Flags for Vec<u8>`, so a
/// `Vec` receiver can run the trait default method and keeps its credit.
#[test]
fn macro_impl_for_vec_keeps_direct_owner_call() -> Result<(), String> {
    let fixture = Fixture::create(VEC_IMPL_LIB, MACRO_CONTAINS)?;
    let finding = fixture.contains_finding()?;
    if cases_reason(&finding) != Some(RelationReason::DirectOwnerCall) {
        return Err(format!(
            "a macro impl for Vec must keep direct_owner_call, got {}",
            dump(&finding)
        ));
    }
    Ok(())
}

/// Control: the same `Vec<(&str, TestFlags)>` receiver without a crate-level
/// `macro_rules!` that spells `impl Flags for` is not `direct_owner_call`
/// (the #11208 production path).
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
