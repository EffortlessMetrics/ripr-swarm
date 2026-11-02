//! #1896: experimental riprAgent contract disposition ledger and public-claim guard.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

const LEDGER_PATH: &str = "policy/ripr-agent-disposition.toml";
const CAPABILITY_FIXTURE: &str = "fixtures/lsp_agent_protocol/capability.json";

fn repo_root() -> Result<std::path::PathBuf, String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| {
            format!(
                "failed to resolve repo root from {}",
                manifest_dir.display()
            )
        })
}

fn repo_path(relative: &str) -> Result<std::path::PathBuf, String> {
    Ok(repo_root()?.join(relative))
}

const ALLOWED_DISPOSITIONS: &[&str] = &[
    "internal_shared_type",
    "private_vscode_compatibility",
    "migrate_to_standard_lsp",
    "migrate_to_mcp",
    "historical_fixture_only",
    "retire",
    "experimental_not_public",
];

const FORBIDDEN_PUBLIC_CLAIMS: &[&str] = &[
    "generic headless client",
    "generic headless clients",
    "for generic clients",
    "for generic headless",
];

const SCANNED_PATHS: &[&str] = &[
    "schemas/ripr/ripr-agent-capability.schema.json",
    "schemas/ripr/ripr-agent-error.schema.json",
    "schemas/ripr/ripr-agent-request.schema.json",
    "schemas/ripr/ripr-agent-success.schema.json",
    "fixtures/lsp_agent_protocol/SPEC.md",
    "docs/specs/RIPR-SPEC-0131-ripr-agent-protocol-contract.md",
];

#[derive(Debug, Deserialize)]
struct Ledger {
    schema_version: String,
    #[serde(default)]
    element: Vec<Element>,
}

#[derive(Debug, Deserialize)]
struct Element {
    id: String,
    kind: String,
    disposition: String,
    evidence: String,
    #[serde(default)]
    #[allow(dead_code)]
    retirement: String,
}

pub(crate) fn check() -> Result<(), String> {
    let spec = crate::PolicyReportSpec {
        report_file: "ripr-agent-disposition.md",
        check: "check-ripr-agent-disposition",
        why_it_matters: "Experimental riprAgent contracts must stay dispositioned and must not read as generic third-party LSP agent support (#1896).",
        fix_kind: crate::FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Add or correct rows in policy/ripr-agent-disposition.toml for every contract element.",
            "Remove generic-agent support language from ripr-agent schemas and the lsp_agent_protocol fixture spec.",
            "Keep docs/verification/README.md ripr-agent rows marked experimental/private, not standard integration.",
        ],
        rerun_command: "cargo xtask check-ripr-agent-disposition",
        exception_template: None,
    };

    let mut violations = Vec::new();
    violations.extend(load_and_validate_ledger()?);
    violations.extend(check_forbidden_public_claims()?);
    violations.extend(check_verification_readme_fencing()?);
    crate::finish_policy_report(spec, &violations)
}

