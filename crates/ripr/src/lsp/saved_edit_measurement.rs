//! Host-scoped saved-edit measurement packet (#4863 / LSPPERF-B).
//!
//! Reuses the #4689 sequence harness without a rival implementation. Fixture
//! and self-dogfood rows retain separate identity; governed external rows stay
//! honestly `not_authorized`.

use super::saved_edit_sequence::{
    SavedEditSequenceReceipt, SequenceIdentity, SequenceStep, evaluate_sequence, honest_receipt,
    receipt_to_json,
};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

pub(super) const MEASUREMENT_SCHEMA: &str = "ripr-lsp-saved-edit-measurement-v1";

const FIXTURE_REPOSITORY: &str = "fixture:lsp-seq-harness";
const SWARM_REPOSITORY: &str = "EffortlessMetrics/ripr-swarm";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SubjectTier {
    DeterministicFixture,
    RiprSwarmSelfDogfood,
    GovernedExternal,
}

impl SubjectTier {
    fn as_str(self) -> &'static str {
        match self {
            Self::DeterministicFixture => "deterministic_fixture",
            Self::RiprSwarmSelfDogfood => "ripr_swarm_self_dogfood",
            Self::GovernedExternal => "governed_external",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RunState {
    Executed,
    NotAuthorized,
    #[expect(dead_code, reason = "policy:4863: reserved for authorized external subjects with no scheduled run")]
    NotRun,
}

impl RunState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Executed => "executed",
            Self::NotAuthorized => "not_authorized",
            Self::NotRun => "not_run",
        }
    }
}

#[derive(Clone, Debug)]
struct SampleSeries {
    population: &'static str,
    cache_reset_procedure: String,
    sample_count: u32,
    elapsed_ms_samples: Vec<u128>,
}

impl SampleSeries {
    fn distribution_json(&self) -> Value {
        let mut sorted = self.elapsed_ms_samples.clone();
        sorted.sort_unstable();
        let n = sorted.len();
        let p50 = if n == 0 {
            Value::String("not_measured".to_string())
        } else {
            json!(sorted[n / 2])
        };
        let tail = if n < 20 {
            json!({
                "tail_not_established": true,
                "reason": "sample_count_below_p95_denominator",
                "raw_samples_ms": sorted,
            })
        } else {
            let p95_index = ((n as f64) * 0.95).floor() as usize;
            json!({
                "tail_not_established": false,
                "p95_ms": sorted[p95_index.min(n.saturating_sub(1))],
                "raw_samples_ms": sorted,
            })
        };
        json!({
            "population": self.population,
            "cache_reset_procedure": self.cache_reset_procedure,
            "sample_count": self.sample_count,
            "p50_ms": p50,
            "distribution": tail,
        })
    }
}

