use super::build::targeted_test_outcome_gap_summary;
use super::*;

pub(crate) fn render_targeted_test_outcome_md(report: &TargetedTestOutcomeReport) -> String {
    let mut out = String::new();
    out.push_str("# ripr targeted-test outcome report\n\n");
    out.push_str("Status: advisory\n\n");
    out.push_str("Inputs:\n");
    out.push_str(&format!("- before: `{}`\n", md_escape(&report.before_path)));
    out.push_str(&format!("- after: `{}`\n\n", md_escape(&report.after_path)));

    out.push_str("## Summary\n\n");
    out.push_str("| Bucket | Count |\n| --- | ---: |\n");
    out.push_str(&format!("| moved | {} |\n", report.moved.len()));
    out.push_str(&format!("| unchanged | {} |\n", report.unchanged.len()));
    out.push_str(&format!("| regressed | {} |\n", report.regressed.len()));
    out.push_str(&format!("| new | {} |\n", report.new.len()));
    out.push_str(&format!("| removed | {} |\n", report.removed.len()));

    push_targeted_outcome_gap_summary_md(&mut out, report);

    out.push_str("\n## Grip Counts\n\n");
    out.push_str("| Class | Before | After |\n| --- | ---: | ---: |\n");
    for class in std::iter::once("seams_total").chain(SEAM_GRIP_CLASS_ORDER.iter().copied()) {
        out.push_str(&format!(
            "| {} | {} | {} |\n",
            class,
            count_for_class(&report.before_counts, class),
            count_for_class(&report.after_counts, class)
        ));
    }

    push_targeted_outcome_movements_md(&mut out, "Moved", &report.moved);
    push_targeted_outcome_movements_md(&mut out, "Unchanged", &report.unchanged);
    push_targeted_outcome_movements_md(&mut out, "Regressed", &report.regressed);
    push_targeted_outcome_seams_md(&mut out, "New", &report.new);
    push_targeted_outcome_seams_md(&mut out, "Removed", &report.removed);
    push_targeted_outcome_review_receipt_md(&mut out, report);
    out.push_str(
        "\nThis report compares two static repo-exposure snapshots. It is advisory and does not run mutation testing.\n",
    );
    out
}

fn count_for_class(counts: &BTreeMap<String, usize>, class: &str) -> usize {
    match counts.get(class) {
        Some(count) => *count,
        None => 0,
    }
}
fn push_targeted_outcome_movements_md(
    out: &mut String,
    title: &str,
    movements: &[TargetedTestOutcomeMovement],
) {
    out.push_str(&format!("\n## {title}\n\n"));
    if movements.is_empty() {
        out.push_str("None.\n");
        return;
    }
    for movement in movements {
        out.push_str(&format!(
            "- `{}` {}:{} {} -> {} ({}; gap {})\n",
            md_escape(&movement.seam_id),
            md_escape(&movement.file),
            movement.line,
            movement.before,
            movement.after,
            movement.direction,
            movement.gap_movement
        ));
        for delta in &movement.evidence_delta {
            out.push_str(&format!("  - {}\n", md_escape(delta)));
        }
        if movement.evidence_delta.is_empty()
            && let Some(reason) = &movement.no_movement_reason
        {
            out.push_str(&format!("  - no movement: {}\n", md_escape(reason)));
        }
    }
}

fn push_targeted_outcome_review_receipt_md(out: &mut String, report: &TargetedTestOutcomeReport) {
    out.push_str("\n## Review Receipt\n\n");
    let gap_summary = [targeted_test_outcome_gap_summary_sentence(report)];
    push_review_receipt_list_md(out, "Gap movement summary", &gap_summary);
    push_review_receipt_list_md(out, "What changed?", &review_what_changed(report));
    push_review_receipt_list_md(
        out,
        "What RIPR flagged before?",
        &review_ripr_flagged_before(report),
    );
    push_review_receipt_list_md(
        out,
        "What focused proof changed?",
        &review_focused_proof_added(report),
    );
    push_review_receipt_list_md(
        out,
        "What moved after verification?",
        &review_movement_after_verification(report),
    );
    push_review_receipt_list_md(
        out,
        "What remains weak or unknown?",
        &review_remaining_weak_or_unknown(report),
    );
    push_review_receipt_list_md(
        out,
        "Reviewer should inspect",
        &review_should_inspect(report),
    );
    push_review_receipt_list_md(out, "Reviewer may believe", &reviewer_may_believe(report));
    push_review_receipt_list_md(
        out,
        "Reviewer should not believe",
        &reviewer_should_not_believe(),
    );
}

