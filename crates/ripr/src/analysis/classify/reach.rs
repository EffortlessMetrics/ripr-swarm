use super::super::rust_index::{FunctionSummary, TestSummary};
use crate::domain::{Confidence, RelationReason, StageEvidence, StageState};

pub(in crate::analysis) fn reach_evidence(
    related_tests: &[(&TestSummary, RelationReason)],
    owner_fn: Option<&FunctionSummary>,
    owner_has_production_caller: bool,
) -> StageEvidence {
    if related_tests.is_empty() {
        return StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            "No static test path found for the changed owner",
        );
    }
    // #3714 round-2 review (devin hDRL2): a `SeamCalleeCall` relation means
    // the test exercises the seam's converted callee — it never invokes the
    // changed owner or its conversion, so the reach summary must not claim
    // owner reach for it. Owner-anchored relations keep the established
    // phrasing; callee-only relations carry their own honest summary (the
    // exposure class stays `weakly_exposed` — the conversion's variant
    // binding remains the typed `wrapper_error_binding_unresolved`
    // limitation per #3700).
    let target = owner_fn.map(|f| f.name.as_str()).unwrap_or("changed owner");
    let owner_anchored: Vec<&TestSummary> = related_tests
        .iter()
        .filter(|(_, reason)| *reason != RelationReason::SeamCalleeCall)
        .map(|(test, _)| *test)
        .collect();
    let callee_only: Vec<&TestSummary> = related_tests
        .iter()
        .filter(|(_, reason)| *reason == RelationReason::SeamCalleeCall)
        .map(|(test, _)| *test)
        .collect();
    // A test that only shares the changed file or a name token with the
    // owner is a suggested location, not evidence that it runs the owner.
    // Treating it as reach let an uncalled function inherit a neighbour's
    // strong assertion and report `exposed`, and treating it as weak reach
    // reported `weakly_exposed` for a function no test calls. Reach is `No`
    // only when nothing else could be calling the owner unseen: it has no
    // production caller (a test may reach it through an unresolved caller
    // chain) and no proximity test invokes a non-assertion macro (whose
    // expansion may call it).
    let proximity_only = !owner_anchored.is_empty()
        && related_tests
            .iter()
            .filter(|(_, reason)| *reason != RelationReason::SeamCalleeCall)
            .all(|(_, reason)| is_proximity_only(*reason));
    if proximity_only {
        let names = owner_anchored
            .iter()
            .take(3)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if owner_has_production_caller
            || owner_anchored
                .iter()
                .any(|test| invokes_opaque_macro(&test.body))
        {
            return StageEvidence::new(
                StageState::Weak,
                Confidence::Low,
                format!(
                    "No test is seen calling {target}; tests share only its file or a name token: {names}"
                ),
            );
        }
        return StageEvidence::new(
            StageState::No,
            Confidence::Medium,
            format!(
                "No test is seen calling {target}; tests share only its file or a name token: {names}"
            ),
        );
    }
    let summary = if owner_anchored.is_empty() {
        let names = callee_only
            .iter()
            .take(3)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Related tests exercise the wrapper seam's converted callee (the changed owner is not invoked by them): {names}"
        )
    } else {
        let names = owner_anchored
            .iter()
            .take(3)
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        format!("Related tests appear to reach {target}: {names}")
    };
    StageEvidence::new(StageState::Yes, Confidence::Medium, summary)
}

/// Macros whose expansion never calls a user function by itself; any other
/// macro invocation in a test body may hide a call to the changed owner.
const TRANSPARENT_TEST_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "assert_matches",
    "matches",
    "vec",
    "format",
    "print",
    "println",
    "eprint",
    "eprintln",
    "write",
    "writeln",
    "panic",
    "todo",
    "unimplemented",
    "unreachable",
    "dbg",
    "concat",
    "stringify",
    "include_str",
    "env",
];