pub(super) fn build_measurement_packet(
    fixture_receipt: &SavedEditSequenceReceipt,
    self_dogfood_receipt: &SavedEditSequenceReceipt,
    fixture_cold_ms: u128,
    fixture_warm_ms: u128,
    dogfood_cold_ms: u128,
    dogfood_warm_ms: u128,
) -> Result<Value, String> {
    evaluate_sequence(fixture_receipt)
        .map_err(|violations| format!("fixture sequence failed evaluation: {violations:?}"))?;
    evaluate_sequence(self_dogfood_receipt)
        .map_err(|violations| format!("self-dogfood sequence failed evaluation: {violations:?}"))?;

    let fixture_cold = SampleSeries {
        population: "cold",
        cache_reset_procedure: fixture_receipt.identity.cache_reset_procedure.clone(),
        sample_count: 1,
        elapsed_ms_samples: vec![fixture_cold_ms],
    };
    let fixture_warm = SampleSeries {
        population: "warm",
        cache_reset_procedure: "reuse fixture cache directory from cold sample".to_string(),
        sample_count: 1,
        elapsed_ms_samples: vec![fixture_warm_ms],
    };
    let dogfood_cold = SampleSeries {
        population: "cold",
        cache_reset_procedure: self_dogfood_receipt.identity.cache_reset_procedure.clone(),
        sample_count: 1,
        elapsed_ms_samples: vec![dogfood_cold_ms],
    };
    let dogfood_warm = SampleSeries {
        population: "warm",
        cache_reset_procedure: "reuse self-dogfood cache directory from cold sample".to_string(),
        sample_count: 1,
        elapsed_ms_samples: vec![dogfood_warm_ms],
    };

    let subjects = vec![
        subject_row(
            SubjectTier::DeterministicFixture,
            RunState::Executed,
            FIXTURE_REPOSITORY,
            fixture_receipt,
            json!({
                "cold": fixture_cold.distribution_json(),
                "warm": fixture_warm.distribution_json(),
            }),
            endpoints_from_receipt(fixture_receipt),
            vec![],
        )?,
        subject_row(
            SubjectTier::RiprSwarmSelfDogfood,
            RunState::Executed,
            SWARM_REPOSITORY,
            self_dogfood_receipt,
            json!({
                "cold": dogfood_cold.distribution_json(),
                "warm": dogfood_warm.distribution_json(),
            }),
            endpoints_from_receipt(self_dogfood_receipt),
            vec![],
        )?,
        json!({
            "tier": SubjectTier::GovernedExternal.as_str(),
            "run_state": RunState::NotAuthorized.as_str(),
            "repository": Value::Null,
            "reason": "no governed #1702/#1560 trial authorization bound to an exact external head in this run",
            "limitations": ["not_run"],
        }),
    ];

    let dominant = dominant_observed_phase(fixture_receipt, self_dogfood_receipt);
    let mut packet = json!({
        "schema_version": MEASUREMENT_SCHEMA,
        "tool": "ripr",
        "report": "lsp-performance-measurement",
        "measurement_method": "in-process saved_edit_sequence harness (#4689); cold and warm populations use isolated versus reused private cache directories",
        "sample_denominator": {
            "warmup_rule": "no warmup discards before the recorded cold sample",
            "cold_samples_per_executed_subject": 1,
            "warm_samples_per_executed_subject": 1,
            "p95_requires_at_least": 20,
        },
        "subjects": subjects,
        "latency_semantic_work_output_memory_axes": "recorded separately per step; rss remains not_measured unless sampled",
        "dominant_observed_phase": dominant,
        "non_claims": [
            "historical 2s/10s/30s figures remain proposals",
            "host-scoped numbers are observations, not universal guarantees",
            "fixture and self-dogfood rows must not be aggregated into one latency claim",
        ],
        "freshness_rule": "reproduce on the bound repository sha, binary digest, host class, and cache-reset procedure; unrelated main movement alone does not refresh host observations",
    });

    let digest = packet_digest_semantic(&packet)?;
    if let Some(object) = packet.as_object_mut() {
        object.insert("packet_digest".to_string(), json!(digest));
    }
    Ok(packet)
}

fn subject_row(
    tier: SubjectTier,
    run_state: RunState,
    repository: &str,
    receipt: &SavedEditSequenceReceipt,
    cold_warm: Value,
    endpoints: Value,
    limitations: Vec<&str>,
) -> Result<Value, String> {
    Ok(json!({
        "tier": tier.as_str(),
        "run_state": run_state.as_str(),
        "repository": repository,
        "identity": identity_json(&receipt.identity),
        "cold_warm": cold_warm,
        "endpoints_ms": endpoints,
        "sequence_receipt": receipt_to_json(receipt),
        "semantic_parity": "checked_by_evaluate_sequence",
        "limitations": limitations,
    }))
}

fn identity_json(identity: &SequenceIdentity) -> Value {
    json!({
        "source_sha": identity.source_sha,
        "binary_path": identity.binary_path,
        "binary_digest": identity.binary_digest,
        "host_class": identity.host_class,
        "features": identity.features,
        "cache_reset_procedure": identity.cache_reset_procedure,
        "sample_count": identity.sample_count,
        "hidden_workspace_binary": identity.hidden_workspace_binary,
    })
}

