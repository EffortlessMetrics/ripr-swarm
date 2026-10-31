//! Behaviour of the step summary the generated workflow prints.
//!
//! These replace the text pins on the retired shell step. The complete
//! fixture carries every artifact the summary reads; the empty root carries
//! none, so between them each block's at-a-glance and not-generated forms
//! render.

use super::jq::{Doc, RepoRelative};
use super::{CiSummaryInput, render_ci_summary};
use crate::agent::loop_commands::WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT;
use crate::app::agent_review_summary::NO_RECEIPT_BEFORE_REPAIR;
use crate::output::first_pr::{
    MANUAL_RECEIPT_LABEL, MANUAL_VERIFY_LABEL, RECEIPT_AFTER_VERIFY_LABEL,
    REPAIR_AFTER_PHASE_LABEL, REPAIR_AFTER_PHASE_STEP, VERIFY_AFTER_EDIT_LABEL,
};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const REPAIR: &str = "ripr agent repair --root . --seam-id s1 --phase before";

/// Every heading the summary can print, in print order.
const SECTIONS: &[&str] = &[
    "## RIPR advisory summary",
    "### What RIPR found in this pull request",
    "### Start here",
    "#### First-run status",
    "### Language preview grouping",
    "### PR review summary",
    "#### PR review at a glance",
    "### Recommended next test",
    "#### Recommended next test at a glance",
    "### Top recommendation",
    "### Agent review packet",
    "### Artifact packet",
    "### Uploaded review artifacts",
    "#### Uploaded artifacts at a glance",
    "### PR evidence ledger",
    "#### PR movement at a glance",
    "### Test-oracle assistant proof",
    "#### Assistant proof at a glance",
    "### Agent proof status",
    "#### Agent proof status at a glance",
    "### Policy readiness",
    "#### Policy readiness at a glance",
    "### Policy operations",
    "#### Policy operations at a glance",
    "### Policy history",
    "#### Policy history at a glance",
    "### Policy promotion packets",
    "### Preview promotion packets",
    "### Waiver aging",
    "#### Waiver aging at a glance",
    "### Suppression health",
    "#### Suppression health at a glance",
    "### Gate decision",
    "#### Gate decision at a glance",
    "### Baseline debt delta",
    "#### Baseline debt movement",
    "### RIPR Zero status",
    "#### RIPR Zero at a glance",
    "### SARIF and badge status",
    "### PR guidance annotations",
    "### PR inline comments",
    "### Known limits",
];

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-ci-summary-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self {
            root: fs::canonicalize(root)?,
        })
    }

    fn write(&self, path: &str, text: &str) -> TestResult {
        let path = self.root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)?;
        Ok(())
    }

    fn json(&self, path: &str, value: &Value) -> TestResult {
        self.write(path, &value.to_string())
    }

    fn render(&self, languages: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        self.render_with(languages, "true", "", "")
    }

    fn render_with(
        &self,
        languages: &[&str],
        upload_sarif: &str,
        gate_baseline: &str,
        comment_mode: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        self.render_with_base(
            languages,
            upload_sarif,
            gate_baseline,
            comment_mode,
            "trunk",
        )
    }

    fn render_with_base(
        &self,
        languages: &[&str],
        upload_sarif: &str,
        gate_baseline: &str,
        comment_mode: &str,
        base_ref: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let input = CiSummaryInput {
            root: self.root.clone(),
            base_ref: base_ref.to_string(),
            upload_sarif: upload_sarif == "true",
            gate_baseline: !gate_baseline.is_empty(),
            gate_configured: false,
            comment_mode: comment_mode.to_string(),
            configured_languages: languages.iter().map(|name| (*name).to_string()).collect(),
        };
        Ok(String::from_utf8(render_ci_summary(&input))?)
    }

    /// Every artifact the summary reads, with a carried repair start.
    fn complete(label: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let fixture = Self::new(label)?;
        let r = "target/ripr/reports";
        fixture.json(
            &format!("{r}/start-here.json"),
            &json!({
                "status": "ready",
                "selected": {
                    "state": "top_gap",
                    "canonical_gap_id": "gap:1",
                    "language": "rust",
                    "language_status": "stable",
                    "kind": "top_gap",
                    "changed_behavior": "threshold moved",
                    "current_evidence_strength": "weak",
                    "missing_discriminator": "boundary at 100",
                    "repair": {"route": "add_boundary_test", "target_file": "tests/a.rs", "related_test": "a::b"},
                    "focused_proof_intent": "assert 100 is rejected",
                    "static_limit_kind": "macro",
                    "static_limit_detail": "opaque",
                    "verify_command": "ripr agent verify --root .",
                    "receipt_command": "ripr agent receipt --root .",
                    "receipt_path": "target/ripr/receipts/r.json",
                    "repair_command": REPAIR,
                },
                "warnings": ["w"],
            }),
        )?;
        fixture.write(&format!("{r}/start-here.md"), "# Start here\n\nbody\n")?;
        fixture.json(
            &format!("{r}/first-useful-action.json"),
            &json!({
                "status": "ready",
                "action_kind": "add_test",
                "title": "Add a boundary test",
                "why": "weak oracle",
                "selected": {"seam_id": "s1"},
                "target": {"file": "tests/a.rs", "related_test": "a::b"},
                "commands": {"repair": REPAIR, "verify": "ripr agent verify --root .", "receipt": "ripr agent receipt --root ."},
            }),
        )?;
        fixture.write(&format!("{r}/first-useful-action.md"), "# First action\n")?;
        fixture.json(
            &format!("{r}/pr-review-front-panel.json"),
            &json!({
                "status": "ready",
                "summary": {"headline": "one gap", "top_issue_state": "open"},
                "top_issue": {"path": "src/a.rs", "line": 4, "classification": "weakly_exposed", "repair_command": REPAIR, "agent_command": REPAIR},
                "policy": {"mode": "visible-only", "decision": "pass"},
            }),
        )?;
        fixture.write(&format!("{r}/pr-review-front-panel.md"), "# Front panel\n")?;
        fixture.write("target/ripr/pilot/pilot-summary.md", "# Pilot\n")?;
        fixture.json(
            WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT,
            &json!({"static_movement": {"state": "missing_artifact"}}),
        )?;
        fixture.json(
            &format!("{r}/index.json"),
            &json!({
                "status": "ok",
                "summary": {"entries": 3, "available": 2, "missing_expected": 1, "start_here": "target/ripr/reports/start-here.md", "gate_authority": "not_configured"},
                "missing_expected": [{"label": "gate decision"}],
                "warnings": [{"kind": "stale"}],
            }),
        )?;
        fixture.write(&format!("{r}/index.md"), "# Index\n")?;
        fixture.json(
            &format!("{r}/pr-evidence-ledger.json"),
            &json!({
                "status": "ok",
                "movement": {"count_source": "not_measured", "acknowledged": 1},
                "top_repair_route": {"path": "src/a.rs", "line": 4, "missing_discriminator": "boundary", "repair_command": REPAIR, "agent_command": "ripr agent brief"},
            }),
        )?;
        fixture.write(&format!("{r}/pr-evidence-ledger.md"), "# Ledger\n")?;
        fixture.json(
            &format!("{r}/test-oracle-assistant-proof.json"),
            &json!({"status": "ok", "seam": {"path": "src/a.rs", "line": 4}}),
        )?;
        fixture.write(&format!("{r}/test-oracle-assistant-proof.md"), "# Proof\n")?;
        fixture.json(
            &format!("{r}/assistant-loop-health.json"),
            &json!({
                "status": "ok",
                "warning_summary": [{"kind": "stale", "count": 2}],
                "repair_queue": [{"repair_kind": "add_test"}],
            }),
        )?;
        fixture.write(&format!("{r}/assistant-loop-health.md"), "# Health\n")?;
        for name in ["policy-readiness", "policy-operations", "policy-history"] {
            fixture.json(&format!("{r}/{name}.json"), &json!({"status": "ok"}))?;
            fixture.write(&format!("{r}/{name}.md"), &format!("# {name}\n"))?;
        }
        fixture.json(
            &format!("{r}/policy-promotion-acknowledgeable.json"),
            &json!({"target_mode": "acknowledgeable", "allowed_now": false, "why_or_why_not": "ceiling"}),
        )?;
        fixture.write(
            &format!("{r}/policy-promotion-acknowledgeable.md"),
            "# Promotion\n",
        )?;
        fixture.json(
            &format!("{r}/preview-promotion-typescript-boundary-gap.json"),
            &json!({"language": "typescript", "candidate_class": "boundary_gap", "allowed_now": false}),
        )?;
        fixture.write(
            &format!("{r}/preview-promotion-typescript-boundary-gap.md"),
            "# Preview\n",
        )?;
        for name in [
            "waiver-aging",
            "suppression-health",
            "baseline-debt-delta",
            "ripr-zero-status",
        ] {
            fixture.json(&format!("{r}/{name}.json"), &json!({"status": "ok"}))?;
            fixture.write(&format!("{r}/{name}.md"), &format!("# {name}\n"))?;
        }
        fixture.json(
            &format!("{r}/gate-decision.json"),
            &json!({
                "status": "fail",
                "mode": "calibrated-gate",
                "summary": {"blocking": 3},
                "inputs": {"labels": ["ripr-waive", "docs"]},
                "policy": {"acknowledgement_labels": ["ripr-waive"]},
                "decisions": [
                    {"decision": "blocking", "gate_reason": "new gap", "evidence": {"mutation_calibration": {"confidence_effect": "lowered"}}},
                    {"decision": "acknowledged", "policy": {"acknowledgement_label": "ripr-waive"}},
                    {"decision": "blocking", "gate_reason": "second"},
                    {"decision": "blocking", "gate_reason": "third", "evidence": {"mutation_calibration": {"confidence_effect": "lowered"}}},
                ],
            }),
        )?;
        fixture.write(&format!("{r}/gate-decision.md"), "# Gate\n")?;
        fixture.json(
            "target/ripr/review/comments.json",
            &json!({
                "summary": {"comments": 2, "summary_only": 1},
                "comments": [{"language": "typescript", "language_status": "preview", "classification": "weakly_exposed", "preview_actionability": {"gap_state": "actionable", "repair_packet_ready": true}}],
            }),
        )?;
        fixture.json(
            "target/ripr/review/comment-publish-plan.json",
            &json!({"status": "planned", "summary": {"publishable": 1, "safe_to_publish": true}}),
        )?;
        fixture.write("target/ripr/review/comment-publish-plan.md", "# Plan\n")?;
        for sarif in [
            "ripr-findings.sarif",
            "ripr-seams.sarif",
            "repo-ripr-badge.json",
        ] {
            fixture.write(&format!("{r}/{sarif}"), "{}")?;
        }
        Ok(fixture)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.root);
    }
}

