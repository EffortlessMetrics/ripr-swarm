use super::*;

fn provenance() -> Provenance {
    Provenance {
        producer: "trusted-controller".into(),
        reference: "run/17/1".into(),
        digest: "a".repeat(64),
    }
}

fn object(path: &str, digit: &str) -> InputObject {
    InputObject {
        path: path.into(),
        object: digit.repeat(40),
        mode: "100644".into(),
    }
}

fn binding() -> Binding {
    Binding {
        repository: "EffortlessMetrics/ripr-swarm".into(),
        request_id: "17-1".into(),
        head: "1".repeat(40),
        base: "2".repeat(40),
        integration: "3".repeat(40),
        ordered_parents: vec!["2".repeat(40), "1".repeat(40)],
        tree: "4".repeat(40),
        configuration: Configuration {
            toolchain: "rustc 1.95.0 (59807616e 2026-04-14)".into(),
            platform: "x86_64-unknown-linux-gnu".into(),
            features: BTreeSet::new(),
            default_features: true,
            cargo_digest: "5".repeat(64),
            configuration_digest: "6".repeat(64),
            controller_generation: "controller/blob".into(),
            plan_generation: PLAN_GENERATION.into(),
            result_generation: "result-v1".into(),
        },
        inventory: vec![ChangedInput {
            before: Some(object("src/lib.rs", "7")),
            after: Some(object("src/lib.rs", "8")),
        }],
        inventory_provenance: provenance(),
        evidence_inventory: provenance(),
    }
}

fn observation(binding: &Binding) -> Observation {
    Observation::Complete {
        binding: Box::new(binding.clone()),
        policy: PolicyTransition::Unchanged,
    }
}

fn passed(plan: &AdmissionPlan) -> Vec<ActualRow> {
    plan.allocations()
        .iter()
        .map(|row| ActualRow {
            binding: row.binding.clone(),
            definition: row.definition.clone(),
            outcome: Outcome::Passed,
            evidence_kind: EvidenceKind::ExecutedGateResult,
            provenance: provenance(),
        })
        .collect()
}

#[test]
fn explicit_admission_conserves_the_canonical_portfolio() {
    let input = binding();
    let plan = AdmissionPlan::admit(Some(&input), &observation(&input));
    assert_eq!(plan.rows.len(), 13);
    assert_eq!(
        plan.rows
            .iter()
            .map(|r| r.definition.clone())
            .collect::<Vec<_>>(),
        product_gate_definitions()
    );
    assert!(
        plan.rows
            .iter()
            .all(|r| r.disposition == Disposition::Execute)
    );
    assert_eq!(plan.allocations().len(), 13);
    assert!(plan.reconcile(&passed(&plan)).accepted());
}

#[test]
fn missing_request_observation_and_policy_transition_allocate_nothing() {
    let input = binding();
    let cases = [
        (None, observation(&input), BlockReason::NoExplicitRequest),
        (
            Some(&input),
            Observation::Unavailable,
            BlockReason::ObservationUnavailable,
        ),
        (
            Some(&input),
            Observation::Complete {
                binding: Box::new(input.clone()),
                policy: PolicyTransition::Changed,
            },
            BlockReason::PolicyTransitionNotProven,
        ),
        (
            Some(&input),
            Observation::Complete {
                binding: Box::new(input.clone()),
                policy: PolicyTransition::Unknown,
            },
            BlockReason::PolicyTransitionNotProven,
        ),
    ];
    for (request, observed, reason) in cases {
        let plan = AdmissionPlan::admit(request, &observed);
        assert_eq!(plan.rows().len(), 13);
        assert!(
            plan.rows()
                .iter()
                .all(|r| r.disposition == Disposition::Blocked(reason))
        );
        assert!(plan.allocations().is_empty());
        assert!(!plan.reconcile(&[]).accepted());
    }
}