fn endpoints_from_receipt(receipt: &SavedEditSequenceReceipt) -> Value {
    let cold = receipt
        .steps
        .iter()
        .find(|step| step.step == SequenceStep::ColdStart);
    let production = receipt
        .steps
        .iter()
        .find(|step| step.step == SequenceStep::ProductionEdit);
    let full = receipt
        .steps
        .iter()
        .find(|step| step.step == SequenceStep::ExplicitFullRefresh);
    json!({
        "first_health_ms": cold.and_then(|step| step.elapsed_ms).unwrap_or(0),
        "first_useful_ms": production.and_then(|step| step.elapsed_ms).unwrap_or(0),
        "complete_ms": full.and_then(|step| step.elapsed_ms).unwrap_or(0),
    })
}

fn dominant_observed_phase(
    fixture: &SavedEditSequenceReceipt,
    dogfood: &SavedEditSequenceReceipt,
) -> Value {
    let mut best: Option<(&str, u128)> = None;
    for receipt in [fixture, dogfood] {
        for step in &receipt.steps {
            let Some(elapsed) = step.elapsed_ms else {
                continue;
            };
            if best.is_none_or(|(_, current)| elapsed > current) {
                best = Some((step.step.as_str(), elapsed));
            }
        }
    }
    match best {
        Some((step, _)) => json!({
            "phase": step,
            "basis": "max recorded step elapsed_ms across executed subjects",
        }),
        None => json!({
            "phase": "not_established",
            "reason": "no elapsed observations were recorded",
        }),
    }
}

pub(super) fn packet_digest_semantic(packet: &Value) -> Result<String, String> {
    let mut normalized = packet.clone();
    if let Some(object) = normalized.as_object_mut() {
        object.remove("packet_digest");
        scrub_host_local(object);
        normalize_subject_order(object);
    }
    let text = serde_json::to_string(&normalized)
        .map_err(|err| format!("serialize measurement packet for digest: {err}"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(text.as_bytes())))
}

fn scrub_host_local(object: &mut Map<String, Value>) {
    if let Some(subjects) = object.get_mut("subjects").and_then(Value::as_array_mut) {
        for subject in subjects {
            if let Some(identity) = subject.get_mut("identity").and_then(Value::as_object_mut) {
                identity.remove("binary_path");
            }
            if let Some(receipt) = subject
                .get_mut("sequence_receipt")
                .and_then(Value::as_object_mut)
            {
                if let Some(identity) = receipt.get_mut("identity").and_then(Value::as_object_mut) {
                    identity.remove("binary_path");
                }
                if let Some(steps) = receipt.get_mut("steps").and_then(Value::as_array_mut) {
                    for step in steps {
                        if let Some(step_object) = step.as_object_mut() {
                            step_object.remove("elapsed_ms");
                        }
                    }
                }
            }
        }
    }
}

fn normalize_subject_order(object: &mut Map<String, Value>) {
    let Some(subjects) = object.get_mut("subjects").and_then(Value::as_array_mut) else {
        return;
    };
    subjects.sort_by(|left, right| {
        let left_tier = left.get("tier").and_then(Value::as_str).unwrap_or("");
        let right_tier = right.get("tier").and_then(Value::as_str).unwrap_or("");
        left_tier.cmp(right_tier)
    });
}

pub(super) fn validate_cold_warm_honesty(
    cache_reset_procedure: &str,
    marked_population: &str,
) -> Result<(), String> {
    if marked_population == "cold"
        && cache_reset_procedure
            .to_ascii_lowercase()
            .contains("warm reuse")
    {
        return Err("cold run marked warm without the declared cache-reset state".to_string());
    }
    Ok(())
}

mod tests {
    use super::*;

    fn sample_receipt(label: &str) -> SavedEditSequenceReceipt {
        let mut receipt = honest_receipt();
        receipt.identity.source_sha = Some(format!("sha-{label}"));
        receipt.identity.binary_path = format!("target/debug/ripr-{label}");
        receipt.identity.binary_digest = Some(format!("sha256:{label}"));
        receipt
            .identity
            .cache_reset_procedure
            .clone_from(&format!("isolated cache for {label}"));
        receipt
    }

