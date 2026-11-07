use super::*;

pub(super) fn plan() -> Plan {
    Plan {
        version: 1,
        subject: serde_json::json!({"request_id":"123-1","merge":"exact-G","parents":["B","H"]}),
        inventory: "full-inventory".into(),
        configuration: "toolchain-features-config".into(),
        producer: "producer-blob".into(),
        rows: rows(),
    }
}

fn results(plan: &Plan) -> Vec<BranchResult> {
    Branch::ALL
        .into_iter()
        .map(|branch| BranchResult {
            plan: plan.clone(),
            branch,
            elapsed_seconds: 1,
            rows: plan
                .rows
                .iter()
                .filter(|row| row.branch == branch)
                .map(|row| ResultRow {
                    id: row.id.clone(),
                    step: row.step.clone(),
                    outcome: "success".into(),
                })
                .collect(),
        })
        .collect()
}

#[test]
fn full_rust_portfolio_is_exact_once_with_real_producer_dependencies() {
    let plan = plan();
    assert_eq!(plan.rows.len(), 21);
    assert_eq!(
        plan.rows
            .iter()
            .map(|r| &r.id)
            .collect::<BTreeSet<_>>()
            .len(),
        21
    );
    assert_eq!(
        plan.rows.iter().map(|r| r.branch).collect::<BTreeSet<_>>(),
        Branch::ALL.into_iter().collect()
    );
    for row in &plan.rows {
        for predecessor in &row.predecessors {
            assert!(plan.rows.iter().any(|other| &other.id == predecessor
                && (other.branch == row.branch || other.branch == Branch::Preflight)));
        }
    }
    assert_eq!(
        ProductGateId::SchemaSweep.predecessors(),
        &[ProductGateId::PrEvidence]
    );
    assert_eq!(
        ProductGateId::WildcardRetention.predecessors(),
        &[ProductGateId::Goldens]
    );
    assert_eq!(reconcile(&plan, &results(&plan)), Ok(()));
}

#[test]
fn every_branch_and_every_row_must_finish_successfully() {
    let plan = plan();
    for branch_index in 0..Branch::ALL.len() {
        let mut actual = results(&plan);
        actual.remove(branch_index);
        assert!(reconcile(&plan, &actual).is_err());
        let mut actual = results(&plan);
        actual.push(actual[branch_index].clone());
        assert!(reconcile(&plan, &actual).is_err());
        for index in 0..results(&plan)[branch_index].rows.len() {
            for outcome in ["failure", "cancelled", "skipped", "missing", "pending", ""] {
                let mut actual = results(&plan);
                actual[branch_index].rows[index].outcome = outcome.into();
                assert!(reconcile(&plan, &actual).is_err());
            }
            let mut actual = results(&plan);
            actual[branch_index].rows.remove(index);
            assert!(reconcile(&plan, &actual).is_err());
            let mut actual = results(&plan);
            let duplicate = actual[branch_index].rows[index].clone();
            actual[branch_index].rows.push(duplicate);
            assert!(reconcile(&plan, &actual).is_err());
        }
    }
}

#[test]
fn stale_subject_configuration_denominator_or_producer_never_reconciles() {
    let expected = plan();
    let mutations: Vec<fn(&mut Plan)> = vec![
        |p| p.version += 1,
        |p| p.subject["request_id"] = "123-2".into(),
        |p| p.subject["parents"] = serde_json::json!(["H", "B"]),
        |p| p.inventory.clear(),
        |p| p.configuration.push('x'),
        |p| p.producer.push('x'),
        |p| p.rows.clear(),
        |p| p.rows[0].command = "true".into(),
        |p| p.rows[0].branch = Branch::BaseMatcher,
    ];
    for mutate in mutations {
        let mut actual = results(&expected);
        mutate(&mut actual[0].plan);
        assert!(reconcile(&expected, &actual).is_err());
    }
    let mut malformed = expected;
    malformed.rows.pop();
    assert!(reconcile(&malformed, &results(&malformed)).is_err());
}

#[test]
fn native_inventory_follows_workspace_preparation_in_the_same_job() {
    use ProductGateId::*;
    assert_eq!(
        WorkspacePreparation.predecessors(),
        &[Formatting, AgentSkills]
    );
    assert_eq!(
        Precommit.predecessors(),
        &[AgentSkills, WorkspacePreparation]
    );
    assert_eq!(WorkspaceTests.predecessors(), &[Precommit]);
    for id in [Formatting, AgentSkills] {
        assert_eq!(id.branch(), Branch::Preflight);
    }
    for id in [WorkspacePreparation, Precommit, WorkspaceTests] {
        assert_eq!(id.branch(), Branch::Tests);
    }
}