#[test]
fn every_binding_dimension_is_checked_at_admission_and_finalization() {
    let original = binding();
    let plan = AdmissionPlan::admit(Some(&original), &observation(&original));
    let mutations: Vec<fn(&mut Binding)> = vec![
        |b| b.repository = "other/repo".into(),
        |b| b.request_id.push('2'),
        |b| b.head = "a".repeat(40),
        |b| b.base = "a".repeat(40),
        |b| b.integration = "a".repeat(40),
        |b| b.tree = "a".repeat(40),
        |b| b.ordered_parents.reverse(),
        |b| b.ordered_parents.push("a".repeat(40)),
        |b| b.configuration.toolchain.push('2'),
        |b| b.configuration.platform.push('2'),
        |b| {
            b.configuration.features.insert("lang-perl".into());
        },
        |b| b.configuration.default_features = false,
        |b| b.configuration.cargo_digest = "a".repeat(64),
        |b| b.configuration.configuration_digest = "a".repeat(64),
        |b| b.configuration.controller_generation.push('2'),
        |b| b.configuration.plan_generation.push('2'),
        |b| b.configuration.result_generation.push('2'),
        |b| b.inventory.clear(),
        |b| b.inventory_provenance.digest = "b".repeat(64),
        |b| b.inventory_provenance.producer.push('2'),
        |b| b.inventory_provenance.reference.push('2'),
        |b| b.evidence_inventory.digest = "b".repeat(64),
        |b| b.evidence_inventory.producer.push('2'),
        |b| b.evidence_inventory.reference.push('2'),
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut changed = original.clone();
        mutate(&mut changed);
        let refused = AdmissionPlan::admit(Some(&original), &observation(&changed));
        assert!(
            refused.allocations().is_empty(),
            "admission mutation {index}"
        );
        let mut results = passed(&plan);
        results[0].binding = changed;
        assert!(
            !plan.reconcile(&results).accepted(),
            "result mutation {index}"
        );
    }
}

#[test]
fn complete_inventory_retains_deleted_renamed_generated_and_mixed_inputs() {
    let mut input = binding();
    input.inventory.extend([
        ChangedInput {
            before: Some(object("deleted.rs", "a")),
            after: None,
        },
        ChangedInput {
            before: Some(object("old.rs", "b")),
            after: Some(object("new.rs", "b")),
        },
        ChangedInput {
            before: None,
            after: Some(object("generated/report.json", "c")),
        },
        ChangedInput {
            before: None,
            after: Some(object("docs/readme.md", "d")),
        },
    ]);
    let plan = AdmissionPlan::admit(Some(&input), &observation(&input));
    assert_eq!(plan.allocations().len(), 13);
    assert!(
        plan.allocations()
            .iter()
            .all(|a| a.binding.inventory == input.inventory)
    );
    for index in 0..input.inventory.len() {
        let mut incomplete = input.clone();
        incomplete.inventory.remove(index);
        assert!(
            AdmissionPlan::admit(Some(&input), &observation(&incomplete))
                .allocations()
                .is_empty()
        );
    }
    // Even an authenticated empty diff does not prove any gate inapplicable.
    input.inventory.clear();
    assert_eq!(
        AdmissionPlan::admit(Some(&input), &observation(&input))
            .allocations()
            .len(),
        13
    );
}

#[test]
fn malformed_bindings_cannot_agree_their_way_into_admission() {
    let mutations: Vec<fn(&mut Binding)> = vec![
        |b| b.head.clear(),
        |b| b.base = "x".repeat(40),
        |b| b.tree = "A".repeat(40),
        |b| b.repository = "owner/repo/extra".into(),
        |b| b.request_id.clear(),
        |b| b.ordered_parents.clear(),
        |b| b.ordered_parents.push(b.ordered_parents[0].clone()),
        |b| b.configuration.toolchain.clear(),
        |b| b.configuration.platform.clear(),
        |b| b.configuration.cargo_digest.clear(),
        |b| b.configuration.configuration_digest.clear(),
        |b| b.configuration.controller_generation.clear(),
        |b| b.configuration.result_generation.clear(),
        |b| b.configuration.plan_generation = "future".into(),
        |b| b.inventory_provenance.reference.clear(),
        |b| b.evidence_inventory.digest.clear(),
        |b| b.inventory.push(b.inventory[0].clone()),
        |b| {
            b.inventory.push(ChangedInput {
                before: None,
                after: None,
            })
        },
        |b| b.inventory[0].after = b.inventory[0].before.clone(),
        |b| b.inventory[0].after = Some(object("../outside", "a")),
        |b| b.inventory[0].after = Some(object("/absolute", "a")),
        |b| b.inventory[0].after = Some(object("bad\\path", "a")),
    ];
    for (index, mutate) in mutations.into_iter().enumerate() {
        let mut invalid = binding();
        mutate(&mut invalid);
        let plan = AdmissionPlan::admit(Some(&invalid), &observation(&invalid));
        assert!(plan.allocations().is_empty(), "invalid mutation {index}");
        assert!(
            plan.rows()
                .iter()
                .all(|r| r.disposition == Disposition::Blocked(BlockReason::InvalidBinding))
        );
    }
}