    #[test]
    fn packet_digest_is_stable_when_subject_rows_are_reordered() -> Result<(), String> {
        let fixture = sample_receipt("fixture");
        let dogfood = sample_receipt("dogfood");
        let mut packet = build_measurement_packet(&fixture, &dogfood, 10, 5, 12, 6)?;
        let first = packet_digest_semantic(&packet)?;
        if let Some(subjects) = packet.get_mut("subjects").and_then(Value::as_array_mut) {
            subjects.reverse();
        }
        let second = packet_digest_semantic(&packet)?;
        if first != second {
            return Err(format!(
                "digest must not depend on subject order: {first} vs {second}"
            ));
        }
        Ok(())
    }

    #[test]
    fn cold_run_marked_warm_without_cache_state_is_rejected() -> Result<(), String> {
        match validate_cold_warm_honesty("warm reuse cache", "cold") {
            Err(message) if message.contains("cold run marked warm") => Ok(()),
            other => Err(format!("expected cold/warm rejection, got {other:?}")),
        }
    }

    #[test]
    fn measurement_packet_retains_executed_fixture_and_self_dogfood_rows() -> Result<(), String> {
        let fixture = sample_receipt("fixture");
        let dogfood = sample_receipt("dogfood");
        let packet = build_measurement_packet(&fixture, &dogfood, 10, 5, 12, 6)?;
        let subjects = packet
            .get("subjects")
            .and_then(Value::as_array)
            .ok_or_else(|| "missing subjects".to_string())?;
        if subjects.len() != 3 {
            return Err(format!("expected three subjects, got {}", subjects.len()));
        }
        let fixture_row = &subjects[0];
        if fixture_row.get("tier").and_then(Value::as_str) != Some("deterministic_fixture") {
            return Err("first subject must be deterministic_fixture".to_string());
        }
        if fixture_row.get("run_state").and_then(Value::as_str) != Some("executed") {
            return Err("fixture row must be executed".to_string());
        }
        let dogfood_row = &subjects[1];
        if dogfood_row.get("tier").and_then(Value::as_str) != Some("ripr_swarm_self_dogfood") {
            return Err("second subject must be ripr_swarm_self_dogfood".to_string());
        }
        if dogfood_row.get("repository").and_then(Value::as_str) != Some(SWARM_REPOSITORY) {
            return Err("self-dogfood row must name ripr-swarm".to_string());
        }
        let external = &subjects[2];
        if external.get("run_state").and_then(Value::as_str) != Some("not_authorized") {
            return Err("external row must stay not_authorized".to_string());
        }
        if packet
            .get("packet_digest")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err("packet must include packet_digest".to_string());
        }
        Ok(())
    }

    #[test]
    fn endpoints_report_health_useful_and_complete_separately() -> Result<(), String> {
        let mut fixture = sample_receipt("fixture");
        if let Some(cold) = fixture
            .steps
            .iter_mut()
            .find(|step| step.step == SequenceStep::ColdStart)
        {
            cold.elapsed_ms = Some(11);
        }
        if let Some(production) = fixture
            .steps
            .iter_mut()
            .find(|step| step.step == SequenceStep::ProductionEdit)
        {
            production.elapsed_ms = Some(22);
        }
        if let Some(full) = fixture
            .steps
            .iter_mut()
            .find(|step| step.step == SequenceStep::ExplicitFullRefresh)
        {
            full.elapsed_ms = Some(33);
        }
        let endpoints = endpoints_from_receipt(&fixture);
        if endpoints.get("first_health_ms").and_then(Value::as_u64) != Some(11) {
            return Err(format!("unexpected endpoints: {endpoints}"));
        }
        if endpoints.get("first_useful_ms").and_then(Value::as_u64) != Some(22) {
            return Err(format!("unexpected endpoints: {endpoints}"));
        }
        if endpoints.get("complete_ms").and_then(Value::as_u64) != Some(33) {
            return Err(format!("unexpected endpoints: {endpoints}"));
        }
        Ok(())
    }
}
