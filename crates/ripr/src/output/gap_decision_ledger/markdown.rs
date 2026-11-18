//! Markdown rendering of the gap decision ledger.
//!
//! Imported values (paths, owners, ids, commands, warnings) go through the
//! shared Markdown owner so the report shows them exactly: a code span keeps
//! the backticks and pipes of a command, and bare text cannot open raw HTML
//! or notify an account.

use super::{
    GapDecisionLedgerReport, GapRecord, eligible_projection_names, fallback_gap_id, non_empty,
};
use crate::agent::command_specs::{command_display_is_nonblank, command_displays_are_complete};
use crate::output::markdown::{code_span, inline_prose};
use serde_json::Value;

pub(crate) fn render_gap_decision_ledger_markdown(report: &GapDecisionLedgerReport) -> String {
    let mut out = String::new();
    out.push_str("# RIPR Gap Decision Ledger\n\n");
    out.push_str(&format!("Status: {}\n\n", md_code(&report.status)));
    out.push_str(&format!("Root: {}\n\n", md_code(&report.root)));
    if let Some(origin) = &report.check_output_provenance {
        let source_root = origin
            .get("root")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        let subject_limit = origin
            .get("source_subject_unavailable")
            .and_then(Value::as_str)
            .unwrap_or("not recorded");
        out.push_str(&format!(
            "Check-output producer root: {}; producer subject limitation: {}. These are provenance disclosures, not complete corpus currentness.\n\n",
            md_code(source_root), md_code(subject_limit)
        ));
    }
    out.push_str("Authority: gate-decision artifacts own pass/fail authority. This report is advisory projection input.\n\n");
    if let Some(outcome) = &report.analysis_outcome {
        let complete = outcome
            .get("analysis_complete")
            .and_then(Value::as_bool)
            .map_or_else(|| "not_available".to_string(), |value| value.to_string());
        let kind = outcome
            .pointer("/outcome/kind")
            .and_then(Value::as_str)
            .unwrap_or("not_available");
        out.push_str(&format!(
            "Analysis complete: {}; analysis outcome: {}\n\n",
            md_code(&complete),
            md_code(kind)
        ));
        // #9010: a budget-bounded check document says so beside the verdict.
        if outcome.get("document_complete").and_then(Value::as_bool) == Some(false) {
            out.push_str(
                "Document complete: `false` (findings array bounded by the byte budget)\n\n",
            );
        }
    }
    // #5203: a ledger derived from a bounded run says so in the human
    // surface too, naming each propagated run state.
    if let Some(limitations) = report.run_limitations.as_ref().and_then(Value::as_array) {
        let states = limitations
            .iter()
            .filter_map(|entry| {
                entry
                    .get("run_status")
                    .and_then(Value::as_str)
                    .or_else(|| entry.get("category").and_then(Value::as_str))
            })
            .collect::<Vec<_>>();
        if !states.is_empty() {
            out.push_str(&format!(
                "Run limitations (propagated from the source run): {}\n\n",
                md_code(&states.join(", "))
            ));
        }
    }

    out.push_str("## Summary\n\n");
    out.push_str(&format!("- Records: `{}`\n", report.summary.records_total));
    out.push_str(&format!(
        "- Repairable: `{}`; static limitations: `{}`; no action: `{}`; missing artifacts: `{}`\n",
        report.summary.repairable_total,
        report.summary.static_limitation_total,
        report.summary.no_action_total,
        report.summary.missing_artifact_total
    ));
    out.push_str(&format!(
        "- Projections: PR comments=`{}`, gate candidates=`{}`, agent packets=`{}`\n",
        report.summary.projection_pr_comment_eligible,
        report.summary.projection_gate_candidate,
        report.summary.projection_agent_packet_eligible
    ));
    out.push_str(&format!(
        "- Badge targets: ripr 0=`{}`, ripr+=`{}`\n",
        report.summary.ripr_zero_target_count, report.summary.ripr_plus_target_count
    ));
    out.push_str(&format!(
        "- Receipts: improved=`{}`, unchanged_after_attempt=`{}`\n",
        report.summary.receipt_improved_total, report.summary.receipt_unchanged_after_attempt_total
    ));
    out.push_str(&format!(
        "- Output-contract gaps: `{}`; preview ineligible: `{}`\n\n",
        report.summary.missing_output_contract_total, report.summary.preview_ineligible_total
    ));

    if !report.warnings.is_empty() {
        out.push_str("## Warnings\n\n");
        for warning in &report.warnings {
            out.push_str(&format!("- {}\n", md_inline(warning)));
        }
        out.push('\n');
    }

    out.push_str("## Records\n\n");
    if report.records.is_empty() {
        // #5264: name the two different empty states. `blocked` means the
        // input could not be read; `no_records` means the input parsed but
        // nothing projected to gap records (the warnings section above names
        // the route coverage when a valid check output carried findings).
        if report.status == "blocked" {
            out.push_str(
                "No gap records could be read from the input; see the warnings above.\n\n",
            );
        } else {
            out.push_str(
                "No gap records were derived from the input; see the warnings above for the reason.\n\n",
            );
        }
    } else {
        for record in &report.records {
            render_record_markdown(record, &mut out);
        }
    }

    out.push_str("## Limits\n\n");
    for limit in &report.limits {
        out.push_str(&format!("- {}\n", md_inline(limit)));
    }
    out
}

