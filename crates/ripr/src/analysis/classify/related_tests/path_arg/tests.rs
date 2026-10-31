//! End-to-end proofs for the `Type::method` path-value relation (#11199).
//!
//! Each case parses real source, seeds the real predicate probe and runs the
//! production classifier, so the relation read here is the one a user's
//! `ripr check` reports.

use super::super::RelatedTestCandidateIndex;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustIndex, RustSyntaxAdapter};
use crate::domain::{ExposureClass, Finding, ProbeFamily, RelationReason};
use std::path::{Path, PathBuf};

const OWNER: &str = "pub struct Pricer;\n\nimpl Pricer {\n    pub fn discount(&self, amount: u32) -> u32 {\n        if amount >= 100 { amount - 10 } else { amount }\n    }\n}\n";

/// A crate whose production type is `Pricer`, with `extra` after the owner,
/// then a `#[cfg(test)]` module holding `helper` and one test named `cases`
/// whose body is `body`. The changed line is the `amount >= 100` predicate.
fn crate_source(extra: &str, helper: &str, body: &str) -> String {
    format!(
        "{OWNER}\n{extra}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{helper}\n    #[test]\n    fn cases() {{\n        {body}\n    }}\n}}\n"
    )
}

const GOOD_HELPER: &str = "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        assert_eq!(f(&Pricer, amount), want);\n    }\n";
const GOOD_BODY: &str = "case(Pricer::discount, 150, 140);";
const OTHER_TYPE: &str = "pub struct Other;\nimpl Other {\n    pub fn discount(&self, amount: u32) -> u32 { amount }\n}\n";

