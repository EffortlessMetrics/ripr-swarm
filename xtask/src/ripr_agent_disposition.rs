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
    "generic lsp client can",
    "generic lsp clients can",
    "generic lsp clients may",
    "generic lsp clients support",
    "any lsp client can",
    "any headless client can",
];

/// Scanned surfaces that must carry the private boundary, so the scan has a
/// positive claim boundary and not only a phrase blocklist.
const BOUNDARY_PATHS: &[(&str, &str)] = &[
    (
        "schemas/ripr/ripr-agent-capability.schema.json",
        "not generic third-party lsp client support",
    ),
    (
        "schemas/ripr/ripr-agent-error.schema.json",
        "not standard lsp/mcp integration",
    ),
    (
        "docs/specs/RIPR-SPEC-0131-ripr-agent-protocol-contract.md",
        "must not treat",
    ),
];

/// Fixed inventory: element id to the one disposition it must carry.
const EXPECTED_DISPOSITIONS: &[(&str, &str)] = &[
    ("integration:standard_lsp", "migrate_to_standard_lsp"),
    ("integration:standard_mcp", "migrate_to_mcp"),
    (
        "capability:experimental.riprAgent",
        "private_vscode_compatibility",
    ),
    (
        "handler:ripr/listActionableItems",
        "private_vscode_compatibility",
    ),
    ("schema:ripr-agent-capability", "experimental_not_public"),
    ("schema:ripr-agent-request", "experimental_not_public"),
    ("schema:ripr-agent-success", "experimental_not_public"),
    ("schema:ripr-agent-error", "experimental_not_public"),
    (
        "fixture_corpus:lsp_agent_protocol",
        "historical_fixture_only",
    ),
    ("spec:RIPR-SPEC-0131", "experimental_not_public"),
    ("shared_type:domain.CommandSpec", "internal_shared_type"),
    (
        "shared_type:domain.identity.catalog",
        "internal_shared_type",
    ),
    (
        "dto:RiprAgentSuccessEnvelope",
        "private_vscode_compatibility",
    ),
    ("dto:RiprAgentErrorEnvelope", "private_vscode_compatibility"),
    (
        "dto:RiprAgentRequestEnvelope",
        "private_vscode_compatibility",
    ),
];

const README_PATH: &str = "docs/verification/README.md";
const README_MARKER: &str = "experimental/private riprAgent";
const README_SCHEMAS: &[&str] = &[
    "ripr-agent-capability.schema.json",
    "ripr-agent-request.schema.json",
    "ripr-agent-success.schema.json",
    "ripr-agent-error.schema.json",
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
}

