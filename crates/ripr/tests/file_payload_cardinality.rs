//! #6501: file-mediated cardinality is unresolved, not a known missing input.
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use serde_json::Value;

#[test]
fn file_payload_cardinality_keeps_unresolved_boundary_and_relation_ownership() -> Result<(), String>
{
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/file_payload_cardinality");
    let report = check_workspace(CheckInput {
        root: root.join("input"),
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let json: Value = serde_json::from_str(&render_check(&report, &OutputFormat::Json)?)
        .map_err(|error| error.to_string())?;
    let findings = json["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(findings.len(), 1);
    let finding = &findings[0];
    assert_eq!(finding["probe"]["family"], "predicate");
    assert_eq!(
        finding["probe"]["expression"],
        "entries.len() + 1 > ENTRY_CAP"
    );
    assert_eq!(finding["classification"], "infection_unknown");
    assert_eq!(finding["ripr"]["infect"]["state"], "unknown");
    assert!(
        finding["ripr"]["infect"]["summary"]
            .as_str()
            .ok_or("missing boundary explanation")?
            .contains("local or computed value")
    );
    assert_eq!(
        finding["activation"]["missing_discriminators"],
        serde_json::json!([])
    );
    let rows = finding["related_tests"]
        .as_array()
        .ok_or("missing test rows")?;
    let caller = rows
        .iter()
        .find(|row| row["name"] == "file_payload_retains_128_and_rejects_129th")
        .ok_or("missing actual caller")?;
    assert_eq!(caller["relation_reason"], "direct_owner_call");
    assert_eq!(caller["oracle"], "assert_eq!(parse_file(&path), expected);");
    let unrelated = rows
        .iter()
        .find(|row| row["name"] == "unrelated_same_file_test")
        .ok_or("missing proximity control")?;
    assert_eq!(unrelated["relation_reason"], "same_test_file");
    assert_ne!(unrelated["relation_confidence"], "high");
    Ok(())
}
