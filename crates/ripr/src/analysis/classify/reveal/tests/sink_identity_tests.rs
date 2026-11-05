use super::*;

#[test]
fn sink_identity_proximity_cannot_supply_yes_but_direct_and_helper_can() {
    let probe = owned_probe(
        ProbeFamily::Predicate,
        "current.status == recorded.status",
        "decision",
    );
    let exact = test_with_assertions(
        "other_status_is_terminal",
        vec![oracle(
            "assert_eq!(record.status, \"terminal\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )],
    );
    for reason in [RelationReason::SameTestFile, RelationReason::SameModule] {
        let (_, discriminate, _) = reveal_evidence(&probe, &[(&exact, reason)]);
        assert_eq!(
            discriminate.state,
            StageState::Weak,
            "{reason:?}: {discriminate:?}"
        );
        assert!(discriminate.summary.contains("identity_unresolved"));
    }
    for reason in [
        RelationReason::DirectOwnerCall,
        RelationReason::HelperOwnerCall,
    ] {
        let (_, unrelated, _) = reveal_evidence(&probe, &[(&exact, reason)]);
        assert_eq!(unrelated.state, StageState::Weak);
        let bound = test_with_assertions(
            "owner_result",
            vec![oracle(
                "assert!(decision());",
                OracleKind::ExactValue,
                OracleStrength::Strong,
            )],
        );
        let (_, discriminate, _) = reveal_evidence(&probe, &[(&bound, reason)]);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "{reason:?}: {discriminate:?}"
        );
    }
}

#[test]
fn sink_identity_equal_strength_bound_oracle_wins_in_either_order() {
    let probe = owned_probe(ProbeFamily::Predicate, "status == expected", "decision");
    let exact = test_with_assertions(
        "status_observation",
        vec![oracle(
            "assert_eq!(status, expected);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )],
    );
    let bound = test_with_assertions(
        "owner_result",
        vec![oracle(
            "assert!(decision());",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )],
    );
    for related in [
        vec![
            (&exact, RelationReason::SameTestFile),
            (&bound, RelationReason::DirectOwnerCall),
        ],
        vec![
            (&bound, RelationReason::DirectOwnerCall),
            (&exact, RelationReason::SameTestFile),
        ],
    ] {
        let (_, discriminate, _) = reveal_evidence_proximity_reach(&probe, &related, &|_| true);
        assert_eq!(discriminate.state, StageState::Yes, "{discriminate:?}");
    }
}

#[test]
fn sink_identity_return_delta_rejects_shared_tokens_and_retains_changed_and_no_before_controls() {
    let mut probe = owned_probe(
        ProbeFamily::ReturnValue,
        "\"verifies catalog consistency\"",
        "description",
    );
    probe.before = Some("\"checks catalog consistency\"".to_string());
    let shared = test_with_assertions(
        "catalog_shape",
        vec![oracle(
            "assert_eq!(catalog.len(), 3);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )],
    );
    let exact = test_with_assertions(
        "description_text",
        vec![oracle(
            "assert_eq!(description(), \"verifies catalog consistency\");",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )],
    );
    let (_, discriminate, _) =
        reveal_evidence(&probe, &[(&shared, RelationReason::DirectOwnerCall)]);
    assert_eq!(
        discriminate.state,
        StageState::Weak,
        "shared: {discriminate:?}"
    );
    let (_, discriminate, _) =
        reveal_evidence(&probe, &[(&exact, RelationReason::DirectOwnerCall)]);
    assert_eq!(
        discriminate.state,
        StageState::Yes,
        "changed: {discriminate:?}"
    );
    probe.before = None;
    let (_, discriminate, _) =
        reveal_evidence(&probe, &[(&shared, RelationReason::DirectOwnerCall)]);
    assert_eq!(
        discriminate.state,
        StageState::Weak,
        "unrelated no-before: {discriminate:?}"
    );
    let (_, observed_no_before, _) =
        reveal_evidence(&probe, &[(&exact, RelationReason::DirectOwnerCall)]);
    assert_eq!(observed_no_before.state, StageState::Yes);
}

#[test]
fn sink_identity_return_delta_cannot_borrow_a_weaker_oracles_changed_token() {
    let mut probe = owned_probe(
        ProbeFamily::ReturnValue,
        "\"verifies catalog consistency\"",
        "description",
    );
    probe.before = Some("\"checks catalog consistency\"".to_string());
    let shared = test_with_assertions(
        "catalog_shape",
        vec![oracle(
            "assert_eq!(catalog.len(), 3);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )],
    );
    let weak_changed = test_with_assertions(
        "description_mentions_change",
        vec![oracle(
            "assert!(description.contains(\"verifies\"));",
            OracleKind::RelationalCheck,
            OracleStrength::Weak,
        )],
    );
    for related in [
        vec![
            (&shared, RelationReason::DirectOwnerCall),
            (&weak_changed, RelationReason::DirectOwnerCall),
        ],
        vec![
            (&weak_changed, RelationReason::DirectOwnerCall),
            (&shared, RelationReason::DirectOwnerCall),
        ],
    ] {
        let (_, discriminate, _) = reveal_evidence(&probe, &related);
        assert_eq!(discriminate.state, StageState::Weak, "{discriminate:?}");
    }
}

#[test]
fn sink_identity_real_parser_discarded_return_cannot_lend_identity_to_another_assertion()
-> Result<(), String> {
    use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    let mut probe = owned_probe(
        ProbeFamily::ReturnValue,
        "\"verifies catalog\".to_string()",
        "description",
    );
    probe.before = Some("\"checks catalog\".to_string()".into());
    for (assertions, expected) in [
        ("assert_eq!(other, \"verifies catalog\");", StageState::Weak),
        (
            "assert_eq!(description(), \"verifies catalog\");",
            StageState::Yes,
        ),
        (
            "assert_eq!(other, \"verifies catalog\"); assert_eq!(description(), \"verifies catalog\");",
            StageState::Yes,
        ),
        (
            "assert_eq!(description(), \"verifies catalog\"); assert_eq!(other, \"verifies catalog\");",
            StageState::Yes,
        ),
        // This parser-only harness has no indexed return pin. The workspace
        // integration control separately requires this genuine binding exposed.
        (
            "let result = description(); assert_eq!(result, \"verifies catalog\");",
            StageState::Weak,
        ),
    ] {
        let source = format!(
            "pub fn description() -> String {{ \"verifies catalog\".into() }}\n#[test] fn checks_text() {{ let _ = description(); let other = \"verifies catalog\"; {assertions} }}"
        );
        let facts = RaRustSyntaxAdapter.summarize_file(&PathBuf::from("src/lib.rs"), &source)?;
        assert!(!facts.used_lexical_fallback);
        let test = facts.tests.first().ok_or("parsed test missing")?;
        assert!(!test.assertions.is_empty());
        assert!(test.calls.iter().any(|call| call.name == "description"));
        let (_, discriminate, _) =
            reveal_evidence(&probe, &[(test, RelationReason::DirectOwnerCall)]);
        assert_eq!(
            discriminate.state, expected,
            "{assertions}: {discriminate:?}"
        );
    }
    Ok(())
}
