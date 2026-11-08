use super::*;
use serde_json::Value;

const FIXTURE_ROOT: &str = "fixtures/packaged_repair_parity";
const SCHEMA: &str = "packaged_repair_parity_fixture.v1";
const PARENT_MANIFEST: &str = "fixtures/blind_journey_installed_rust/manifest.json";

pub(crate) fn validate_packaged_repair_parity_fixture(
    violations: &mut Vec<String>,
) -> Result<(), String> {
    let workspace = Path::new(FIXTURE_ROOT);
    let manifest_path = workspace.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "packaged repair parity manifest cannot be read ({}): {error}",
            manifest_path.display()
        )
    })?;
    let manifest: Value = serde_json::from_str(&manifest_text)
        .map_err(|error| format!("packaged repair parity manifest is not JSON: {error}"))?;
    if json_string_field(&manifest, "schema_version").as_deref() != Some(SCHEMA) {
        violations.push(format!(
            "{FIXTURE_ROOT}/manifest.json schema_version must be {SCHEMA}"
        ));
    }
    if manifest.get("dogfood_denominator").and_then(Value::as_str) != Some("excluded") {
        violations.push(format!(
            "{FIXTURE_ROOT}/manifest.json must set dogfood_denominator=excluded"
        ));
    }
    let Some(parent) = manifest
        .pointer("/parent_fixture/manifest")
        .and_then(Value::as_str)
    else {
        violations.push(format!(
            "{FIXTURE_ROOT}/manifest.json must name parent_fixture.manifest"
        ));
        return Ok(());
    };
    if parent != PARENT_MANIFEST {
        violations.push(format!(
            "{FIXTURE_ROOT}/manifest.json parent_fixture.manifest must be {PARENT_MANIFEST}, got {parent}"
        ));
    }
    let spec = workspace.join("SPEC.md");
    if !spec.is_file() {
        violations.push(format!("{FIXTURE_ROOT} is missing SPEC.md"));
    }
    let skeleton = workspace.join("expected/skeleton-receipt.json");
    if !skeleton.is_file() {
        violations.push(format!(
            "{FIXTURE_ROOT} is missing expected/skeleton-receipt.json"
        ));
    } else if let Ok(text) = fs::read_to_string(&skeleton) {
        match crate::packaged_repair_receipt::parse_receipt(&text) {
            Ok(receipt) => {
                let gate = crate::packaged_repair_receipt::evaluate(
                    &crate::packaged_repair_receipt::GateInput {
                        receipt: &receipt,
                        expected_fixture:
                            &crate::packaged_repair_receipt::ExpectedFixtureIdentity {
                                base_commit: manifest
                                    .pointer("/repository/snapshots/base/commit")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                head_commit: manifest
                                    .pointer("/repository/snapshots/head/commit")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                canonical_gap_id: manifest
                                    .pointer("/canonical_transaction/canonical_gap_id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                allowed_edit_surface: string_array_field(
                                    &manifest,
                                    "/canonical_transaction/allowed_edit_surface",
                                ),
                                must_not_change: string_array_field(
                                    &manifest,
                                    "/canonical_transaction/must_not_change",
                                ),
                            },
                        require_full_qualification: false,
                    },
                );
                if gate.verdict != crate::packaged_repair_receipt::GateVerdict::Passed {
                    violations.push(format!(
                        "{FIXTURE_ROOT}/expected/skeleton-receipt.json gate failed: {:?}",
                        gate.failures
                    ));
                } else {
                    let markdown = crate::packaged_repair_receipt::render_gate_markdown(&gate);
                    if !markdown.contains("missing_evidence") {
                        violations.push(format!(
                            "{FIXTURE_ROOT}/expected/skeleton-receipt.json gate markdown missing closed state"
                        ));
                    }
                    if let Err(error) = crate::packaged_repair_receipt::render_gate_json(&gate) {
                        violations.push(format!(
                            "{FIXTURE_ROOT}/expected/skeleton-receipt.json gate JSON render failed: {error}"
                        ));
                    }
                    if let Err(error) =
                        crate::packaged_repair_receipt::render_receipt_json(&receipt)
                    {
                        violations.push(format!(
                            "{FIXTURE_ROOT}/expected/skeleton-receipt.json receipt JSON render failed: {error}"
                        ));
                    }
                }
            }
            Err(error) => violations.push(format!(
                "{FIXTURE_ROOT}/expected/skeleton-receipt.json fails typed parse: {error}"
            )),
        }
    }
    for channel in ["installed_cli", "packaged_vsix"] {
        if manifest.pointer(&format!("/channels/{channel}")).is_none() {
            violations.push(format!(
                "{FIXTURE_ROOT}/manifest.json must declare channels.{channel}"
            ));
        }
    }
    let axes = manifest
        .get("semantic_comparison_axes")
        .and_then(Value::as_array);
    if axes.is_none_or(|items| items.len() < 5) {
        violations.push(format!(
            "{FIXTURE_ROOT}/manifest.json semantic_comparison_axes must list the cross-channel axes"
        ));
    }
    Ok(())
}

fn string_array_field(manifest: &Value, pointer: &str) -> Option<Vec<String>> {
    manifest.pointer(pointer).and_then(|value| {
        value
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .filter(|items| !items.is_empty())
    })
}
