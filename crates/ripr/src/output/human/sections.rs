use crate::config::RiprConfig;
use crate::domain::{Finding, LanguageId, LanguageStatus};

use super::evidence_lines::{evidence_path_lines, weakness_lines};

pub(crate) fn render_finding_with_config(finding: &Finding, config: &RiprConfig) -> String {
    let mut out = String::new();
    let severity = config.severity().for_exposure(&finding.class).as_str();
    out.push_str(&format!(
        "{} {}:{}\n",
        severity.to_ascii_uppercase(),
        finding.probe.location.file.display(),
        finding.probe.location.line
    ));

    out.push_str("\nChanged\n");
    if let Some(before) = &finding.probe.before {
        out.push_str(&format!("  before: {before}\n"));
    }
    if let Some(after) = &finding.probe.after {
        out.push_str(&format!("  after:  {after}\n"));
    } else {
        out.push_str(&format!("  expr:   {}\n", finding.probe.expression));
    }

    out.push_str("\nProbe\n");
    out.push_str(&format!(
        "  family: {}\n  delta:  {}\n",
        finding.probe.family.as_str(),
        finding.probe.delta.as_str()
    ));
    if let Some(owner) = &finding.probe.owner {
        out.push_str(&format!("  owner:  {owner}\n"));
    }
    if let Some(gap) = &finding.canonical_gap {
        out.push_str(&format!("  canonical gap: {}\n", gap.id));
    }

    if should_render_language_metadata(finding) {
        out.push_str("\nLanguage\n");
        if let Some(language) = finding.language {
            out.push_str(&format!("  language: {}\n", language.as_str()));
        }
        if let Some(status) = finding.language_status {
            out.push_str(&format!("  status: {}\n", status.as_str()));
        }
        if let Some(owner_kind) = finding.owner_kind {
            out.push_str(&format!("  owner kind: {}\n", owner_kind.as_str()));
        }
    }

    out.push_str("\nStatic exposure\n");
    out.push_str(&format!(
        "  {} ({}, confidence {:.2})\n",
        finding.class.as_str(),
        severity,
        finding.confidence
    ));

    out.push_str("\nEvidence\n");
    for line in evidence_path_lines(finding) {
        out.push_str(&format!("  - {line}\n"));
    }

    let weakness = weakness_lines(finding);
    if !weakness.is_empty() {
        out.push_str("\nWeakness\n");
        for line in weakness {
            out.push_str(&format!("  - {line}\n"));
        }
    }

    let stop_reasons = finding.effective_stop_reasons();
    if !stop_reasons.is_empty() {
        out.push_str("\nStop reasons:\n");
        for reason in &stop_reasons {
            out.push_str(&format!("  - {}\n", reason.as_str()));
        }
    }

    if let Some(placement) = repair_placement_from_evidence(finding) {
        out.push_str("\nRepair placement\n");
        out.push_str(&format!("  suggested file: {}\n", placement.test_file));
        out.push_str(&format!("  suggested test: {}\n", placement.test_name));
        if let Some(node_id) = placement.test_node_id {
            out.push_str(&format!("  pytest node: {node_id}\n"));
        }
        out.push_str(&format!(
            "  verify: {} ({})\n",
            placement.verify_command, placement.verify_confidence
        ));
    }

    if let Some(step) = &finding.recommended_next_step {
        out.push_str("\nNext step\n");
        out.push_str(&format!("  {step}\n"));
    }

    out
}

struct RepairPlacement<'a> {
    test_file: &'a str,
    test_name: &'a str,
    test_node_id: Option<&'a str>,
    verify_command: &'a str,
    verify_confidence: &'a str,
}

fn repair_placement_from_evidence(finding: &Finding) -> Option<RepairPlacement<'_>> {
    Some(RepairPlacement {
        test_file: evidence_value(finding, "suggested_test_file: ")?,
        test_name: evidence_value(finding, "suggested_test_name: ")?,
        test_node_id: evidence_value(finding, "suggested_test_node_id: "),
        verify_command: evidence_value(finding, "suggested_verify_command: ")?,
        verify_confidence: evidence_value(finding, "suggested_verify_command_confidence: ")?,
    })
}

fn evidence_value<'a>(finding: &'a Finding, prefix: &str) -> Option<&'a str> {
    finding
        .evidence
        .iter()
        .find_map(|entry| entry.strip_prefix(prefix))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn should_render_language_metadata(finding: &Finding) -> bool {
    finding
        .language
        .is_some_and(|language| language != LanguageId::Rust)
        || finding
            .language_status
            .is_some_and(|status| status != LanguageStatus::Stable)
        || finding.owner_kind.is_some()
}
