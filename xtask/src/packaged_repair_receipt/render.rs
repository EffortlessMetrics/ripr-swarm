use super::model::{PackagedRepairParityGate, PackagedRepairParityReceipt};

pub(crate) fn render_receipt_json(receipt: &PackagedRepairParityReceipt) -> Result<String, String> {
    serde_json::to_string_pretty(receipt).map_err(|error| error.to_string())
}

pub(crate) fn render_gate_json(gate: &PackagedRepairParityGate) -> Result<String, String> {
    serde_json::to_string_pretty(gate).map_err(|error| error.to_string())
}

pub(crate) fn render_gate_markdown(gate: &PackagedRepairParityGate) -> String {
    let mut out = String::new();
    out.push_str("# Packaged repair parity gate\n\n");
    out.push_str(&format!("verdict={}\n", gate_verdict(gate)));
    out.push_str(&format!(
        "aggregate_section_state={}\n",
        gate.aggregate_section_state.as_str()
    ));
    out.push_str(&format!(
        "parity_verdict={}\n",
        gate.parity_verdict.as_str()
    ));
    out.push_str(&format!(
        "fixture.base_commit={}\n",
        gate.fixture.base_commit
    ));
    out.push_str(&format!(
        "fixture.head_commit={}\n",
        gate.fixture.head_commit
    ));
    out.push_str(&format!(
        "fixture.canonical_gap_id={}\n",
        gate.fixture.canonical_gap_id
    ));
    out.push_str(&format!("candidate.commit={}\n", gate.candidate.commit));
    out.push_str(&format!("candidate.tree={}\n", gate.candidate.tree));
    if gate.failures.is_empty() {
        out.push_str("failures=none\n");
    } else {
        out.push_str("failures:\n");
        for failure in &gate.failures {
            out.push_str(&format!("- {failure}\n"));
        }
    }
    out.push_str("\n## Non-claims\n\n");
    for claim in &gate.non_claims {
        out.push_str(&format!("- {claim}\n"));
    }
    out.push_str(&format!("\n{}\n", gate.claim_boundary));
    out
}

fn gate_verdict(gate: &PackagedRepairParityGate) -> &'static str {
    match gate.verdict {
        super::model::GateVerdict::Passed => "passed",
        super::model::GateVerdict::Failed => "failed",
    }
}