fn finding(source: &str) -> Result<Finding, String> {
    let path = PathBuf::from("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    let mut index = RustIndex::default();
    index.insert_file(path.clone(), facts, true);
    let line = source
        .lines()
        .position(|text| text.contains("amount >= 100"))
        .ok_or("setup: predicate line missing")?
        + 1;
    let changed = ChangedFile {
        path,
        added_lines: vec![ChangedLine {
            line,
            text: source.lines().nth(line - 1).unwrap_or_default().to_string(),
            new_side_line: line,
        }],
        removed_lines: Vec::new(),
    };
    let seeded = crate::analysis::probes::probes_for_file_with_relations(
        Path::new("."),
        &changed,
        &index,
        &[],
    );
    let subject = seeded
        .iter()
        .find(|subject| subject.probe.family == ProbeFamily::Predicate)
        .ok_or("setup: no predicate probe")?;
    Ok(
        crate::analysis::classifier::classify_probe_with_candidate_index(
            &subject.probe,
            &index,
            true,
            None,
            &RelatedTestCandidateIndex::new(&index),
            &crate::analysis::classify::transitive_reach::TransitiveReachIndex::new(&index),
            None,
        ),
    )
}

/// The relation the `cases` test received, `None` when it is not related.
fn relation(source: &str) -> Result<Option<RelationReason>, String> {
    let finding = finding(source)?;
    Ok(finding
        .related_tests
        .iter()
        .find(|related| related.name == "cases")
        .and_then(|related| related.relation_reason))
}

fn assert_relation(source: &str, expected: Option<RelationReason>) -> Result<(), String> {
    let got = relation(source)?;
    if got == expected {
        Ok(())
    } else {
        Err(format!("expected {expected:?}, got {got:?} for\n{source}"))
    }
}

fn assert_helper_reach(source: &str) -> Result<(), String> {
    assert_relation(source, Some(RelationReason::HelperOwnerCall))
}

/// Anything but a credited call: the weaker same-file proximity the test
/// already had, or no relation at all.
fn assert_no_path_credit(source: &str) -> Result<(), String> {
    match relation(source)? {
        Some(RelationReason::HelperOwnerCall | RelationReason::DirectOwnerCall) => {
            Err(format!("path credited unexpectedly for\n{source}"))
        }
        _ => Ok(()),
    }
}

#[test]
fn given_path_handed_to_helper_that_runs_it_then_helper_reach() -> Result<(), String> {
    assert_helper_reach(&crate_source("", GOOD_HELPER, GOOD_BODY))?;
    // Reached the same way when the helper binds the result first, takes the
    // function at a later position, or is generic over `Fn`.
    let let_helper = "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        let got = f(&Pricer, amount);\n        assert_eq!(got, want);\n    }\n";
    assert_helper_reach(&crate_source("", let_helper, GOOD_BODY))?;
    let later = "    fn case(amount: u32, f: impl Fn(&Pricer, u32) -> u32, want: u32) {\n        assert!(f(&Pricer, amount) == want);\n    }\n";
    assert_helper_reach(&crate_source(
        "",
        later,
        "case(150, Pricer::discount, 140);",
    ))?;
    let generic = "    fn case<F: Fn(&Pricer, u32) -> u32>(f: F, amount: u32, want: u32) {\n        assert_eq!(f(&Pricer, amount), want);\n    }\n";
    assert_helper_reach(&crate_source("", generic, GOOD_BODY))
}

#[test]
fn given_path_reach_without_an_observing_oracle_then_never_exposed() -> Result<(), String> {
    // The helper runs the owner but no oracle the test is credited with
    // observes it: the relation grants reach only, never an oracle.
    let finding = finding(&crate_source("", GOOD_HELPER, GOOD_BODY))?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn given_a_differently_typed_same_named_method_then_the_path_still_resolves() -> Result<(), String>
{
    // Alternate control: another type's `discount` is not the target of
    // `Pricer::discount`, so it neither blocks nor supplies the credit.
    assert_helper_reach(&crate_source(OTHER_TYPE, GOOD_HELPER, GOOD_BODY))
}

#[test]
fn given_another_type_path_then_no_credit() -> Result<(), String> {
    let helper = "    fn case(f: fn(&Other, u32) -> u32, amount: u32, want: u32) {\n        assert_eq!(f(&Other, amount), want);\n    }\n";
    assert_no_path_credit(&crate_source(
        OTHER_TYPE,
        helper,
        "case(Other::discount, 150, 150);",
    ))?;
    // An unresolvable type head is not the owner's type either.
    assert_no_path_credit(&crate_source(
        "",
        GOOD_HELPER,
        "case(Missing::discount, 150, 140);",
    ))?;
    assert_no_path_credit(&crate_source(
        "",
        GOOD_HELPER,
        "case(other::Pricer::discount, 150, 140);",
    ))
}

#[test]
fn given_ambiguous_type_path_then_no_credit() -> Result<(), String> {
    // Two inherent impls of one generic type: `Pricer::discount` could be
    // either, so neither is credited.
    let source = "pub struct Pricer<T>(T);\n\nimpl Pricer<u8> {\n    pub fn discount(&self, amount: u32) -> u32 {\n        if amount >= 100 { amount - 10 } else { amount }\n    }\n}\n\nimpl Pricer<u16> {\n    pub fn discount(&self, amount: u32) -> u32 { amount }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    fn case(f: fn(&Pricer<u8>, u32) -> u32, amount: u32, want: u32) {\n        assert_eq!(f(&Pricer(1u8), amount), want);\n    }\n    #[test]\n    fn cases() {\n        case(Pricer::discount, 150, 140);\n    }\n}\n";
    assert_no_path_credit(source)
}

#[test]
fn given_trait_method_owner_then_no_credit() -> Result<(), String> {
    let source = "pub struct Pricer;\npub trait Priced {\n    fn discount(&self, amount: u32) -> u32;\n}\n\nimpl Priced for Pricer {\n    fn discount(&self, amount: u32) -> u32 {\n        if amount >= 100 { amount - 10 } else { amount }\n    }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        assert_eq!(f(&Pricer, amount), want);\n    }\n    #[test]\n    fn cases() {\n        case(Pricer::discount, 150, 140);\n    }\n}\n";
    assert_no_path_credit(source)
}

#[test]
fn given_helper_that_does_not_run_the_argument_then_no_credit() -> Result<(), String> {
    for helper in [
        // Stored, never invoked.
        "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        let _kept = f;\n        assert_eq!(amount, want);\n    }\n",
        // Only forwarded to another function.
        "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        forward(f);\n        assert_eq!(amount, want);\n    }\n    fn forward(_f: fn(&Pricer, u32) -> u32) {}\n",
        // Invoked only under a branch.
        "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        if amount > 1000 {\n            assert_eq!(f(&Pricer, amount), want);\n        }\n    }\n",
        // Invoked only in a closure that is never called.
        "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        let _later = || f(&Pricer, amount);\n        assert_eq!(amount, want);\n    }\n",
        // Invoked on the short-circuited side of `&&`.
        "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        assert!(amount > 1000 && f(&Pricer, amount) == want);\n    }\n",
        // Invoked only under a loop that may not run.
        "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        for _ in 0..0 {\n            assert_eq!(f(&Pricer, amount), want);\n        }\n    }\n",
        // The parameter is shadowed by a local of the same name.
        "    fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        let f = |_: &Pricer, a: u32| a;\n        assert_eq!(f(&Pricer, amount), want);\n    }\n",
        // The parameter that is invoked is not the one handed the path.
        "    fn case(f: fn(&Pricer, u32) -> u32, other: fn(&Pricer, u32) -> u32, want: u32) {\n        assert_eq!(other(&Pricer, 1), want);\n        let _ = f;\n    }\n",
        // An `async fn` body does not run when called.
        "    async fn case(f: fn(&Pricer, u32) -> u32, amount: u32, want: u32) {\n        assert_eq!(f(&Pricer, amount), want);\n    }\n",
    ] {
        assert_no_path_credit(&crate_source("", helper, GOOD_BODY))?;
    }
    Ok(())
}

#[test]
fn given_callee_the_test_cannot_pin_then_no_credit() -> Result<(), String> {
    // A qualified callee, a test-local rebinding or nested definition of the
    // helper name, a borrowed or turbofished path and a second same-named
    // helper all fail to name one same-file free function and one exact path.
    for body in [
        "support::case(Pricer::discount, 150, 140);",
        "let case = |_f: fn(&Pricer, u32) -> u32, _a: u32, _w: u32| {}; case(Pricer::discount, 150, 140);",
        "fn case(_f: fn(&Pricer, u32) -> u32, _a: u32, _w: u32) {} case(Pricer::discount, 150, 140);",
        "case(&Pricer::discount, 150, 140);",
        "case(Pricer::discount::<u8>, 150, 140);",
    ] {
        assert_no_path_credit(&crate_source("", GOOD_HELPER, body))?;
    }
    let twin = "mod elsewhere {\n    pub fn case(_f: fn(&super::Pricer, u32) -> u32, _a: u32, _w: u32) {}\n}\n";
    assert_no_path_credit(&crate_source(twin, GOOD_HELPER, GOOD_BODY))
}

#[test]
fn given_type_the_test_file_rebinds_then_no_credit() -> Result<(), String> {
    assert_no_path_credit(&crate_source(
        "",
        GOOD_HELPER,
        "use foreign::Pricer; case(Pricer::discount, 150, 140);",
    ))?;
    let rebinding = "mod foreign {\n    pub struct Pricer;\n}\n";
    let imported = crate_source(rebinding, GOOD_HELPER, GOOD_BODY).replace(
        "    use super::*;\n",
        "    use super::*;\n    use crate::foreign::Pricer;\n",
    );
    assert_no_path_credit(&imported)?;
    let aliased = crate_source("", GOOD_HELPER, GOOD_BODY).replace(
        "    use super::*;\n",
        "    use super::*;\n    type Pricer = u8;\n",
    );
    assert_no_path_credit(&aliased)
}

#[test]
fn given_a_file_that_rebinds_assert_macros_then_no_credit() -> Result<(), String> {
    let shadow = "macro_rules! assert_eq {\n    ($a:expr, $b:expr) => {};\n}\n";
    assert_no_path_credit(&crate_source(shadow, GOOD_HELPER, GOOD_BODY))
}

#[test]
fn given_a_direct_call_too_then_the_call_relation_is_unchanged() -> Result<(), String> {
    // The path credit never outranks or replaces what a real call earns: the
    // relation with the extra path argument equals the relation without it.
    let call = "let priced: Pricer = Pricer; assert_eq!(priced.discount(150), 140);";
    let with_path = format!("case(Pricer::discount, 150, 140); {call}");
    let without_path = relation(&crate_source("", GOOD_HELPER, call))?;
    assert_ne!(without_path, Some(RelationReason::HelperOwnerCall));
    assert_eq!(
        relation(&crate_source("", GOOD_HELPER, &with_path))?,
        without_path
    );
    Ok(())
}
