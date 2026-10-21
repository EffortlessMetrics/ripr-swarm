//! Public-API controls for `assert_eq!`/`assert_ne!` as a call of a changed
//! user `PartialEq` (#8999).
//!
//! `assert_eq!(a, b)` runs `PartialEq::eq` without naming it. A same-file
//! test comparing two values whose type is syntactically established as the
//! impl's self type reaches the changed `eq` directly; a comparison with an
//! unresolved operand, a shadowed type or an unrelated type does not, and
//! none of this raises an exposure class.

use ripr::domain::{RelationReason, StopReason};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OLD_EQ: &str = "        self.name == other.name\n";
const NEW_EQ: &str = "        self.name == other.name && self.id == other.id\n";

fn source(impl_header: &str, extra_impl: &str, preamble: &str, tests: &str, body: &str) -> String {
    format!(
        "pub struct Key {{
    name: String,
    id: u32,
}}

impl Key {{
    pub fn new(name: &str, id: u32) -> Key {{
        Key {{ name: name.to_string(), id }}
    }}
}}

{impl_header} {{
    fn eq(&self, other: &Key) -> bool {{
{body}    }}
{extra_impl}}}

#[cfg(test)]
mod tests {{
    use super::*;
{preamble}
{tests}}}
"
    )
}

struct TempRepo {
    root: PathBuf,
    eq_line: usize,
}

impl TempRepo {
    fn create(make: impl Fn(&str) -> String) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-partialeq-assert-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"keyed\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let lib = make(NEW_EQ);
        let eq_line = lib
            .lines()
            .position(|line| line == NEW_EQ.trim_end_matches('\n'))
            .map(|index| index + 1)
            .ok_or("premise: the changed eq body is in the source")?;
        std::fs::write(root.join("src/lib.rs"), &lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(
            root.join("diff.patch"),
            format!(
                "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{eq_line},1 +{eq_line},1 @@
-{OLD_EQ}+{NEW_EQ}"
            ),
        )
        .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root, eq_line })
    }

    /// Every finding the changed `eq` line produced.
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
        let findings: Vec<Finding> = output
            .findings
            .into_iter()
            .filter(|finding| finding.probe.location.line == self.eq_line)
            .collect();
        if findings.is_empty() {
            return Err("premise: the changed eq line yields a finding".to_string());
        }
        Ok(findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Names of the tests some finding relates to the owner by a direct call.
fn direct_tests(findings: &[Finding]) -> Vec<String> {
    let mut names: Vec<String> = findings
        .iter()
        .flat_map(|finding| &finding.related_tests)
        .filter(|test| test.relation_reason == Some(RelationReason::DirectOwnerCall))
        .map(|test| test.name.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn plain(tests: &str) -> Result<TempRepo, String> {
    TempRepo::create(|body| source("impl PartialEq for Key", "", "", tests, body))
}

#[test]
fn an_assert_eq_over_two_keys_reaches_the_changed_eq() -> Result<(), String> {
    let repo = plain(
        "    #[test]
    fn keys_equal_by_name() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        assert_eq!(a, b);
    }

    #[test]
    fn keys_differ_by_name() {
        assert_ne!(Key::new(\"a\", 1), Key::new(\"b\", 1));
    }

    #[test]
    fn literal_operands_reach_it() {
        assert_eq!(&Key { name: String::new(), id: 1 }, &Key { name: String::new(), id: 1 });
    }
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        direct_tests(&findings),
        [
            "keys_differ_by_name",
            "keys_equal_by_name",
            "literal_operands_reach_it"
        ],
        "{findings:?}"
    );
    for finding in &findings {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "relating the test to eq never raises a class by itself: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn comparisons_that_do_not_establish_the_key_type_stay_unrelated() -> Result<(), String> {
    let repo = plain(
        "    fn make() -> Key {
        Key::new(\"a\", 1)
    }

    #[test]
    fn std_strings_only() {
        assert_eq!(String::from(\"x\"), String::from(\"x\"));
    }

    #[test]
    fn one_operand_is_unresolved() {
        let a = Key::new(\"a\", 1);
        assert_eq!(a, make());
    }

    #[test]
    fn a_key_projection_is_not_a_key() {
        let a = Key::new(\"a\", 1);
        assert_eq!(a.name, \"a\");
    }

    #[test]
    fn a_boolean_equality_is_not_the_macro() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 1);
        assert!(a == b);
    }

    #[test]
    fn a_later_rebinding_hides_the_type() {
        let a = Key::new(\"a\", 1);
        let a = make();
        assert_eq!(a, a);
    }
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        direct_tests(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_test_module_key_shadow_never_reaches_the_production_eq() -> Result<(), String> {
    let repo = TempRepo::create(|body| {
        source(
            "impl PartialEq for Key",
            "",
            "    #[derive(Debug, PartialEq)]\n    struct Key {\n        id: u32,\n    }\n",
            "    #[test]
    fn compares_the_local_key() {
        let a = Key { id: 1 };
        let b = Key { id: 1 };
        assert_eq!(a, b);
    }
",
            body,
        )
    })?;
    let findings = repo.findings()?;
    assert_eq!(
        direct_tests(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_custom_ne_means_assert_ne_does_not_run_eq() -> Result<(), String> {
    let repo = TempRepo::create(|body| {
        source(
            "impl PartialEq for Key",
            "    fn ne(&self, other: &Key) -> bool {\n        self.id != other.id\n    }\n",
            "",
            "    #[test]
    fn eq_macro() {
        assert_eq!(Key::new(\"a\", 1), Key::new(\"a\", 1));
    }

    #[test]
    fn ne_macro() {
        assert_ne!(Key::new(\"a\", 1), Key::new(\"a\", 2));
    }
",
            body,
        )
    })?;
    let findings = repo.findings()?;
    assert_eq!(direct_tests(&findings), ["eq_macro"], "{findings:?}");
    Ok(())
}

#[test]
fn a_generic_partial_eq_is_not_a_same_type_comparison() -> Result<(), String> {
    let repo = TempRepo::create(|body| {
        source(
            "impl PartialEq<u32> for Key",
            "",
            "",
            "    #[test]
    fn compares_keys() {
        assert_eq!(Key::new(\"a\", 1), Key::new(\"a\", 1));
    }
",
            body,
        )
    })?;
    let findings = repo.findings()?;
    assert_eq!(
        direct_tests(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_fixture_word_in_an_unrelated_test_is_not_an_opaque_fixture() -> Result<(), String> {
    let repo = plain(
        "    #[test]
    fn keys_equal_by_name() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        assert_eq!(a, b);
    }

    #[test]
    fn unrelated_builder_check() {
        let builder = String::from(\"builder\");
        assert_eq!(builder.len(), 7);
    }
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(
        direct_tests(&findings),
        ["keys_equal_by_name"],
        "{findings:?}"
    );
    for finding in &findings {
        assert!(
            !finding.stop_reasons.contains(&StopReason::FixtureOpaque),
            "the builder word sits in a test that does not reach eq: {finding:?}"
        );
    }
    Ok(())
}

#[test]
fn a_fixture_word_in_the_reaching_test_still_stops_the_reading() -> Result<(), String> {
    let repo = plain(
        "    #[test]
    fn keys_equal_by_name() {
        let fixture = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        assert_eq!(fixture, b);
    }
",
    )?;
    let findings = repo.findings()?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.stop_reasons.contains(&StopReason::FixtureOpaque)),
        "{findings:?}"
    );
    Ok(())
}