/// True when `body` invokes a macro (`name!(`, `name![`, `name! {`) outside
/// [`TRANSPARENT_TEST_MACROS`]. Text inside string literals counts too, which
/// only keeps reach uncertain.
fn invokes_opaque_macro(body: &str) -> bool {
    let bytes = body.as_bytes();
    bytes.iter().enumerate().any(|(bang, byte)| {
        if *byte != b'!' {
            return false;
        }
        let opener = bytes[bang + 1..]
            .iter()
            .find(|next| !next.is_ascii_whitespace());
        if !matches!(opener, Some(b'(' | b'[' | b'{')) {
            return false;
        }
        let start = body[..bang]
            .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .map_or(0, |index| index + 1);
        let name = &body[start..bang];
        !name.is_empty() && !TRANSPARENT_TEST_MACROS.contains(&name)
    })
}

/// Relations that come from file or name proximity alone, with no captured
/// call, helper chain, or assertion affinity tying the test to the owner.
fn is_proximity_only(reason: RelationReason) -> bool {
    matches!(
        reason,
        RelationReason::SameTestFile
            | RelationReason::SameModule
            | RelationReason::WeakTokenSubstring
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::facts::FunctionSourceRole;
    use crate::domain::SymbolId;
    use std::path::PathBuf;

    #[test]
    fn given_no_related_tests_when_building_reach_evidence_then_stage_is_no() {
        let evidence = reach_evidence(&[], None, false);

        assert_eq!(evidence.state, StageState::No);
        assert_eq!(evidence.confidence, Confidence::Medium);
        assert_eq!(
            evidence.summary,
            "No static test path found for the changed owner"
        );
    }

    #[test]
    fn given_related_tests_when_building_reach_evidence_then_names_owner_and_tests() {
        let owner = function("discounted_total");
        let first = test("below_threshold");
        let second = test("at_threshold");
        let third = test("above_threshold");
        let fourth = test("large_amount");
        let related = vec![
            (&first, RelationReason::DirectOwnerCall),
            (&second, RelationReason::DirectOwnerCall),
            (&third, RelationReason::DirectOwnerCall),
            (&fourth, RelationReason::DirectOwnerCall),
        ];

        let evidence = reach_evidence(&related, Some(&owner), false);

        assert_eq!(evidence.state, StageState::Yes);
        assert_eq!(evidence.confidence, Confidence::Medium);
        assert_eq!(
            evidence.summary,
            "Related tests appear to reach discounted_total: below_threshold, at_threshold, above_threshold"
        );
    }

    #[test]
    fn given_only_proximity_relations_when_building_reach_evidence_then_reach_is_no() {
        let owner = function("untested_rounding");
        let neighbour = test("discount_applies_at_100");
        let token_match = test("rounding_table_loads");
        let module_peer = test("module_smoke");
        let related = vec![
            (&neighbour, RelationReason::SameTestFile),
            (&token_match, RelationReason::WeakTokenSubstring),
            (&module_peer, RelationReason::SameModule),
        ];

        let evidence = reach_evidence(&related, Some(&owner), false);

        assert_eq!(evidence.state, StageState::No);
        assert_eq!(evidence.confidence, Confidence::Medium);
        assert_eq!(
            evidence.summary,
            "No test is seen calling untested_rounding; tests share only its file or a name token: discount_applies_at_100, rounding_table_loads, module_smoke"
        );
    }

    #[test]
    fn given_proximity_test_invoking_a_custom_macro_when_building_reach_evidence_then_reach_stays_weak()
     {
        let owner = function("tax_total");
        let mut macro_caller = test("vat_boundary_is_checked_by_macro");
        macro_caller.body = "assert_eq!(macro_tax_case!(100), 120);".to_string();
        let related = vec![(&macro_caller, RelationReason::WeakTokenSubstring)];

        let evidence = reach_evidence(&related, Some(&owner), false);

        assert_eq!(evidence.state, StageState::Weak);
        assert_eq!(evidence.confidence, Confidence::Low);
    }

    #[test]
    fn given_only_proximity_relations_for_an_owner_with_a_production_caller_then_reach_stays_weak()
    {
        let owner = function("check_score_invariants");
        let neighbour = test("validate_score_accepts_in_range_values");
        let related = vec![(&neighbour, RelationReason::WeakTokenSubstring)];

        let evidence = reach_evidence(&related, Some(&owner), true);

        assert_eq!(evidence.state, StageState::Weak);
        assert_eq!(evidence.confidence, Confidence::Low);
    }

    #[test]
    fn opaque_macro_detection_ignores_assertion_macros_and_operators() {
        assert!(!invokes_opaque_macro(
            "assert_eq!(tax_bps(\"EU\"), 2000); assert!(a != b); let v = vec![1];"
        ));
        assert!(!invokes_opaque_macro("assert!(!flag);"));
        assert!(invokes_opaque_macro("assert_eq!(case!(1), 2);"));
        assert!(invokes_opaque_macro("proptest! { }"));
        assert!(invokes_opaque_macro("check_all![1, 2];"));
    }

    #[test]
    fn given_one_calling_test_among_proximity_relations_when_building_reach_evidence_then_reach_is_yes()
     {
        let owner = function("discount");
        let neighbour = test("tags_nonempty");
        let caller = test("discount_applies_at_100");
        let related = vec![
            (&neighbour, RelationReason::SameTestFile),
            (&caller, RelationReason::DirectOwnerCall),
        ];

        let evidence = reach_evidence(&related, Some(&owner), false);

        assert_eq!(evidence.state, StageState::Yes);
    }

    // #3714 round-2 review (devin hDRL2): callee-only relations must not
    // claim owner reach in the summary.
    #[test]
    fn given_callee_only_relations_when_building_reach_evidence_then_summary_names_callee_affinity()
    {
        let owner = function("parse_summary");
        let first = test("observes_callee_outcome");
        let second = test("other_callee_probe");
        let related = vec![
            (&first, RelationReason::SeamCalleeCall),
            (&second, RelationReason::SeamCalleeCall),
        ];

        let evidence = reach_evidence(&related, Some(&owner), false);

        assert_eq!(evidence.state, StageState::Yes);
        assert_eq!(
            evidence.summary,
            "Related tests exercise the wrapper seam's converted callee (the changed owner is not invoked by them): observes_callee_outcome, other_callee_probe"
        );
    }

    // #3714 round-2 review (devin hDRL2): mixed relations keep the
    // established owner-reach phrasing for the owner-anchored tests.
    #[test]
    fn given_mixed_relations_when_building_reach_evidence_then_owner_reach_is_named() {
        let owner = function("parse_summary");
        let callee_only = test("observes_callee_outcome");
        let anchored = test("parse_summary_fails_closed");
        let related = vec![
            (&callee_only, RelationReason::SeamCalleeCall),
            (&anchored, RelationReason::OwnerNamedTest),
        ];

        let evidence = reach_evidence(&related, Some(&owner), false);

        assert_eq!(
            evidence.summary,
            "Related tests appear to reach parse_summary: parse_summary_fails_closed"
        );
    }

    fn function(name: &str) -> FunctionSummary {
        FunctionSummary {
            id: SymbolId(format!("src/lib.rs::{name}")),
            name: name.to_string(),
            file: PathBuf::from("src/lib.rs"),
            start_line: 1,
            end_line: 3,
            body: String::new(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            source_role: FunctionSourceRole::Production,
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }

    fn test(name: &str) -> TestSummary {
        TestSummary {
            name: name.to_string(),
            file: PathBuf::from("tests/pricing.rs"),
            start_line: 1,
            end_line: 3,
            body: String::new(),
            calls: Vec::new(),
            assertions: Vec::new(),
            literals: Vec::new(),
            attrs: Vec::new(),
            nested_fn_names: Vec::new(),
            let_bindings: Vec::new(),
        }
    }
}
