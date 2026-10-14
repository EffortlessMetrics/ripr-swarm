use match_arm_tuple_derived_local::{ProofReceipt, terminalize_proof};
use std::collections::BTreeMap;
#[test]
fn terminal_projection_0() {
    let receipts = vec![ProofReceipt { id: "receipt-1".to_string(), result: "head_passed".to_string() }];
    let proof_request_ids = BTreeMap::from([("different-task".to_string(), vec!["request-1".to_string(), "request-1".to_string()])]);
    let receipt_request_ids = BTreeMap::from([("receipt-1".to_string(), vec!["request-1".to_string()])]);
    let terminal = terminalize_proof("different-task", "planned", &receipts, &proof_request_ids, &receipt_request_ids).unwrap();
    assert_eq!(terminal.0, "head_passed");
    assert_eq!(terminal.1, "receipt-1=head_passed join=request_identity_v2", "request_identity_v2");
    assert_eq!(terminal.2, vec!["request-1".to_string()]);
    assert_eq!(terminal.3, vec!["receipt-1".to_string()]);
}
