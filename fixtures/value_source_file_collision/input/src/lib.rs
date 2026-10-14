pub fn coverage_summary_receipt(present: bool) -> serde_json::Value {
    let text: Option<String> = if present {
        Some(String::new())
    } else {
        None
    };
    serde_json::json!({"status": if text.is_some() { "collected" } else { "not_collected" }})
}
#[test]
fn json_indexed_receipt() {
    let summary = coverage_summary_receipt(false);
    assert_eq!(summary["status"], "not_collected");
}
#[cfg(test)]
mod decoy;
