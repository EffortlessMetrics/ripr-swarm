use crate::reports::pr_evidence_summary::io::file_state;
use crate::reports::pr_evidence_summary::model::JsonInput;
use crate::reports::pr_evidence_summary::util::{
    string_field, summary_bool, summary_string_or_null, summary_u64,
};
use serde_json::Value;
use std::path::Path;

pub(super) struct SummaryRenderInput<'a> {
    pub(super) repo: &'a Path,
    pub(super) pr_evidence_json: &'a str,
    pub(super) review_comments_json: &'a str,
    pub(super) pr_evidence_md: &'a str,
    pub(super) review_comments_md: &'a str,
    pub(super) pr_summary_md: &'a str,
    pub(super) pr_evidence: &'a JsonInput,
    pub(super) review_comments: &'a JsonInput,
}

pub(super) fn render_pr_evidence_summary(input: &SummaryRenderInput<'_>) -> String {
    let pr_value = input.pr_evidence.value.as_ref();
    let review_value = input.review_comments.value.as_ref();

    let mut out = String::new();
    out.push_str("# PR Evidence Summary\n\n");
    render_fast_gate(
        &mut out,
        pr_value,
        review_value,
        input.pr_evidence,
        input.review_comments,
    );
    render_ripr(&mut out, pr_value, review_value);
    render_targeted_mutation(&mut out, pr_value);
    render_artifacts(&mut out, input);
    out.push_str(
        "\n_This summary is generated from diff-scoped artifacts. Do not copy it into public badge state._\n",
    );
    out
}

fn render_fast_gate(
    out: &mut String,
    pr_value: Option<&Value>,
    review_value: Option<&Value>,
    pr_evidence: &JsonInput,
    review_comments: &JsonInput,
) {
    out.push_str("## Fast Gate\n\n");
    out.push_str(&format!("- PR evidence JSON: {}\n", pr_evidence.state));
    out.push_str(&format!(
        "- review guidance JSON: {}\n",
        review_comments.state
    ));
    out.push_str(&format!(
        "- PR evidence status: `{}`\n",
        string_field(pr_value, "status")
    ));
    out.push_str(&format!(
        "- review guidance status: `{}`\n",
        string_field(review_value, "status")
    ));
    out.push_str(&format!("- base: `{}`\n", string_field(pr_value, "base")));
    out.push_str(&format!("- head: `{}`\n", string_field(pr_value, "head")));
    out.push_str(&format!(
        "- changed files: {}\n\n",
        summary_u64(pr_value, "changed_files")
    ));
}

fn render_ripr(out: &mut String, pr_value: Option<&Value>, review_value: Option<&Value>) {
    out.push_str("## RIPR\n\n");
    out.push_str(&format!(
        "- changed-line comments: {}\n",
        summary_u64(review_value.or(pr_value), "comments")
    ));
    out.push_str(&format!(
        "- summary-only guidance: {}\n",
        summary_u64(review_value.or(pr_value), "summary_only")
    ));
    out.push_str(&format!(
        "- suppressed guidance: {}\n",
        summary_u64(review_value.or(pr_value), "suppressed")
    ));
    out.push_str(&format!(
        "- weakly_exposed: {}\n",
        summary_u64(pr_value, "weakly_exposed")
    ));
    out.push_str(&format!(
        "- reachable_unrevealed: {}\n",
        summary_u64(pr_value, "reachable_unrevealed")
    ));
    out.push_str(&format!(
        "- no_static_path: {}\n",
        summary_u64(pr_value, "no_static_path")
    ));
    out.push_str(&format!(
        "- severe gaps: {}\n\n",
        summary_u64(pr_value, "severe_gaps")
    ));
}

fn render_targeted_mutation(out: &mut String, pr_value: Option<&Value>) {
    out.push_str("## Targeted Mutation\n\n");
    out.push_str(&format!(
        "- requires_targeted_mutation: {}\n",
        summary_bool(pr_value, "requires_targeted_mutation")
    ));
    out.push_str(&format!(
        "- ripr_severe_gap: {}\n",
        summary_bool(pr_value, "ripr_severe_gap")
    ));
    out.push_str(&format!(
        "- routing_reason: `{}`\n\n",
        summary_string_or_null(pr_value, "routing_reason")
    ));
}

fn render_artifacts(out: &mut String, input: &SummaryRenderInput<'_>) {
    out.push_str("## Artifacts\n\n");
    out.push_str("| Artifact | Path | State |\n");
    out.push_str("| --- | --- | --- |\n");
    out.push_str(&format!(
        "| PR evidence JSON | `{}` | {} |\n",
        input.pr_evidence_json, input.pr_evidence.state
    ));
    out.push_str(&format!(
        "| PR evidence Markdown | `{}` | {} |\n",
        input.pr_evidence_md,
        file_state(input.repo, input.pr_evidence_md)
    ));
    out.push_str(&format!(
        "| Review guidance JSON | `{}` | {} |\n",
        input.review_comments_json, input.review_comments.state
    ));
    out.push_str(&format!(
        "| Review guidance Markdown | `{}` | {} |\n",
        input.review_comments_md,
        file_state(input.repo, input.review_comments_md)
    ));
    out.push_str(&format!(
        "| PR evidence summary Markdown | `{}` | generated |\n",
        input.pr_summary_md
    ));
}
