use super::FirstUsefulActionReport;
use crate::output::first_pr::{
    MANUAL_RECEIPT_LABEL, MANUAL_VERIFY_LABEL, RECEIPT_AFTER_VERIFY_LABEL,
    REPAIR_AFTER_PHASE_LABEL, REPAIR_AFTER_PHASE_STEP, VERIFY_AFTER_EDIT_LABEL,
};

/// Verify and receipt labels for a first-useful-action report (#3906).
///
/// A carried repair start makes verify and receipt the manual alternative to
/// the repair's after phase; without one they are steps that run after the
/// focused test edit.
struct ProofPathLabels {
    verify: &'static str,
    receipt: &'static str,
    verify_heading: &'static str,
    receipt_heading: &'static str,
}

impl ProofPathLabels {
    fn for_report(report: &FirstUsefulActionReport) -> Self {
        if report.commands.repair.is_some() {
            Self {
                verify: MANUAL_VERIFY_LABEL,
                receipt: MANUAL_RECEIPT_LABEL,
                verify_heading: "Manual Verify Without A Repair Attempt",
                receipt_heading: "Manual Receipt Without A Repair Attempt",
            }
        } else {
            Self {
                verify: VERIFY_AFTER_EDIT_LABEL,
                receipt: RECEIPT_AFTER_VERIFY_LABEL,
                verify_heading: "Verify After The Test Edit",
                receipt_heading: "Receipt After Verify",
            }
        }
    }
}

pub(crate) fn render_first_useful_action_markdown(report: &FirstUsefulActionReport) -> String {
    let mut out = String::new();
    out.push_str("# RIPR First Useful Action\n\n");
    out.push_str(&format!("Status: {}\n", report.status));
    out.push_str(&format!("Audience: {}\n", report.audience));
    out.push_str(&format!("Action: {}\n\n", report.action_kind));
    out.push_str("## Next\n\n");
    out.push_str(&format!("{}\n\n", with_period(&report.title)));

    if should_render_one_screen_recommendation(report) {
        render_one_screen_recommendation_markdown(report, &mut out);
    }

    if !report.why_first.is_empty() {
        out.push_str("## Why First\n\n");
        for reason in &report.why_first {
            push_wrapped_bullet(&mut out, reason);
        }
        out.push('\n');
    }

    if matches!(
        report.action_kind.as_str(),
        "write_focused_test" | "revise_focused_test"
    ) && let Some(target) = &report.target
    {
        out.push_str("## Where\n\n");
        out.push_str(&format!(
            "- File: `{}`\n",
            str_or(target.file.as_deref(), "unknown")
        ));
        out.push_str(&format!(
            "- Related test: `{}`\n",
            str_or(target.related_test.as_deref(), "unknown")
        ));
        out.push_str(&format!(
            "- Suggested test: `{}`\n\n",
            str_or(target.suggested_test_name.as_deref(), "unknown")
        ));
    }

    let labels = ProofPathLabels::for_report(report);
    if let Some(repair) = &report.commands.repair {
        out.push_str("## Start Repair\n\n");
        out.push_str(&format!("`{repair}`\n\n"));
        out.push_str(&format!(
            "{REPAIR_AFTER_PHASE_LABEL}: {REPAIR_AFTER_PHASE_STEP}\n\n"
        ));
    }

    if let Some(verify) = &report.commands.verify {
        out.push_str(&format!("## {}\n\n", labels.verify_heading));
        out.push_str(&format!("`{verify}`\n\n"));
    }

    if let Some(receipt) = &report.commands.receipt {
        out.push_str(&format!("## {}\n\n", labels.receipt_heading));
        out.push_str(&format!("`{receipt}`\n\n"));
    }

    if report.status != "actionable"
        && report.status != "unchanged_after_attempt"
        && let Some(fallback) = &report.fallback
    {
        out.push_str("## Fallback\n\n");
        if let Some(missing) = &fallback.missing {
            out.push_str("Missing required artifact:\n");
            out.push_str(&format!("`{missing}`\n\n"));
        } else if let Some(summary) = &fallback.summary {
            push_wrapped_paragraph(&mut out, summary);
            out.push('\n');
        }
    }

    if !report.limits.is_empty() {
        out.push_str("## Limits\n\n");
        for limit in &report.limits {
            push_wrapped_bullet(&mut out, limit);
        }
    }

    out
}

fn should_render_one_screen_recommendation(report: &FirstUsefulActionReport) -> bool {
    report.selected.is_some()
        || matches!(
            report.action_kind.as_str(),
            "write_focused_test" | "revise_focused_test" | "generate_missing_artifact"
        )
}