#[test]
fn finalization_refuses_missing_duplicate_contradictory_and_nonpassing_results() {
    let input = binding();
    let plan = AdmissionPlan::admit(Some(&input), &observation(&input));
    for outcome in [
        Outcome::Failed,
        Outcome::Cancelled,
        Outcome::Pending,
        Outcome::Incomplete,
        Outcome::Skipped,
    ] {
        let mut results = passed(&plan);
        results[0].outcome = outcome;
        assert!(
            plan.reconcile(&results)
                .problems
                .contains(&ResultProblem::NotProven(results[0].definition.id))
        );
    }
    for index in 0..13 {
        let mut missing = passed(&plan);
        let removed = missing.remove(index);
        assert!(
            plan.reconcile(&missing)
                .problems
                .contains(&ResultProblem::Missing(removed.definition.id))
        );
        let mut duplicates = passed(&plan);
        duplicates.push(duplicates[index].clone());
        assert!(
            plan.reconcile(&duplicates)
                .problems
                .contains(&ResultProblem::Duplicate(removed.definition.id))
        );
        duplicates[13].outcome = Outcome::Failed;
        assert!(!plan.reconcile(&duplicates).accepted());
    }
    assert!(!plan.reconcile(&[]).accepted());
    let blocked = AdmissionPlan::admit(None, &observation(&input));
    let report = blocked.reconcile(&passed(&plan));
    assert!(report.problems.contains(&ResultProblem::BlockedPlan));
    assert!(
        report
            .problems
            .iter()
            .any(|p| matches!(p, ResultProblem::Unexpected(_)))
    );
}

#[test]
fn same_gate_name_cannot_substitute_a_different_proposition_or_cache() {
    let input = binding();
    let plan = AdmissionPlan::admit(Some(&input), &observation(&input));
    let mutations: Vec<fn(&mut ActualRow)> = vec![
        |r| r.definition.command = "cargo check",
        |r| r.definition.claim = "different claim",
        |r| r.definition.non_claim = "different boundary",
        |r| r.definition.surfaces.clear(),
        |r| r.definition.role = super::super::ProductGateRole::Advisory,
        |r| r.provenance.digest.clear(),
        |r| r.provenance.reference.clear(),
        |r| r.provenance.producer.clear(),
    ];
    for mutate in mutations {
        let mut results = passed(&plan);
        mutate(&mut results[0]);
        assert!(!plan.reconcile(&results).accepted());
    }
    // No API takes cache metadata, or promotes the old surface NoOp into reuse.
    assert!(plan.rows().iter().all(|r| !matches!(
        r.disposition,
        Disposition::ReuseVerified | Disposition::NotApplicable
    )));
}

#[test]
fn valid_cache_or_unknown_receipts_are_not_executed_gate_proof() {
    let input = binding();
    let plan = AdmissionPlan::admit(Some(&input), &observation(&input));
    for kind in [
        EvidenceKind::CompilerCache,
        EvidenceKind::DependencyCache,
        EvidenceKind::Unknown,
    ] {
        let mut results = passed(&plan);
        results[0].evidence_kind = kind;
        results[0].provenance.producer = "cache-producer".into();
        results[0].provenance.reference = "cache/key".into();
        assert!(results[0].provenance.valid());
        assert!(
            plan.reconcile(&results)
                .problems
                .contains(&ResultProblem::NotProven(results[0].definition.id))
        );
    }
}

#[test]
fn empty_or_duplicate_expected_denominator_cannot_pass() {
    let input = binding();
    let plan = AdmissionPlan::admit(Some(&input), &observation(&input));
    let results = passed(&plan);
    let mut duplicate = plan.clone();
    duplicate.rows.push(duplicate.rows[0].clone());
    assert!(duplicate.allocations().is_empty());
    assert!(!duplicate.reconcile(&results).accepted());
    let mut empty = plan;
    empty.rows.clear();
    assert!(!empty.reconcile(&[]).accepted());
}