/// The block under `heading`, up to the next `### ` heading.
fn block<'a>(summary: &'a str, heading: &str) -> &'a str {
    summary
        .split(&format!("{heading}\n"))
        .nth(1)
        .and_then(|rest| rest.split("\n### ").next())
        .unwrap_or_default()
}

#[test]
fn complete_artifacts_render_every_section_in_order() -> TestResult {
    let fixture = Fixture::complete("sections")?;
    let summary = fixture.render(&["rust", "typescript"])?;
    let mut from = 0;
    for heading in SECTIONS {
        let found = summary
            .get(from..)
            .and_then(|rest| rest.find(&format!("{heading}\n")))
            .ok_or_else(|| format!("{heading:?} is missing or out of order:\n{summary}"))?;
        from += found + heading.len();
    }
    assert!(!summary.contains("@RIPR_"), "unsubstituted placeholder");
    assert!(summary.ends_with("- No runtime mutation execution is performed by this workflow.\n"));
    Ok(())
}

#[test]
fn empty_base_names_the_missing_base_instead_of_inventing_origin_main() -> TestResult {
    // #7368: with neither base_ref nor default_branch derivable, the summary
    // must not print an `origin/main` recovery command that cannot succeed on
    // a repository whose default branch is anything else.
    let fixture = Fixture::new("empty-base")?;
    let summary = fixture.render_with_base(&[], "true", "", "", "")?;
    assert!(
        summary.contains("no base is derivable from the event payload"),
        "must name the real condition:\n{summary}"
    );
    assert!(
        !summary.contains("--base origin/ "),
        "no invented base refspec may appear:\n{summary}"
    );
    Ok(())
}

#[test]
fn empty_root_names_every_missing_artifact_and_its_regeneration() -> TestResult {
    let fixture = Fixture::new("empty")?;
    let summary = fixture.render(&[])?;
    for line in [
        "- Start-here artifact: not generated yet; inspect uploaded artifacts and job logs.",
        "- Status: `missing_start_here`",
        "- State: `missing_artifact`",
        "- Safe next action: run `ripr first-pr --root . --base origin/trunk --head HEAD --gap-ledger target/ripr/reports/gap-decision-ledger.json --first-action target/ripr/reports/first-useful-action.json --review-comments target/ripr/review/comments.json --agent-packet target/ripr/workflow/agent-packet.json --gate-decision target/ripr/reports/gate-decision.json --receipts-dir target/ripr/receipts --out-dir target/ripr/reports`.",
        "PR review summary was not generated.",
        "Safe next action: run `ripr pr-review front-panel --root . --pr-guidance target/ripr/review/comments.json --out target/ripr/reports/pr-review-front-panel.json --out-md target/ripr/reports/pr-review-front-panel.md` after attaching at least one explicit input.",
        "Recommended next test was not generated.",
        "Pilot summary was not generated.",
        "Agent review summary was not generated.",
        "- PR test guidance report: not generated yet",
        "Regenerate command: `ripr reports index --root . --reports-dir target/ripr/reports --review-dir target/ripr/review --receipts-dir target/ripr/receipts --workflow-dir target/ripr/workflow --agent-dir target/ripr/agent --pilot-dir target/ripr/pilot --ci-dir target/ci --out target/ripr/reports/index.json --out-md target/ripr/reports/index.md`.",
        "PR evidence ledger was not run.",
        "Policy readiness was not generated.",
        "Policy operations was not generated.",
        "Policy history was not generated.",
        "Policy promotion packets were not generated.",
        "Preview promotion packets were not generated.",
        "Waiver aging was not run.",
        "Suppression health was not generated.",
        "Gate decision was not run. Set `RIPR_GATE_MODE`",
        "Baseline debt delta was not run. Set `RIPR_GATE_BASELINE`",
        "RIPR Zero status was not run.",
        "- Diff SARIF: missing or skipped",
        "- Badge JSON: missing or skipped",
        "No PR test guidance report was generated.",
        "- Mode: `off`",
        "- Inline comments are disabled by default.",
    ] {
        assert!(summary.contains(line), "missing {line:?}:\n{summary}");
    }
    // Optional blocks stay out entirely when nothing feeds them.
    for heading in [
        "### Language preview grouping",
        "### Test-oracle assistant proof",
        "### Agent proof status",
    ] {
        assert!(
            !summary.contains(heading),
            "{heading} rendered with no input"
        );
    }
    Ok(())
}

const PILOT_SUMMARY_JSON: &str = "target/ripr/pilot/pilot-summary.json";

/// The First-run status block, which the pilot deadline notice belongs to.
fn first_run_block(summary: &str) -> &str {
    summary
        .split("#### First-run status\n")
        .nth(1)
        .and_then(|rest| rest.split("\n### ").next())
        .unwrap_or_default()
}

/// #11205: a pilot that stopped at its deadline is named as the cause where
/// the `missing_artifact` state is reported, with the effect and a next step
/// that names the control the packet actually reads.
#[test]
fn a_pilot_deadline_is_named_as_the_cause_of_the_missing_artifacts() -> TestResult {
    let fixture = Fixture::new("pilot-deadline")?;
    fixture.write(
        PILOT_SUMMARY_JSON,
        r#"{"status":"partial","reason":"timeout","timeout_ms":270000}"#,
    )?;
    let summary = fixture.render(&[])?;
    let block = first_run_block(&summary);
    for line in [
        "- Pilot deadline: `ripr pilot` stopped at its deadline after `270000` ms and wrote a partial packet.",
        "- Cause: the pilot deadline, not a missing setup step.",
        "- Effect: repo exposure and agent seam packets were not written",
        "- Next step: set `RIPR_PILOT_TIMEOUT_MS` in the workflow `env:` to at least `540000`",
        "`target/ripr/cache`",
    ] {
        assert!(block.contains(line), "missing {line:?} in:\n{block}");
    }
    // One explanation per summary: the pilot report embedded further down is
    // the producer's own text and does not repeat this notice.
    assert_eq!(summary.matches("- Pilot deadline:").count(), 1, "{summary}");
    // The cause leads the downstream `missing_artifact` state it explains.
    let deadline = block.find("- Pilot deadline:").ok_or("no deadline line")?;
    let missing = block
        .find("- State: `missing_artifact`")
        .ok_or("no missing_artifact line")?;
    assert!(deadline < missing, "{block}");

    // A timeout summary without a readable budget still names the deadline.
    fixture.write(
        PILOT_SUMMARY_JSON,
        r#"{"status":"partial","reason":"timeout"}"#,
    )?;
    let block_text = fixture.render(&[])?;
    let block = first_run_block(&block_text);
    assert!(
        block.contains("- Pilot deadline: `ripr pilot` stopped at its deadline and wrote"),
        "{block}"
    );
    assert!(
        block.contains("- Next step: raise `RIPR_PILOT_TIMEOUT_MS` in the workflow `env:`"),
        "{block}"
    );
    Ok(())
}