pub(crate) fn check() -> Result<(), String> {
    let spec = crate::PolicyReportSpec {
        report_file: "ripr-agent-disposition.md",
        check: "check-ripr-agent-disposition",
        why_it_matters: "Experimental riprAgent contracts must stay dispositioned, fenced as private, and must not use the enumerated generic-client support phrases (#1896).",
        fix_kind: crate::FixKind::AuthorDecisionRequired,
        recommended_fixes: &[
            "Add or correct rows in policy/ripr-agent-disposition.toml for every contract element.",
            "Remove the enumerated generic-client support phrases from ripr-agent schemas, the lsp_agent_protocol fixture spec and RIPR-SPEC-0131.",
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
    validate_ledger_text(&text)
}

fn validate_ledger_text(text: &str) -> Result<Vec<String>, String> {
    let ledger: Ledger =
        toml::from_str(text).map_err(|error| format!("parse {LEDGER_PATH}: {error}"))?;
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
    for (id, expected) in EXPECTED_DISPOSITIONS {
        if let Some(element) = ledger.element.iter().find(|element| element.id == *id)
            && element.disposition != *expected
        {
            violations.push(format!(
                "{LEDGER_PATH} element `{id}` must carry disposition `{expected}`, got `{}`",
                element.disposition
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
        violations.extend(claim_violations(relative, &text));
    }
    for (relative, boundary) in BOUNDARY_PATHS {
        let path = repo_path(relative)?;
        if !path.exists() {
            continue;
        }
        let text = crate::read_text_lossy(&path)?;
        violations.extend(boundary_violations(relative, boundary, &text));
    }
    Ok(violations)
}

fn claim_violations(relative: &str, text: &str) -> Vec<String> {
    // Normalize whitespace so a phrase hard-wrapped across lines is still caught.
    let lower = text
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    FORBIDDEN_PUBLIC_CLAIMS
        .iter()
        .filter(|phrase| lower.contains(*phrase))
        .map(|phrase| {
            format!("{relative} must not claim `{phrase}` public generic-agent support (#1896)")
        })
        .collect()
}

fn boundary_violations(relative: &str, boundary: &str, text: &str) -> Vec<String> {
    // Normalize whitespace so a wrapped sentence still satisfies the boundary.
    let normalized = text
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.contains(boundary) {
        Vec::new()
    } else {
        vec![format!(
            "{relative} must keep the private-boundary statement `{boundary}` (#1896)"
        )]
    }
}

fn check_verification_readme_fencing() -> Result<Vec<String>, String> {
    let path = repo_path(README_PATH)?;
    if !path.exists() {
        return Ok(vec![format!("{README_PATH} is missing")]);
    }
    let text = crate::read_text_lossy(&path)?;
    Ok(readme_fencing_violations(&text))
}

/// Each ripr-agent schema row must itself carry the private marker; a marker
/// on a sibling row or elsewhere in the document does not count.
fn readme_fencing_violations(text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for schema in README_SCHEMAS {
        let rows: Vec<&str> = text
            .lines()
            .filter(|line| line.trim_start().starts_with('|') && line.contains(schema))
            .collect();
        if rows.is_empty() {
            violations.push(format!("{README_PATH} must list `{schema}`"));
            continue;
        }
        if !rows.iter().all(|row| row.contains(README_MARKER)) {
            violations.push(format!(
                "{README_PATH} row for `{schema}` must fence it as `{README_MARKER}` integration, not standard LSP/MCP (#1896)"
            ));
        }
    }
    violations
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

    fn ledger_with_disposition(id: &str, replacement: &str) -> Result<String, String> {
        let text = crate::read_text_lossy(&repo_path(LEDGER_PATH)?)?;
        let marker = format!("id = \"{id}\"");
        let start = text
            .find(&marker)
            .ok_or_else(|| format!("fixture ledger lacks {id}"))?;
        let rest = &text[start..];
        let line_start = rest
            .find("disposition = \"")
            .ok_or_else(|| format!("no disposition after {id}"))?;
        let line_end = rest[line_start..]
            .find('\n')
            .ok_or_else(|| "unterminated disposition line".to_string())?;
        let mut out = text[..start].to_string();
        out.push_str(&rest[..line_start]);
        out.push_str(&format!("disposition = \"{replacement}\""));
        out.push_str(&rest[line_start + line_end..]);
        Ok(out)
    }

    #[test]
    fn ledger_rejects_disposition_not_bound_to_its_id() -> Result<(), String> {
        let mutated = ledger_with_disposition(
            "capability:experimental.riprAgent",
            "migrate_to_standard_lsp",
        )?;
        let violations = validate_ledger_text(&mutated)?;
        if violations.iter().any(|v| {
            v.contains("capability:experimental.riprAgent")
                && v.contains("must carry disposition `private_vscode_compatibility`")
        }) {
            Ok(())
        } else {
            Err(format!("expected binding violation, got {violations:?}"))
        }
    }

    #[test]
    fn generic_lsp_client_wording_is_rejected() {
        let violations = claim_violations(
            "schemas/ripr/ripr-agent-capability.schema.json",
            "Generic LSP clients can use riprAgent",
        );
        assert_eq!(violations.len(), 1, "{violations:?}");
    }

    #[test]
    fn hard_wrapped_generic_headless_claim_is_rejected() {
        let violations = claim_violations("x", "wire contract for generic headless\nclients");
        assert!(!violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn negated_third_party_boundary_text_is_not_a_claim() {
        let violations = claim_violations(
            "x",
            "Not generic third-party LSP client support. Third-party headless clients must not treat this as standard.",
        );
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn missing_private_boundary_statement_is_rejected() {
        assert!(boundary_violations("x", "must not treat", "Anything goes").len() == 1);
        assert!(boundary_violations("x", "must not treat", "It must\nnot   treat it").is_empty());
    }

    fn readme_rows(markers: [bool; 4]) -> String {
        README_SCHEMAS
            .iter()
            .zip(markers)
            .map(|(schema, marked)| {
                let note = if marked {
                    "experimental/private riprAgent envelope"
                } else {
                    "envelope"
                };
                format!("| [`schemas/ripr/{schema}`](x) | {note}. |\n")
            })
            .collect()
    }

    #[test]
    fn readme_fencing_accepts_all_rows_marked() {
        assert!(readme_fencing_violations(&readme_rows([true; 4])).is_empty());
    }

    #[test]
    fn readme_fencing_rejects_one_unmarked_row_when_others_are_marked() {
        let violations = readme_fencing_violations(&readme_rows([true, false, true, true]));
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("ripr-agent-request.schema.json"));
    }
}