fn push_targeted_outcome_gap_summary_md(out: &mut String, report: &TargetedTestOutcomeReport) {
    let summary = targeted_test_outcome_gap_summary(report);
    out.push_str("\n## Gap Movement\n\n");
    out.push_str("| Movement | Count |\n| --- | ---: |\n");
    out.push_str(&format!("| closed | {} |\n", summary.closed));
    out.push_str(&format!("| opened | {} |\n", summary.opened));
    out.push_str(&format!("| strengthened | {} |\n", summary.strengthened));
    out.push_str(&format!("| weakened | {} |\n", summary.weakened));
    out.push_str(&format!("| unchanged | {} |\n", summary.unchanged));
    out.push_str(&format!("| new | {} |\n", summary.new));
    out.push_str(&format!("| removed | {} |\n", summary.removed));
    out.push_str(&format!("| changed | {} |\n", summary.changed));
}

fn push_review_receipt_list_md(out: &mut String, title: &str, items: &[String]) {
    out.push_str(&format!("### {title}\n\n"));
    for item in items {
        out.push_str(&format!("- {}\n", md_escape(item)));
    }
    out.push('\n');
}

fn push_targeted_outcome_seams_md(
    out: &mut String,
    title: &str,
    seams: &[TargetedTestOutcomeSeam],
) {
    out.push_str(&format!("\n## {title}\n\n"));
    if seams.is_empty() {
        out.push_str("None.\n");
        return;
    }
    for seam in seams {
        out.push_str(&format!(
            "- `{}` {}:{} {} ({})\n",
            md_escape(&seam.seam_id),
            md_escape(&seam.file),
            seam.line,
            seam.grip_class,
            seam.seam_kind
        ));
    }
}

pub(super) fn review_what_changed(report: &TargetedTestOutcomeReport) -> Vec<String> {
    vec![
        format!(
            "Compared before snapshot {} with after snapshot {}.",
            report.before_path, report.after_path
        ),
        format!(
            "Static seam movement: {} moved, {} unchanged, {} regressed, {} new, {} removed.",
            report.moved.len(),
            report.unchanged.len(),
            report.regressed.len(),
            report.new.len(),
            report.removed.len()
        ),
    ]
}

pub(super) fn review_ripr_flagged_before(report: &TargetedTestOutcomeReport) -> Vec<String> {
    let mut items = Vec::new();
    for movement in report
        .moved
        .iter()
        .chain(report.unchanged.iter())
        .chain(report.regressed.iter())
    {
        if review_attention_class(&movement.before) {
            items.push(format!(
                "{} before {} at {}:{}.",
                movement.before, movement.seam_kind, movement.file, movement.line
            ));
        }
    }
    for seam in &report.removed {
        if review_attention_class(&seam.grip_class) {
            items.push(format!(
                "{} before {} at {}:{} later disappeared from the after snapshot.",
                seam.grip_class, seam.seam_kind, seam.file, seam.line
            ));
        }
    }
    review_limit_or_default(
        items,
        "No before-snapshot weak or unknown seams were present in the compared artifacts.",
    )
}

pub(super) fn review_focused_proof_added(report: &TargetedTestOutcomeReport) -> Vec<String> {
    let mut items = Vec::new();
    for movement in report
        .moved
        .iter()
        .chain(report.unchanged.iter())
        .chain(report.regressed.iter())
    {
        let proof_deltas = movement
            .evidence_delta
            .iter()
            .filter(|delta| positive_proof_delta(delta))
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        if proof_deltas.is_empty() {
            continue;
        }
        items.push(format!(
            "{} at {}:{} shows static evidence movement for focused proof outside RIPR: {}.",
            movement.seam_kind,
            movement.file,
            movement.line,
            proof_deltas.join("; ")
        ));
    }
    review_limit_or_default(
        items,
        "No focused proof signal from a test or output proof outside RIPR was visible in the rendered static snapshots.",
    )
}

