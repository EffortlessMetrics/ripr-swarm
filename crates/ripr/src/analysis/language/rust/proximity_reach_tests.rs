//! RIPR-SPEC-0114/0117 on proximity-only reach: a `weakly_exposed` finding
//! whose related tests only share the owner's file or a name token names the
//! transitive or macro reach limit instead of reporting a gap. RIPR-SPEC-0120
//! (#7089): a direct test whose only observer is an unread assertion macro
//! names that limit beside a proximity-only weak oracle.
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
fn a_chain_past_five_hops_does_not_displace_the_macro_witness() -> Result<(), String> {
    // RIPR-SPEC-0114 step 5 (#8047): the walk past the bound runs after
    // every other reach witness, so a second test entering a seven-hop chain
    // to the same owner leaves the one-macro-hop witness named.
    let chain = "pub fn d7(x: i64) -> i64 { d6(x) }
fn d6(x: i64) -> i64 { d5(x) }
fn d5(x: i64) -> i64 { d4(x) }
fn d4(x: i64) -> i64 { d3(x) }
fn d3(x: i64) -> i64 { d2(x) }
fn d2(x: i64) -> i64 { d1(x) }
fn d1(x: i64) -> i64 { inner_rate(x) }
";
    let fixture = Fixture::new(
        "macro-and-deep",
        &format!(
            "{MACRO_ROUTED}    #[test]\n    fn deep_triples() {{\n        assert_eq!(d7(4), 12);\n    }}\n}}\n{chain}"
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert!(
        all_relations(&finding, crate::analysis::classify::is_proximity_only),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustMacroReachUnresolved),
        "{:?}",
        finding.evidence
    );
    assert!(
        !finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved),
        "{:?}",
        finding.stop_reasons
    );

    // Discriminating half: without the macro route, the same chain names
    // the transitive limit through the walk past the bound.
    let no_macro = MACRO_ROUTED.replace("call_inner!(x)", "x");
    let fixture = Fixture::new(
        "deep-only",
        &format!(
            "{no_macro}    #[test]\n    fn deep_triples() {{\n        assert_eq!(d7(4), 12);\n    }}\n}}\n{chain}"
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert!(
        finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved),
        "{:?}",
        finding.evidence
    );
    // Pin the full inspect pointer, not a substring of the test name: a
    // wording or location regression must fail (#8106).
    assert_eq!(
        finding
            .evidence
            .iter()
            .find(|line| line.starts_with("For example, the test "))
            .map(String::as_str),
        Some(
            "For example, the test `deep_triples` (src/lib.rs:20) calls `d7`, an entry point that may lead here. Inspect it to judge whether this change is observed."
        ),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_no_static_path_chain_past_five_hops_does_not_displace_the_macro_witness() -> Result<(), String>
{
    // The `no_static_path` consumer (#8047): with no related test at all, a
    // seven-hop chain is still tried only after the macro witness.
    let lib = "fn inner_rate(x: i64) -> i64 {
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
pub fn d7(x: i64) -> i64 { d6(x) }
fn d6(x: i64) -> i64 { d5(x) }
fn d5(x: i64) -> i64 { d4(x) }
fn d4(x: i64) -> i64 { d3(x) }
fn d3(x: i64) -> i64 { d2(x) }
fn d2(x: i64) -> i64 { d1(x) }
fn d1(x: i64) -> i64 { inner_rate(x) }
";
    let tests = "#[test]\nfn macro_path() {\n    assert_eq!(reach::outer(4), 12);\n}\n#[test]\nfn deep_path() {\n    assert_eq!(reach::d7(4), 12);\n}\n";
    let fixture = Fixture::new("nsp-macro-and-deep", lib)?;
    fs::create_dir_all(fixture.0.join("tests")).map_err(|e| e.to_string())?;
    fs::write(fixture.0.join("tests/paths.rs"), tests).map_err(|e| e.to_string())?;
    let finding = fixture.rate_finding()?;
    // Fixture construction: no test relates to the owner.
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(
        finding.related_tests.is_empty(),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustMacroReachUnresolved),
        "{:?}",
        finding.evidence
    );

    // Discriminating half: without the macro route, the chain past the
    // bound names the transitive limit and its test.
    let fixture = Fixture::new("nsp-deep-only", &lib.replace("call_inner!(x)", "x"))?;
    fs::create_dir_all(fixture.0.join("tests")).map_err(|e| e.to_string())?;
    fs::write(fixture.0.join("tests/paths.rs"), tests).map_err(|e| e.to_string())?;
    let finding = fixture.rate_finding()?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(
        finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved),
        "{:?}",
        finding.evidence
    );
    // Pin the full inspect pointer, not a substring of the test name: a
    // wording or location regression must fail (#8106).
    assert_eq!(
        finding
            .evidence
            .iter()
            .find(|line| line.starts_with("For example, the test "))
            .map(String::as_str),
        Some(
            "For example, the test `deep_path` (tests/paths.rs:6) calls `d7`, an entry point that may lead here. Inspect it to judge whether this change is observed."
        ),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn a_forwarding_helper_chain_relates_and_establishes_reach_without_credit() -> Result<(), String> {
    // A same-file test that reaches a private helper through its forwarding
    // caller relates by the chain, not file proximity.
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
    // RIPR-SPEC-0159: the chain relation outranks file proximity, so the
    // same-file test now relates to `inner_rate` through `outer` as a
    // `helper_owner_call` and reach reads yes. The wrapper's return-value
    // oracle is not credited for the helper's change (no return-value
    // crediting through wrappers), so the class stays `weakly_exposed`
    // without the transitive-reach limit.
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.relation_reason == Some(RelationReason::HelperOwnerCall)),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{finding:?}");
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    assert!(
        !finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved),
        "{:?}",
        finding.stop_reasons
    );
    Ok(())
}

#[test]
fn a_weak_direct_test_preserves_the_unresolved_macro_witness() -> Result<(), String> {
    // The same macro witness exists, but a test calls the owner directly:
    // direct reach is established, but its weak oracle cannot erase the unresolved macro witness.
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
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustMacroReachUnresolved),
        "{:?}",
        finding.evidence
    );
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
            "test `rate_triples` (src/lib.rs:16) is generated by macro `rate_case!` at src/lib.rs:7"
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
fn an_owner_named_only_in_a_generator_comment_or_string_gets_no_witness() -> Result<(), String> {
    // The generated test never calls the owner; the name appears only in a
    // comment and a string inside the generator body.
    let lib = GENERATED.replacen(
        "check(inner_rate($input));",
        "check($input * 3); // inner_rate\n                let _label = \"inner_rate\";",
        1,
    );
    // Fixture construction: the owner's name survives only in the comment and
    // the string.
    assert!(lib.contains("// inner_rate") && lib.contains("\"inner_rate\""));
    assert!(!lib.contains("inner_rate($input)"));
    let fixture = Fixture::new("owner-in-text", &lib)?;
    let finding = fixture.rate_finding()?;
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    assert!(
        !finding
            .evidence
            .iter()
            .any(|line| line.contains("generated by macro")),
        "{:?}",
        finding.evidence
    );
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

/// Weak classification can coexist with a confirmed discriminator when an
/// infection or propagation stage is weak. Do not replace that discriminator
/// with a macro limit. These are gate-boundary states, not runtime evidence.
#[test]
fn mixed_macro_limit_preserves_confirmed_discrimination_and_examined_misses() -> Result<(), String>
{
    let fixture = Fixture::new(
        "mixed-guard",
        &format!(
            "{MACRO_ROUTED}    #[test]\n    fn rate_is_positive() {{ assert!(inner_rate(2) > 0); }}\n}}\n"
        ),
    )?;
    let mut finding = fixture.rate_finding()?;
    finding.static_limit_kind = None;
    assert!(super::needs_mixed_macro_reach_limit(&finding));
    let before = finding.clone();
    for state in [StageState::Weak, StageState::No] {
        let mut confirmed = before.clone();
        confirmed.ripr.reveal.discriminate.state = StageState::Yes;
        confirmed.ripr.infect.state = state.clone();
        assert!(!super::needs_mixed_macro_reach_limit(&confirmed));
        confirmed.ripr.infect.state = StageState::Yes;
        confirmed.ripr.propagate.state = state;
        assert!(!super::needs_mixed_macro_reach_limit(&confirmed));
    }
    let mut miss = before.clone();
    let mut direct_count = 0;
    for test in &mut miss.related_tests {
        if test.relation_reason == Some(RelationReason::DirectOwnerCall) {
            direct_count += 1;
            test.miss = Some(crate::domain::RelatedTestMiss::AssertionNotObserving);
        }
    }
    assert_eq!(direct_count, 1);
    assert!(!super::needs_mixed_macro_reach_limit(&miss));
    let mut no_direct_oracle = before.clone();
    let mut direct_count = 0;
    for test in &mut no_direct_oracle.related_tests {
        if test.relation_reason == Some(RelationReason::DirectOwnerCall) {
            direct_count += 1;
            assert!(test.oracle.is_some());
            test.oracle = None;
        }
    }
    assert_eq!(direct_count, 1);
    assert!(!super::needs_mixed_macro_reach_limit(&no_direct_oracle));
    finding.static_limit_kind = Some(StaticLimitKind::RustAssertionContextUnresolved);
    assert!(!super::needs_mixed_macro_reach_limit(&finding));
    Ok(())
}

/// #7089: the only test that calls the owner observes it through a custom
/// assertion macro ripr cannot read; a weak oracle in a test that only shares
/// the file made the finding `weakly_exposed`.
const CUSTOM_ASSERT: &str = "fn inner_rate(x: i64) -> i64 {
    3 * x
}
pub fn other(x: i64) -> i64 {
    x + 1
}
#[cfg(test)]
mod tests {
    use super::*;
    macro_rules! assert_rate {
        ($actual:expr, $expected:expr) => {
            if $actual != $expected {
                panic!(\"rate mismatch\");
            }
        };
    }
    #[test]
    fn rate_triples() {
        assert_rate!(inner_rate(4), 12);
    }
    #[test]
    fn other_is_positive() {
        assert!(other(1) > 0);
    }
}
";

/// Fixture construction for the #7089 cases: `rate_triples` is related by a
/// direct relation with the given oracle presence, and a proximity-only test
/// holds a recognized oracle.
fn assert_custom_assert_construction(finding: &Finding, direct_has_oracle: bool) {
    let direct = finding
        .related_tests
        .iter()
        .find(|test| test.name == "rate_triples");
    assert!(
        direct.is_some_and(|test| test
            .relation_reason
            .is_some_and(|reason| !crate::analysis::classify::is_proximity_only(reason))
            && test.oracle.is_some() == direct_has_oracle),
        "{:?}",
        finding.related_tests
    );
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.oracle.is_some()
                && test
                    .relation_reason
                    .is_some_and(crate::analysis::classify::is_proximity_only)),
        "{:?}",
        finding.related_tests
    );
}

