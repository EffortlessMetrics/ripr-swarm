use super::model::{
    CLAIM_BOUNDARY, GATE_KIND, GATE_SCHEMA_VERSION, GateInput, GateVerdict,
    PackagedRepairParityGate, ParityVerdict, QualificationSectionState,
    RepairAttemptSemanticSnapshot,
};

pub(crate) fn evaluate(input: &GateInput<'_>) -> PackagedRepairParityGate {
    let receipt = input.receipt;
    let mut failures = Vec::new();

    push_mismatch(
        &mut failures,
        "fixture.base_commit",
        input.expected_fixture.base_commit.as_deref(),
        &receipt.fixture.base_commit,
        "stale fixture base commit",
    );
    push_mismatch(
        &mut failures,
        "fixture.head_commit",
        input.expected_fixture.head_commit.as_deref(),
        &receipt.fixture.head_commit,
        "stale fixture head commit",
    );
    push_mismatch(
        &mut failures,
        "fixture.canonical_gap_id",
        input.expected_fixture.canonical_gap_id.as_deref(),
        &receipt.fixture.canonical_gap_id,
        "mismatched canonical gap id",
    );

    for section in &receipt.required_sections {
        if section.state.is_passing() {
            if section_executed_steps(receipt, section.channel.as_deref()).is_empty() {
                failures.push(format!(
                    "section `{}` is qualified without executed channel steps",
                    section.section_id
                ));
            }
            if section.channel.is_some()
                && channel_semantic(receipt, section.channel.as_deref()).is_none()
            {
                failures.push(format!(
                    "section `{}` is qualified without a channel semantic snapshot",
                    section.section_id
                ));
            }
        }
    }

    for channel in [
        &receipt.channels.installed_cli,
        &receipt.channels.packaged_vsix,
    ] {
        if channel.section_state.is_passing() && channel.executed_steps.is_empty() {
            failures.push(format!(
                "channel `{}` cannot be qualified with zero executed_steps",
                channel.channel
            ));
        }
        if channel.section_state.is_passing() && channel.semantic.is_none() {
            failures.push(format!(
                "channel `{}` cannot be qualified without semantic snapshot",
                channel.channel
            ));
        }
    }

    let parity_failures = compare_channel_semantics(receipt);
    failures.extend(parity_failures);

    if receipt.parity.verdict == ParityVerdict::Qualified && !parity_ready(receipt) {
        failures.push(
            "parity.verdict is qualified but both channels lack matching semantic evidence"
                .to_string(),
        );
    }

    if receipt.parity.verdict == ParityVerdict::Qualified && !receipt.parity.mismatches.is_empty() {
        failures.push(
            "parity.verdict cannot be qualified while parity.mismatches is nonempty".to_string(),
        );
    }

    let aggregate = aggregate_section_state(receipt);
    let parity_verdict = receipt.parity.verdict;

    let require_pass = input.require_full_qualification;
    if require_pass {
        if aggregate != QualificationSectionState::Qualified {
            failures.push(format!(
                "aggregate section state is {}; full qualification requires qualified",
                aggregate.as_str()
            ));
        }
        if parity_verdict != ParityVerdict::Qualified {
            failures.push(format!(
                "parity verdict is {}; full qualification requires qualified",
                parity_verdict.as_str()
            ));
        }
    }

    let verdict = if failures.is_empty() {
        GateVerdict::Passed
    } else {
        GateVerdict::Failed
    };

    PackagedRepairParityGate {
        schema_version: GATE_SCHEMA_VERSION.to_string(),
        kind: GATE_KIND.to_string(),
        verdict,
        aggregate_section_state: aggregate,
        parity_verdict,
        failures,
        fixture: receipt.fixture.clone(),
        candidate: receipt.candidate.clone(),
        limitations: receipt.limitations.clone(),
        non_claims: receipt.non_claims.clone(),
        claim_boundary: CLAIM_BOUNDARY.to_string(),
    }
}

fn push_mismatch(
    failures: &mut Vec<String>,
    field: &str,
    expected: Option<&str>,
    actual: &str,
    label: &str,
) {
    if let Some(expected) = expected
        && expected != actual
    {
        failures.push(format!(
            "{label} ({field}): expected {expected}, got {actual}"
        ));
    }
}

