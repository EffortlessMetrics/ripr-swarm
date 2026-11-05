use super::*;

#[test]
fn sink_identity_proximity_cannot_supply_yes_but_direct_and_helper_can() {
    let probe = probe(ProbeFamily::Predicate, "current.status == recorded.status");
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
        let (_, discriminate, _) = reveal_evidence(&probe, &[(&exact, reason)]);
        assert_eq!(
            discriminate.state,
            StageState::Yes,
            "{reason:?}: {discriminate:?}"
        );
    }
}

#[test]
fn sink_identity_equal_strength_bound_oracle_wins_in_either_order() {
    let probe = probe(ProbeFamily::Predicate, "status == expected");
    let exact = test_with_assertions(
        "status_observation",
        vec![oracle(
            "assert_eq!(status, expected);",
            OracleKind::ExactValue,
            OracleStrength::Strong,
        )],
    );
    for related in [
        vec![
            (&exact, RelationReason::SameTestFile),
            (&exact, RelationReason::DirectOwnerCall),
        ],
        vec![
            (&exact, RelationReason::DirectOwnerCall),
            (&exact, RelationReason::SameTestFile),
        ],
    ] {
        let (_, discriminate, _) = reveal_evidence_proximity_reach(&probe, &related, &|_| true);
        assert_eq!(discriminate.state, StageState::Yes, "{discriminate:?}");
    }
}

#[test]
fn sink_identity_return_delta_rejects_shared_tokens_and_retains_changed_and_no_before_controls() {
    let mut probe = probe(ProbeFamily::ReturnValue, "\"verifies catalog consistency\"");
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
            "assert_eq!(description, \"verifies catalog consistency\");",
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
        StageState::Yes,
        "no-before: {discriminate:?}"
    );
}

#[test]
fn sink_identity_return_delta_cannot_borrow_a_weaker_oracles_changed_token() {
    let mut probe = probe(ProbeFamily::ReturnValue, "\"verifies catalog consistency\"");
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
