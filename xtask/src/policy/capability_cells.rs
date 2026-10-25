//! Explicit identity join; product status and runtime semantic support retain
//! their separate authorities (#10065, bind-not-migrate decision in #9294).
use std::collections::BTreeSet;

use serde_json::Value;

use crate::Capability;

#[cfg(test)]
mod tests;

pub(crate) fn check_capability_cells(
    capabilities: &[Capability],
    violations: &mut Vec<String>,
) -> Result<(), String> {
    let args = [
        "run",
        "--locked",
        "-p",
        "ripr",
        "--quiet",
        "--",
        "capabilities",
        "--json",
    ]
    .map(String::from);
    let text = crate::run_output_owned_with_timeout(
        "cargo",
        &args,
        crate::tool_build_timeout()?,
        "check-capabilities runtime registry",
    )?;
    let registry: Value = serde_json::from_str(&text)
        .map_err(|err| format!("capability registry output is not JSON: {err}"))?;
    violations.extend(cell_join_violations(capabilities, &registry));
    Ok(())
}

fn cell_join_violations(capabilities: &[Capability], registry: &Value) -> Vec<String> {
    // A filtered or empty registry cannot establish completeness. Availability,
    // support state, tiers and capability names intentionally play no role.
    if registry["kind"] != "ripr_language_capabilities"
        || registry["schema_version"] != "0.1"
        || registry["digest_scope"] != "whole_table"
        || registry.get("filtered_by") != Some(&Value::Null)
    {
        return vec![
            "cell join requires an unfiltered whole-table capability registry (schema 0.1)"
                .to_string(),
        ];
    }
    let Some(cells) = registry["cells"]
        .as_array()
        .filter(|cells| !cells.is_empty())
    else {
        return vec!["cell join registry has no nonempty cells array".to_string()];
    };
    let mut violations = Vec::new();
    let mut registered = BTreeSet::new();
    for cell in cells {
        match cell["id"].as_str().filter(|id| !id.trim().is_empty()) {
            Some(id) => {
                if !registered.insert(id) {
                    violations.push(format!("duplicate registry cell identity `{id}`"));
                }
            }
            None => violations.push("registry cell is missing a nonempty identity".to_string()),
        }
    }
    let mut claimed = BTreeSet::new();
    for capability in capabilities {
        let owner = capability.id.as_deref().unwrap_or("<missing id>");
        let mut local = BTreeSet::new();
        for id in capability.cells.as_deref().unwrap_or_default() {
            if !local.insert(id.as_str()) {
                violations.push(format!("capability `{owner}` repeats cell `{id}`"));
            }
            if !registered.contains(id.as_str()) {
                violations.push(format!(
                    "capability `{owner}` claims missing registry cell `{id}`"
                ));
            }
            claimed.insert(id.as_str());
        }
    }
    for id in registered.difference(&claimed) {
        violations.push(format!(
            "registry cell `{id}` is unclaimed by metrics/capabilities.toml"
        ));
    }
    violations
}