pub(super) fn review_movement_after_verification(
    report: &TargetedTestOutcomeReport,
) -> Vec<String> {
    let mut items = Vec::new();
    let improved = report
        .moved
        .iter()
        .filter(|movement| movement.direction == "improved")
        .count();
    let changed = report
        .moved
        .iter()
        .filter(|movement| movement.direction != "improved")
        .count();
    items.push(format!(
        "{} improved, {} changed without ranking higher, {} regressed, {} unchanged.",
        improved,
        changed,
        report.regressed.len(),
        report.unchanged.len()
    ));
    items.push(targeted_test_outcome_gap_summary_sentence(report));
    for movement in report.moved.iter().chain(report.regressed.iter()).take(4) {
        items.push(format!(
            "{} at {}:{} moved {} -> {} ({}).",
            movement.seam_kind,
            movement.file,
            movement.line,
            movement.before,
            movement.after,
            movement.direction
        ));
    }
    let unchanged_with_delta = report
        .unchanged
        .iter()
        .filter(|movement| !movement.evidence_delta.is_empty())
        .take(3)
        .map(|movement| {
            format!(
                "{} at {}:{} kept {} but evidence changed: {}.",
                movement.seam_kind,
                movement.file,
                movement.line,
                movement.after,
                movement.evidence_delta.join("; ")
            )
        });
    items.extend(unchanged_with_delta);
    items
}

fn targeted_test_outcome_gap_summary_sentence(report: &TargetedTestOutcomeReport) -> String {
    let summary = targeted_test_outcome_gap_summary(report);
    format!(
        "Gap movement: {} closed, {} opened, {} strengthened, {} weakened, {} unchanged, {} new, {} removed, {} changed.",
        summary.closed,
        summary.opened,
        summary.strengthened,
        summary.weakened,
        summary.unchanged,
        summary.new,
        summary.removed,
        summary.changed
    )
}

pub(super) fn review_remaining_weak_or_unknown(report: &TargetedTestOutcomeReport) -> Vec<String> {
    let mut items = Vec::new();
    for movement in report
        .moved
        .iter()
        .chain(report.unchanged.iter())
        .chain(report.regressed.iter())
    {
        if review_attention_class(&movement.after) {
            items.push(format!(
                "{} remains {} at {}:{}.",
                movement.seam_kind, movement.after, movement.file, movement.line
            ));
        }
    }
    for seam in &report.new {
        if review_attention_class(&seam.grip_class) {
            items.push(format!(
                "New {} is {} at {}:{}.",
                seam.seam_kind, seam.grip_class, seam.file, seam.line
            ));
        }
    }
    review_limit_or_default(
        items,
        "No weak or unknown after-snapshot seams were present in the compared artifacts.",
    )
}

pub(super) fn review_should_inspect(report: &TargetedTestOutcomeReport) -> Vec<String> {
    vec![
        format!(
            "Open the compared artifacts: {} and {}.",
            report.before_path, report.after_path
        ),
        "Inspect the focused test or output proof corresponding to each listed evidence delta."
            .to_string(),
        "Review remaining weak, unknown, new, or regressed seams before treating the repair loop as complete."
            .to_string(),
    ]
}

pub(super) fn reviewer_may_believe(report: &TargetedTestOutcomeReport) -> Vec<String> {
    let mut items = vec![format!(
        "RIPR compared only the listed static snapshots: {} and {}.",
        report.before_path, report.after_path
    )];
    let has_focused_proof_signal = report
        .moved
        .iter()
        .chain(report.unchanged.iter())
        .chain(report.regressed.iter())
        .any(|movement| {
            movement
                .evidence_delta
                .iter()
                .any(|delta| positive_proof_delta(delta))
        });
    if has_focused_proof_signal {
        items.push(
            "The listed focused-proof signals are static evidence visible after a test or output proof changed outside RIPR."
                .to_string(),
        );
    } else {
        items.push(
            "No focused-proof signal was visible; this receipt only records before/after static movement."
                .to_string(),
        );
    }
    items.push(
        "The movement and remaining-weak sections define the static claim boundary for this receipt."
            .to_string(),
    );
    items
}

pub(super) fn reviewer_should_not_believe() -> Vec<String> {
    vec![
        "Runtime mutation result.".to_string(),
        "Coverage adequacy.".to_string(),
        "General correctness.".to_string(),
        "Merge approval.".to_string(),
        "That RIPR edited source or generated tests.".to_string(),
    ]
}

fn review_attention_class(class: &str) -> bool {
    !matches!(class, "strongly_gripped" | "intentional" | "suppressed")
}

fn positive_proof_delta(delta: &str) -> bool {
    delta.contains("missing discriminator no longer reported")
        || delta.contains("new observed value")
        || delta.contains("stronger related oracle visible")
        || delta.contains("related test count increased")
        || delta.contains("evidence moved from missing to yes")
        || delta.contains("evidence moved from weak to yes")
}

fn review_limit_or_default(mut items: Vec<String>, fallback: &str) -> Vec<String> {
    if items.is_empty() {
        return vec![fallback.to_string()];
    }
    items.truncate(5);
    items
}