fn render_one_screen_recommendation_markdown(report: &FirstUsefulActionReport, out: &mut String) {
    let changed_behavior = if report.why.trim().is_empty() {
        "changed behavior unavailable"
    } else {
        report.why.trim()
    };
    let evidence_strength = report
        .selected
        .as_ref()
        .and_then(|selected| {
            selected
                .current_evidence_strength
                .as_deref()
                .or(selected.classification.as_deref())
        })
        .unwrap_or(report.status.as_str());
    let missing_discriminator = report
        .selected
        .as_ref()
        .and_then(|selected| selected.missing_discriminator.as_deref())
        .or_else(|| {
            report
                .target
                .as_ref()
                .and_then(|target| target.suggested_assertion.as_deref())
        })
        .unwrap_or("missing discriminator unavailable");
    let focused_proof_intent = report
        .target
        .as_ref()
        .and_then(|target| target.suggested_assertion.as_deref())
        .unwrap_or(report.title.as_str());
    let verify_command = report.commands.verify.as_deref().unwrap_or("not_available");
    let receipt_command = report
        .commands
        .receipt
        .as_deref()
        .unwrap_or("not_available");
    let artifacts = one_screen_artifacts(report);

    out.push_str("## One-Screen Recommendation\n\n");
    out.push_str(&format!("- Changed behavior: {changed_behavior}\n"));
    out.push_str(&format!(
        "- Current evidence strength: `{evidence_strength}`\n"
    ));
    out.push_str(&format!(
        "- Missing discriminator: {missing_discriminator}\n"
    ));
    out.push_str(&format!("- Focused proof intent: {focused_proof_intent}\n"));
    // #3906: a carried repair start leads the command lines; its after phase
    // runs verify and writes the receipt, so the low-level verify and
    // receipt commands become the manual alternative.
    let labels = ProofPathLabels::for_report(report);
    if let Some(repair) = &report.commands.repair {
        out.push_str(&format!("- Repair start: `{repair}`\n"));
        out.push_str(&format!(
            "- {REPAIR_AFTER_PHASE_LABEL}: {REPAIR_AFTER_PHASE_STEP}\n"
        ));
    }
    out.push_str(&format!("- {}: `{verify_command}`\n", labels.verify));
    out.push_str(&format!("- {}: `{receipt_command}`\n", labels.receipt));
    if !artifacts.is_empty() {
        let joined = artifacts
            .into_iter()
            .map(|artifact| format!("`{artifact}`"))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!("- Artifacts: {joined}\n"));
    }
    out.push_str(
        "- Boundary: static advisory evidence only; not runtime, coverage, mutation, or gate proof.\n\n",
    );
}

fn one_screen_artifacts(report: &FirstUsefulActionReport) -> Vec<&str> {
    let mut artifacts = Vec::new();
    if let Some(selected) = report.selected.as_ref() {
        push_unique_str(&mut artifacts, selected.source_artifact.as_str());
    }
    if let Some(path) = report.evidence.pr_guidance.as_deref() {
        push_unique_str(&mut artifacts, path);
    }
    if let Some(path) = report.evidence.assistant_proof.as_deref() {
        push_unique_str(&mut artifacts, path);
    }
    if let Some(path) = report.evidence.gap_ledger.as_deref() {
        push_unique_str(&mut artifacts, path);
    }
    if let Some(path) = report.evidence.ledger.as_deref() {
        push_unique_str(&mut artifacts, path);
    }
    if let Some(path) = report.evidence.receipt.as_deref() {
        push_unique_str(&mut artifacts, path);
    }
    artifacts
}

fn push_unique_str<'a>(items: &mut Vec<&'a str>, value: &'a str) {
    if !items.contains(&value) {
        items.push(value);
    }
}

pub(super) fn with_period(value: &str) -> String {
    if value.ends_with('.') {
        value.to_string()
    } else {
        format!("{value}.")
    }
}

pub(super) fn str_or<'a>(value: Option<&'a str>, fallback: &'static str) -> &'a str {
    match value {
        Some(value) => value,
        None => fallback,
    }
}

fn push_wrapped_bullet(out: &mut String, text: &str) {
    push_wrapped(out, "- ", "  ", &with_period(text), 79);
}

pub(super) fn push_wrapped_paragraph(out: &mut String, text: &str) {
    push_wrapped(out, "", "", &with_period(text), 79);
}

fn push_wrapped(
    out: &mut String,
    first_prefix: &str,
    continuation_prefix: &str,
    text: &str,
    width: usize,
) {
    let mut line = String::from(first_prefix);
    let mut first_word = true;
    for word in text.split_whitespace() {
        let separator = if first_word { "" } else { " " };
        if !first_word && line.len() + separator.len() + word.len() > width {
            out.push_str(&line);
            out.push('\n');
            line.clear();
            line.push_str(continuation_prefix);
            line.push_str(word);
        } else {
            line.push_str(separator);
            line.push_str(word);
        }
        first_word = false;
    }
    out.push_str(&line);

    out.push('\n');
}
