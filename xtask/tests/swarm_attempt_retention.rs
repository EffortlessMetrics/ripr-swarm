//! Exercise the retained report across successive invocations of its writer.

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

use serde_json::{Value, json};
use std::{fs, path::Path, process::Output};

fn refresh(root: &Path, receipt: &str, gap: &str) -> Result<Output, Box<dyn std::error::Error>> {
    let outcomes = json!({
        "inputs": {"agent_receipt": receipt},
        "outcomes": [{
            "canonical_gap_id": gap,
            "receipt_state": "receipt_movement_improved",
            "outcome_state": "evidence_improved",
            "movement_source": "agent_receipt",
            "verify_result": "pass"
        }]
    });
    fs::write(root.join("outcomes.json"), serde_json::to_vec(&outcomes)?)?;
    Ok(xtask_bin::output(|command| {
        command.current_dir(root).args([
            "ripr-swarm",
            "attempt-ledger",
            "--swarm-plan",
            "plan.json",
            "--actionable-gap-outcomes",
            "outcomes.json",
            "--real-repair-attempts",
            "corpus.json",
        ]);
    })?)
}

#[test]
fn retains_registered_receipts_across_replay_and_refuses_incomplete_history()
-> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "swarm-register-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&root)?;
    fs::write(root.join("plan.json"), r#"{"top_ready_packets":[]}"#)?;
    fs::write(root.join("corpus.json"), r#"{"cases":[]}"#)?;
    let ledger = root.join("target/ripr/reports/swarm-attempt-ledger.json");
    let first = "receipts/run-a.json";
    let second = "receipts/run_a.json";
    let mut original_id = None;
    let mut second_id = None;
    for (receipt, count) in [(first, 1), (second, 2), (second, 2), (first, 2)] {
        let output = refresh(&root, receipt, "gap:retention")?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&fs::read(&ledger)?)?;
        let rows = report["attempts"].as_array().ok_or("missing attempts")?;
        assert_eq!(rows.len(), count);
        for row in rows {
            assert_eq!(row["verify_result"], "pass");
        }
        let id_for = |path: &str| -> Result<String, Box<dyn std::error::Error>> {
            let row = rows
                .iter()
                .find(|row| row["receipt_path"] == path)
                .ok_or("missing registered receipt")?;
            Ok(row["attempt_id"]
                .as_str()
                .ok_or("missing attempt ID")?
                .to_string())
        };
        let id = id_for(first)?;
        assert_eq!(original_id.get_or_insert(id.clone()), &id);
        if count == 2 {
            let other = id_for(second)?;
            assert_ne!(id, other);
            assert_eq!(second_id.get_or_insert(other.clone()), &other);
        }
    }

    let original = fs::read(&ledger)?;
    let collision = refresh(&root, first, "gap_retention")?;
    assert!(!collision.status.success());
    assert!(
        String::from_utf8_lossy(&collision.stderr)
            .contains("refresh would replace registered attempt")
    );
    assert_eq!(fs::read(&ledger)?, original);

    let retained: Value = serde_json::from_slice(&fs::read(&ledger)?)?;
    // Keep the count unchanged to isolate the ID membership check.
    let mut missing = retained.clone();
    missing["latest_attempts"][0]["attempt_id"] = json!("attempt:registered-elsewhere");
    // Drop a historical row while keeping the latest projection consistent.
    let mut conflicting = retained.clone();
    conflicting["latest_attempts"][0]["receipt_path"] = json!("receipts/other.json");
    let mut truncated = retained;
    let latest_id = truncated["latest_attempts"][0]["attempt_id"].clone();
    truncated["attempts"]
        .as_array_mut()
        .ok_or("missing attempts")?
        .retain(|row| row["attempt_id"] == latest_id);
    let mut damaged_encoding = original.clone();
    damaged_encoding.push(0xff);
    for bytes in [
        serde_json::to_vec(&missing)?,
        serde_json::to_vec(&conflicting)?,
        serde_json::to_vec(&truncated)?,
        damaged_encoding,
    ] {
        fs::write(&ledger, &bytes)?;
        let output = refresh(&root, first, "gap:retention")?;
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("attempt history unavailable"));
        assert_eq!(fs::read(&ledger)?, bytes);
    }
    fs::remove_dir_all(root)?;
    Ok(())
}