pub(super) fn render_record_markdown(record: &GapRecord, out: &mut String) {
    out.push_str(&format!("### {}\n\n", md_code(&fallback_gap_id(record))));
    out.push_str(&format!(
        "- Kind: {}; scope: {}; policy: {}; repairability: {}\n",
        md_code(&record.kind),
        md_code(&record.scope),
        md_code(&record.policy_state),
        md_code(&record.repairability)
    ));
    out.push_str(&format!(
        "- Evidence: {} / {}; language: {} / {}\n",
        md_code(&record.evidence_class),
        md_code(&record.gap_state),
        md_code(&record.language),
        md_code(&record.language_status)
    ));
    if let Some(anchor) = &record.anchor {
        out.push_str(&format!(
            "- Anchor: {}{}{}\n",
            md_code(anchor.file.as_deref().unwrap_or("unknown")),
            anchor
                .line
                .map(|line| format!(":{line}"))
                .unwrap_or_default(),
            anchor
                .owner
                .as_ref()
                .map(|owner| format!(" owner {}", md_code(owner)))
                .unwrap_or_default()
        ));
    }
    if let Some(seam_id) = record.seam_id.as_deref().and_then(non_empty) {
        out.push_str(&format!("- Seam: {}\n", md_code(seam_id)));
    }
    if let Some(route) = &record.repair_route {
        out.push_str(&format!(
            "- Repair: {}{}\n",
            md_code(&route.route_kind),
            route
                .target_file
                .as_ref()
                .map(|target| format!(" in {}", md_code(target)))
                .unwrap_or_default()
        ));
        if let Some(assertion) = &route.assertion_shape {
            out.push_str(&format!(
                "- Assertion or observer: {}\n",
                md_code(assertion)
            ));
        }
        if let Some(command) = route.inspection_command.as_deref().and_then(non_empty) {
            out.push_str(&format!("- Inspect: {}\n", md_code(command)));
        }
    }
    let eligible = eligible_projection_names(record);
    if !eligible.is_empty() {
        out.push_str(&format!(
            "- Eligible projections: `{}`\n",
            eligible.join("`, `")
        ));
    }
    if command_displays_are_complete(&record.verification_commands) {
        out.push_str("- Verify:\n");
        for command in &record.verification_commands {
            out.push_str(&format!("  - {}\n", md_code(command)));
        }
    } else if !record.verification_commands.is_empty() {
        out.push_str("- Verify: `unavailable_incomplete_command_list`\n");
    }
    if !record.regeneration_commands.is_empty() {
        out.push_str("- Regenerate:\n");
        for command in &record.regeneration_commands {
            out.push_str(&format!("  - {}\n", md_code(command)));
        }
    }
    if let Some(command) = record
        .receipt_command
        .as_deref()
        .filter(|command| command_display_is_nonblank(command))
    {
        out.push_str("- Receipt:\n");
        out.push_str(&format!("  - {}\n", md_code(command)));
    }
    if let Some(receipt) = &record.receipt {
        out.push_str(&format!(
            "- Receipt movement: {}\n",
            md_code(receipt.movement.as_deref().unwrap_or("unknown"))
        ));
    }
    out.push('\n');
}

/// Imported text in a bare list item or sentence: `<` cannot open raw HTML,
/// `@name` cannot notify an account and a line ending cannot start a block.
fn md_inline(value: &str) -> String {
    inline_prose(value)
}

/// Imported value as an inline code span that reads back as exactly `value`
/// (line endings become spaces). The report has no table, so `|` stays bare.
fn md_code(value: &str) -> String {
    code_span(value)
}