const EMPTY_STEPS: &[String] = &[];

fn section_executed_steps<'a>(
    receipt: &'a super::model::PackagedRepairParityReceipt,
    channel: Option<&str>,
) -> &'a [String] {
    match channel {
        Some("installed_cli") => &receipt.channels.installed_cli.executed_steps,
        Some("packaged_vsix") => &receipt.channels.packaged_vsix.executed_steps,
        _ => EMPTY_STEPS,
    }
}

fn channel_semantic<'a>(
    receipt: &'a super::model::PackagedRepairParityReceipt,
    channel: Option<&str>,
) -> Option<&'a RepairAttemptSemanticSnapshot> {
    match channel {
        Some("installed_cli") => receipt.channels.installed_cli.semantic.as_ref(),
        Some("packaged_vsix") => receipt.channels.packaged_vsix.semantic.as_ref(),
        _ => None,
    }
}

fn compare_channel_semantics(receipt: &super::model::PackagedRepairParityReceipt) -> Vec<String> {
    let cli = receipt.channels.installed_cli.semantic.as_ref();
    let vsix = receipt.channels.packaged_vsix.semantic.as_ref();
    let (Some(cli), Some(vsix)) = (cli, vsix) else {
        return Vec::new();
    };
    let mut mismatches = Vec::new();
    compare_field(
        &mut mismatches,
        "canonical_gap_id",
        &cli.canonical_gap_id,
        &vsix.canonical_gap_id,
    );
    compare_vec(
        &mut mismatches,
        "allowed_edit_surface",
        &cli.allowed_edit_surface,
        &vsix.allowed_edit_surface,
    );
    compare_vec(
        &mut mismatches,
        "must_not_change",
        &cli.must_not_change,
        &vsix.must_not_change,
    );
    compare_field(
        &mut mismatches,
        "status_class",
        &cli.status_class,
        &vsix.status_class,
    );
    compare_field(
        &mut mismatches,
        "movement_state",
        &cli.movement_state,
        &vsix.movement_state,
    );
    compare_field(
        &mut mismatches,
        "receipt_status",
        &cli.receipt_status,
        &vsix.receipt_status,
    );
    compare_field(
        &mut mismatches,
        "verify_status",
        &cli.verify_status,
        &vsix.verify_status,
    );
    compare_field(
        &mut mismatches,
        "currentness",
        &cli.currentness,
        &vsix.currentness,
    );
    compare_field(
        &mut mismatches,
        "next_action_kind",
        &cli.next_action_kind,
        &vsix.next_action_kind,
    );
    mismatches
}

fn compare_field(out: &mut Vec<String>, field: &str, left: &str, right: &str) {
    if left != right {
        out.push(format!(
            "semantic mismatch on {field}: cli={left}, vsix={right}"
        ));
    }
}

fn compare_vec(out: &mut Vec<String>, field: &str, left: &[String], right: &[String]) {
    if left != right {
        out.push(format!("semantic mismatch on {field}"));
    }
}

fn parity_ready(receipt: &super::model::PackagedRepairParityReceipt) -> bool {
    receipt.channels.installed_cli.semantic.is_some()
        && receipt.channels.packaged_vsix.semantic.is_some()
        && compare_channel_semantics(receipt).is_empty()
}

fn aggregate_section_state(
    receipt: &super::model::PackagedRepairParityReceipt,
) -> QualificationSectionState {
    let mut saw_missing = false;
    let mut saw_failure = false;
    let mut all_qualified = true;
    for section in &receipt.required_sections {
        match section.state {
            QualificationSectionState::Qualified => {}
            QualificationSectionState::MissingEvidence => {
                all_qualified = false;
                saw_missing = true;
            }
            QualificationSectionState::SemanticFailure => {
                all_qualified = false;
                saw_failure = true;
            }
            QualificationSectionState::StaleOrWrongHead
            | QualificationSectionState::InfrastructureTempfail
            | QualificationSectionState::Incomparable => {
                all_qualified = false;
                saw_failure = true;
            }
        }
    }
    if all_qualified {
        QualificationSectionState::Qualified
    } else if saw_failure {
        QualificationSectionState::SemanticFailure
    } else if saw_missing {
        QualificationSectionState::MissingEvidence
    } else {
        QualificationSectionState::Incomparable
    }
}