fn load_and_validate_ledger() -> Result<Vec<String>, String> {
    let path = repo_path(LEDGER_PATH)?;
    if !path.exists() {
        return Ok(vec![format!("{LEDGER_PATH} is missing")]);
    }
    let text = crate::read_text_lossy(&path)?;
    let ledger: Ledger =
        toml::from_str(&text).map_err(|error| format!("parse {LEDGER_PATH}: {error}"))?;
    if ledger.schema_version != "1" {
        return Ok(vec![format!(
            "{LEDGER_PATH} schema_version must be \"1\", got `{}`",
            ledger.schema_version
        )]);
    }

    let mut violations = Vec::new();
    let mut ids = BTreeSet::new();
    for element in &ledger.element {
        if !ids.insert(element.id.clone()) {
            violations.push(format!(
                "{LEDGER_PATH} duplicate element id `{}`",
                element.id
            ));
        }
        if element.id.trim().is_empty() {
            violations.push(format!("{LEDGER_PATH} element with empty id"));
        }
        if element.kind.trim().is_empty() {
            violations.push(format!(
                "{LEDGER_PATH} element `{}` missing kind",
                element.id
            ));
        }
        if element.evidence.trim().is_empty() {
            violations.push(format!(
                "{LEDGER_PATH} element `{}` missing evidence",
                element.id
            ));
        }
        if !ALLOWED_DISPOSITIONS.contains(&element.disposition.as_str()) {
            violations.push(format!(
                "{LEDGER_PATH} element `{}` has unknown disposition `{}`",
                element.id, element.disposition
            ));
        }
    }

    let capability_path = repo_path(CAPABILITY_FIXTURE)?;
    let capability_text = crate::read_text_lossy(&capability_path)?;
    let capability: serde_json::Value = serde_json::from_str(&capability_text)
        .map_err(|error| format!("decode {CAPABILITY_FIXTURE}: {error}"))?;

    let mut required = BTreeSet::new();
    required.insert("integration:standard_lsp".to_string());
    required.insert("integration:standard_mcp".to_string());
    required.insert("capability:experimental.riprAgent".to_string());
    required.insert("handler:ripr/listActionableItems".to_string());
    for key in ["reserved_requests", "supported_requests"] {
        if let Some(values) = capability.get(key).and_then(serde_json::Value::as_array) {
            for value in values {
                if let Some(name) = value.as_str() {
                    required.insert(format!("request:{name}"));
                }
            }
        }
    }
    for key in ["reserved_profiles", "supported_profiles"] {
        if let Some(values) = capability.get(key).and_then(serde_json::Value::as_array) {
            for value in values {
                if let Some(name) = value.as_str() {
                    required.insert(format!("profile:{name}"));
                }
            }
        }
    }
    if let Some(values) = capability
        .get("error_kinds")
        .and_then(serde_json::Value::as_array)
    {
        for value in values {
            if let Some(name) = value.as_str() {
                required.insert(format!("error:{name}"));
            }
        }
    }
    for schema in [
        "ripr-agent-capability",
        "ripr-agent-request",
        "ripr-agent-success",
        "ripr-agent-error",
    ] {
        required.insert(format!("schema:{schema}"));
    }
    required.insert("fixture_corpus:lsp_agent_protocol".to_string());
    required.insert("spec:RIPR-SPEC-0131".to_string());
    required.insert("shared_type:domain.CommandSpec".to_string());
    required.insert("shared_type:domain.identity.catalog".to_string());
    required.insert("dto:RiprAgentSuccessEnvelope".to_string());
    required.insert("dto:RiprAgentErrorEnvelope".to_string());
    required.insert("dto:RiprAgentRequestEnvelope".to_string());

    for id in &required {
        if !ids.contains(id) {
            violations.push(format!(
                "{LEDGER_PATH} missing required element `{id}` (derived from {CAPABILITY_FIXTURE} and #1896 inventory)"
            ));
        }
    }

    Ok(violations)
}

fn check_forbidden_public_claims() -> Result<Vec<String>, String> {
    let mut violations = Vec::new();
    for relative in SCANNED_PATHS {
        let path = repo_path(relative)?;
        if !path.exists() {
            violations.push(format!("scanned path missing: {relative}"));
            continue;
        }
        let text = crate::read_text_lossy(&path)?;
        let lower = text.to_ascii_lowercase();
        for phrase in FORBIDDEN_PUBLIC_CLAIMS {
            if lower.contains(phrase) {
                violations.push(format!(
                    "{relative} must not claim `{phrase}` public generic-agent support (#1896)"
                ));
            }
        }
    }
    Ok(violations)
}

fn check_verification_readme_fencing() -> Result<Vec<String>, String> {
    let path = repo_path("docs/verification/README.md")?;
    let mut violations = Vec::new();
    if !path.exists() {
        violations.push("docs/verification/README.md is missing".to_string());
        return Ok(violations);
    }
    let text = crate::read_text_lossy(&path)?;
    for schema in [
        "ripr-agent-capability.schema.json",
        "ripr-agent-request.schema.json",
        "ripr-agent-success.schema.json",
        "ripr-agent-error.schema.json",
    ] {
        if !text.contains(schema) {
            violations.push(format!("docs/verification/README.md must list `{schema}`"));
            continue;
        }
        let marker = "experimental/private riprAgent";
        if !text.contains(marker) {
            violations.push(format!(
                "docs/verification/README.md must fence `{schema}` as `{marker}` integration, not standard LSP/MCP (#1896)"
            ));
        }
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_covers_capability_fixture_inventory() -> Result<(), String> {
        let violations = load_and_validate_ledger()?;
        if !violations.is_empty() {
            return Err(violations.join("\n"));
        }
        Ok(())
    }

    #[test]
    fn scanned_surfaces_reject_generic_headless_claims() -> Result<(), String> {
        let violations = check_forbidden_public_claims()?;
        if !violations.is_empty() {
            return Err(violations.join("\n"));
        }
        Ok(())
    }
}