#[cfg(test)]
mod tests {
    use super::super::{
        GapAnchor, GapDecisionLedgerInput, GapDecisionLedgerSourceKind, GapRecord, GapRepairRoute,
        build_gap_decision_ledger_report,
    };
    use super::{render_gap_decision_ledger_markdown, render_record_markdown};
    use pulldown_cmark::{Event, Parser};

    /// Every code span and all plain text of `markdown` as a CommonMark
    /// renderer shows them. Raw HTML anywhere is an error.
    fn rendered(markdown: &str) -> Result<(Vec<String>, String), String> {
        let mut spans = Vec::new();
        let mut text = String::new();
        for event in Parser::new(markdown) {
            match event {
                Event::Code(code) => spans.push(code.to_string()),
                Event::Text(part) => text.push_str(&part),
                Event::Html(html) | Event::InlineHtml(html) => {
                    return Err(format!("raw HTML reached the report: {html}"));
                }
                _ => {}
            }
        }
        Ok((spans, text))
    }

    #[test]
    fn record_markdown_shows_commands_and_anchors_exactly() -> Result<(), String> {
        // A pipe, a backtick and a generic: the shapes the report used to
        // rewrite (`\|`, an apostrophe) so a copied command was not the
        // command in the ledger.
        let verify = "cargo test -p demo -- --exact 'a`b' | tee out.log";
        let inspect = "ripr explain --seam \"Vec<T>|x\"";
        let owner = "Stack<T>::push`1";
        let record = GapRecord {
            gap_id: "gap:`x`|y".to_string(),
            kind: "MissingBoundaryAssertion".to_string(),
            anchor: Some(GapAnchor {
                file: Some("app/@modal/__tests__/page.tsx".to_string()),
                line: Some(7),
                owner: Some(owner.to_string()),
                ..GapAnchor::default()
            }),
            repair_route: Some(GapRepairRoute {
                route_kind: "AddBoundaryAssertion".to_string(),
                inspection_command: Some(inspect.to_string()),
                ..GapRepairRoute::default()
            }),
            verification_commands: vec![verify.to_string()],
            ..GapRecord::default()
        };
        let mut out = String::new();
        render_record_markdown(&record, &mut out);
        let (spans, _) = rendered(&out)?;
        for expected in [
            "gap:`x`|y",
            "app/@modal/__tests__/page.tsx",
            owner,
            inspect,
            verify,
        ] {
            assert!(
                spans.iter().any(|span| span == expected),
                "no code span reads back as {expected:?}: {spans:?}\n{out}"
            );
        }
        Ok(())
    }

    #[test]
    fn ordinary_record_markdown_keeps_its_bytes() {
        let record = GapRecord {
            gap_id: "gap:plain".to_string(),
            kind: "MissingBoundaryAssertion".to_string(),
            scope: "repo_scoped".to_string(),
            anchor: Some(GapAnchor {
                file: Some("src/lib.rs".to_string()),
                line: Some(3),
                owner: Some("demo::run".to_string()),
                ..GapAnchor::default()
            }),
            verification_commands: vec!["cargo test -p demo".to_string()],
            ..GapRecord::default()
        };
        let mut out = String::new();
        render_record_markdown(&record, &mut out);
        assert!(out.starts_with("### `gap:plain`\n\n"), "{out}");
        assert!(
            out.contains("- Anchor: `src/lib.rs`:3 owner `demo::run`\n"),
            "{out}"
        );
        assert!(
            out.contains("- Verify:\n  - `cargo test -p demo`\n"),
            "{out}"
        );
    }

    #[test]
    fn ledger_warnings_cannot_open_html_or_notify() -> Result<(), String> {
        let report = build_gap_decision_ledger_report(GapDecisionLedgerInput {
            root: ".".to_string(),
            generated_at: "test".to_string(),
            source_kind: GapDecisionLedgerSourceKind::Records,
            records_path: "records.json".to_string(),
            records_json: Err("could not read <b>records</b> for @reviewer".to_string()),
        });
        let markdown = render_gap_decision_ledger_markdown(&report);
        let (_, text) = rendered(&markdown)?;
        assert!(
            text.contains("could not read <b>records</b> for @\u{2060}reviewer"),
            "warning text is not shown literally:\n{markdown}"
        );
        assert!(!markdown.contains("@reviewer"), "live mention:\n{markdown}");
        Ok(())
    }
}
