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

    /// The finding on line 5 of the `numeric_identifier` fixtures, where
    /// `digit > b'9'` became `digit >= b'9'` (#11481).
    fn boundary_finding(&self) -> Result<Finding, String> {
        let diff_text = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5,1 +5,1 @@\n-        if digit < b'0' || digit > b'9' {\n+        if digit < b'0' || digit >= b'9' {\n";
        self.finding_on_line(diff_text, 5)
    }

    fn finding_on_line(&self, diff_text: &str, line: usize) -> Result<Finding, String> {
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
            .find(|finding| finding.probe.location.line == line)
            .ok_or_else(|| "no finding on the changed line".to_string())
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
fn a_forwarding_helper_chain_relates_and_credits_an_exact_entry_pin() -> Result<(), String> {
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
    // same-file test relates to `inner_rate` through `outer` as a
    // `helper_owner_call` and reach reads yes. `outer` hands the helper's
    // result on unchanged, both are unique bare-name functions of one scope,
    // and the exact `outer(4) == 12` pin moves with the helper (free-call
    // entry pin, #9638), so the class is `exposed` without the
    // transitive-reach limit.
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.relation_reason == Some(RelationReason::HelperOwnerCall)),
        "{:?}",
        finding.related_tests
    );
    assert_eq!(finding.ripr.reach.state, StageState::Yes, "{finding:?}");
    assert_eq!(finding.class, ExposureClass::Exposed);
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
            test.role = crate::domain::RelatedTestRole::ExaminedOnly;
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
fn a_direct_test_whose_own_boolean_assert_is_unadmitted_beside_a_custom_macro_is_named_not_credited()
-> Result<(), String> {
    // The custom macro is opaque, so the test's own `assert!` is not shown to
    // run and no longer counts as a recognized oracle (#10389: the boolean
    // `assert!` takes the same execution admission as `assert_eq!`). The
    // direct test then has no oracle and the evidence names the macro.
    let fixture = Fixture::new(
        "custom-assert-direct-oracle",
        &CUSTOM_ASSERT.replacen(
            "        assert_rate!(inner_rate(4), 12);",
            "        assert!(inner_rate(4) > 0);\n        assert_rate!(inner_rate(4), 12);",
            1,
        ),
    )?;
    let finding = fixture.rate_finding()?;
    assert_custom_assert_construction(&finding, false);
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    assert!(
        finding
            .evidence
            .iter()
            .any(|line| line.contains("assertion not credited") && line.contains("`assert_rate!`")),
        "{:?}",
        finding.evidence
    );
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

/// Runs `work` with the classify pool pinned to `threads` workers.
fn with_classify_threads<T>(threads: usize, work: impl FnOnce() -> T) -> T {
    super::CLASSIFY_THREADS_FOR_TEST.with(|pinned| pinned.set(Some(threads)));
    let out = work();
    super::CLASSIFY_THREADS_FOR_TEST.with(|pinned| pinned.set(None));
    out
}

#[test]
fn classified_findings_are_identical_in_order_on_one_and_many_threads() -> Result<(), String> {
    let fixture = Fixture::new("thread-parity", &shared_reach_source())?;
    let encode = |findings: &[Finding]| serde_json::to_string(findings).map_err(|e| e.to_string());
    let serial = encode(&with_classify_threads(1, || fixture.arm_findings())?)?;
    // Repeated so a fill-order race between workers has several chances to
    // change a memo-dependent verdict.
    for threads in [2, 4, 8] {
        for round in 0..5 {
            let parallel = encode(&with_classify_threads(threads, || fixture.arm_findings())?)?;
            if serial != parallel {
                return Err(format!(
                    "{threads} classify threads differ from one thread (round {round})"
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn classify_workers_follow_the_callers_cancellation() -> Result<(), String> {
    use crate::analysis::cancellation::{self, AnalysisAbortKind, AnalysisCancellationToken};

    let fixture = Fixture::new("cancel", &shared_reach_source())?;
    let live = AnalysisCancellationToken::new();
    let completed = cancellation::with_token(&live, || {
        with_classify_threads(4, || fixture.arm_findings())
    })?;
    assert_eq!(completed.len(), 2, "a live token must not drop a probe");

    let cancelled = AnalysisCancellationToken::new();
    cancelled.cancel(AnalysisAbortKind::Cancelled);
    let outcome = cancellation::with_token(&cancelled, || {
        with_classify_threads(4, || fixture.arm_findings())
    });
    assert!(
        outcome.is_err(),
        "a cancelled run must fail, not return partial findings"
    );

    // A budget that only a classify worker sees as spent: the caller thread's
    // seeding checkpoints pass, so the abort can only come from a worker that
    // installed the caller's token. A worker that ignored the token would
    // finish every probe and this run would succeed.
    let started = std::time::Instant::now();
    let worker_sees_spent_budget: cancellation::AnalysisClock = std::sync::Arc::new(move || {
        let on_worker = std::thread::current()
            .name()
            .is_some_and(|name| name.starts_with("ripr-classify-"));
        if on_worker {
            started + std::time::Duration::from_mins(2)
        } else {
            started
        }
    });
    let budgeted = AnalysisCancellationToken::with_budget(
        started,
        std::time::Duration::from_mins(1),
        worker_sees_spent_budget,
    );
    let outcome = cancellation::with_token(&budgeted, || {
        with_classify_threads(4, || fixture.arm_findings())
    });
    assert!(
        outcome.is_err(),
        "a budget spent on a worker must fail the run, not publish a partial result"
    );
    assert_eq!(
        budgeted.abort_kind(),
        Some(AnalysisAbortKind::DeadlineExceeded),
        "the worker's checkpoint must be what recorded the abort"
    );
    Ok(())
}

/// #9211: the workspace macro-site scan a classify run needs is paid before
/// the probes are queued, fanned out over the classify pool, not run in line
/// on whichever worker reaches it first while its siblings sleep on the memo.
#[test]
fn the_classify_run_fans_the_workspace_macro_scan_out_before_its_probes() -> Result<(), String> {
    use std::hash::{Hash, Hasher};

    let source = format!("{}\n// convoy-9211-unique-fixture\n", shared_reach_source());
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.as_str().hash(&mut hasher);
    let fixture_hash = hasher.finish();
    let fixture = Fixture::new("convoy-9211", &source)?;
    let findings = with_classify_threads(4, || fixture.arm_findings())?;
    assert_eq!(findings.len(), 2, "both probes must classify");

    let log = crate::analysis::classify::SCAN_LOG
        .lock()
        .map_err(|e| e.to_string())?;
    let decisions: Vec<bool> = log
        .iter()
        .filter(|(hash, _)| *hash == fixture_hash)
        .map(|(_, fanned_out)| *fanned_out)
        .collect();
    assert!(
        !decisions.is_empty(),
        "the run must scan the fixture's macro sites"
    );
    assert!(
        decisions.iter().all(|fanned_out| *fanned_out),
        "a scan ran in line on a classify worker: {decisions:?}"
    );
    Ok(())
}

#[test]
fn nested_scans_run_in_line_on_a_pool_worker_only() -> Result<(), String> {
    use crate::analysis::classify::nested_parallelism_allowed;

    assert!(
        nested_parallelism_allowed(),
        "the caller thread may fan out"
    );
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(2)
        .build()
        .map_err(|e| e.to_string())?;
    assert!(
        !pool.install(nested_parallelism_allowed),
        "a worker must not block in a nested par_iter while it can hold a memo lock"
    );
    Ok(())
}

// #11481: an `infection_unknown` finding whose related tests are all
// proximity-only names the transitive limit like a `weakly_exposed` one.
// The private owner is reached only through `Version::parse` ->
// `Version::from_str`; the test is related by name alone.
const PRIVATE_HELPER_BEHIND_PUBLIC_ENTRY: &str = r####"use std::str::FromStr;
fn numeric_identifier(input: &str) -> Result<u64, String> {
    let mut value = 0u64;
    for digit in input.bytes() {
        if digit < b'0' || digit > b'9' {
            return Err("bad digit".to_string());
        }
        value = value * 10 + (digit - b'0') as u64;
    }
    Ok(value)
}
pub struct Version(pub u64);
pub struct Req(pub u64);
impl FromStr for Version {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        Ok(Version(numeric_identifier(text)?))
    }
}
impl FromStr for Req {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        Ok(Req(text.len() as u64))
    }
}
impl Version {
    pub fn parse(text: &str) -> Result<Self, String> {
        Version::from_str(text)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_identifier_rejects_garbage() {
        assert!(Version::parse("1x").is_err());
    }
}
"####;

const ENTRY_NEVER_CALLED_BY_A_TEST: &str = r####"use std::str::FromStr;
fn numeric_identifier(input: &str) -> Result<u64, String> {
    let mut value = 0u64;
    for digit in input.bytes() {
        if digit < b'0' || digit > b'9' {
            return Err("bad digit".to_string());
        }
        value = value * 10 + (digit - b'0') as u64;
    }
    Ok(value)
}
pub struct Version(pub u64);
pub struct Req(pub u64);
impl FromStr for Version {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        Ok(Version(numeric_identifier(text)?))
    }
}
impl FromStr for Req {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        Ok(Req(text.len() as u64))
    }
}
impl Version {
    pub fn parse(text: &str) -> Result<Self, String> {
        Version::from_str(text)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_identifier_rejects_garbage() {
        assert_eq!(1 + 1, 2);
    }
}
"####;

const ENTRY_NEVER_CALLS_THE_OWNER: &str = r####"use std::str::FromStr;
fn numeric_identifier(input: &str) -> Result<u64, String> {
    let mut value = 0u64;
    for digit in input.bytes() {
        if digit < b'0' || digit > b'9' {
            return Err("bad digit".to_string());
        }
        value = value * 10 + (digit - b'0') as u64;
    }
    Ok(value)
}
pub struct Version(pub u64);
pub struct Req(pub u64);
impl FromStr for Version {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        Ok(Version(numeric_identifier(text)?))
    }
}
impl FromStr for Req {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        Ok(Req(text.len() as u64))
    }
}
impl Version {
    pub fn parse(text: &str) -> Result<Self, String> {
        Ok(Version(text.len() as u64))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_identifier_rejects_garbage() {
        assert!(Version::parse("1x").is_err());
    }
}
"####;

#[test]
fn an_infection_unknown_private_helper_behind_a_public_entry_names_the_limit() -> Result<(), String>
{
    let fixture = Fixture::new("private-entry", PRIVATE_HELPER_BEHIND_PUBLIC_ENTRY)?;
    let finding = fixture.boundary_finding()?;
    // Fixture construction: infection_unknown, weak reach, and the only
    // relation is the test's name.
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    assert_eq!(finding.ripr.reach.state, StageState::Weak);
    assert!(
        all_relations(&finding, |reason| reason == RelationReason::OwnerNamedTest),
        "{:?}",
        finding.related_tests
    );
    // The class and reach stay; only the named limitation is added, as
    // indirect candidate-path evidence and never a related-test relation.
    assert!(
        finding.static_limit_kind.is_some(),
        "{:?}",
        finding.evidence
    );
    assert!(
        finding
            .stop_reasons
            .contains(&StopReason::TransitiveReachUnresolved)
    );
    assert_eq!(
        finding.related_tests.len(),
        1,
        "{:?}",
        finding.related_tests
    );
    assert!(
        finding.evidence.iter().any(|line| line.contains("`parse`")),
        "{:?}",
        finding.evidence
    );
    Ok(())
}

#[test]
fn an_infection_unknown_helper_no_test_enters_keeps_its_bare_reading() -> Result<(), String> {
    for (label, lib) in [
        ("never-called", ENTRY_NEVER_CALLED_BY_A_TEST),
        ("never-calls-owner", ENTRY_NEVER_CALLS_THE_OWNER),
    ] {
        let fixture = Fixture::new(label, lib)?;
        let finding = fixture.boundary_finding()?;
        assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{label}");
        assert_eq!(finding.ripr.reach.state, StageState::Weak, "{label}");
        assert_eq!(finding.static_limit_kind, None, "{label}");
        assert!(
            finding.stop_reasons.iter().all(|reason| {
                *reason != StopReason::TransitiveReachUnresolved
                    && *reason != StopReason::MacroReachUnresolved
            }),
            "{label}"
        );
    }
    Ok(())
}

// #11481 negatives for the `infection_unknown` + `owner_named_test` admission.
const NUMERIC_OWNER: &str = r####"use std::str::FromStr;
fn numeric_identifier(input: &str) -> Result<u64, String> {
    let mut value = 0u64;
    for digit in input.bytes() {
        if digit < b'0' || digit > b'9' {
            return Err("bad digit".to_string());
        }
        value = value * 10 + (digit - b'0') as u64;
    }
    Ok(value)
}
"####;

const OWNER_NAMED_TEST: &str = r####"#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numeric_identifier_rejects_garbage() {
        assert!(other::parse("1x").is_err());
    }
}
"####;

fn assert_unlimited(finding: &Finding, label: &str) {
    assert_eq!(finding.static_limit_kind, None, "{label}");
    assert!(
        finding.stop_reasons.iter().all(|reason| {
            *reason != StopReason::TransitiveReachUnresolved
                && *reason != StopReason::MacroReachUnresolved
        }),
        "{label}"
    );
}

// (a) A same-named function in a different module is what the entry calls.
// Observed behavior: the lexical walk matches callee names only, so it does
// not distinguish `other::numeric_identifier` from the changed owner and
// still names the (advisory) limit. This pins that known limit; the class,
// reach and related tests are unchanged either way.
#[test]
fn a_same_named_function_in_another_module_is_witnessed_by_name_only_known_limit()
-> Result<(), String> {
    let lib = format!(
        "{NUMERIC_OWNER}pub mod other {{\n    pub fn numeric_identifier(input: &str) -> Result<u64, String> {{\n        Ok(input.len() as u64)\n    }}\n    pub fn parse(text: &str) -> Result<u64, String> {{\n        numeric_identifier(text)\n    }}\n}}\n{OWNER_NAMED_TEST}"
    );
    let fixture = Fixture::new("same-name-other-module", &lib)?;
    let finding = fixture.boundary_finding()?;
    assert_eq!(finding.class, ExposureClass::InfectionUnknown);
    assert_eq!(finding.ripr.reach.state, StageState::Weak);
    assert!(all_relations(&finding, |reason| {
        reason == RelationReason::OwnerNamedTest
    }));
    // Known limit: witnessed through the name-only walk, still advisory.
    assert!(
        finding.static_limit_kind.is_some(),
        "{:?}",
        finding.evidence
    );
    assert_eq!(finding.related_tests.len(), 1);
    Ok(())
}

// (d) A `#[cfg(test)]`-only helper as the entry is not production reach.
#[test]
fn a_cfg_test_only_helper_entry_names_no_limit() -> Result<(), String> {
    let lib = format!(
        "{NUMERIC_OWNER}#[cfg(test)]\nmod helpers {{\n    pub fn parse(text: &str) -> Result<u64, String> {{\n        super::numeric_identifier(text)\n    }}\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn numeric_identifier_rejects_garbage() {{\n        assert!(helpers::parse(\"1x\").is_err());\n    }}\n}}\n"
    );
    let fixture = Fixture::new("cfg-test-helper", &lib)?;
    let finding = fixture.boundary_finding()?;
    assert_eq!(finding.ripr.reach.state, StageState::Weak);
    assert_unlimited(&finding, "cfg-test-helper");
    Ok(())
}

fn related(
    template: &crate::domain::RelatedTest,
    reason: RelationReason,
) -> crate::domain::RelatedTest {
    let mut row = template.clone();
    row.relation_reason = Some(reason);
    row
}

// (b) and (c) pin the trigger on a real owner-named finding, varying only the
// class and the relation set.
#[test]
fn the_owner_named_admission_is_limited_to_infection_unknown_all_owner_named() -> Result<(), String>
{
    let lib = format!(
        "{NUMERIC_OWNER}pub struct Version(pub u64);\nimpl Version {{\n    pub fn parse(text: &str) -> Result<Self, String> {{\n        Ok(Version(numeric_identifier(text)?))\n    }}\n}}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn numeric_identifier_rejects_garbage() {{\n        assert!(Version::parse(\"1x\").is_err());\n    }}\n}}\n"
    );
    let fixture = Fixture::new("admission-matrix", &lib)?;
    let mut finding = fixture.boundary_finding()?;
    finding.static_limit_kind = None;
    finding.class = ExposureClass::InfectionUnknown;
    finding.ripr.reach.state = StageState::Weak;
    let template = finding
        .related_tests
        .first()
        .cloned()
        .ok_or("fixture has no related test")?;
    let base = related(&template, RelationReason::OwnerNamedTest);
    finding.related_tests = vec![base.clone()];
    assert!(super::needs_proximity_reach_limit(&finding));
    // (b) mixed with another non-proximity relation stays unlimited.
    for other in [
        RelationReason::ImportPathAffinity,
        RelationReason::FixtureOwnerAffinity,
        RelationReason::DirectOwnerCall,
    ] {
        finding.related_tests = vec![base.clone(), related(&template, other)];
        assert!(!super::needs_proximity_reach_limit(&finding), "{other:?}");
    }
    // (c) deliberate asymmetry: owner_named_test alone does not limit a
    // weakly_exposed finding (decided, #11481; see the natural-fixture pair below).
    finding.class = ExposureClass::WeaklyExposed;
    finding.related_tests = vec![base];
    assert!(!super::needs_proximity_reach_limit(&finding));
    Ok(())
}

// RIPR-SPEC-0114 (#11719): an owner called from several `impl Trait for X {
// fn fmt }` names the trait-dispatch ambiguity instead of guessing an impl.

/// `pad` (`len >= min` on line 6, changed to `len > min`) plus `impls`
/// (each a full item).
fn pad_lib(impls: &str) -> String {
    format!(
        "use std::fmt;\npub struct A;\npub struct B;\n\nfn pad(len: usize, min: usize) -> bool {{\n    len >= min\n}}\n{impls}"
    )
}

const PAD_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -6,1 +6,1 @@\n-    len >= min\n+    len > min\n";

fn display_impl(ty: &str) -> String {
    format!(
        "impl fmt::Display for {ty} {{\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{\n        let _ = pad(1, 2);\n        f.write_str(\"x\")\n    }}\n}}\n"
    )
}

/// An integration test that formats an `A`; no call fact names `fmt`.
const FORMAT_TEST: &str =
    "#[test]\nfn displays() {\n    assert_eq!(format!(\"{}\", reach::A), \"x\");\n}\n";

/// A test that formats the other type, `B`, whose `fmt` never calls `pad`.
const OTHER_TEST: &str =
    "#[test]\nfn displays_b() {\n    assert_eq!(format!(\"{}\", reach::B), \"b\");\n}\n";

fn pad_finding(label: &str, lib: &str) -> Result<Finding, String> {
    pad_finding_with(label, lib, FORMAT_TEST)
}

fn pad_finding_with(label: &str, lib: &str, test: &str) -> Result<Finding, String> {
    let fixture = Fixture::new(label, lib)?;
    fs::create_dir_all(fixture.0.join("tests")).map_err(|e| e.to_string())?;
    fs::write(fixture.0.join("tests/fmt.rs"), test).map_err(|e| e.to_string())?;
    fixture.finding_on_line(PAD_DIFF, 6)
}

fn assert_bare_no_static_path(finding: &Finding, label: &str) {
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{label}");
    assert_eq!(finding.ripr.reach.state, StageState::No, "{label}");
    assert!(finding.related_tests.is_empty(), "{label}");
}

#[test]
fn an_owner_called_from_several_fmt_impls_names_the_dispatch_ambiguity() -> Result<(), String> {
    let impls = format!("{}{}", display_impl("A"), display_impl("B"));
    let finding = pad_finding("pad-ambiguous", &pad_lib(&impls))?;
    // Class, reach, related tests and confidence are those of the bare reading.
    assert_bare_no_static_path(&finding, "ambiguous");
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
    let line = finding
        .evidence
        .iter()
        .find(|line| line.starts_with("Trait-dispatch ambiguity: "))
        .ok_or("no ambiguity line")?;
    assert!(
        line.contains("owner `pad` is called from 2 impls of `fmt` (A, B), one of 2 `fmt` impls")
            && line.contains("cannot tell whether any test reaches one of these impls"),
        "{line}"
    );
    // No witness test is named and no runtime claim is made.
    assert!(
        !finding
            .evidence
            .iter()
            .any(|l| l.starts_with(crate::domain::TRANSITIVE_REACH_WITNESS_PREFIX))
    );
    Ok(())
}

#[test]
fn the_ambiguity_lists_a_bounded_number_of_candidates() -> Result<(), String> {
    let mut impls = String::new();
    let mut types = String::new();
    for ty in ["A", "B", "C", "D", "E", "F", "G"] {
        impls.push_str(&display_impl(ty));
        if ty > "B" {
            types.push_str(&format!(" pub struct {ty};"));
        }
    }
    let lib = pad_lib(&impls).replace("pub struct B;\n", &format!("pub struct B;{types}\n"));
    let finding = pad_finding("pad-bounded", &lib)?;
    let line = finding
        .evidence
        .iter()
        .find(|line| line.starts_with("Trait-dispatch ambiguity: "))
        .ok_or("no ambiguity line")?;
    assert!(
        line.contains("7 impls of `fmt` (A, B, C, D, E, and 2 more), one of 7 `fmt` impls"),
        "{line}"
    );
    Ok(())
}

#[test]
fn one_calling_impl_among_many_names_the_ambiguity_with_that_candidate() -> Result<(), String> {
    // The dtolnay/semver `display::pad` shape (#11719): one `Display::fmt`
    // calls the private helper, the crate has other `fmt` impls.
    let impls = format!(
        "{}impl fmt::Display for B {{\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{\n        f.write_str(\"b\")\n    }}\n}}\n",
        display_impl("A")
    );
    let finding = pad_finding_with("pad-single-of-many", &pad_lib(&impls), OTHER_TEST)?;
    assert_bare_no_static_path(&finding, "single-of-many");
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::RustTransitiveReachUnresolved)
    );
    let line = finding
        .evidence
        .iter()
        .find(|line| line.starts_with("Trait-dispatch ambiguity: "))
        .ok_or("no ambiguity line")?;
    assert!(
        line.contains("called from 1 impl of `fmt` (A), one of 2 `fmt` impls"),
        "{line}"
    );
    Ok(())
}

#[test]
fn a_unique_fmt_impl_calling_the_owner_keeps_the_bare_reading() -> Result<(), String> {
    // The name `fmt` identifies the impl, so there is no ambiguity to name.
    let finding = pad_finding("pad-unique", &pad_lib(&display_impl("A")))?;
    assert_unlimited(&finding, "unique");
    assert!(
        !finding
            .evidence
            .iter()
            .any(|l| l.starts_with("Trait-dispatch ambiguity: "))
    );
    Ok(())
}

#[test]
fn a_second_impl_that_does_not_call_the_owner_is_not_a_caller() -> Result<(), String> {
    let impls = format!(
        "{}impl fmt::Display for B {{\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{\n        f.write_str(\"b\")\n    }}\n}}\n",
        display_impl("A")
    );
    let finding = pad_finding_with("pad-not-caller", &pad_lib(&impls), OTHER_TEST)?;
    let line = finding
        .evidence
        .iter()
        .find(|line| line.starts_with("Trait-dispatch ambiguity: "))
        .ok_or("no ambiguity line")?;
    assert!(!line.contains("(A, B)") && !line.contains(", B)"), "{line}");
    Ok(())
}

#[test]
fn no_calling_impl_keeps_the_bare_reading() -> Result<(), String> {
    let impls = "impl fmt::Display for A {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        f.write_str(\"a\")\n    }\n}\nimpl fmt::Display for B {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        f.write_str(\"b\")\n    }\n}\n";
    let finding = pad_finding("pad-none", &pad_lib(impls))?;
    assert_bare_no_static_path(&finding, "none");
    assert_unlimited(&finding, "none");
    Ok(())
}

#[test]
fn several_impls_of_a_trait_method_ripr_does_not_dispatch_implicitly_stay_bare()
-> Result<(), String> {
    let run_impl = |ty: &str| {
        format!(
            "impl Run for {ty} {{\n    fn run(&self) -> bool {{\n        pad(1, 2)\n    }}\n}}\n"
        )
    };
    let impls = format!(
        "pub trait Run {{\n    fn run(&self) -> bool;\n}}\n{}{}",
        run_impl("A"),
        run_impl("B")
    );
    let finding = pad_finding("pad-run", &pad_lib(&impls))?;
    assert_bare_no_static_path(&finding, "run");
    assert_unlimited(&finding, "run");
    Ok(())
}

#[test]
fn several_impls_without_any_test_stay_bare() -> Result<(), String> {
    let impls = format!("{}{}", display_impl("A"), display_impl("B"));
    let lib = format!(
        "use std::fmt;\npub struct A;\npub struct B;\n\nfn pad(len: usize, min: usize) -> bool {{\n    len >= min\n}}\n{impls}"
    );
    let finding = Fixture::new("pad-no-tests", &lib)?.finding_on_line(PAD_DIFF, 6)?;
    assert_eq!(finding.static_limit_kind, None, "{:?}", finding.evidence);
    assert!(finding.related_tests.is_empty());
    Ok(())
}

const CALLING_B_FMT: &str = "fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        let _ = pad(1, 2);\n        f.write_str(\"x\")\n    }";

#[test]
fn a_cfg_test_only_impl_is_neither_a_caller_nor_an_impl() -> Result<(), String> {
    // One production `fmt` caller; a `#[cfg(test)]` impl that also calls the
    // owner must not make the name look ambiguous.
    let impls = format!(
        "{}#[cfg(test)]\nimpl fmt::Display for B {{\n    {CALLING_B_FMT}\n}}\n",
        display_impl("A")
    );
    let finding = pad_finding_with("pad-cfg-test-impl", &pad_lib(&impls), OTHER_TEST)?;
    assert_bare_no_static_path(&finding, "cfg-test-impl");
    assert_unlimited(&finding, "cfg-test-impl");
    Ok(())
}

#[test]
fn inherent_fmt_methods_are_not_trait_impls_and_do_not_count() -> Result<(), String> {
    // Two inherent `fmt` methods calling the owner: no trait dispatch.
    let inherent = |ty: &str| {
        format!("impl {ty} {{\n    pub fn fmt(&self) -> bool {{\n        pad(1, 2)\n    }}\n}}\n")
    };
    let both = format!("{}{}", inherent("A"), inherent("B"));
    let finding = pad_finding("pad-inherent-both", &pad_lib(&both))?;
    assert_bare_no_static_path(&finding, "inherent-both");
    assert_unlimited(&finding, "inherent-both");
    // One trait `fmt` caller plus an inherent `fmt` is still one trait impl.
    let mixed = format!("{}{}", display_impl("A"), inherent("B"));
    let finding = pad_finding("pad-inherent-mixed", &pad_lib(&mixed))?;
    assert_unlimited(&finding, "inherent-mixed");
    Ok(())
}

#[test]
fn a_self_qualified_call_from_the_single_caller_behaves_as_the_plain_call() -> Result<(), String> {
    // `Self::pad(..)` normalises to the callee name `pad`, so the single
    // calling impl among many still names the ambiguity.
    let impls = format!(
        "impl fmt::Display for A {{\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{\n        let _ = Self::pad(1, 2);\n        f.write_str(\"x\")\n    }}\n}}\n{}",
        "impl fmt::Display for B {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        f.write_str(\"b\")\n    }\n}\n"
    );
    let finding = pad_finding_with("pad-self-call", &pad_lib(&impls), OTHER_TEST)?;
    assert_bare_no_static_path(&finding, "self-call");
    let plain = pad_finding_with(
        "pad-plain-call",
        &pad_lib(&format!(
            "{}impl fmt::Display for B {{\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {{\n        f.write_str(\"b\")\n    }}\n}}\n",
            display_impl("A")
        )),
        OTHER_TEST,
    )?;
    assert_eq!(finding.static_limit_kind, plain.static_limit_kind);
    assert_eq!(finding.evidence, plain.evidence);
    Ok(())
}

// #11481 follow-up decision (fail-closed, no extension): `owner_named_test`
// is NOT admitted for `weakly_exposed`. That class only exists because a
// related test carries a credited weak assertion; an owner-named test is the
// strongest "this test is about the owner" signal short of a call, so its weak
// assertion is a real gap. The witness walk is repo-wide and name-only and is
// not tied to that test, so naming "reach unresolved" and dropping the gap
// lines could hide a weak assertion the test really holds. The pair below
// differs only in the test's name, so the relation alone decides.
fn weak_assertion_fixture(test_name: &str) -> Result<Finding, String> {
    assert!(
        MACRO_ROUTED.contains("fn outer_triples() {\n        assert_eq!(outer(4), 12);"),
        "fixture template drifted"
    );
    let lib = format!(
        "{}}}\n",
        MACRO_ROUTED.replace(
            "fn outer_triples() {\n        assert_eq!(outer(4), 12);",
            &format!("fn {test_name}() {{\n        assert!(outer(4) > 0);")
        )
    );
    assert!(lib.contains(&format!("fn {test_name}()")), "{lib}");
    Fixture::new(test_name, &lib)?.rate_finding()
}

#[test]
fn a_weakly_exposed_owner_named_weak_assertion_keeps_its_gap_while_proximity_is_limited()
-> Result<(), String> {
    let named = weak_assertion_fixture("inner_rate_is_positive")?;
    // Fixture construction: one owner-named row holding a real weak oracle.
    assert_eq!(named.class, ExposureClass::WeaklyExposed);
    assert_eq!(named.ripr.reach.state, StageState::Weak);
    assert!(all_relations(&named, |reason| {
        reason == RelationReason::OwnerNamedTest
    }));
    assert!(
        named.related_tests.iter().any(|test| test.oracle.is_some()
            && test.oracle_strength == crate::domain::OracleStrength::Weak),
        "{:?}",
        named.related_tests
    );
    assert!(!super::needs_proximity_reach_limit(&named));
    assert_unlimited(&named, "owner-named weak assertion");

    // Discriminating alternate: the same shape without the owner's name in the
    // test is proximity-only and still names the macro limit.
    let proximity = weak_assertion_fixture("outer_is_positive")?;
    assert_eq!(proximity.class, ExposureClass::WeaklyExposed);
    assert!(
        all_relations(&proximity, crate::analysis::classify::is_proximity_only),
        "{:?}",
        proximity.related_tests
    );
    assert_eq!(
        proximity.static_limit_kind,
        Some(StaticLimitKind::RustMacroReachUnresolved),
        "{:?}",
        proximity.evidence
    );
    Ok(())
}
