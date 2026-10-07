//! RIPR-SPEC-0114/0117 on proximity-only reach: a `weakly_exposed` finding
//! whose related tests only share the owner's file or a name token names the
//! transitive or macro reach limit instead of reporting a gap.
use super::RustAdapter;
use crate::analysis::language::LanguageAdapter;
use crate::analysis::{AnalysisMode, AnalysisOptions, diff};
use crate::config::OraclePolicy;
use crate::domain::{
    ExposureClass, Finding, RelationReason, StageState, StaticLimitKind, StopReason,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str, lib: &str) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-proximity-reach-{label}-{stamp}"));
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='reach'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|e| e.to_string())?;
        fs::write(root.join("src/lib.rs"), lib).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    /// The finding on line 2, where `x * 3` became `3 * x`.
    fn rate_finding(&self) -> Result<Finding, String> {
        let diff_text = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n fn inner_rate(x: i64) -> i64 {\n-    x * 3\n+    3 * x\n }\n";
        let changed_files = diff::parse_unified_diff(diff_text);
        let result = RustAdapter.analyze_diff(
            &AnalysisOptions {
                root: self.0.clone(),
                base: None,
                diff_file: None,
                mode: AnalysisMode::Ready,
                resolved_subject_identity: None,
                open_rust_index_paths: Default::default(),
                include_unchanged_tests: true,
                resolve_tsconfig_paths: false,
                perl_facts_path: None,
                perl_producer_failure: None,
                git_timeout: None,
                git_candidate: None,
                production_like_targets: Default::default(),
                test_harnesses: Vec::new(),
            },
            &OraclePolicy::default(),
            &changed_files,
        )?;
        result
            .findings
            .into_iter()
            .find(|finding| finding.probe.location.line == 2)
            .ok_or_else(|| "no finding on the changed line".to_string())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

const MACRO_ROUTED: &str = "fn inner_rate(x: i64) -> i64 {
    3 * x
}
macro_rules! call_inner {
    ($x:expr) => {
        inner_rate($x)
    };
}
pub fn outer(x: i64) -> i64 {
    call_inner!(x)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outer_triples() {
        assert_eq!(outer(4), 12);
    }
";

fn all_relations(finding: &Finding, wanted: impl Fn(RelationReason) -> bool) -> bool {
    !finding.related_tests.is_empty()
        && finding
            .related_tests
            .iter()
            .all(|test| test.relation_reason.is_some_and(&wanted))
}

#[test]
fn proximity_only_reach_through_a_macro_names_the_macro_limit() -> Result<(), String> {
    let fixture = Fixture::new("macro", &format!("{MACRO_ROUTED}}}\n"))?;
    let finding = fixture.rate_finding()?;
    // Fixture construction: the test reaches `inner_rate` only through
    // `call_inner!`, so its relation is proximity.
    assert!(
        all_relations(&finding, crate::analysis::classify::is_proximity_only),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustMacroReachUnresolved),
        "{:?}",
        finding.evidence
    );
    assert!(
        finding
            .stop_reasons
            .contains(&StopReason::MacroReachUnresolved)
    );
    // The witness is a candidate path, never a related test, and the gap's
    // discriminator lines go with the gap.
    assert_eq!(
        finding.related_tests.len(),
        1,
        "{:?}",
        finding.related_tests
    );
    assert!(finding.activation.missing_discriminators.is_empty());
    assert_eq!(
        finding.missing,
        vec![
            StaticLimitKind::RustMacroReachUnresolved
                .describe()
                .to_string()
        ]
    );
    let next = finding.recommended_next_step.unwrap_or_default();
    assert!(
        next.contains("`outer_triples`") && next.contains("does not establish a missing test"),
        "{next}"
    );
    Ok(())
}

#[test]
fn proximity_only_reach_through_a_helper_names_the_transitive_limit() -> Result<(), String> {
    // A same-file test is proximity even when it calls a helper that calls
    // the owner, so the transitive witness names the limit.
    let fixture = Fixture::new(
        "transitive",
        "fn inner_rate(x: i64) -> i64 {
    3 * x
}
pub fn outer(x: i64) -> i64 {
    inner_rate(x)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outer_triples() {
        assert_eq!(outer(4), 12);
    }
}
",
    )?;
    let finding = fixture.rate_finding()?;
    assert!(
        all_relations(&finding, crate::analysis::classify::is_proximity_only),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Weak);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustTransitiveReachUnresolved),
        "{:?}",
        finding.evidence
    );
    assert!(
        finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved)
    );
    assert!(
        finding
            .recommended_next_step
            .as_deref()
            .is_some_and(|next| next.contains("`outer_triples`") && next.contains("`outer`")),
        "{:?}",
        finding.recommended_next_step
    );
    Ok(())
}

