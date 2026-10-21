#[macro_use] extern crate counterfeit_format;
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, String>;
#[derive(Debug, PartialEq, Eq)]
pub struct ProofReceipt { pub id: String, pub result: String }
pub fn terminalize_proof(
    task_id: &str,
    plan_status: &str,
    receipts: &[ProofReceipt],
    proof_request_ids: &BTreeMap<String, Vec<String>>,
    receipt_request_ids: &BTreeMap<String, Vec<String>>,
) -> Result<(String, String, Vec<String>, Vec<String>)> {
    let mut request_ids = proof_request_ids.get(task_id).cloned().unwrap_or_default();
    request_ids.sort();
    request_ids.dedup();
    let request_set = request_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut matching = receipts
        .iter()
        .filter_map(|receipt| {
            let request_identity_matches =
                receipt_request_ids
                    .get(&receipt.id)
                    .is_some_and(|receipt_requests| {
                        receipt_requests
                            .iter()
                            .any(|request_id| request_set.contains(request_id.as_str()))
                    });
            let task_identity_matches = receipt.id == task_id;
            let relation = match (request_identity_matches, task_identity_matches) {
                (true, true) => "request_and_task_identity",
                (true, false) => "request_identity_v2",
                (false, true) => "task_identity",
                (false, false) => return None,
            };
            Some((receipt, relation))
        })
        .collect::<Vec<_>>();
    matching.sort_by(|(left, _), (right, _)| left.id.cmp(&right.id));
    let receipt_ids = matching
        .iter()
        .map(|(receipt, _)| receipt.id.clone())
        .collect::<Vec<_>>();
    if matching.is_empty() {
        let (status, reason) = if plan_status == "planned" {
            (
                "not_executed".to_owned(),
                "planned proof task has no terminal receipt joined by request or task identity"
                    .to_owned(),
            )
        } else {
            (
                plan_status.to_owned(),
                "proof task retained its terminal planner disposition".to_owned(),
            )
        };
        return Ok((status, reason, request_ids, receipt_ids));
    }
    let mut results = matching
        .iter()
        .map(|(receipt, _)| terminal_proof_receipt_status(receipt))
        .collect::<Result<Vec<_>>>()?;
    results.sort();
    results.dedup();
    let status = if results.len() == 1 {
        results[0].clone()
    } else {
        "multiple_terminal_receipts".to_owned()
    };
    let reason = matching
        .iter()
        .map(|(receipt, relation)| format!("{}={} join={relation}", receipt.id, receipt.result))
        .collect::<Vec<_>>()
        .join("; ");
    Ok((status, reason, request_ids, receipt_ids))
}

fn terminal_proof_receipt_status(receipt: &ProofReceipt) -> Result<String> {
    if receipt.result.trim().is_empty() || !matches!(receipt.result.as_str(),
        "head_passed" | "head_failed" | "discriminating" | "non_discriminating"
        | "base_patch_failed" | "timed_out" | "skipped_budget" | "skipped_profile") {
        return Err("unsupported terminal result".to_string());
    }
    Ok(receipt.result.clone())
}
