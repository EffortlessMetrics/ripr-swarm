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
        Self::create_changed(make, OLD_EQ, NEW_EQ)
    }

    /// A repo whose `src/lib.rs` is `make(new)` and whose diff replaces the
    /// source line `old` with `new`.
    fn create_changed(make: impl Fn(&str) -> String, old: &str, new: &str) -> Result<Self, String> {
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
        let lib = make(new);
        let eq_line = lib
            .lines()
            .position(|line| line == new.trim_end_matches('\n'))
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
-{old}+{new}"
            ),
        )
        .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root, eq_line })
    }

    /// Writes an extra workspace file next to `src/lib.rs`.
    fn with_file(self, name: &str, content: &str) -> Result<Self, String> {
        std::fs::write(self.root.join("src").join(name), content)
            .map_err(|error| format!("write {name} failed: {error}"))?;
        Ok(self)
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

/// Sorted `family:class` pairs of every finding.
fn classes(findings: &[Finding]) -> Vec<String> {
    let mut pairs: Vec<String> = findings
        .iter()
        .map(|finding| format!("{:?}:{:?}", finding.probe.family, finding.class))
        .collect();
    pairs.sort();
    pairs
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

/// Names of tests some finding relates to the owner by a non-direct reason.
fn indirect_tests(findings: &[Finding]) -> Vec<String> {
    let mut names: Vec<String> = findings
        .iter()
        .flat_map(|finding| &finding.related_tests)
        .filter(|test| test.relation_reason != Some(RelationReason::DirectOwnerCall))
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

const THREE_KEY_TESTS: &str = "    #[test]
    fn keys_equal_by_name() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        assert_eq!(a, b);
    }
";

/// The wiring only relates a test to the owner; the verdict classes must
/// equal what an explicit `a.eq(&b)` call (already a direct owner call on
/// main) produces, and the pinned list is the same fixture's classes on main.
#[test]
fn reaching_eq_through_assert_eq_moves_no_class_beyond_an_explicit_eq_call() -> Result<(), String> {
    let by_macro = plain(THREE_KEY_TESTS)?.findings()?;
    assert_eq!(direct_tests(&by_macro), ["keys_equal_by_name"]);
    // The same test spelling the call out is a direct owner call on main
    // already; crediting the macro must land on exactly its classes.
    let by_call = plain(
        "    #[test]
    fn keys_equal_by_name() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        assert!(a.eq(&b));
    }
",
    )?
    .findings()?;
    assert_eq!(direct_tests(&by_call), ["keys_equal_by_name"]);
    assert_eq!(classes(&by_macro), classes(&by_call), "{by_macro:?}");
    // Hand-derived: the changed `&&` predicate is an infection question and
    // its value is returned, so exactly these two probe families appear.
    assert_eq!(
        classes(&by_macro),
        [
            "Predicate:InfectionUnknown",
            "ReturnValue:PropagationUnknown"
        ],
        "{by_macro:?}"
    );
    Ok(())
}

#[test]
fn a_user_assert_eq_macro_is_not_the_std_comparison() -> Result<(), String> {
    let repo = TempRepo::create(|body| {
        source(
            "impl PartialEq for Key",
            "",
            "    macro_rules! assert_eq {
        ($left:expr, $right:expr) => {
            let _ = ($left, $right);
        };
    }
",
            THREE_KEY_TESTS,
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
fn an_import_that_may_name_another_key_is_not_credited() -> Result<(), String> {
    let repo = TempRepo::create(|body| {
        source(
            "impl PartialEq for Key",
            "",
            "    use super::other::Key;\n",
            THREE_KEY_TESTS,
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
fn a_pattern_rebinding_of_an_operand_is_not_credited() -> Result<(), String> {
    let repo = plain(
        "    #[test]
    fn rebound_by_a_loop() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        for a in [1, 2] {
            let _ = a;
        }
        assert_eq!(a, b);
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
fn a_non_partial_eq_owner_keeps_the_fixture_stop_reason_from_every_related_test()
-> Result<(), String> {
    let repo = TempRepo::create_changed(
        |line| {
            format!(
                "pub fn score(x: u32) -> u32 {{
{line}}}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn score_is_three() {{
        assert_eq!(score(1), 3);
    }}

    #[test]
    fn unrelated_builder_check() {{
        let builder = String::from(\"builder\");
        assert_eq!(builder.len(), 7);
    }}
}}
"
            )
        },
        "    x + 1\n",
        "    x + 2\n",
    )?;
    let findings = repo.findings()?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.stop_reasons.contains(&StopReason::FixtureOpaque)),
        "the scoped change must not touch other owners: {findings:?}"
    );
    Ok(())
}

/// Whole-file source with the changed `eq` line spliced in at `{body}`.
fn custom(template: &'static str) -> Result<TempRepo, String> {
    TempRepo::create(move |body| template.replace("{body}\n", body))
}

#[test]
fn a_constructor_named_call_returning_another_type_is_not_a_key() -> Result<(), String> {
    let repo = custom(
        "#[derive(Debug, PartialEq)]
pub struct Other;

pub struct Key {
    name: String,
    id: u32,
}

impl Key {
    pub fn new() -> Other {
        Other
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
{body}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_returns_other() {
        assert_eq!(Key::new(), Key::new());
    }
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
fn a_local_partial_eq_trait_is_not_std_equality() -> Result<(), String> {
    let repo = custom(
        "trait PartialEq {
    fn eq(&self, other: &Self) -> bool;
}

#[derive(Debug)]
pub struct Key {
    name: String,
    id: u32,
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
{body}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_two_keys() {
        let a = Key { name: String::new(), id: 1 };
        let b = Key { name: String::new(), id: 1 };
        assert_eq!(a, b);
    }
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
fn a_file_declaring_two_keys_is_ambiguous_for_the_nested_owner() -> Result<(), String> {
    let repo = custom(
        "#[derive(Debug, PartialEq)]
pub struct Key {
    id: u32,
}

mod inner {
    #[derive(Debug)]
    pub struct Key {
        name: String,
        id: u32,
    }

    impl PartialEq for Key {
        fn eq(&self, other: &Key) -> bool {
{body}
        }
    }

    #[cfg(test)]
    mod tests {
        use crate::Key;

        #[test]
        fn compares_the_root_key() {
            let a = Key { id: 1 };
            let b = Key { id: 1 };
            assert_eq!(a, b);
        }
    }
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
fn a_name_only_eq_call_does_not_hide_the_macro_dispatch() -> Result<(), String> {
    let repo = custom(
        "#[derive(Debug)]
pub struct Other(u32);

impl PartialEq for Other {
    fn eq(&self, other: &Other) -> bool {
        self.0 == other.0
    }
}

#[derive(Debug)]
pub struct Key {
    name: String,
    id: u32,
}

impl PartialEq for Key {
    fn eq(&self, other: &Key) -> bool {
{body}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_both() {
        let o = Other(1);
        assert!(o.eq(&Other(1)));
        let a = Key { name: String::new(), id: 1 };
        let b = Key { name: String::new(), id: 1 };
        assert_eq!(a, b);
    }
}
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(direct_tests(&findings), ["mixes_both"], "{findings:?}");
    Ok(())
}

#[test]
fn a_debug_only_assertion_reaches_the_changed_eq() -> Result<(), String> {
    let repo = plain(
        "    #[test]
    fn debug_only() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 1);
        debug_assert_eq!(a, b);
    }
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(direct_tests(&findings), ["debug_only"], "{findings:?}");
    Ok(())
}

#[test]
fn an_assert_eq_inside_an_opaque_property_macro_is_not_credited() -> Result<(), String> {
    let repo = plain(
        "    #[test]
    fn plain_control() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 1);
        assert_eq!(a, b);
    }

    #[test]
    fn inside_a_property_macro() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 1);
        proptest! {
            assert_eq!(a, b);
        }
    }
",
    )?;
    let findings = repo.findings()?;
    assert_eq!(direct_tests(&findings), ["plain_control"], "{findings:?}");
    Ok(())
}

/// Asserts the same two-Key comparison reaches the owner with a plain
/// preamble (control) and not with `preamble` or the varied `tests`.
fn assert_withheld(preamble: &str, tests: &str) -> Result<(), String> {
    let control =
        TempRepo::create(|body| source("impl PartialEq for Key", "", "", THREE_KEY_TESTS, body))?
            .findings()?;
    assert_eq!(direct_tests(&control), ["keys_equal_by_name"], "control");
    let repo =
        TempRepo::create(|body| source("impl PartialEq for Key", "", preamble, tests, body))?;
    let findings = repo.findings()?;
    assert_eq!(
        direct_tests(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    assert_eq!(
        indirect_tests(&findings),
        ["keys_equal_by_name"],
        "the test stays related by a non-direct reason: {findings:?}"
    );
    Ok(())
}

#[test]
fn a_non_std_partial_eq_import_or_glob_is_not_credited() -> Result<(), String> {
    assert_withheld("    use other::PartialEq;\n", THREE_KEY_TESTS)?;
    assert_withheld("    use other::*;\n", THREE_KEY_TESTS)
}

#[test]
fn an_imported_assert_eq_macro_is_not_credited() -> Result<(), String> {
    assert_withheld("    use helpers::assert_eq;\n", THREE_KEY_TESTS)
}

#[test]
fn a_macro_use_attribute_is_not_credited() -> Result<(), String> {
    assert_withheld(
        "    #[macro_use]\n    extern crate helpers;\n",
        THREE_KEY_TESTS,
    )
}

#[test]
fn a_macro_use_in_another_workspace_file_is_not_credited() -> Result<(), String> {
    let repo =
        TempRepo::create(|body| source("impl PartialEq for Key", "", "", THREE_KEY_TESTS, body))?
            .with_file("macros.rs", "#[macro_use]\nextern crate helpers;\n")?;
    let findings = repo.findings()?;
    assert_eq!(
        direct_tests(&findings),
        Vec::<String>::new(),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn a_user_assert_eq_in_another_workspace_file_is_not_credited() -> Result<(), String> {
    let repo =
        TempRepo::create(|body| source("impl PartialEq for Key", "", "", THREE_KEY_TESTS, body))?
            .with_file(
            "macros.rs",
            "macro_rules! assert_eq {\n    ($a:expr, $b:expr) => {};\n}\n",
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
fn qualified_std_assert_eq_is_the_std_comparison() -> Result<(), String> {
    let std_path = plain(
        "    #[test]
    fn keys_equal_by_name() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        std::assert_eq!(a, b);
    }
",
    )?
    .findings()?;
    assert_eq!(direct_tests(&std_path), ["keys_equal_by_name"]);
    // A path whose root is only suffixed `_std` is some other crate's macro.
    assert_withheld(
        "",
        "    #[test]
    fn keys_equal_by_name() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        foo_std::assert_eq!(a, b);
    }
",
    )
}

#[test]
fn closure_if_let_and_match_rebinding_are_not_credited() -> Result<(), String> {
    for (label, rebind) in [
        ("closure", "let f = |a: u32| a + 1;\n        let _ = f(1);"),
        (
            "if_let",
            "if let Some(a) = Some(1u32) {\n            let _ = a;\n        }",
        ),
        (
            "match_arm",
            "match Some(1u32) {\n            Some(a) => {\n                let _ = a;\n            }\n            None => {}\n        }",
        ),
    ] {
        let tests = format!(
            "    #[test]
    fn keys_equal_by_name() {{
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        {rebind}
        assert_eq!(a, b);
    }}
"
        );
        let repo = plain(&tests)?;
        let findings = repo.findings()?;
        assert_eq!(
            direct_tests(&findings),
            Vec::<String>::new(),
            "{label}: {findings:?}"
        );
    }
    Ok(())
}

#[test]
fn a_shadowing_let_in_a_nested_block_leaves_the_binding_ambiguous() -> Result<(), String> {
    assert_withheld(
        "",
        "    #[test]
    fn keys_equal_by_name() {
        let a = Key::new(\"a\", 1);
        let b = Key::new(\"a\", 2);
        {
            let a = 5u32;
            let _ = a;
        }
        assert_eq!(a, b);
    }
",
    )
}
