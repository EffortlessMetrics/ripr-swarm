//! Agent-facing MCP prose must not leak internal issue or ADR references (#11570 item 7).

use super::gaps::{GapItem, RepairReadiness, test_finding};
use super::protocol::INSTRUCTIONS;
use std::path::Path;

pub(super) fn assert_agent_text_has_no_internal_issue_or_adr_references(
    text: &str,
) -> Result<(), String> {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'#' {
            let mut digits = 0usize;
            let mut end = index + 1;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                digits += 1;
                end += 1;
            }
            if digits >= 3 {
                return Err(format!(
                    "agent-facing text leaked an issue reference: {}",
                    &text[index..end]
                ));
            }
        }
        if text[index..].starts_with("ADR ") {
            let rest = &text[index + 4..];
            if rest
                .chars()
                .next()
                .is_some_and(|character| character.is_ascii_digit())
            {
                return Err(format!(
                    "agent-facing text leaked an ADR reference near: {}",
                    &text[index..index.saturating_add(12).min(text.len())]
                ));
            }
        }
        index += 1;
    }
    Ok(())
}

#[test]
fn instructions_contain_no_internal_issue_or_adr_references() -> Result<(), String> {
    assert_agent_text_has_no_internal_issue_or_adr_references(INSTRUCTIONS)
}

#[test]
fn repair_readiness_refusal_reasons_contain_no_internal_references() -> Result<(), String> {
    for ineligibility in [
        "not_candidate_actionable",
        "discriminator_not_populated_for_language",
    ] {
        let readiness = RepairReadiness {
            ready: false,
            fix_site: None,
            ineligibility: Some(ineligibility),
            seam_detail: None,
        };
        assert_agent_text_has_no_internal_issue_or_adr_references(readiness.reason())?;
    }
    Ok(())
}

#[test]
fn gap_repair_attempt_note_contains_no_internal_references() -> Result<(), String> {
    let finding = test_finding()?;
    let item = GapItem::from_finding(&finding, Path::new("."))?;
    let document = item.document("snapshot-test");
    let note = document
        .pointer("/item/links/repair_attempt_note")
        .and_then(|value| value.as_str())
        .ok_or("gap document lost repair_attempt_note")?;
    assert_agent_text_has_no_internal_issue_or_adr_references(note)
}
