use super::model::{
    CLAIM_BOUNDARY, GATE_KIND, GATE_SCHEMA_VERSION, GateInput, GateVerdict,
    PackagedRepairParityGate, ParityVerdict, QualificationSectionState,
    RepairAttemptSemanticSnapshot,
};

pub(crate) const REQUIRED_SECTION_IDS: &[&str] = &[
    "installed_cli_positive_journey",
    "packaged_vsix_positive_journey",
    "unchanged_edit_control",
    "limited_item_cannot_start",
    "restart_survives_status",
    "no_manual_artifact_threading",
];

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

    failures.extend(validate_required_section_inventory(receipt));
    failures.extend(validate_control_inventory(receipt));

    for section in &receipt.required_sections {
        if section.state.is_passing() {
            if let Some(channel) = section.channel.as_deref() {
                if section_executed_steps(receipt, Some(channel)).is_empty() {
                    failures.push(format!(
                        "section `{}` is qualified without executed channel steps",
                        section.section_id
                    ));
                }
                if channel_semantic(receipt, Some(channel)).is_none() {
                    failures.push(format!(
                        "section `{}` is qualified without a channel semantic snapshot",
                        section.section_id
                    ));
                }
            } else if !control_qualified(receipt, &section.section_id) {
                failures.push(format!(
                    "section `{}` is qualified without a matching qualified control record",
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
        if let Some(semantic) = channel.semantic.as_ref() {
            failures.extend(validate_semantic_against_fixture(
                semantic,
                input.expected_fixture,
                &channel.channel,
            ));
        }
    }

    let parity_failures = compare_channel_semantics(receipt);
    failures.extend(parity_failures);
    failures.extend(compare_channel_artifact_identities(receipt));

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

    if input.require_full_qualification {
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
        for channel in [
            &receipt.channels.installed_cli,
            &receipt.channels.packaged_vsix,
        ] {
            if !channel.section_state.is_passing() {
                failures.push(format!(
                    "channel `{}` section_state is {}; full qualification requires qualified",
                    channel.channel,
                    channel.section_state.as_str()
                ));
            }
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

fn validate_required_section_inventory(
    receipt: &super::model::PackagedRepairParityReceipt,
) -> Vec<String> {
    let mut failures = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for section in &receipt.required_sections {
        if !seen.insert(section.section_id.clone()) {
            failures.push(format!(
                "duplicate required section `{}`",
                section.section_id
            ));
        }
    }
    for required in REQUIRED_SECTION_IDS {
        if !seen.contains(*required) {
            failures.push(format!("missing required section `{required}`"));
        }
    }
    failures
}

fn validate_control_inventory(receipt: &super::model::PackagedRepairParityReceipt) -> Vec<String> {
    let mut failures = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for control in receipt
        .anti_ambient_controls
        .iter()
        .chain(receipt.negative_controls.iter())
    {
        if !seen.insert(control.control_id.clone()) {
            failures.push(format!("duplicate control `{}`", control.control_id));
        }
    }
    failures
}

fn control_qualified(
    receipt: &super::model::PackagedRepairParityReceipt,
    section_id: &str,
) -> bool {
    receipt
        .anti_ambient_controls
        .iter()
        .chain(receipt.negative_controls.iter())
        .any(|control| control.control_id == section_id && control.state.is_passing())
}

fn validate_semantic_against_fixture(
    semantic: &RepairAttemptSemanticSnapshot,
    expected: &super::model::ExpectedFixtureIdentity,
    channel: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    if let Some(gap) = expected.canonical_gap_id.as_deref() {
        if semantic.canonical_gap_id != gap {
            failures.push(format!(
                "channel `{channel}` semantic canonical_gap_id must match fixture ({gap})"
            ));
        }
    }
    if let Some(surface) = expected.allowed_edit_surface.as_ref() {
        if &semantic.allowed_edit_surface != surface {
            failures.push(format!(
                "channel `{channel}` semantic allowed_edit_surface must match fixture contract"
            ));
        }
    }
    if let Some(forbidden) = expected.must_not_change.as_ref() {
        if &semantic.must_not_change != forbidden {
            failures.push(format!(
                "channel `{channel}` semantic must_not_change must match fixture contract"
            ));
        }
    }
    failures
}

fn compare_channel_artifact_identities(
    receipt: &super::model::PackagedRepairParityReceipt,
) -> Vec<String> {
    let cli = &receipt.channels.installed_cli.artifact_identities;
    let vsix = &receipt.channels.packaged_vsix.artifact_identities;
    if cli.is_empty() || vsix.is_empty() {
        return Vec::new();
    }
    if cli != vsix {
        vec!["artifact_identities mismatch between installed_cli and packaged_vsix".to_string()]
    } else {
        Vec::new()
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
    compare_vec(
        &mut mismatches,
        "limitations",
        &cli.limitations,
        &vsix.limitations,
    );
    compare_vec(
        &mut mismatches,
        "non_claims",
        &cli.non_claims,
        &vsix.non_claims,
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
        && compare_channel_artifact_identities(receipt).is_empty()
}

fn aggregate_section_state(
    receipt: &super::model::PackagedRepairParityReceipt,
) -> QualificationSectionState {
    let mut saw_missing = false;
    let mut saw_failure = false;
    let mut all_qualified = true;
    for required in REQUIRED_SECTION_IDS {
        let Some(section) = receipt
            .required_sections
            .iter()
            .find(|section| section.section_id == *required)
        else {
            all_qualified = false;
            saw_missing = true;
            continue;
        };
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
