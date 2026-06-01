use super::access::{str_or, with_period};
use super::{
    ActionCommands, ActionEvidence, ActionFallback, ActionInputs, ActionSelected, ActionTarget,
    FirstUsefulActionReport, REPORT_KIND, SCHEMA_VERSION, push_wrapped_bullet,
    push_wrapped_paragraph,
};
use serde::Serialize;

pub(crate) fn render_first_useful_action_json(
    report: &FirstUsefulActionReport,
) -> Result<String, String> {
    #[derive(Serialize)]
    struct JsonReport<'a> {
        schema_version: &'static str,
        tool: &'static str,
        kind: &'static str,
        status: &'a str,
        audience: &'a str,
        action_kind: &'a str,
        root: &'a str,
        generated_at: &'a str,
        inputs: &'a ActionInputs,
        selected: &'a Option<ActionSelected>,
        title: &'a str,
        why: &'a str,
        why_first: &'a [String],
        target: &'a Option<ActionTarget>,
        commands: &'a ActionCommands,
        evidence: &'a ActionEvidence,
        fallback: &'a Option<ActionFallback>,
        warnings: &'a [String],
        limits: &'a [String],
    }

    serde_json::to_string_pretty(&JsonReport {
        schema_version: SCHEMA_VERSION,
        tool: "ripr",
        kind: REPORT_KIND,
        status: &report.status,
        audience: &report.audience,
        action_kind: &report.action_kind,
        root: &report.root,
        generated_at: &report.generated_at,
        inputs: &report.inputs,
        selected: &report.selected,
        title: &report.title,
        why: &report.why,
        why_first: &report.why_first,
        target: &report.target,
        commands: &report.commands,
        evidence: &report.evidence,
        fallback: &report.fallback,
        warnings: &report.warnings,
        limits: &report.limits,
    })
    .map_err(|err| format!("render first useful action JSON failed: {err}"))
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

    if let Some(verify) = &report.commands.verify {
        out.push_str("## Verify\n\n");
        out.push_str(&format!("`{verify}`\n\n"));
    }

    if let Some(receipt) = &report.commands.receipt {
        out.push_str("## Receipt\n\n");
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
    out.push_str(&format!("- Verify command: `{verify_command}`\n"));
    out.push_str(&format!("- Receipt command: `{receipt_command}`\n"));
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
