//! The single native-to-registered result projection. A supplied document cannot
//! gain assurance by labeling a row; replay reconstructs this exact projection.
use super::*;
use crate::formal_evidence::{self as formal, Covers, Row, Status, Verdict};
use crate::formal_registry::{self as registry, PreparedRegistry};

pub fn registered_rows(
    prepared: &PreparedRegistry,
    report: &Run,
    captured: &SourceManifest,
) -> AuditResult<Vec<Row>> {
    registry::validate_prepared(prepared)?;
    let execution = report
        .execution
        .as_ref()
        .ok_or("registration lacks native execution")?;
    let mut rows = Vec::new();
    for id in &report.inventory.expected_claim_ids {
        let binding = prepared
            .claims
            .iter()
            .find(|claim| &claim.claim.claim_id == id)
            .ok_or("selected registered claim is absent from its retained registry")?;
        let subject = binding
            .subject
            .as_ref()
            .ok_or("selected claim has no supported native subject")?;
        let observation = report
            .unregistered_results
            .iter()
            .find(|row| row.subject == *subject);
        let mut row = Row {
            claim_id: id.clone(),
            claim_generation: binding.claim.claim_generation,
            subject: subject.id(),
            source_digest: binding.source_digest.clone(),
            status: Status::NotEstablished,
            native_verdict: Verdict::Missing,
            covers: Covers {
                required: binding.required_cover_count,
                satisfied: 0,
                unreachable: 0,
            },
            assumptions_digest: Some(binding.assumptions_digest.clone()),
            bounds_digest: binding.bounds_digest.clone(),
            duration_ms: None,
            peak_memory_bytes: None,
            raw_block_digest: None,
            current: execution.source_current,
            reasons: vec!["the exact registered native subject was not observed".to_string()],
            non_claims: binding.claim.non_claims.clone(),
        };
        if let Some(observed) = observation {
            row.native_verdict = observed.native_verdict.clone();
            row.duration_ms = observed.duration_ms;
            row.raw_block_digest = observed.raw_block_digest.clone();
            row.reasons = vec![observed.detail.clone()];
            let covers = registry::match_native_covers(binding, &observed.cover_properties);
            if let Ok(covers) = &covers {
                row.covers = covers.clone();
            }
            let invocation = execution.invocations.iter().find(|i| {
                i.phase == "verification" && i.package.as_ref() == Some(&subject.package)
            });
            let inputs_admitted = execution.source_current
                && execution.configuration_admitted
                && report.inventory.complete
                && execution
                    .package_inputs
                    .get(&subject.package)
                    .is_some_and(|paths| {
                        paths
                            .iter()
                            .all(|path| binding.source_manifest.contains_key(path))
                    })
                && binding.provider_profile.as_ref().is_some_and(|profile| {
                    u64::from(execution.default_unwind.unwrap_or(32)) == profile.default_unwind
                })
                && report.process.cleanup_state == formal::Cleanup::Complete
                && observed.duration_ms.is_some()
                && observed.raw_block_digest.is_some();
            if inputs_admitted
                && invocation.is_some_and(Invocation::succeeded)
                && observed.status == ObservationStatus::Verified
                && observed.native_verdict == Verdict::Verified
                && covers.is_ok()
                && row.covers.satisfied == row.covers.required
                && row.covers.unreachable == 0
            {
                row.status = Status::Established;
                row.reasons.clear();
            } else if inputs_admitted
                && invocation
                    .is_some_and(|i| i.capture_complete && !i.timed_out && i.error.is_none())
                && observed.status == ObservationStatus::Failed
                && (row.native_verdict == Verdict::Counterexample
                    || (row.native_verdict == Verdict::UnreachableCover
                        && covers.is_ok()
                        && row.covers.unreachable > 0))
            {
                row.status = Status::Failed;
            } else {
                row.reasons.push(
                    "registered native admission is incomplete or outside the captured profile"
                        .to_string(),
                );
                if let Err(error) = covers {
                    row.reasons.push(error);
                }
            }
        }
        rows.push(row);
    }
    let mut bound = report.clone();
    bound.results = rows.clone();
    if let Err(error) = registry::validate_binding(prepared, &bound, captured) {
        for row in &mut rows {
            row.status = Status::NotEstablished;
            row.reasons
                .push(format!("registered binding not established: {error}"));
        }
    }
    // A not-started run cannot borrow observations, even from supplied bytes.
    if report.process.state == formal::ProcessState::NotStarted {
        for row in &mut rows {
            row.status = Status::NotEstablished;
            row.native_verdict = Verdict::Missing;
            row.raw_block_digest = None;
            row.duration_ms = None;
            row.covers.satisfied = 0;
            row.covers.unreachable = 0;
        }
    }
    Ok(rows)
}