#[test]
fn an_owner_call_inside_an_unread_assertion_macro_names_the_macro_limit() -> Result<(), String> {
    let fixture = Fixture::new("custom-assert", CUSTOM_ASSERT)?;
    let finding = fixture.rate_finding()?;
    // Fixture construction: the direct test has no recognized oracle, and the
    // only recognized one comes from a test that just shares the file.
    let direct = finding
        .related_tests
        .iter()
        .find(|test| test.name == "rate_triples")
        .ok_or("rate_triples is not related")?;
    assert!(direct.oracle.is_none(), "{direct:?}");
    assert!(
        finding
            .related_tests
            .iter()
            .filter(|test| test.oracle.is_some())
            .all(|test| test
                .relation_reason
                .is_some_and(crate::analysis::classify::is_proximity_only)),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved),
        "{:?}",
        finding.evidence
    );
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("`rate_triples`") && line.contains("`assert_rate!`")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn an_unread_assertion_macro_without_the_owner_call_keeps_the_gap() -> Result<(), String> {
    // The owner's value reaches the macro only through a binding.
    let fixture = Fixture::new(
        "custom-assert-binding",
        &CUSTOM_ASSERT.replacen(
            "        assert_rate!(inner_rate(4), 12);",
            "        let rate = inner_rate(4);\n        assert_rate!(rate, 12);",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert_custom_assert_construction(&finding, false);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn a_direct_test_with_a_recognized_oracle_keeps_the_gap() -> Result<(), String> {
    // The direct test's own weak oracle is what ripr judged; the custom macro
    // beside it does not turn that judgment into a limit.
    let fixture = Fixture::new(
        "custom-assert-direct-oracle",
        &CUSTOM_ASSERT.replacen(
            "        assert_rate!(inner_rate(4), 12);",
            "        assert!(inner_rate(4) > 0);\n        assert_rate!(inner_rate(4), 12);",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert_custom_assert_construction(&finding, true);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn an_owner_call_quoted_by_stringify_keeps_the_gap() -> Result<(), String> {
    // `stringify!` turns the call into text; the owner never runs.
    let fixture = Fixture::new(
        "custom-assert-stringify",
        &CUSTOM_ASSERT.replacen(
            "        assert_rate!(inner_rate(4), 12);",
            "        assert_rate!(stringify!(inner_rate(4)), \"inner_rate(4)\");",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert_custom_assert_construction(&finding, false);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

#[test]
fn a_trait_default_method_owner_keeps_the_gap() -> Result<(), String> {
    // A bare `inner_rate(` call cannot name a trait default method, so it is
    // no evidence that the macro asserts on the owner.
    let fixture = Fixture::new(
        "custom-assert-trait-default",
        &CUSTOM_ASSERT.replacen(
            "fn inner_rate(x: i64) -> i64 {\n    3 * x\n}\n",
            "pub trait Rate { fn inner_rate(x: i64) -> i64 {\n    3 * x\n} }\n",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert!(
        finding
            .probe
            .owner
            .as_ref()
            .is_some_and(|owner| owner.0.ends_with("::inner_rate")),
        "{:?}",
        finding.probe.owner
    );
    assert_custom_assert_construction(&finding, false);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    Ok(())
}

// #6778: one two-line diff asks the proximity gate about two arms. Count
// actual lazy graph builds through the retained Rust adapter, not calls to
// its cheap constructor, and pin output separately from the resource oracle.
const TWO_ARM_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -20,6 +20,6 @@ impl FromStr for Unit {\n pub fn seconds(u: Unit) -> u64 {\n     match u {\n-        Unit::Week => 600_000,\n-        Unit::Fortnight => 1_296_000,\n+        Unit::Week => 604_800,\n+        Unit::Fortnight => 1_209_600,\n     }\n }\n";

fn shared_reach_source() -> String {
    // Keep the retained fixture bytes local so packaged crate tests do not
    // acquire a compile-time dependency on the repository fixture directory.
    let source = r#"use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub enum Unit {
    Week,
    Fortnight,
}

impl FromStr for Unit {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "week" => Ok(Unit::Week),
            "fortnight" => Ok(Unit::Fortnight),
            _ => Err(()),
        }
    }
}

pub fn seconds(u: Unit) -> u64 {
    match u {
        Unit::Week => 604_800,
        Unit::Fortnight => 1_209_600,
    }
}

pub fn seconds_bridge(u: Unit) -> u64 {
    seconds(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_total() {
        let total: u64 = ["week", "fortnight"]
            .iter()
            .map(|s| seconds(Unit::from_str(s).unwrap()))
            .sum();
        assert_eq!(total, 1_814_400);
    }

    #[test]
    fn bridge_fortnight() {
        assert_eq!(seconds_bridge(Unit::Fortnight), 1_209_600);
    }

    #[test]
    fn from_str_fortnight() {
        assert!(matches!(Unit::from_str("fortnight"), Ok(Unit::Fortnight)));
    }
}
"#;
    format!(
        "{source}\npub fn fixture(u: Unit) -> u64 {{\n    match u {{ Unit::Week => 604_800, Unit::Fortnight => 1_209_600 }}\n}}\n"
    )
}

impl Fixture {
    fn arm_findings(&self) -> Result<Vec<Finding>, String> {
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
            &diff::parse_unified_diff(TWO_ARM_DIFF),
        )?;
        let arms: Vec<_> = result
            .findings
            .into_iter()
            .filter(|finding| finding.probe.family == crate::domain::ProbeFamily::MatchArm)
            .collect();
        assert_eq!(arms.len(), 2, "both changed arms must reach classification");
        Ok(arms)
    }
}

fn fortnight_class(findings: &[Finding]) -> Result<ExposureClass, String> {
    findings
        .iter()
        .find(|finding| finding.probe.expression == "Unit::Fortnight =>")
        .map(|finding| finding.class.clone())
        .ok_or_else(|| "missing changed Fortnight arm".to_string())
}

#[test]
fn proximity_graph_is_shared_across_changed_arms_cold_and_warm() -> Result<(), String> {
    use crate::analysis::classify::TransitiveReachIndex;

    let fixture = Fixture::new("shared-graph", &shared_reach_source())?;
    let start = TransitiveReachIndex::graph_build_count();
    let cold = fixture.arm_findings()?;
    assert_eq!(fortnight_class(&cold)?, ExposureClass::Exposed);
    assert_eq!(TransitiveReachIndex::graph_build_count() - start, 1);

    let start = TransitiveReachIndex::graph_build_count();
    let warm = fixture.arm_findings()?;
    assert_eq!(
        serde_json::to_string(&cold).map_err(|e| e.to_string())?,
        serde_json::to_string(&warm).map_err(|e| e.to_string())?
    );
    assert_eq!(TransitiveReachIndex::graph_build_count() - start, 1);
    Ok(())
}

#[test]
fn proximity_graph_rebuilds_after_a_same_path_helper_edit() -> Result<(), String> {
    use crate::analysis::classify::TransitiveReachIndex;

    let source = shared_reach_source();
    let fixture = Fixture::new("edited-graph", &source)?;
    let before = fixture.arm_findings()?;
    assert_eq!(fortnight_class(&before)?, ExposureClass::Exposed);

    // Both versions return the same literal to the test, but the edited
    // helper calls the independent implementation and cannot run `seconds`.
    let edited = source.replacen("    seconds(u)\n", "    fixture(u)\n", 1);
    assert_ne!(source, edited, "the helper edit must change its call edge");
    fs::write(fixture.0.join("src/lib.rs"), edited).map_err(|e| e.to_string())?;
    let start = TransitiveReachIndex::graph_build_count();
    let after = fixture.arm_findings()?;
    assert_eq!(fortnight_class(&after)?, ExposureClass::WeaklyExposed);
    assert_eq!(TransitiveReachIndex::graph_build_count() - start, 1);

    fs::write(fixture.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
    let restored = fixture.arm_findings()?;
    assert_eq!(
        serde_json::to_string(&before).map_err(|e| e.to_string())?,
        serde_json::to_string(&restored).map_err(|e| e.to_string())?
    );
    Ok(())
}