/// Controls for #11205: the notice never names a cause the pilot artifacts do
/// not show, so each of these stays silent.
#[test]
fn the_pilot_deadline_notice_stays_silent_without_a_timeout_partial() -> TestResult {
    for (label, pilot) in [
        (
            "complete",
            Some(r#"{"status":"complete","timeout_ms":30000}"#),
        ),
        (
            "other-partial",
            Some(r#"{"status":"partial","reason":"inventory_failed","timeout_ms":30000}"#),
        ),
        (
            "reason-without-partial",
            Some(r#"{"status":"complete","reason":"timeout"}"#),
        ),
        ("malformed", Some("{ not json")),
        ("absent", None),
    ] {
        let fixture = Fixture::new(&format!("pilot-quiet-{label}"))?;
        if let Some(pilot) = pilot {
            fixture.write(PILOT_SUMMARY_JSON, pilot)?;
        }
        let summary = fixture.render(&[])?;
        assert!(
            !summary.contains("Pilot deadline") && !summary.contains("RIPR_PILOT_TIMEOUT_MS"),
            "{label}: unexpected deadline notice:\n{summary}"
        );
        // The downstream state is unchanged, so the notice only adds a cause.
        assert!(summary.contains("- State: `missing_artifact`"), "{label}");
    }
    let complete = Fixture::complete("pilot-quiet-complete-packet")?;
    assert!(
        !complete
            .render(&["rust"])?
            .contains("RIPR_PILOT_TIMEOUT_MS")
    );
    Ok(())
}

/// #3906 (F60-3, F60-14): a carried repair start leads each block with its
/// after phase, and the low-level verify and receipt become the manual
/// alternative.
#[test]
fn a_carried_repair_start_leads_every_block_that_has_one() -> TestResult {
    let fixture = Fixture::complete("repair-lead")?;
    let summary = fixture.render(&[])?;
    let after = format!("- {REPAIR_AFTER_PHASE_LABEL}: {REPAIR_AFTER_PHASE_STEP}\n");
    for (heading, label) in [
        ("#### First-run status", "Start repair"),
        ("#### PR review at a glance", "Repair start"),
        ("#### Recommended next test at a glance", "Repair start"),
        ("#### PR movement at a glance", "Repair start"),
    ] {
        let lead = format!("{heading}\n- {label}: `{REPAIR}`\n{after}");
        assert!(
            summary.contains(&lead),
            "{heading} must lead with the repair start:\n{summary}"
        );
    }
    let first_run = block(&summary, "#### First-run status");
    assert!(first_run.contains(&format!(
        "- {MANUAL_VERIFY_LABEL}: `ripr agent verify --root .`"
    )));
    assert!(first_run.contains(&format!(
        "- {MANUAL_RECEIPT_LABEL}: `ripr agent receipt --root .`"
    )));
    // The front panel's agent handoff repeats the repair start, so it is
    // not printed twice; the ledger's differs and is.
    assert!(!block(&summary, "### PR review summary").contains("- Agent handoff:"));
    assert!(
        block(&summary, "### PR evidence ledger").contains("- Agent command: `ripr agent brief`")
    );

    // Without a repair start the pair runs after the test edit.
    fixture.json(
        "target/ripr/reports/start-here.json",
        &json!({"selected": {"verify_command": "v", "receipt_command": "r"}}),
    )?;
    let summary = fixture.render(&[])?;
    let first_run = block(&summary, "#### First-run status");
    assert!(
        first_run.starts_with("- Status: `unknown`\n"),
        "{first_run}"
    );
    assert!(first_run.contains(&format!("- {VERIFY_AFTER_EDIT_LABEL}: `v`")));
    assert!(first_run.contains(&format!("- {RECEIPT_AFTER_VERIFY_LABEL}: `r`")));
    assert!(first_run.contains("- Safe next action command: `none`"));
    Ok(())
}

/// #3906 (N5): before any test edit the review packet names the missing
/// receipt as expected and leads with the repair start, not the post-edit
/// loop; once a receipt can exist the full packet is collapsed instead.
#[test]
fn the_agent_review_packet_leads_with_the_repair_before_any_edit() -> TestResult {
    let fixture = Fixture::complete("review-packet")?;
    let summary = fixture.render(&[])?;
    let packet = block(&summary, "### Agent review packet");
    assert!(
        packet.starts_with(&format!(
            "- Receipt: {NO_RECEIPT_BEFORE_REPAIR}\n- Start repair: `{REPAIR}`\n- {REPAIR_AFTER_PHASE_LABEL}: {REPAIR_AFTER_PHASE_STEP}\n- Full packet: `target/ripr/workflow/agent-review-summary.md` (workflow artifact).\n"
        )),
        "{packet}"
    );

    fixture.json(
        WORKFLOW_AGENT_REVIEW_SUMMARY_ARTIFACT,
        &json!({"static_movement": {"state": "improved"}}),
    )?;
    fixture.write("target/ripr/workflow/agent-review-summary.md", "# Review\n")?;
    let summary = fixture.render(&[])?;
    assert!(block(&summary, "### Agent review packet").starts_with(
        "<details><summary>Full report: target/ripr/workflow/agent-review-summary.md</summary>\n\n# Review\n\n</details>\n"
    ));
    Ok(())
}

/// RIPR-SPEC-0051: source delimiters must stay literal in both first-run
/// artifact routes. Expected spans are literal pins, not the helper under test.
#[test]
fn given_changed_behavior_delimiters_when_rendered_then_one_literal_code_span() -> TestResult {
    for path in [super::START_HERE_JSON, super::FIRST_ACTION_JSON] {
        let fixture = Fixture::new("changed-behavior")?;
        for (source, expected) in [
            ("amount >= limit", "`amount >= limit`"),
            ("a`b", "``a`b``"),
            (
                "format!(\"a`b``c```d | \\path <value>\")",
                "````format!(\"a`b``c```d | \\path <value>\")````",
            ),
            ("`edge`", "`` `edge` ``"),
            ("  spaced  ", "`   spaced   `"),
            ("a`b\nc\r\nd\re", "``a`b c d e``"),
        ] {
            fixture.json(path, &json!({"selected": {"changed_behavior": source}}))?;
            let summary = fixture.render(&[])?;
            let first_run = block(&summary, "#### First-run status");
            let lines = first_run
                .lines()
                .filter(|line| line.starts_with("- Changed behavior: "))
                .collect::<Vec<_>>();
            assert_eq!(
                lines,
                vec![format!("- Changed behavior: {expected}")],
                "changed behavior from {path}: {source:?}"
            );
            // Rendering must not rewrite the backing JSON's original string.
            let artifact: Value = serde_json::from_slice(&fs::read(fixture.root.join(path))?)?;
            assert_eq!(artifact["selected"]["changed_behavior"], source);
        }
    }
    Ok(())
}

#[test]
fn given_changed_behavior_fallback_when_rendered_then_delimiters_stay_literal() -> TestResult {
    let fixture = Fixture::new("changed-behavior-fallback")?;
    fixture.json(
        super::FIRST_ACTION_JSON,
        &json!({"why": "compare `a` and ``b``"}),
    )?;
    let summary = fixture.render(&[])?;
    let first_run = block(&summary, "#### First-run status");
    assert!(first_run.contains("- Changed behavior: ``` compare `a` and ``b`` ```\n"));
    Ok(())
}

/// RIPR-SPEC-0051: Why is a literal value in both summary projections,
/// even when its source contains intentional Markdown prose.
#[test]
fn given_ci_summary_why_markup_when_rendered_then_one_literal_span() -> TestResult {
    let fixture = Fixture::new("why-markup")?;
    for (source, expected) in [
        ("weak oracle", "`weak oracle`"),
        ("compare `a` and ``b``", "``` compare `a` and ``b`` ```"),
        ("*bold* [link](url)", "`*bold* [link](url)`"),
        (
            "a` @octocat <img> | \\path",
            "``a` @octocat <img> | \\path``",
        ),
        ("  spaced  ", "`   spaced   `"),
        ("a`b\nc\r\nd\re\n", "``a`b c d e``"),
    ] {
        fixture.json(super::FIRST_ACTION_JSON, &json!({"why": source}))?;
        let summary = fixture.render(&[])?;
        for heading in [
            "#### First-run status",
            "#### Recommended next test at a glance",
        ] {
            let lines = block(&summary, heading)
                .lines()
                .filter(|line| line.starts_with("- Why: "))
                .map(str::to_string)
                .collect::<Vec<_>>();
            assert_eq!(
                lines,
                vec![format!("- Why: {expected}")],
                "{heading}: {source:?}"
            );
        }
        let artifact: Value =
            serde_json::from_slice(&fs::read(fixture.root.join(super::FIRST_ACTION_JSON))?)?;
        assert_eq!(artifact["why"], source);
    }
    Ok(())
}

/// Legacy field spelling is retained outside the explicitly repaired fields.
#[test]
fn field_values_are_escaped_into_one_inline_code_span() -> TestResult {
    let fixture = Fixture::new("inline")?;
    fixture.json(
        "target/ripr/reports/first-useful-action.json",
        &json!({"title": "a `b`\nc\r\n", "why": 7, "action_kind": ["x"]}),
    )?;
    let summary = fixture.render(&[])?;
    let first_run = block(&summary, "#### First-run status");
    // Trailing newlines drop, inner ones become spaces, backticks escape.
    assert!(first_run.contains("- Title: `a \\`b\\` c `"), "{first_run}");
    assert!(first_run.contains("- Why: `7`"));
    assert!(
        first_run.contains("- Safe next action: `[   \"x\" ]`"),
        "{first_run}"
    );
    Ok(())
}

/// A malformed artifact, a whitespace-only one, and a field whose type
/// breaks the read each keep the fallback the shell step printed.
#[test]
fn malformed_and_mistyped_artifacts_keep_their_fallbacks() -> TestResult {
    let fixture = Fixture::new("fallbacks")?;
    fixture.write("target/ripr/reports/gate-decision.json", "{not json")?;
    fixture.write("target/ripr/reports/waiver-aging.json", " \n")?;
    fixture.json(
        "target/ripr/reports/ripr-zero-status.json",
        &json!({"ripr_zero": "flat", "top_debt_areas": {"area": "x"}, "repair_routes": []}),
    )?;
    fixture.json(
        "target/ripr/reports/index.json",
        &json!({"missing_expected": "none", "warnings": [{"kind": "a"}, 3]}),
    )?;
    let summary = fixture.render(&[])?;
    let gate = block(&summary, "### Gate decision");
    assert!(gate.contains("- Mode: `unknown`"));
    assert!(gate.contains("- Counts: blocking=`0`, acknowledged=`0`"));
    assert!(gate.contains("- Active PR labels: `unknown`"));
    // jq printed nothing for an empty input and succeeded.
    assert!(block(&summary, "### Waiver aging").contains("- Status: ``"));
    let zero = block(&summary, "### RIPR Zero status");
    assert!(zero.contains("- State: `unknown`"), "{zero}");
    assert!(zero.contains("- Visible unresolved: `0`"));
    assert!(zero.contains("- Top debt area: `unknown`"));
    assert!(zero.contains("- Top repair route: `none`"));
    let index = block(&summary, "### Uploaded review artifacts");
    assert!(index.contains("- Missing expected: `none`"), "{index}");
    assert!(index.contains("- Warning kinds: `unknown`"), "{index}");
    Ok(())
}

#[test]
fn the_gate_block_summarises_labels_waivers_effects_and_reasons() -> TestResult {
    let fixture = Fixture::complete("gate")?;
    let summary = fixture.render(&[])?;
    let gate = block(&summary, "### Gate decision");
    for line in [
        "- Mode: `calibrated-gate`",
        "- Counts: blocking=`3`, acknowledged=`0`, advisory=`0`, suppressed=`0`, not_applicable=`0`, unknown_confidence=`0`",
        "- Active PR labels: `ripr-waive, docs`",
        "- Acknowledgement labels: `ripr-waive`",
        "- Applied waiver label: `ripr-waive`",
        "- Baseline artifact: `not supplied`",
        "- Recommendation calibration: `not supplied` (effects: none)",
        "- Mutation calibration: `not supplied` (effects: lowered)",
        "- Blocking reason (`3`): `new gap (+2 more, see gate-decision.md)`",
    ] {
        assert!(gate.contains(line), "missing {line:?}:\n{gate}");
    }
    Ok(())
}

/// F60-4: counts with no baseline delta or RIPR Zero status behind them
/// were never measured, so their zeros are not printed.
#[test]
fn unmeasured_ledger_counts_are_not_printed_as_zeros() -> TestResult {
    let fixture = Fixture::complete("ledger")?;
    let ledger = fixture.render(&[])?;
    let ledger = block(&ledger, "### PR evidence ledger");
    assert!(ledger.contains("- Counts: gap counts not measured (no baseline debt delta or RIPR Zero status); acknowledged=`1`, suppressed=`0`, blocking_candidates=`0`"));
    assert!(ledger.contains("- Top repair route: `src/a.rs:4 boundary`"));

    fixture.json(
        "target/ripr/reports/pr-evidence-ledger.json",
        &json!({"movement": {"visible_unresolved": 2}}),
    )?;
    let ledger = fixture.render(&[])?;
    assert!(block(&ledger, "### PR evidence ledger").contains("visible_unresolved=`2`"));
    Ok(())
}

/// #3999: commands the summary prints for another machine name the
/// repository root, not this runner's checkout, wherever the checkout is a
/// whole path token; reports that are included raw stay as written.
#[test]
fn checkout_paths_become_the_repository_root_in_copied_commands() -> TestResult {
    let fixture = Fixture::new("relative")?;
    let checkout = fixture.root.to_string_lossy().into_owned();
    let verify =
        format!("ripr agent verify --root '{checkout}' --json > '{checkout}/target/v.json'");
    let sibling = format!("{checkout}-other/notes.md");
    fixture.json(
        "target/ripr/reports/pr-review-front-panel.json",
        &json!({"top_issue": {"verify_command": verify}}),
    )?;
    fixture.write(
        "target/ripr/reports/pr-review-front-panel.md",
        &format!("- Verify: `{verify}`\n- Notes: `{sibling}`"),
    )?;
    fixture.write(
        "target/ripr/pilot/pilot-summary.md",
        &format!("`{verify}`\n"),
    )?;
    let summary = fixture.render(&[])?;
    let relative = "ripr agent verify --root '.' --json > './target/v.json'";
    let panel = block(&summary, "### PR review summary");
    assert_eq!(panel.matches(relative).count(), 2, "{panel}");
    // A sibling that only shares the prefix stays, and the report's last
    // line gains the newline the shell's line rewrite gave it.
    assert!(
        panel.contains(&format!("- Notes: `{sibling}`\n\n</details>")),
        "{panel}"
    );
    assert!(block(&summary, "### Top recommendation").contains(&verify));
    Ok(())
}

#[test]
fn repo_relative_rewrites_only_whole_path_tokens() {
    let relative = RepoRelative {
        physical: b"/w/repo".to_vec(),
        logical: b"/link/repo".to_vec(),
    };
    assert_eq!(
        relative.value("cd /w/repo && ls /w/repo/src"),
        "cd . && ls ./src"
    );
    assert_eq!(relative.value("x=/link/repo:(/w/repo)"), "x=.:(.)");
    assert_eq!(
        relative.value("/w/repository /w/repo2 a/w/repo"),
        "/w/repository /w/repo2 a/w/repo"
    );
    assert_eq!(relative.value("one /w/repo\ntwo\n\n"), "one .\ntwo");
    assert_eq!(relative.file(b""), b"");
    assert_eq!(relative.file(b"\n"), b"\n");
    assert_eq!(relative.file(b"a /w/repo\nb"), b"a .\nb\n");
}

#[test]
fn doc_reads_follow_jq_exit_status() {
    let query = |value: &Value| super::jq::alt(value, &["a"], Value::from("d"));
    assert_eq!(Doc::parse(b"").text(query, "fail"), "");
    assert_eq!(Doc::parse(b"{\"a\":1}").text(query, "fail"), "1");
    assert_eq!(Doc::parse(b"{bad").text(query, "fail"), "fail");
    // jq keeps going after a failed input and exits on the last one.
    assert_eq!(Doc::parse(b"\"s\" {\"a\":1}").text(query, "fail"), "1");
    assert_eq!(
        Doc::parse(b"{\"a\":1} \"s\"").text(query, "fail"),
        "1\nfail"
    );
    assert_eq!(Doc::parse(b"{\"a\":1} {bad").text(query, "fail"), "1\nfail");
    assert_eq!(Doc::parse(b"\"s\"").text(query, ""), "");
}

#[test]
fn preview_languages_group_only_when_configured() -> TestResult {
    let fixture = Fixture::complete("languages")?;
    let rust_only = fixture.render(&["rust"])?;
    assert!(!rust_only.contains("### Language preview grouping"));

    let summary = fixture.render(&["rust", "typescript"])?;
    let grouping = summary
        .find("### Language preview grouping\n")
        .ok_or("no grouping block")?;
    let review = summary
        .find("### PR review summary\n")
        .ok_or("no review block")?;
    assert!(
        grouping < review,
        "grouping must precede the PR review summary"
    );
    let grouping = block(&summary, "### Language preview grouping");
    assert!(grouping.contains("- Configured languages: `rust,typescript`"));
    assert!(grouping.contains("- Grouped preview evidence languages: `javascript typescript`"));
    assert!(grouping.contains("- `javascript`: configured preview/advisory; no language findings were emitted in this run; gate_impact=`none`."));
    assert!(grouping.contains("- `typescript`: artifact_entries=`1`, preview_entries=`1`, missing_preview_status=`0`, static_limit_entries=`0`, classifications=`weakly_exposed=1`, static_limit_kinds=`none`, actionability_states=`actionable=1`, actionability_categories=`none`, repair_packet_ready=`1`, gate_impact=`none`"), "{grouping}");

    // A malformed input fails every count, as `jq -s` did.
    fixture.write("target/ripr/reports/gate-decision.json", "{bad")?;
    let summary = fixture.render(&["python"])?;
    let grouping = block(&summary, "### Language preview grouping");
    assert!(
        grouping.contains("- `python`: configured preview/advisory"),
        "{grouping}"
    );
    Ok(())
}

#[test]
fn workflow_settings_choose_the_sarif_baseline_and_comment_lines() -> TestResult {
    let fixture = Fixture::complete("settings")?;
    let enabled = fixture.render_with(&[], "true", "", "")?;
    let sarif = block(&enabled, "### SARIF and badge status");
    assert!(sarif.starts_with("- Diff SARIF: generated\n- Repo seam SARIF: generated\n- Badge JSON: generated\n- Badge Shields JSON: missing or skipped\n"), "{sarif}");
    let disabled = fixture.render_with(&[], "false", "", "plan`x")?;
    assert!(
        block(&disabled, "### SARIF and badge status")
            .starts_with("- SARIF upload: disabled by `RIPR_UPLOAD_SARIF`\n")
    );
    let comments = block(&disabled, "### PR inline comments");
    assert!(comments.starts_with("- Mode: `plan\\`x`\n- Status: `planned`\n- Counts: publishable=`1`, skipped=`0`, blocked=`0`\n- Safe to publish: `true`\n"), "{comments}");

    let empty = Fixture::new("settings-empty")?;
    let without = empty.render_with(&[], "", "", "")?;
    assert!(without.contains("Baseline debt delta was not run."));
    let with = empty.render_with(&[], "", ".ripr/gate-baseline.json", "")?;
    assert!(
        with.contains(
            "Baseline debt delta was not generated. Check that `RIPR_GATE_MODE` produced"
        )
    );
    Ok(())
}

#[test]
fn preview_promotion_packets_follow_the_shell_glob() -> TestResult {
    let fixture = Fixture::new("preview-glob")?;
    let r = "target/ripr/reports";
    fixture.json(
        &format!("{r}/preview-promotion-python-b.json"),
        &json!({"language": "python", "candidate_class": "b"}),
    )?;
    fixture.json(
        &format!("{r}/preview-promotion-python-a.json"),
        &json!({"language": "python", "candidate_class": "a"}),
    )?;
    // No second dash after the prefix: outside `preview-promotion-*-*.json`.
    fixture.json(
        &format!("{r}/preview-promotion-python.json"),
        &json!({"language": "skip"}),
    )?;
    fixture.write(&format!("{r}/preview-promotion-python-a.md"), "# A")?;
    let summary = fixture.render(&[])?;
    let packets = block(&summary, "### Preview promotion packets");
    let a = packets.find("- `python`/`a`").ok_or("no a")?;
    let b = packets.find("- `python`/`b`").ok_or("no b")?;
    assert!(a < b, "{packets}");
    assert!(!packets.contains("skip"));
    assert!(packets.contains("<details><summary>Full report: target/ripr/reports/preview-promotion-python-a.md</summary>\n\n# A\n</details>"), "{packets}");
    Ok(())
}

/// #11161: when `target/ripr/pr/check.json` discloses a producer refusal,
/// the summary opens First-run status with `#### PR analysis did not run`
/// naming the typed status, the producer's own message, that nothing was
/// analyzed, and the next step. A complete, absent, or malformed check
/// document adds nothing; a non-refusal envelope adds nothing either.
#[test]
fn producer_refusal_check_document_is_named_while_others_add_nothing() -> TestResult {
    let refusal = || {
        json!({
            "analysis_scope": {
                "scope": "diff",
                "run_status": "diff_scope_oversized",
                "downstream_consumable": false,
                "repair_route": "analysis/diff-scope-budget"
            },
            "run_limitations": [{
                "category": "diff_scope_oversized",
                "run_status": "diff_scope_oversized",
                "downstream_consumable": false,
                "message": "diff_scope_oversized: 2878 changed Rust lines exceed the limit (2000); analysis was not run.",
                "repair_route": "analysis/diff-scope-budget"
            }]
        })
    };
    let fixture = Fixture::new("refusal-named")?;
    fixture.json("target/ripr/pr/check.json", &refusal())?;
    let summary = fixture.render(&[])?;
    let first_run = block(&summary, "#### First-run status");
    for line in [
        "#### PR analysis did not run",
        "- Producer run status: `diff_scope_oversized`",
        "2878 changed Rust lines",
        "nothing was analyzed",
        "analysis/diff-scope-budget",
    ] {
        assert!(summary.contains(line), "missing {line:?}:\n{summary}");
    }
    assert!(
        first_run.contains("#### PR analysis did not run"),
        "the refusal block must open First-run status:\n{first_run}"
    );
    // A complete document (no refusal disclosure) adds nothing.
    let fixture = Fixture::new("refusal-complete")?;
    fixture.json("target/ripr/pr/check.json", &json!({"findings": []}))?;
    assert!(
        !fixture
            .render(&[])?
            .contains("#### PR analysis did not run"),
        "a complete check document must add no refusal block"
    );
    // An absent check document adds nothing.
    let fixture = Fixture::new("refusal-absent")?;
    assert!(
        !fixture
            .render(&[])?
            .contains("#### PR analysis did not run"),
        "an absent check document must add no refusal block"
    );
    // A malformed check document adds nothing.
    let fixture = Fixture::new("refusal-malformed")?;
    fixture.write("target/ripr/pr/check.json", "{not json")?;
    assert!(
        !fixture
            .render(&[])?
            .contains("#### PR analysis did not run"),
        "a malformed check document must add no refusal block"
    );
    // A non-refusal envelope (consumable, no outcome) adds nothing either.
    let fixture = Fixture::new("refusal-non-refusal")?;
    fixture.json(
        "target/ripr/pr/check.json",
        &json!({"schema_version": "0.2", "findings": []}),
    )?;
    assert!(
        !fixture
            .render(&[])?
            .contains("#### PR analysis did not run"),
        "a non-refusal envelope must add no refusal block"
    );
    // An unknown-status refusal still names the refusal with a fallback.
    // Built as a literal (no mutation helpers) so the test stays off the
    // panic-family allowlist.
    let fixture = Fixture::new("refusal-unknown-status")?;
    fixture.json(
        "target/ripr/pr/check.json",
        &json!({
            "analysis_scope": {
                "scope": "diff",
                "downstream_consumable": false,
                "repair_route": "analysis/diff-scope-budget"
            },
            "run_limitations": [{
                "downstream_consumable": false,
                "message": "diff_scope_oversized: 2878 changed Rust lines exceed the limit (2000); analysis was not run.",
                "repair_route": "analysis/diff-scope-budget"
            }]
        }),
    )?;
    let summary = fixture.render(&[])?;
    assert!(
        summary.contains("#### PR analysis did not run"),
        "an untyped refusal must still be named:\n{summary}"
    );
    assert!(
        summary.contains("producer refusal"),
        "fallback status:\n{summary}"
    );
    // A mixed bounded-plus-refusal document keeps the bounded handling: the
    // bounded guard runs before the refusal reader, so no refusal block
    // renders. The legacy limitations-only shape trips only the bounded
    // guard (no `analysis_outcome`, so the incomplete guard stays quiet):
    // removing the bounded guard would name the refusal here — the document
    // carries an explicit non-consumable record — and fail this assertion.
    // (The producer never emits this mix; it pins guard order.)
    let fixture = Fixture::new("refusal-mixed-bounded")?;
    let mut mixed = refusal();
    mixed["run_limitations"] = json!([
        {
            "category": "limited_findings_bound",
            "run_status": "limited_findings_bound",
            "downstream_consumable": false,
            "message": "findings array was cut by the byte budget",
            "repair_route": "analysis/findings-budget"
        },
        {
            "category": "diff_scope_oversized",
            "run_status": "diff_scope_oversized",
            "downstream_consumable": false,
            "message": "diff_scope_oversized: 2878 changed Rust lines exceed the limit (2000); analysis was not run.",
            "repair_route": "analysis/diff-scope-budget"
        }
    ]);
    fixture.json("target/ripr/pr/check.json", &mixed)?;
    assert!(
        !fixture
            .render(&[])?
            .contains("#### PR analysis did not run"),
        "a bounded document that also discloses a refusal must keep the bounded handling"
    );
    // A mixed partial-plus-refusal document keeps the partial handling: the
    // partial guard runs before the refusal reader. Removing that guard
    // would name the refusal here and fail this assertion.
    let fixture = Fixture::new("refusal-mixed-partial")?;
    let mut mixed = refusal();
    mixed["language_runs"] = json!([{ "status": "partial" }]);
    fixture.json("target/ripr/pr/check.json", &mixed)?;
    assert!(
        !fixture
            .render(&[])?
            .contains("#### PR analysis did not run"),
        "a partial document that also discloses a refusal must keep the partial handling"
    );
    // A mixed incomplete-plus-refusal document keeps the incomplete
    // handling: the incomplete guard runs before the refusal reader.
    // Removing that guard would name the refusal here and fail this
    // assertion.
    let fixture = Fixture::new("refusal-mixed-incomplete")?;
    let mut mixed = refusal();
    mixed["analysis_outcome"] = json!({ "analysis_complete": false });
    fixture.json("target/ripr/pr/check.json", &mixed)?;
    assert!(
        !fixture
            .render(&[])?
            .contains("#### PR analysis did not run"),
        "an incomplete document that also discloses a refusal must keep the incomplete handling"
    );
    Ok(())
}

const HEADLINE: &str = "### What RIPR found in this pull request";

fn card() -> Value {
    json!({
        "seam": {"file": "src/pricing.rs", "line": 88},
        "owner": "pricing::discounted_total",
        "missing_discriminator": "amount == discount_threshold",
        "llm_guidance": {"repair_command": REPAIR},
    })
}

#[test]
fn the_pull_requests_own_verdict_comes_before_ripr_orientation() -> TestResult {
    let fixture = Fixture::new("headline")?;
    fixture.json(
        "target/ripr/review/comments.json",
        &json!({
            "summary": {"comments": 1, "summary_only": 2},
            "analysis_outcome": {"analysis_complete": true},
            "comments": [card()],
        }),
    )?;
    let summary = fixture.render(&[])?;
    let headline = summary.find(HEADLINE).ok_or("headline is missing")?;
    let start_here = summary
        .find("### Start here\n")
        .ok_or("start here is missing")?;
    assert!(headline < start_here, "{summary}");
    let head = block(&summary, HEADLINE);
    assert!(
        head.contains(
            "**3 changed-behavior spots where the tests may not notice a wrong result.**"
        ),
        "{head}"
    );
    assert!(
        head.contains("- First: `src/pricing.rs:88` in `pricing::discounted_total`"),
        "{head}"
    );
    assert!(
        head.contains("- Missing check: `amount == discount_threshold`"),
        "{head}"
    );
    assert!(head.contains(&format!("- Next step: `{REPAIR}`")), "{head}");
    Ok(())
}

#[test]
fn a_clean_run_is_not_reported_as_an_unfinished_one() -> TestResult {
    let clean = Fixture::new("headline-clean")?;
    clean.json(
        "target/ripr/review/comments.json",
        &json!({"summary": {"comments": 0}, "analysis_outcome": {"analysis_complete": true}, "comments": []}),
    )?;
    let summary = clean.render(&[])?;
    assert!(
        block(&summary, HEADLINE).contains("No changed-behavior spot with weak test evidence"),
        "{summary}"
    );
    // Discriminating alternate: the same empty report without a completed
    // analysis must not read as clean.
    let unfinished = Fixture::new("headline-unfinished")?;
    unfinished.json(
        "target/ripr/review/comments.json",
        &json!({"summary": {"comments": 0}, "comments": []}),
    )?;
    let summary = unfinished.render(&[])?;
    let head = block(&summary, HEADLINE);
    assert!(head.contains("did not complete"), "{head}");
    assert!(!head.contains("No changed-behavior spot"), "{head}");
    Ok(())
}

#[test]
fn no_review_report_means_no_headline_and_a_cut_command_is_not_shown() -> TestResult {
    let none = Fixture::new("headline-none")?;
    assert!(!none.render(&[])?.contains(HEADLINE));
    let unparsable = Fixture::new("headline-bad")?;
    unparsable.write("target/ripr/review/comments.json", "{not json")?;
    assert!(!unparsable.render(&[])?.contains(HEADLINE));
    let not_object = Fixture::new("headline-array")?;
    not_object.write("target/ripr/review/comments.json", "[]")?;
    let rendered = not_object.render(&[])?;
    assert!(!rendered.contains(HEADLINE));
    assert!(
        rendered.starts_with(
            "## RIPR advisory summary\n\nRIPR is advisory static evidence. It does not edit source, generate tests, or run mutation testing.\n\n### Start here"
        ),
        "no stray blank line:\n{rendered}"
    );
    let mut shell = card();
    shell["llm_guidance"]["repair_command"] = json!("ripr agent repair; rm -rf .");
    let hostile = Fixture::new("headline-hostile")?;
    hostile.json(
        "target/ripr/review/comments.json",
        &json!({"summary": {}, "comments": [shell]}),
    )?;
    let head = block(&hostile.render(&[])?, HEADLINE).to_string();
    assert!(!head.contains("Next step"), "{head}");
    assert!(head.contains("1 changed-behavior spot where"), "{head}");
    // A command that is plain but not a repair start is withheld too.
    let mut other = card();
    other["llm_guidance"]["repair_command"] = json!("rm -rf target");
    let plain = Fixture::new("headline-plain")?;
    plain.json(
        "target/ripr/review/comments.json",
        &json!({"summary": {}, "comments": [other]}),
    )?;
    assert!(!block(&plain.render(&[])?, HEADLINE).contains("Next step"));
    Ok(())
}

#[test]
fn report_text_is_cleaned_and_bounded_in_the_first_line() -> TestResult {
    let mut row = card();
    row["seam"]["file"] = json!("src/a\nb\u{202e}c\u{200b}d.rs");
    row["owner"] = json!(format!("fn x @team{}", "o".repeat(300)));
    row["missing_discriminator"] = json!("line1\nline2\u{202e}tick");
    let fixture = Fixture::new("headline-clean-text")?;
    fixture.json(
        "target/ripr/review/comments.json",
        &json!({"summary": {}, "comments": [row]}),
    )?;
    let summary = fixture.render(&[])?;
    let head = block(&summary, HEADLINE);
    let owner: String = format!("fn x @team{}", "o".repeat(300))
        .chars()
        .take(160)
        .collect();
    assert!(
        head.contains(&format!("- First: `src/a bcd.rs:88` in `{owner}`\n")),
        "{head}"
    );
    assert!(
        head.contains("- Missing check: `line1 line2tick`\n"),
        "{head}"
    );
    assert!(
        !head.contains('\u{202e}') && !head.contains('\u{200b}'),
        "{head}"
    );
    Ok(())
}

#[test]
fn an_unproven_clean_report_is_never_clean() -> TestResult {
    // analysis_complete with no comments array.
    let missing = Fixture::new("headline-no-array")?;
    missing.json(
        "target/ripr/review/comments.json",
        &json!({"analysis_outcome": {"analysis_complete": true}}),
    )?;
    let head = block(&missing.render(&[])?, HEADLINE).to_string();
    assert!(!head.contains("No changed-behavior spot"), "{head}");
    assert!(head.contains("not a clean result"), "{head}");
    // summary.comments states rows the array does not carry.
    let stated = Fixture::new("headline-stated")?;
    stated.json(
        "target/ripr/review/comments.json",
        &json!({"summary": {"comments": 2}, "analysis_outcome": {"analysis_complete": true}, "comments": []}),
    )?;
    let head = block(&stated.render(&[])?, HEADLINE).to_string();
    assert!(head.contains("**2 changed-behavior spots"), "{head}");
    assert!(
        head.contains("The items are in the PR guidance section below."),
        "{head}"
    );
    Ok(())
}

#[test]
fn a_summary_only_report_names_its_first_spot_from_the_root_array() -> TestResult {
    // The producer writes the count under summary and the cards at the root.
    let fixture = Fixture::new("headline-summary-only")?;
    fixture.json(
        "target/ripr/review/comments.json",
        &json!({
            "summary": {"comments": 0, "summary_only": 1},
            "comments": [],
            "summary_only": [card()],
        }),
    )?;
    let head = block(&fixture.render(&[])?, HEADLINE).to_string();
    assert!(head.contains("**1 changed-behavior spot where"), "{head}");
    assert!(
        head.contains("- First: `src/pricing.rs:88` in `pricing::discounted_total`"),
        "{head}"
    );
    assert!(!head.contains("PR guidance section below"), "{head}");
    // A count with no cards still points at the guidance section.
    let counted = Fixture::new("headline-count-only")?;
    counted.json(
        "target/ripr/review/comments.json",
        &json!({"summary": {"comments": 0, "summary_only": 2}, "comments": []}),
    )?;
    let head = block(&counted.render(&[])?, HEADLINE).to_string();
    assert!(head.contains("**2 changed-behavior spots"), "{head}");
    assert!(head.contains("PR guidance section below"), "{head}");
    Ok(())
}

/// #11146: the summary and the packet's plan stage share one reading of
/// `RIPR_COMMENT_MODE`: whitespace is ignored and empty means `off`.
#[test]
fn comment_mode_trims_and_reads_empty_as_off() {
    use super::effective_comment_mode;
    assert_eq!(effective_comment_mode(""), "off");
    assert_eq!(effective_comment_mode("  "), "off");
    assert_eq!(effective_comment_mode(" inline "), "inline");
    assert_eq!(effective_comment_mode("plan"), "plan");
    assert_eq!(effective_comment_mode("post"), "post");
}

const GOVERNANCE_HEADINGS: [&str; 7] = [
    "### Policy readiness\n",
    "### Policy operations\n",
    "### Policy history\n",
    "### Policy promotion packets\n",
    "### Preview promotion packets\n",
    "### Waiver aging\n",
    "### Suppression health\n",
];

fn render_gate(
    fixture: &Fixture,
    gate_configured: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let input = CiSummaryInput {
        root: fixture.root.clone(),
        base_ref: "trunk".to_string(),
        upload_sarif: true,
        gate_baseline: false,
        gate_configured,
        comment_mode: String::new(),
        configured_languages: vec!["rust".to_string()],
    };
    Ok(String::from_utf8(render_ci_summary(&input))?)
}

#[test]
fn governance_sections_fold_unless_a_gate_is_configured() -> TestResult {
    let fixture = Fixture::complete("fold-governance")?;
    write_quiet(&fixture)?;
    let folded = render_gate(&fixture, false)?;
    let open = render_gate(&fixture, true)?;
    let fold = "<details><summary>Policy and governance (advisory; no gate configured; no warnings or unknowns)</summary>";
    let start = folded.find(fold).ok_or("governance fold is missing")?;
    let gate = folded
        .find("### Gate decision\n")
        .or_else(|| folded.find("#### Gate decision at a glance"))
        .ok_or("gate decision is missing")?;
    assert!(start < gate, "{folded}");
    let inside = &folded[start..gate];
    for heading in GOVERNANCE_HEADINGS {
        assert!(inside.contains(heading), "{heading:?} escaped the fold");
        assert_eq!(folded.matches(heading).count(), 1, "{heading:?}");
    }
    // Full reports nest their own <details>; the fold must still be closed
    // by its own marker before the gate section.
    let opens = inside.matches("<details").count();
    assert!(opens > 1, "expected nested full reports:\n{inside}");
    assert_eq!(opens, inside.matches("</details>").count(), "{inside}");
    assert!(inside.trim_end().ends_with("</details>"), "{inside}");
    // Folded, not dropped: a configured gate leaves them unfolded.
    assert!(!open.contains("Policy and governance"), "{open}");
    for heading in GOVERNANCE_HEADINGS {
        assert!(open.contains(heading), "{heading:?}");
    }
    Ok(())
}

const R: &str = "target/ripr/reports";

/// Every folded report, parsed, with every count it states at exactly zero.
fn quiet_reports() -> Vec<(String, serde_json::Value)> {
    let warn = || json!({"warnings": [], "unknowns": []});
    vec![
        (format!("{R}/policy-readiness.json"), warn()),
        (
            format!("{R}/policy-operations.json"),
            json!({"promotion_blockers": [], "warnings": [], "unknowns": []}),
        ),
        (format!("{R}/policy-history.json"), warn()),
        (
            format!("{R}/policy-promotion-acknowledgeable.json"),
            json!({"target_mode": "acknowledgeable", "warnings": [], "unknowns": []}),
        ),
        (
            format!("{R}/preview-promotion-typescript-boundary-gap.json"),
            json!({"language": "typescript", "warnings": [], "unknowns": []}),
        ),
        (
            format!("{R}/waiver-aging.json"),
            json!({"summary": {"durable_suppression_candidates": 0, "warnings": 0}}),
        ),
        (
            format!("{R}/suppression-health.json"),
            json!({"summary": {
                "missing_owner": 0, "missing_reason": 0, "stale": 0,
                "overbroad_scope": 0, "unknown_selector": 0,
                "preview_without_preview_label": 0, "warnings": 0, "config_errors": 0,
            }}),
        ),
    ]
}

fn write_quiet(fixture: &Fixture) -> TestResult {
    for (path, doc) in quiet_reports() {
        fixture.json(&path, &doc)?;
    }
    Ok(())
}

fn set_path(doc: &mut serde_json::Value, path: &str, value: serde_json::Value) {
    let mut node = doc;
    let keys: Vec<&str> = path.split('.').collect();
    for key in &keys[..keys.len() - 1] {
        node = &mut node[*key];
    }
    node[keys[keys.len() - 1]] = value;
}

const OPEN: &str = "<details open><summary>Policy and governance";
const CLEAN: &str = "no gate configured; no warnings or unknowns)";

#[test]
fn every_count_source_keeps_the_governance_fold_open_when_nonzero() -> TestResult {
    let control = Fixture::complete("fold-control")?;
    write_quiet(&control)?;
    let summary = render_gate(&control, false)?;
    assert!(
        summary.contains(CLEAN) && !summary.contains(OPEN),
        "{summary}"
    );

    let mut cases: Vec<(&str, String, serde_json::Value)> = Vec::new();
    for file in [
        "policy-readiness.json",
        "policy-operations.json",
        "policy-history.json",
        "policy-promotion-acknowledgeable.json",
        "preview-promotion-typescript-boundary-gap.json",
    ] {
        cases.push((file, "warnings".to_string(), json!(["w"])));
        cases.push((file, "unknowns".to_string(), json!(["u"])));
    }
    cases.push((
        "policy-operations.json",
        "promotion_blockers".to_string(),
        json!([{"repair_action": "fix"}]),
    ));
    for path in ["summary.durable_suppression_candidates", "summary.warnings"] {
        cases.push(("waiver-aging.json", path.to_string(), json!(1)));
    }
    for path in [
        "missing_owner",
        "missing_reason",
        "stale",
        "overbroad_scope",
        "unknown_selector",
        "preview_without_preview_label",
        "warnings",
        "config_errors",
    ] {
        cases.push((
            "suppression-health.json",
            format!("summary.{path}"),
            json!(1),
        ));
    }
    for (file, path, value) in cases {
        let fixture = Fixture::complete(&format!("fold-open-{file}-{path}"))?;
        write_quiet(&fixture)?;
        let full = format!("{R}/{file}");
        let mut doc = quiet_reports()
            .into_iter()
            .find(|(name, _)| *name == full)
            .map(|(_, doc)| doc)
            .ok_or("unknown report")?;
        set_path(&mut doc, &path, value);
        fixture.json(&full, &doc)?;
        let summary = render_gate(&fixture, false)?;
        assert!(summary.contains(OPEN), "{file} {path}: {summary}");
        assert!(!summary.contains(CLEAN), "{file} {path}");
    }
    Ok(())
}

#[test]
fn removing_any_required_policy_category_keeps_the_fold_open_and_not_clean() -> TestResult {
    let control = Fixture::complete("fold-required-control")?;
    write_quiet(&control)?;
    let summary = render_gate(&control, false)?;
    assert!(
        summary.contains(CLEAN) && !summary.contains(OPEN),
        "{summary}"
    );

    // Each required category: the file names removed together. The promotion
    // category is satisfied by any `policy-promotion-*.json`, so a second one
    // is added and all of them are removed.
    let categories: [(&str, &[&str]); 5] = [
        ("readiness", &["policy-readiness.json"]),
        ("operations", &["policy-operations.json"]),
        ("history", &["policy-history.json"]),
        ("suppression-health", &["suppression-health.json"]),
        (
            "promotion",
            &[
                "policy-promotion-acknowledgeable.json",
                "policy-promotion-visible-only.json",
            ],
        ),
    ];
    for (category, files) in categories {
        let fixture = Fixture::complete(&format!("fold-required-{category}"))?;
        write_quiet(&fixture)?;
        fixture.json(
            &format!("{R}/policy-promotion-visible-only.json"),
            &json!({"target_mode": "visible-only", "warnings": [], "unknowns": []}),
        )?;
        let clean = render_gate(&fixture, false)?;
        assert!(
            clean.contains(CLEAN) && !clean.contains(OPEN),
            "{category} control: {clean}"
        );
        for file in files {
            let path = fixture.root.join(R).join(file);
            assert!(path.is_file(), "{category}: {file} was not written");
            fs::remove_file(path)?;
        }
        let summary = render_gate(&fixture, false)?;
        assert!(summary.contains(OPEN), "{category}: {summary}");
        assert!(!summary.contains(CLEAN), "{category}: {summary}");
    }
    Ok(())
}

#[test]
fn unreadable_wrong_shape_and_missing_counts_are_not_clean() -> TestResult {
    let shapes: [(&str, Option<serde_json::Value>); 6] = [
        ("array", Some(json!([]))),
        ("number", Some(json!(42))),
        (
            "bool-count",
            Some(json!({"warnings": true, "unknowns": []})),
        ),
        ("summary-array", Some(json!({"summary": []}))),
        ("no-counts", Some(json!({"status": "ok"}))),
        ("malformed", None),
    ];
    for (reports_file, _) in quiet_reports() {
        for (shape, doc) in &shapes {
            let fixture = Fixture::complete(&format!("fold-shape-{reports_file}-{shape}"))?;
            write_quiet(&fixture)?;
            match doc {
                Some(doc) => fixture.json(&reports_file, doc)?,
                None => fixture.write(&reports_file, "{not json")?,
            }
            let summary = render_gate(&fixture, false)?;
            assert!(summary.contains(OPEN), "{reports_file} {shape}: {summary}");
            assert!(!summary.contains(CLEAN), "{reports_file} {shape}");
        }
    }
    Ok(())
}

#[test]
fn a_missing_policy_report_keeps_the_fold_open_beside_clean_ones() -> TestResult {
    let fixture = Fixture::complete("fold-partial")?;
    write_quiet(&fixture)?;
    fs::remove_file(
        fixture
            .root
            .join("target/ripr/reports/policy-readiness.json"),
    )?;
    let summary = render_gate(&fixture, false)?;
    assert!(
        summary.contains("<details open><summary>Policy and governance (advisory; no gate configured; some policy reports not found)"),
        "{summary}"
    );
    assert!(!summary.contains(CLEAN), "{summary}");
    Ok(())
}

#[test]
fn extra_promotion_files_cannot_stand_in_for_a_missing_category() -> TestResult {
    let fixture = Fixture::complete("fold-promotion-stand-in")?;
    write_quiet(&fixture)?;
    let reports = "target/ripr/reports";
    // Remove the only policy-promotion report. The non-promotion preview
    // promotion and waiver reports stay clean and present, and must not
    // stand in for the missing required category.
    fs::remove_file(
        fixture
            .root
            .join(reports)
            .join("policy-promotion-acknowledgeable.json"),
    )?;
    assert!(
        fixture
            .root
            .join(reports)
            .join("preview-promotion-typescript-boundary-gap.json")
            .is_file()
    );
    let summary = render_gate(&fixture, false)?;
    assert!(summary.contains(OPEN), "{summary}");
    assert!(!summary.contains(CLEAN), "{summary}");
    Ok(())
}

#[test]
fn conditional_preview_and_waiver_reports_are_not_required_for_a_clean_fold() -> TestResult {
    let fixture = Fixture::complete("fold-conditional")?;
    write_quiet(&fixture)?;
    let reports = "target/ripr/reports";
    fs::remove_file(fixture.root.join(reports).join("waiver-aging.json"))?;
    fs::remove_file(
        fixture
            .root
            .join(reports)
            .join("preview-promotion-typescript-boundary-gap.json"),
    )?;
    fixture.json(
        &format!("{reports}/policy-promotion-visible-only.json"),
        &json!({"target_mode": "visible-only", "warnings": [], "unknowns": []}),
    )?;
    let summary = render_gate(&fixture, false)?;
    assert!(
        summary.contains(CLEAN) && !summary.contains(OPEN),
        "{summary}"
    );
    Ok(())
}

#[test]
fn no_policy_reports_keeps_the_fold_open_and_not_clean() -> TestResult {
    let fixture = Fixture::new("fold-none")?;
    let summary = render_gate(&fixture, false)?;
    assert!(
        summary.contains(
            "<details open><summary>Policy and governance (advisory; no gate configured; no policy reports found)"
        ),
        "{summary}"
    );
    assert!(!summary.contains(CLEAN), "{summary}");
    Ok(())
}

#[test]
fn first_action_status_omits_not_available_rows() -> TestResult {
    let fixture = Fixture::new("first-action-rows")?;
    fixture.json(
        "target/ripr/reports/first-useful-action.json",
        &json!({"status": "no_action"}),
    )?;
    let summary = fixture.render(&[])?;
    let status = block(&summary, "#### First-run status");
    assert!(status.contains("- Status: `no_action`"), "{status}");
    assert!(!status.contains("`not_available`"), "{status}");
    assert!(!status.contains("- Title:"), "{status}");
    Ok(())
}

#[test]
fn an_index_without_a_start_here_path_prints_only_the_placeholder() -> TestResult {
    let fixture = Fixture::new("index-no-start")?;
    fixture.json("target/ripr/reports/index.json", &json!({"summary": {}}))?;
    let summary = fixture.render(&[])?;
    assert!(
        summary.contains("- Start-here artifact: `not_available`\n"),
        "{summary}"
    );
    Ok(())
}

#[test]
fn first_run_status_omits_only_the_not_available_placeholder() -> TestResult {
    let fixture = Fixture::new("status-rows")?;
    fixture.json(
        "target/ripr/reports/start-here.json",
        &json!({"status": "no_action", "selected": {"state": "no_action"}}),
    )?;
    let summary = fixture.render(&[])?;
    assert!(summary.contains("- Status: `no_action`"), "{summary}");
    assert!(
        summary.contains("- Selected state: `no_action`"),
        "{summary}"
    );
    assert!(summary.contains("- Top gap/no-action: `none`"), "{summary}");
    let status = block(&summary, "#### First-run status");
    assert!(!status.contains("`not_available`"), "{status}");
    assert!(!status.contains("- Repair target:"), "{status}");
    Ok(())
}

#[test]
fn start_here_is_named_by_artifact_not_a_target_path() -> TestResult {
    let fixture = Fixture::new("artifact-name")?;
    fixture.write("target/ripr/reports/start-here.md", "# Start\n")?;
    let summary = fixture.render(&[])?;
    let start = block(&summary, "### Start here");
    assert!(
        start.contains("`start-here.md` in the `ripr-reports` artifact"),
        "{start}"
    );
    assert!(
        !start.contains("Open `target/ripr/reports/start-here.md`"),
        "{start}"
    );
    Ok(())
}