#[test]
fn a_test_calling_the_owner_keeps_the_gap_despite_a_macro_witness() -> Result<(), String> {
    // The same macro witness exists, but a test calls the owner directly:
    // reach is established, so the weak oracle stays a gap.
    let fixture = Fixture::new(
        "direct",
        &format!(
            "{MACRO_ROUTED}    #[test]\n    fn rate_is_positive() {{\n        assert!(inner_rate(2) > 0);\n    }}\n}}\n"
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.relation_reason == Some(RelationReason::DirectOwnerCall)),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn proximity_only_reach_without_a_witness_keeps_the_gap() -> Result<(), String> {
    // A function pointer names the owner, so reach stays weak, but no test
    // walks toward it through a call chain or macro: nothing to name.
    let fixture = Fixture::new(
        "no-witness",
        "fn inner_rate(x: i64) -> i64 {
    3 * x
}
pub const RATE: fn(i64) -> i64 = inner_rate;
pub fn unrelated(x: i64) -> i64 {
    x + 1
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unrelated_adds_one() {
        assert_eq!(unrelated(4), 5);
    }
}
",
    )?;
    let finding = fixture.rate_finding()?;
    assert!(
        all_relations(&finding, crate::analysis::classify::is_proximity_only),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Weak);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

const GENERATED: &str = "fn inner_rate(x: i64) -> i64 {
    3 * x
}
#[cfg(test)]
mod tests {
    use super::*;
    macro_rules! rate_case {
        ($name:ident, $input:expr, |$out:ident| $body:block) => {
            #[test]
            fn $name() {
                let check = |$out: i64| $body;
                check(inner_rate($input));
            }
        };
    }
    rate_case!(rate_triples, 4, |out| {
        assert_eq!(out, 12);
    });
    #[test]
    fn unrelated_is_true() {
        assert!(true);
    }
}
";

#[test]
fn a_test_generated_by_a_macro_naming_the_owner_names_the_macro_limit() -> Result<(), String> {
    let fixture = Fixture::new("generator", GENERATED)?;
    let finding = fixture.rate_finding()?;
    // Fixture construction: the generated test is not indexed, so the only
    // related test is the hand-written one sharing the file.
    assert!(
        all_relations(&finding, crate::analysis::classify::is_proximity_only),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustMacroReachUnresolved),
        "{:?}",
        finding.evidence
    );
    assert!(
        finding.evidence.iter().any(|line| line.contains(
            "`rate_triples` (src/lib.rs:16) is generated by macro `rate_case!` at src/lib.rs:7"
        )),
        "{:?}",
        finding.evidence
    );
    let next = finding.recommended_next_step.unwrap_or_default();
    assert!(
        next.contains("`rate_triples`") && next.contains("`rate_case!`"),
        "{next}"
    );
    Ok(())
}

#[test]
fn a_generator_without_a_test_attribute_names_no_limit() -> Result<(), String> {
    // The same macro emitting a plain fn generates no test.
    let fixture = Fixture::new(
        "not-a-generator",
        &GENERATED.replacen(
            "            #[test]\n            fn $name",
            "            #[inline]\n            fn $name",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn an_owner_no_macro_names_gets_no_generator_witness() -> Result<(), String> {
    let fixture = Fixture::new(
        "other-owner",
        &GENERATED.replacen("check(inner_rate($input))", "check($input * 3)", 1),
    )?;
    let finding = fixture.rate_finding()?;
    // Nothing in the tests names the owner now, so reach is `no`; the
    // generator still must not supply a witness on that path.
    assert_eq!(finding.class, ExposureClass::NoStaticPath);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn a_generator_invoked_only_inside_a_macro_body_names_no_limit() -> Result<(), String> {
    // An invocation inside another definition is part of that macro, not a
    // generated test; with no outer invocation there is no witness.
    let fixture = Fixture::new(
        "nested-only",
        &GENERATED.replacen(
            "    rate_case!(rate_triples, 4, |out| {\n        assert_eq!(out, 12);\n    });\n",
            "    macro_rules! unused_cases {\n        () => {\n            rate_case!(rate_triples, 4, |out| { assert_eq!(out, 12); });\n        };\n    }\n",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn a_generator_with_a_second_same_named_definition_names_no_limit() -> Result<(), String> {
    // Name-only lookup cannot tell which `rate_case!` the invocation
    // expands, so a duplicate definition withholds the witness.
    let fixture = Fixture::new(
        "duplicate-generator",
        &GENERATED.replacen(
            "    rate_case!(rate_triples",
            "    #[cfg(any())]\n    macro_rules! rate_case {\n        ($($t:tt)*) => {};\n    }\n    rate_case!(rate_triples",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}
