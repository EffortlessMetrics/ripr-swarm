//! Repository workflow and agent policy, independent of the RIPR product.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
pub mod agent_skills;
pub mod ci_scratch;
mod identity;
pub use identity::{enter_workspace_root, preflight, verify_executable_identity, verify_preflight};
pub const ROUTED_RUST_REQUIRED_RESULT_NAME: &str = "Ripr Rust Small Result";

pub fn json_summary_count(value: &Value, key: &str) -> usize {
    value
        .get("summary")
        .and_then(|summary| summary.get(key))
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or(0)
}

pub fn validate_assistant_loop_health_fixture_corpus(
    violations: &mut Vec<String>,
) -> Result<(), String> {
    let base = Path::new("fixtures/boundary_gap/expected/assistant-loop-health");
    validate_assistant_loop_health_fixture_corpus_at(base, violations)
}

pub fn validate_assistant_loop_health_fixture_corpus_at(
    base: &Path,
    violations: &mut Vec<String>,
) -> Result<(), String> {
    if !base.exists() {
        violations.push(format!(
            "assistant-loop-health corpus is missing {}",
            normalize_path(base)
        ));
        return Ok(());
    }

    for required in ["README.md", "corpus.json"] {
        let path = base.join(required);
        if !path.exists() {
            violations.push(format!(
                "assistant-loop-health corpus is missing {}",
                normalize_path(&path)
            ));
        }
    }

    let corpus_path = base.join("corpus.json");
    if !corpus_path.exists() {
        return Ok(());
    }

    let corpus = match read_json_value(&corpus_path) {
        Ok(value) => value,
        Err(err) => {
            violations.push(err);
            return Ok(());
        }
    };
    if json_string_field(&corpus, "kind").as_deref() != Some("assistant_loop_health_corpus") {
        violations.push(
            "assistant-loop-health corpus kind must be assistant_loop_health_corpus".to_string(),
        );
    }
    if json_string_field(&corpus, "spec").as_deref() != Some("RIPR-SPEC-0022") {
        violations.push("assistant-loop-health corpus spec must be RIPR-SPEC-0022".to_string());
    }

    let cases = match corpus.get("cases").and_then(Value::as_array) {
        Some(cases) => cases,
        None => {
            violations.push("assistant-loop-health corpus is missing cases array".to_string());
            return Ok(());
        }
    };

    let required_cases = [
        "complete_improved",
        "partial_missing_optional",
        "missing_required_input",
        "unchanged_after_attempt",
        "regressed_after_attempt",
        "warning_heavy",
        "multi_proof",
    ];
    let mut seen_cases = BTreeSet::new();

    for case in cases {
        let case_id = json_string_field(case, "id").unwrap_or_else(|| "unknown".to_string());
        seen_cases.insert(case_id.clone());

        let expected = match case.get("expected") {
            Some(value) => value,
            None => {
                violations.push(format!(
                    "assistant-loop-health case {case_id} is missing expected"
                ));
                continue;
            }
        };
        let expected_status =
            json_string_field(expected, "status").unwrap_or_else(|| "missing".to_string());

        let report_path = match json_string_field(case, "expected_report") {
            Some(path) => path,
            None => {
                violations.push(format!(
                    "assistant-loop-health case {case_id} is missing expected_report"
                ));
                continue;
            }
        };
        let markdown_path = match json_string_field(case, "expected_markdown") {
            Some(path) => path,
            None => {
                violations.push(format!(
                    "assistant-loop-health case {case_id} is missing expected_markdown"
                ));
                continue;
            }
        };

        if let Some(proofs) = case.get("proofs").and_then(Value::as_array) {
            if proofs.is_empty() {
                violations.push(format!(
                    "assistant-loop-health case {case_id} must name at least one proof input"
                ));
            }
            for proof in proofs {
                match proof.as_str() {
                    Some(path) if Path::new(path).exists() => {}
                    Some(path) => violations.push(format!(
                        "assistant-loop-health case {case_id} proof input is missing {path}"
                    )),
                    None => violations.push(format!(
                        "assistant-loop-health case {case_id} has a non-string proof path"
                    )),
                }
            }
        } else {
            violations.push(format!(
                "assistant-loop-health case {case_id} is missing proofs array"
            ));
        }

        let report = match read_json_value(Path::new(&report_path)) {
            Ok(value) => value,
            Err(err) => {
                violations.push(format!("assistant-loop-health case {case_id}: {err}"));
                continue;
            }
        };
        if json_string_field(&report, "kind").as_deref() != Some("assistant_loop_health") {
            violations.push(format!(
                "assistant-loop-health case {case_id} report kind must be assistant_loop_health"
            ));
        }
        if json_string_field(&report, "status").as_deref() != Some(expected_status.as_str()) {
            violations.push(format!(
                "assistant-loop-health case {case_id} expected status {expected_status}"
            ));
        }
        if serde_json::to_string(&report)
            .map(|text| text.contains("\"static_class\""))
            .unwrap_or(false)
        {
            violations.push(format!(
                "assistant-loop-health case {case_id} report must use grip_class, not static_class"
            ));
        }
        validate_assistant_loop_health_count(violations, &case_id, expected, &report, "proofs");
        for key in [
            "complete",
            "partial",
            "missing_required_input",
            "missing_optional_input",
            "improved",
            "unchanged",
            "regressed",
            "unknown_movement",
            "warnings",
            "repair_queue",
        ] {
            validate_assistant_loop_health_count(violations, &case_id, expected, &report, key);
        }
        if json_usize_field(expected, "repair_queue").unwrap_or(0) > 0
            && !report
                .get("repair_queue")
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    items
                        .iter()
                        .all(|item| json_string_field(item, "repair_kind").is_some())
                })
        {
            violations.push(format!(
                "assistant-loop-health case {case_id} repair_queue entries must include repair_kind"
            ));
        }
        if !report
            .get("limits")
            .and_then(Value::as_array)
            .is_some_and(|limits| {
                limits
                    .iter()
                    .any(|limit| limit.as_str() == Some("Static RIPR evidence only."))
            })
        {
            violations.push(format!(
                "assistant-loop-health case {case_id} report is missing static evidence limit"
            ));
        }

        let markdown = match fs::read_to_string(&markdown_path) {
            Ok(markdown) => markdown,
            Err(err) => {
                violations.push(format!(
                    "assistant-loop-health case {case_id} Markdown missing {}: {err}",
                    markdown_path
                ));
                continue;
            }
        };
        if !markdown.contains(&format!("Status: {expected_status}")) {
            violations.push(format!(
                "assistant-loop-health case {case_id} Markdown must pin status {expected_status}"
            ));
        }
        if json_usize_field(expected, "repair_queue").unwrap_or(0) > 0
            && ![
                "regenerate_proof",
                "regenerate_missing_artifact",
                "rerun_verify_and_receipt",
                "refresh_before_after_evidence",
                "inspect_unchanged_attempt",
                "inspect_regression",
                "inspect_summary_only_guidance",
                "attach_receipt",
                "no_repair",
            ]
            .iter()
            .any(|repair_kind| markdown.contains(repair_kind))
        {
            violations.push(format!(
                "assistant-loop-health case {case_id} Markdown repair queue must include repair_kind"
            ));
        }
    }

    for required in required_cases {
        if !seen_cases.contains(required) {
            violations.push(format!(
                "assistant-loop-health corpus is missing required case {required}"
            ));
        }
    }

    Ok(())
}

pub fn validate_assistant_loop_health_count(
    violations: &mut Vec<String>,
    case_id: &str,
    expected: &Value,
    report: &Value,
    key: &str,
) {
    let Some(expected_count) = json_usize_field(expected, key) else {
        violations.push(format!(
            "assistant-loop-health case {case_id} expected is missing {key}"
        ));
        return;
    };
    let actual_count = json_summary_count(report, key);
    if actual_count != expected_count {
        violations.push(format!(
            "assistant-loop-health case {case_id} expected {key}={expected_count}, got {actual_count}"
        ));
    }
}

pub fn finish_policy_report(
    spec: PolicyReportSpec<'_>,
    violations: &[String],
) -> Result<(), String> {
    finish_policy_report_with_disclosures(spec, violations, &[])
}

/// One advisory report section rendered regardless of pass/fail status: a
/// disclosed limitation of this run (for example the files a parser-backed
/// scan had to fall back on), never a violation.
pub struct PolicyDisclosure {
    pub heading: String,
    pub intro: String,
    pub items: Vec<String>,
}

pub fn finish_policy_report_with_disclosures(
    spec: PolicyReportSpec<'_>,
    violations: &[String],
    disclosures: &[PolicyDisclosure],
) -> Result<(), String> {
    let body = policy_report_body(&spec, violations, disclosures);
    write_report(spec.report_file, &body)?;

    if violations.is_empty() {
        println!(
            "{}: pass (target/ripr/reports/{})",
            spec.check, spec.report_file
        );
        Ok(())
    } else {
        Err(format!(
            "{} failed; see target/ripr/reports/{}\n{}",
            spec.check,
            spec.report_file,
            violations.join("\n")
        ))
    }
}

/// The full markdown body of one policy report: status, why-it-matters,
/// violations, any disclosed limitations of this run, fix guidance, and
/// the rerun command. Pure so the rendering contract stays unit-testable.
pub fn policy_report_body(
    spec: &PolicyReportSpec<'_>,
    violations: &[String],
    disclosures: &[PolicyDisclosure],
) -> String {
    let status = if violations.is_empty() {
        "pass"
    } else {
        "fail"
    };
    let mut body = format!("# {}\n\nStatus: {status}\n\n", spec.check);
    body.push_str("## Why This Matters\n\n");
    body.push_str(spec.why_it_matters);
    body.push_str("\n\n");

    if violations.is_empty() {
        body.push_str("## Violations\n\nNone detected.\n\n");
    } else {
        body.push_str("## Violations\n\n");
        for violation in violations {
            body.push_str("```text\n");
            body.push_str(violation);
            body.push_str("\n```\n\n");
        }
    }

    for disclosure in disclosures {
        body.push_str("## ");
        body.push_str(&disclosure.heading);
        body.push_str("\n\n");
        body.push_str(&disclosure.intro);
        body.push_str("\n\n");
        for item in &disclosure.items {
            body.push_str("- ");
            body.push_str(item);
            body.push('\n');
        }
        body.push('\n');
    }

    if !violations.is_empty() {
        body.push_str("## Fix Kind\n\n```text\n");
        body.push_str(fix_kind_name(&spec.fix_kind));
        body.push_str("\n```\n\n");

        body.push_str("## Recommended Fixes\n\n");
        for (index, fix) in spec.recommended_fixes.iter().enumerate() {
            body.push_str(&format!("{}. {fix}\n", index + 1));
        }
        body.push('\n');

        if let Some(template) = spec.exception_template {
            body.push_str("## Exception Template\n\n```text\n");
            body.push_str(template);
            body.push_str("\n```\n\n");
        }
    }

    body.push_str("## Rerun\n\n```bash\n");
    body.push_str(spec.rerun_command);
    body.push_str("\n```\n");
    body
}

pub fn fix_kind_name(fix_kind: &FixKind) -> &'static str {
    match fix_kind {
        FixKind::AutoFixable => "auto_fixable",
        FixKind::AuthorDecisionRequired => "author_decision_required",
        FixKind::ReviewerDecisionRequired => "reviewer_decision_required",
        FixKind::PolicyExceptionRequired => "policy_exception_required",
    }
}

pub fn check_workflows_impl() -> Result<(), String> {
    let budgets = read_workflow_budgets("policy/workflow_allowlist.txt")?;
    let runtime_allowlist = read_count_allowlist("policy/workflow_action_runtime_allowlist.txt")?;
    let mut violations = Vec::new();

    for path in collect_files(Path::new(".github/workflows"))? {
        let normalized = normalize_path(&path);
        if !(normalized.ends_with(".yml") || normalized.ends_with(".yaml")) {
            continue;
        }
        let Some(budget) = budgets.get(&normalized) else {
            violations.push(format!(
                "missing workflow budget for {normalized} in policy/workflow_allowlist.txt"
            ));
            continue;
        };
        let text = read_text_lossy(&path)?;
        violations.extend(workflow_runtime_violations(
            &normalized,
            &text,
            &runtime_allowlist,
        ));
        violations.extend(workflow_review_thread_mutation_violations(
            &normalized,
            &text,
        ));
        violations.extend(workflow_bare_self_hosted_violations(&normalized, &text));
        violations.extend(workflow_plain_scalar_comment_violations(&normalized, &text));
        violations.extend(scratch_gc_concurrency_violations(&normalized, &text));
        for block in extract_workflow_run_blocks(&text) {
            if block.non_empty_lines > budget.max_non_empty_lines {
                violations.push(format!(
                    "{normalized}:{} run block has {} non-empty line(s), allowed {} ({})",
                    block.line_number,
                    block.non_empty_lines,
                    budget.max_non_empty_lines,
                    budget.reason
                ));
            }
            let lower = block.text.to_ascii_lowercase();
            if lower.contains(shell_fetch_tool_name()) && lower.contains("| sh") {
                violations.push(format!(
                    "{normalized}:{} run block contains network fetch piped to sh",
                    block.line_number
                ));
            }
            if lower.contains(shell_fetch_tool_name()) && lower.contains("| bash") {
                violations.push(format!(
                    "{normalized}:{} run block contains network fetch piped to bash",
                    block.line_number
                ));
            }
        }
    }
    violations.extend(composite_action_run_block_violations(&budgets)?);
    violations.extend(repository_owned_review_thread_mutation_violations()?);
    validate_assistant_loop_health_fixture_corpus(&mut violations)?;
    violations.extend(routed_rust_workflow_contract_violations_for_repo()?);
    violations.extend(ci_scratch::scratch_lease_contract_violations_for_repo()?);

    finish_policy_report(
        PolicyReportSpec {
            report_file: "workflows.md",
            check: "check-workflows",
            why_it_matters: "GitHub Actions should orchestrate xtask, Cargo, and npm commands instead of hiding complex shell logic in workflow YAML.",
            fix_kind: FixKind::PolicyExceptionRequired,
            recommended_fixes: &[
                "Move complex workflow logic into xtask or an npm script owned by the extension surface.",
                "Keep workflow run blocks under the documented line budget.",
                "Use Node-24-backed action majors where official releases exist.",
                "Use Node 24 for VS Code extension build and publish workflows.",
                "Add or adjust a workflow budget entry only when the workflow surface is intentionally larger.",
            ],
            rerun_command: "cargo xtask check-workflows",
            exception_template: Some(
                "policy/workflow_allowlist.txt entry:\n.github/workflows/name.yml|max_non_empty_lines|reason\n\npolicy/workflow_action_runtime_allowlist.txt entry:\n.github/workflows/name.yml|action/ref|max_count|reason",
            ),
        },
        &violations,
    )
}

/// Budget composite-action run blocks like workflow run blocks.
///
/// A local composite action is executed by the workflows that call it, so
/// shell moved into `.github/actions/*/action.yml` must not escape the
/// visible run-block budget that `policy/workflow_allowlist.txt` keeps for
/// workflow YAML (#3841 moved scratch reclamation into such an action).
pub fn composite_action_run_block_violations(
    budgets: &BTreeMap<String, WorkflowBudget>,
) -> Result<Vec<String>, String> {
    let root = Path::new(".github/actions");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut violations = Vec::new();
    for path in collect_files(root)? {
        let normalized = normalize_path(&path);
        if !(normalized.ends_with("/action.yml") || normalized.ends_with("/action.yaml")) {
            continue;
        }
        let text = read_text_lossy(&path)?;
        let blocks = extract_workflow_run_blocks(&text);
        if blocks.is_empty() {
            continue;
        }
        let Some(budget) = budgets.get(&normalized) else {
            violations.push(format!(
                "missing composite action run-block budget for {normalized} in policy/workflow_allowlist.txt"
            ));
            continue;
        };
        for block in blocks {
            if block.non_empty_lines > budget.max_non_empty_lines {
                violations.push(format!(
                    "{normalized}:{} run block has {} non-empty line(s), allowed {} ({})",
                    block.line_number,
                    block.non_empty_lines,
                    budget.max_non_empty_lines,
                    budget.reason
                ));
            }
        }
    }
    Ok(violations)
}

/// Keep the scratch-GC matrix isolated by pool.
///
/// A workflow-level concurrency group serializes the whole matrix behind the
/// slowest or unavailable self-hosted pool. The resulting pending-run
/// eviction is especially dangerous here because `cancelled` is not a failed
/// workflow and therefore produces no useful CI signal.
pub fn scratch_gc_concurrency_violations(path: &str, text: &str) -> Vec<String> {
    const WORKFLOW: &str = ".github/workflows/scratch-gc.yml";
    const GROUP: &str = "group: scratch-gc-${{ github.repository }}-${{ matrix.pool }}";

    if path != WORKFLOW {
        return Vec::new();
    }

    let lines: Vec<&str> = text.lines().collect();
    let has_top_level_concurrency = lines
        .iter()
        .any(|line| line.trim_start().len() == line.len() && line.trim() == "concurrency:");
    let mut in_scratch_job = false;
    let mut in_concurrency = false;
    let mut concurrency_lines = Vec::new();
    for line in &lines {
        let indent = line.len() - line.trim_start().len();
        if *line == "  scratch-gc:" {
            in_scratch_job = true;
            continue;
        }
        if in_scratch_job && indent == 2 && !line.trim().is_empty() {
            in_scratch_job = false;
            in_concurrency = false;
        }
        if in_scratch_job && indent == 4 && line.trim() == "concurrency:" {
            in_concurrency = true;
            continue;
        }
        if in_concurrency {
            if indent <= 4 && !line.trim().is_empty() {
                in_concurrency = false;
            } else {
                concurrency_lines.push(line.trim());
            }
        }
    }
    let has_pool_group = concurrency_lines.contains(&GROUP);
    let has_non_cancelling_pool_queue = concurrency_lines.contains(&"cancel-in-progress: false");

    let mut violations = Vec::new();
    if has_top_level_concurrency {
        violations.push(format!(
            "{WORKFLOW}: scratch-GC concurrency must be job-level and keyed by matrix.pool; workflow-level concurrency starves the matrix when one pool is unavailable"
        ));
    }
    if !has_pool_group || !has_non_cancelling_pool_queue {
        violations.push(format!(
            "{WORKFLOW}: scratch-GC must preserve a non-cancelling per-pool concurrency group ({GROUP})"
        ));
    }
    violations
}

/// Workflow automation must not perform review-thread resolution without adjudication.
///
/// A workflow-side review-thread mutation turns provider failure or an
/// unreviewed finding into an apparently resolved conversation. Review-thread
/// resolution remains an explicit, evidence-backed operator action; the policy
/// rejects common GraphQL/name variants so a renamed blind resolver cannot
/// re-enter unnoticed. Repository-owned delegated automation is scanned too,
/// so a local composite action or xtask helper cannot hide the mutation.
pub fn workflow_review_thread_mutation_violations(path: &str, text: &str) -> Vec<String> {
    if text.lines().any(review_thread_mutation_line) {
        return vec![format!(
            "{path}: workflow contains a review-thread resolution mutation; review-thread resolution requires explicit adjudication outside automated workflow mutation"
        )];
    }
    Vec::new()
}

pub fn review_thread_mutation_line(line: &str) -> bool {
    let normalized = line
        .chars()
        .map(|character| character.to_ascii_lowercase())
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();
    normalized.contains(&review_thread_mutation_token())
}

pub fn review_thread_mutation_token() -> String {
    [
        'r', 'e', 's', 'o', 'l', 'v', 'e', 'r', 'e', 'v', 'i', 'e', 'w', 't', 'h', 'r', 'e', 'a',
        'd',
    ]
    .into_iter()
    .collect()
}

pub fn repository_owned_review_thread_mutation_violations() -> Result<Vec<String>, String> {
    let mut violations = Vec::new();

    for root in [
        Path::new(".github/actions"),
        Path::new(".github/scripts"),
        Path::new("scripts"),
        Path::new("tools"),
        Path::new("xtask/src"),
    ] {
        if !root.exists() {
            continue;
        }
        for path in collect_files(root)? {
            let normalized = normalize_path(&path);
            if normalized.ends_with("xtask/src/tests.rs") {
                continue;
            }
            let text = read_text_lossy(&path)?;
            if text.lines().any(review_thread_mutation_line) {
                violations.push(format!(
                    "{normalized}: repository-owned automation contains a review-thread resolution mutation; review-thread resolution requires explicit adjudication outside automated workflow mutation"
                ));
            }
        }
    }

    Ok(violations)
}

/// Flag a `run:` written as a plain YAML scalar that contains ` #`.
///
/// YAML treats ` #` in a plain scalar as the start of a comment, so the command
/// is silently truncated at that point. When the truncated remainder holds an
/// unterminated quote the shell fails with `unexpected EOF`, and when it does
/// not the step runs a *different, shorter* command with no error at all — the
/// worse outcome. A block scalar (`run: |`) has no comment rule and is immune.
///
/// This is enforced because it actually happened: a `printf` summary line
/// containing an issue reference was cut mid-string, and nothing local caught it
/// — `check-workflows` passed because it never parsed the YAML.
pub fn workflow_plain_scalar_comment_violations(path: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let Some((_, after)) = line.split_once("run:") else {
            continue;
        };
        // Only plain scalars are affected; `run: |` and `run: >` are safe, and a
        // fully quoted scalar carries its own delimiters.
        let body = after.trim_start();
        if body.starts_with('|') || body.starts_with('>') || body.is_empty() {
            continue;
        }
        if body.starts_with('"') || body.starts_with('\'') {
            continue;
        }
        if body.contains(" #") {
            violations.push(format!(
                "{path}:{} plain-scalar `run:` contains ` #`, which YAML reads as a comment and truncates the command; use a `run: |` block scalar",
                index + 1
            ));
        }
    }
    violations
}

pub fn workflow_bare_self_hosted_violations(path: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("runs-on:")
            && lower.contains('[')
            && lower.contains("self-hosted")
            && lower.contains("linux")
            && lower.contains("x64")
        {
            violations.push(format!(
                "{path}:{} bare inline self-hosted/linux/x64 runs-on is forbidden; use explicit group and capacity labels",
                index + 1
            ));
        }

        if line.trim() != "- self-hosted" {
            continue;
        }
        let end = (index + 17).min(lines.len());
        let start = index.saturating_sub(8);
        let window = &lines[start..end];
        let has_linux = window.iter().any(|candidate| candidate.trim() == "- linux");
        let has_x64 = window.iter().any(|candidate| candidate.trim() == "- x64");
        let has_group = window
            .iter()
            .any(|candidate| candidate.trim_start().starts_with("group: em-ci-"));
        let has_capacity = window.iter().any(|candidate| {
            matches!(
                candidate.trim(),
                "- em-ci"
                    | "- ci-nano"
                    | "- policy-nano"
                    | "- workflow-nano"
                    | "- rust-tiny"
                    | "- rust-medium"
                    | "- rust-large"
                    | "- rust-16gb"
                    | "- cx23"
                    | "- cx33"
                    | "- cx43"
                    | "- cx53"
                    | "- cpx42"
            )
        });
        if has_linux && has_x64 && !(has_group && has_capacity) {
            violations.push(format!(
                "{path}:{} bare self-hosted block lacks group/capacity labels",
                index + 1
            ));
        }
    }
    violations
}

pub fn routed_rust_workflow_contract_violations_for_repo() -> Result<Vec<String>, String> {
    let workflow_path = Path::new(".github/workflows/routed-rust.yml");
    if !workflow_path.exists() {
        return Ok(Vec::new());
    }

    let workflow = read_text_lossy(workflow_path)?;
    let reusable_workflow = if workflow.contains(ROUTED_RUST_REUSABLE_WORKFLOW_REF) {
        optional_policy_text(ROUTED_RUST_REUSABLE_WORKFLOW_PATH)?
    } else {
        None
    };
    let settings = optional_policy_text(".github/settings.yml")?;
    let lane_whitelist = optional_policy_text("policy/ci-lane-whitelist.toml")?;

    Ok(routed_rust_workflow_contract_violations_with_reusable(
        &workflow,
        reusable_workflow.as_deref(),
        settings.as_deref(),
        lane_whitelist.as_deref(),
    ))
}

pub fn optional_policy_text(path: &str) -> Result<Option<String>, String> {
    let path = Path::new(path);
    if path.exists() {
        read_text_lossy(path).map(Some)
    } else {
        Ok(None)
    }
}

/// Routed-rust jobs that must each carry an explicit `timeout-minutes`
/// deadline (issue #2230).
pub const ROUTED_RUST_DEADLINE_JOBS: [&str; 8] = [
    "route",
    "detect-docs-only",
    "rust-cx43",
    "rust-cpx42",
    "rust-cx53",
    "rust-github",
    "docs-gate",
    "result",
];

pub const ROUTED_RUST_IMPLEMENTATION_JOBS: [&str; 4] =
    ["rust-cx43", "rust-cpx42", "rust-cx53", "rust-github"];

pub const ROUTED_RUST_REUSABLE_WORKFLOW_PATH: &str = ".github/workflows/rust-gates.yml";

pub const ROUTED_RUST_REUSABLE_WORKFLOW_REF: &str = "uses: ./.github/workflows/rust-gates.yml";

/// Whether any line in the named job block satisfies `predicate`.
/// Job keys are exactly two-space-indented `name:` lines under `jobs:`;
/// anything deeper belongs to the current block.
pub fn routed_rust_job_block_any(
    workflow: &str,
    job: &str,
    mut predicate: impl FnMut(&str) -> bool,
) -> bool {
    let job_header = format!("{job}:");
    let mut in_block = false;
    for line in workflow.lines() {
        let job_level_key = line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('-');
        if job_level_key {
            in_block = line.trim() == job_header;
            continue;
        }
        if in_block && !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        if in_block && predicate(line) {
            return true;
        }
    }
    false
}

pub fn routed_rust_job_block_has_deadline(workflow: &str, job: &str) -> bool {
    routed_rust_job_block_any(workflow, job, |line| {
        !line.trim_start().starts_with('#') && line.contains("timeout-minutes:")
    })
}

pub fn routed_rust_job_uses_reusable_workflow(workflow: &str, job: &str) -> bool {
    routed_rust_job_block_any(workflow, job, |line| {
        line.strip_prefix("    ") == Some(ROUTED_RUST_REUSABLE_WORKFLOW_REF)
    })
}

pub fn routed_rust_job_block_has_with_value(workflow: &str, job: &str, value: &str) -> bool {
    let mut in_with = false;
    routed_rust_job_block_any(workflow, job, |line| {
        if line == "    with:" {
            in_with = true;
            return false;
        }
        if in_with && line.starts_with("    ") && !line.starts_with("     ") {
            in_with = false;
        }
        in_with && line.strip_prefix("      ") == Some(value)
    })
}

pub fn reusable_workflow_jobs(workflow: &str) -> Vec<String> {
    let mut jobs = Vec::new();
    let mut in_jobs = false;
    for line in workflow.lines() {
        if line.trim_end() == "jobs:" {
            in_jobs = true;
            continue;
        }
        if in_jobs && !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        if in_jobs
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('-')
        {
            jobs.push(line.trim().trim_end_matches(':').to_string());
        }
    }
    jobs
}

pub fn routed_rust_workflow_contract_violations(
    workflow: &str,
    settings: Option<&str>,
    lane_whitelist: Option<&str>,
) -> Vec<String> {
    routed_rust_workflow_contract_violations_with_reusable(workflow, None, settings, lane_whitelist)
}

/// How Routed Rust Small treats one GitHub event after reading the workflow YAML.
///
/// The workflow file is the authority. This classifier inspects `on.pull_request.types`
/// so a re-added Draft or label event cannot be hidden behind a hardcoded
/// desired policy (#4380, #4986).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutedRustEventRoute {
    LaunchFullGate,
    WorkflowNotTriggered,
}

pub const ROUTED_RUST_READY_EVENT: &str = "ready_for_review";

pub const ROUTED_RUST_CONCURRENCY_GROUP_SNIPPET: &str = "group: ${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}-${{ github.event_name }}";

pub const ROUTED_RUST_READY_CANCEL_SNIPPET: &str =
    "cancel-in-progress: ${{ github.event_name == 'pull_request' }}";

pub const ROUTED_RUST_IGNORED_LABEL_RESULT_NAME: &str = "Ripr Rust Small Ignored Label Event";

pub const ROUTED_RUST_DRAFT_GUARD_SNIPPET: &str = "github.event.pull_request.draft";

pub fn routed_rust_pull_request_types(workflow: &str) -> Option<Vec<String>> {
    workflow.lines().map(str::trim).find_map(|line| {
        line.strip_prefix("types:")
            .map(str::trim)
            .filter(|rest| rest.starts_with('['))
            .map(|rest| {
                rest.trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
    })
}

pub fn routed_rust_event_route(
    workflow: &str,
    event_name: &str,
    action: Option<&str>,
    _label: Option<&str>,
) -> RoutedRustEventRoute {
    if event_name == "pull_request" {
        let action = action.unwrap_or("");
        if let Some(types) = routed_rust_pull_request_types(workflow)
            && !types.iter().any(|value| value == action)
        {
            return RoutedRustEventRoute::WorkflowNotTriggered;
        }
    }
    RoutedRustEventRoute::LaunchFullGate
}

/// Enforce the Ready-only pull-request admission law declared by
/// `.github/workflows/routed-rust.yml` (#4986).
///
/// The native Draft -> Ready transition is the sole pull-request qualification
/// request. A skipped required job reports success, so this validator fails
/// closed on every path that could resurrect Draft or label participation in
/// the protected required context. The old label-event law (opened /
/// synchronize / reopened / labeled admission, the ignored-label pseudo-result,
/// and the synchronize-only cancellation) is rejected here as the actual
/// failure mode instead of being mandated.
pub fn routed_rust_ready_event_contract_violations(workflow: &str) -> Vec<String> {
    let has_pull_request_trigger = workflow
        .lines()
        .map(str::trim)
        .any(|line| line == "pull_request:");
    if !has_pull_request_trigger {
        return Vec::new();
    }
    let Some(types) = routed_rust_pull_request_types(workflow) else {
        return vec![
            ".github/workflows/routed-rust.yml must declare an inline pull_request types array so the Ready-only admission contract is auditable (#4986)".to_string(),
        ];
    };
    let mut violations = Vec::new();
    let ready_only = [ROUTED_RUST_READY_EVENT];
    let unexpected: Vec<&str> = types
        .iter()
        .map(String::as_str)
        .filter(|value| !ready_only.contains(value))
        .collect();
    if !unexpected.is_empty() || types.len() != ready_only.len() {
        violations.push(format!(
            ".github/workflows/routed-rust.yml pull_request types must be exactly `[{ROUTED_RUST_READY_EVENT}]`; unexpected activity types: {unexpected:?}. The Draft -> Ready transition is the sole pull-request qualification request and every other pull_request activity type must stay withheld (#4986)"
        ));
    }
    if !workflow.contains(ROUTED_RUST_CONCURRENCY_GROUP_SNIPPET) {
        violations.push(
            ".github/workflows/routed-rust.yml must keep the event-qualified concurrency group `${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}-${{ github.event_name }}` so push and manual work cannot replace each other (#4986)".to_string(),
        );
    }
    if !workflow.contains(ROUTED_RUST_READY_CANCEL_SNIPPET) {
        violations.push(
            ".github/workflows/routed-rust.yml must keep `cancel-in-progress: ${{ github.event_name == 'pull_request' }}` so a second Ready transition replaces the prior admission attempt without cancelling push or manual runs (#4986)".to_string(),
        );
    }
    for job in ROUTED_RUST_IMPLEMENTATION_JOBS {
        let mut in_job_condition = false;
        if routed_rust_job_block_any(workflow, job, |line| {
            // Only the job's own `if:` (and its folded continuation lines)
            // decides cancellation; step-level `if: always()` cleanup is fine.
            if line.starts_with("    ") && !line.starts_with("     ") {
                in_job_condition = line.trim_start().starts_with("if:");
            } else if !line.starts_with("      ") && !line.trim().is_empty() {
                in_job_condition = false;
            }
            let normalized: String = line
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
                .to_ascii_lowercase();
            in_job_condition && normalized.contains("always()")
        }) {
            violations.push(format!(
                ".github/workflows/routed-rust.yml implementation job `{job}` must not use `always()` in its job condition; a job-level `always()` survives cancellation, so a second Ready transition queues behind the obsolete head's full gate. Use `!cancelled()` (#6729)"
            ));
        }
    }
    if workflow.contains(ROUTED_RUST_DRAFT_GUARD_SNIPPET) {
        violations.push(
            ".github/workflows/routed-rust.yml must not gate jobs on `github.event.pull_request.draft`; a skipped required job reports success, so job guards cannot distinguish withheld context from proof (#4986)".to_string(),
        );
    }
    if workflow.contains(ROUTED_RUST_IGNORED_LABEL_RESULT_NAME) {
        violations.push(
            ".github/workflows/routed-rust.yml must not post the retired `Ripr Rust Small Ignored Label Event` pseudo-result; the required context is either earned by an exact Ready-head run or absent (#4986)".to_string(),
        );
    }
    let required_name_line = format!("name: {ROUTED_RUST_REQUIRED_RESULT_NAME}");
    if !routed_rust_job_block_any(workflow, "result", |line| {
        line.trim() == required_name_line.as_str()
    }) {
        violations.push(format!(
            ".github/workflows/routed-rust.yml result job must post the static `{ROUTED_RUST_REQUIRED_RESULT_NAME}` context on every run; a conditional or renamed result can hide Draft or label activity from branch protection (#4986)"
        ));
    }
    violations
}

pub fn routed_rust_workflow_contract_violations_with_reusable(
    workflow: &str,
    reusable_workflow: Option<&str>,
    settings: Option<&str>,
    lane_whitelist: Option<&str>,
) -> Vec<String> {
    let mut violations = Vec::new();
    let delegated_jobs: Vec<&str> = ROUTED_RUST_IMPLEMENTATION_JOBS
        .iter()
        .copied()
        .filter(|job| routed_rust_job_uses_reusable_workflow(workflow, job))
        .collect();
    let delegated = !delegated_jobs.is_empty();

    if !delegated_jobs.is_empty() && delegated_jobs.len() != ROUTED_RUST_IMPLEMENTATION_JOBS.len() {
        violations.push(format!(
            ".github/workflows/routed-rust.yml must keep all four implementation jobs inline or delegate all four to `{ROUTED_RUST_REUSABLE_WORKFLOW_PATH}`; delegated {} of 4",
            delegated_jobs.len()
        ));
    }

    let implementation_workflow = if delegated {
        match reusable_workflow {
            Some(reusable) => reusable,
            None => {
                violations.push(format!(
                    ".github/workflows/routed-rust.yml delegates implementation jobs to missing `{ROUTED_RUST_REUSABLE_WORKFLOW_PATH}`"
                ));
                ""
            }
        }
    } else {
        workflow
    };
    let implementation_copies = if delegated { 1 } else { 3 };
    let required_workflow_snippets = [
        (
            "org runner discovery",
            "orgs/EffortlessMetrics/actions/runners",
        ),
        (
            "runner read token fallback",
            "secrets.EM_RUNNER_READ_TOKEN || github.token",
        ),
        ("slurped idle runner query", "jq -s -e --arg model"),
        ("trusted fork fallback reason", "fork_or_untrusted_pr"),
        ("runner API fallback reason", "runner_api_failed"),
        ("no-idle fallback reason", "no_idle_runner"),
        (
            "runner capacity fallback reason",
            "runner_capacity_unavailable",
        ),
        ("CX43 idle route reason", "cx43_idle"),
        ("CPX42 idle route reason", "cpx42_idle"),
        ("CX53 idle route reason", "cx53_idle"),
        ("CX43 capacity label", "rust-medium"),
        ("CPX42 capacity label", "rust-16gb"),
        ("CX53 capacity label", "rust-large"),
        ("normalized result job", "Ripr Rust Small Result"),
        (
            "CX43 conditional implementation job",
            "if: needs.route.outputs.router_target == 'cx43'",
        ),
        (
            "CPX42 conditional implementation job",
            "if: needs.route.outputs.router_target == 'cpx42'",
        ),
        (
            "CX53 conditional implementation job",
            "if: needs.route.outputs.router_target == 'cx53'",
        ),
        (
            "hosted fallback conditional job",
            "needs.route.outputs.router_target == 'github'",
        ),
        (
            "hosted fallback docs-detection guard",
            "needs.detect-docs-only.result == 'success'",
        ),
        (
            "CX43 tempfail fallback predicate",
            "needs.rust-cx43.outputs.scratch_status == 'tempfail'",
        ),
        (
            "CPX42 tempfail fallback predicate",
            "needs.rust-cpx42.outputs.scratch_status == 'tempfail'",
        ),
        (
            "CX53 tempfail fallback predicate",
            "needs.rust-cx53.outputs.scratch_status == 'tempfail'",
        ),
        (
            "normalized tempfail fallback result",
            "disk-guard tempfailed; GitHub-hosted fallback succeeded",
        ),
        (
            "normalized docs detection failure",
            "docs-surface detection result was $DOCS_DETECT_RESULT",
        ),
    ];

    for (label, snippet) in required_workflow_snippets {
        if !workflow.contains(snippet) {
            violations.push(format!(
                ".github/workflows/routed-rust.yml is missing {label}: `{snippet}`"
            ));
        }
    }

    let scratch_tempfail_output = "scratch_status: ${{ steps.scratch.outputs.status }}";
    if !implementation_workflow.contains(scratch_tempfail_output) {
        violations.push(format!(
            "routed Rust implementation authority is missing self-hosted scratch tempfail output: `{scratch_tempfail_output}`"
        ));
    }

    if delegated {
        for job in ROUTED_RUST_IMPLEMENTATION_JOBS {
            if !routed_rust_job_block_any(workflow, job, |line| {
                line.trim_start().starts_with("runner-config:")
            }) {
                violations.push(format!(
                    ".github/workflows/routed-rust.yml delegated job `{job}` must pass the reusable runner-config input"
                ));
            }
        }
        if !implementation_workflow.contains("runs-on: ${{ fromJSON(inputs.runner-config) }}") {
            violations.push(
                "rust-gates.yml must convert the string runner-config input with fromJSON before assigning runs-on".to_string(),
            );
        }
        if !implementation_workflow.contains(
            "runner-config:\n        description: JSON string or object accepted by jobs.<job_id>.runs-on.\n        required: true\n        type: string",
        ) {
            violations.push(
                "rust-gates.yml runner-config input must remain a required string contract for JSON runner values".to_string(),
            );
        }
        if workflow.matches("runner-config: '").count() < ROUTED_RUST_IMPLEMENTATION_JOBS.len() {
            violations.push(
                ".github/workflows/routed-rust.yml must pass JSON-string runner-config values to all four delegated jobs".to_string(),
            );
        }
        for (label, snippet) in [
            ("workflow_call trigger", "workflow_call:"),
            (
                "workflow_call scratch-status output",
                "      scratch_status:\n        value: ${{ jobs.rust-gates.outputs.scratch_status }}",
            ),
            ("disk-guard threshold input", "disk-guard-threshold:"),
            (
                "parameterized scratch free-space floor",
                "ci-disk-guard /mnt/ci-scratch \"${{ inputs.disk-guard-threshold }}\"",
            ),
        ] {
            if !implementation_workflow.contains(snippet) {
                violations.push(format!(
                    "{ROUTED_RUST_REUSABLE_WORKFLOW_PATH} is missing {label}: `{snippet}`"
                ));
            }
        }
        for (job, threshold) in [("rust-cx43", 35), ("rust-cpx42", 35), ("rust-cx53", 50)] {
            let value = format!("disk-guard-threshold: {threshold}");
            if !routed_rust_job_block_has_with_value(workflow, job, &value) {
                violations.push(format!(
                    ".github/workflows/routed-rust.yml delegated job `{job}` must pass `with.{value}`"
                ));
            }
        }
        if !routed_rust_job_block_any(workflow, "rust-cpx42", |line| line.contains("rust-medium")) {
            violations.push(
                ".github/workflows/routed-rust.yml CPX42 implementation job must retain the rust-medium capacity label".to_string(),
            );
        }
    } else {
        for (label, snippet) in [
            (
                "CX43/CPX42 scratch free-space floor",
                "ci-disk-guard /mnt/ci-scratch 35",
            ),
            (
                "CX53 scratch free-space floor",
                "ci-disk-guard /mnt/ci-scratch 50",
            ),
        ] {
            if !workflow.contains(snippet) {
                violations.push(format!(
                    ".github/workflows/routed-rust.yml is missing {label}: `{snippet}`"
                ));
            }
        }
    }

    if !workflow.contains("- name: Upload docs-gate reports\n        if: always()") {
        violations.push(
            ".github/workflows/routed-rust.yml docs-gate artifacts must upload on both successful and failed docs runs".to_string(),
        );
    }
    if !implementation_workflow.contains("if: success() && inputs.run-advisory-reports") {
        violations.push(
            "rust-gates.yml advisory reports must require successful required proof and explicit opt-in".to_string(),
        );
    }
    if !implementation_workflow.contains("if: failure() || inputs.upload-success-artifacts") {
        violations.push(
            "rust-gates.yml must retain failure artifacts and permit explicitly opted-in successful artifacts".to_string(),
        );
    }

    let toolchain_temp_steps = implementation_workflow
        .matches("name: Prepare toolchain temp")
        .count();
    let toolchain_temp_mkdirs = implementation_workflow
        .matches("run: mkdir -p \"$TMPDIR\"")
        .count();
    if toolchain_temp_steps < implementation_copies || toolchain_temp_mkdirs < implementation_copies
    {
        violations.push(format!(
            "routed Rust implementation authority must include `Prepare toolchain temp` before setup; expected {implementation_copies} copy/copies, found {toolchain_temp_steps} step(s) and {toolchain_temp_mkdirs} mkdir command(s)"
        ));
    }

    let scratch_cargo_home =
        "CARGO_HOME: /mnt/ci-scratch/cargo-home/${{ github.run_id }}-${{ github.run_attempt }}";
    let scratch_cargo_homes = implementation_workflow.matches(scratch_cargo_home).count();
    let scratch_cargo_home_cleanups = implementation_workflow
        .matches("rm -rf \"$CARGO_HOME\" \"$CARGO_TARGET_DIR\" \"$TMPDIR\"")
        .count();
    if scratch_cargo_homes < implementation_copies
        || scratch_cargo_home_cleanups < implementation_copies
    {
        violations.push(format!(
            "routed Rust implementation authority must use scratch CARGO_HOME and clean it; expected {implementation_copies} copy/copies, found {scratch_cargo_homes} scratch home(s) and {scratch_cargo_home_cleanups} cleanup command(s)"
        ));
    }

    // Proof-routing slice 6 (docs/PROOF_ROUTING.md): every PR-evidence path must
    // emit the proof route as an advisory dry-run artifact. The command is
    // appended with `|| true` so a route-computation failure never fails the
    // lane, and it runs on all three self-hosted jobs and the hosted fallback so
    // the artifact cannot silently regress. No lane is skipped or gated by it.
    let proof_route_dry_runs = implementation_workflow
        .matches("cargo xtask proof route --base \"$BASE_SHA\" --head \"$HEAD_SHA\" || true")
        .count();
    let expected_proof_route_dry_runs = if delegated { 1 } else { 4 };
    if proof_route_dry_runs < expected_proof_route_dry_runs {
        violations.push(format!(
            "routed Rust implementation authority must emit the advisory proof-route dry-run artifact (`cargo xtask proof route --base \"$BASE_SHA\" --head \"$HEAD_SHA\" || true`); expected {expected_proof_route_dry_runs} copy/copies, found {proof_route_dry_runs}"
        ));
    }

    // Issue #2230 (PR #2228 hang): every routed-rust job carries an explicit
    // job deadline so a hung step fails the job in bounded time instead of
    // holding the required aggregate check open indefinitely. The check is
    // anchored to each named job block, not a global occurrence count: a
    // duplicate or stray `timeout-minutes:` token elsewhere cannot stand in
    // for a job that lost its deadline.
    for job in ROUTED_RUST_DEADLINE_JOBS {
        if delegated_jobs.contains(&job) {
            continue;
        }
        if !routed_rust_job_block_has_deadline(workflow, job) {
            violations.push(format!(
                ".github/workflows/routed-rust.yml job `{job}` must set an explicit `timeout-minutes` job deadline so a hung step fails in bounded time"
            ));
        }
    }
    if delegated {
        let reusable_jobs = reusable_workflow_jobs(implementation_workflow);
        if reusable_jobs.is_empty() {
            violations.push(format!(
                "{ROUTED_RUST_REUSABLE_WORKFLOW_PATH} deadline check analyzed zero `jobs:` entries"
            ));
        }
        for job in reusable_jobs
            .into_iter()
            .filter(|job| !routed_rust_job_block_has_deadline(implementation_workflow, job))
        {
            violations.push(format!(
                "{ROUTED_RUST_REUSABLE_WORKFLOW_PATH} job `{job}` must set an explicit `timeout-minutes` deadline"
            ));
        }
    }

    if workflow.contains("repos/${REPOSITORY}/actions/runners")
        || workflow.contains("repos/$REPOSITORY/actions/runners")
        || workflow.contains("repos/EffortlessMetrics/ripr-swarm/actions/runners")
    {
        violations.push(
            ".github/workflows/routed-rust.yml must use organization runner discovery, not repo-local runner discovery".to_string(),
        );
    }

    if !(workflow.contains("[ \"$EVENT_NAME\" = \"pull_request\" ]")
        && workflow.contains("[ \"$HEAD_REPO\" != \"$REPOSITORY\" ]"))
    {
        violations.push(
            ".github/workflows/routed-rust.yml must guard pull_request events from forks before selecting self-hosted runners".to_string(),
        );
    }

    for forbidden in [
        "github.event.pull_request.head.repo.full_name == github.repository",
        "github.event.pull_request.head.repo.full_name != github.repository",
    ] {
        if workflow.contains(forbidden) {
            violations.push(format!(
                ".github/workflows/routed-rust.yml must keep fork routing in the route job, not on self-hosted implementation job condition `{forbidden}`"
            ));
        }
    }

    if let Some(settings) = settings
        && (settings.contains("name: ripr-swarm") || settings.contains("Ripr Rust Small Result"))
    {
        if !settings.contains("Ripr Rust Small Result") {
            violations.push(
                ".github/settings.yml must require the normalized `Ripr Rust Small Result` check for ripr-swarm".to_string(),
            );
        }
        for forbidden in [
            "Route Ripr Rust Small",
            "Ripr Rust Small on CX43",
            "Ripr Rust Small on CPX42",
            "Ripr Rust Small on CX53",
            "Ripr Rust Small on GitHub Hosted",
        ] {
            if settings.contains(forbidden) {
                violations.push(format!(
                    ".github/settings.yml must not require conditional implementation job `{forbidden}`"
                ));
            }
        }
    }

    if let Some(lane_whitelist) = lane_whitelist
        && lane_whitelist.contains("routed-rust-small")
    {
        if !lane_whitelist.contains("workflow = \".github/workflows/routed-rust.yml\"") {
            violations.push(
                "policy/ci-lane-whitelist.toml must point `routed-rust-small` at `.github/workflows/routed-rust.yml`".to_string(),
            );
        }
        if !lane_whitelist.contains("jobs = [\"Ripr Rust Small Result\"]") {
            violations.push(
                "policy/ci-lane-whitelist.toml must list only `Ripr Rust Small Result` for the routed Rust lane".to_string(),
            );
        }
    }

    violations.extend(routed_rust_ready_event_contract_violations(workflow));

    violations.sort();
    violations.dedup();
    violations
}

pub fn workflow_runtime_violations(
    path: &str,
    text: &str,
    allowlist: &BTreeMap<(String, String), usize>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (old_ref, new_ref) in deprecated_workflow_action_refs() {
        let count = text.matches(old_ref).count();
        if count > 0 {
            violations.push(format!(
                "{path} uses deprecated action runtime ref `{old_ref}` {count} time(s); use `{new_ref}`"
            ));
        }
    }

    if is_extension_node_workflow(path) {
        for (line_number, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if matches!(
                trimmed,
                "node-version: 20" | "node-version: '20'" | "node-version: \"20\""
            ) {
                violations.push(format!(
                    "{path}:{} uses Node 20 for extension tooling; use Node 24",
                    line_number + 1
                ));
            }
        }
    }

    for pattern in workflow_runtime_exception_patterns() {
        let count = text.matches(pattern).count();
        if count == 0 {
            continue;
        }
        let allowed = allowlist
            .get(&(path.to_string(), pattern.to_string()))
            .copied()
            .unwrap_or(0);
        if count > allowed {
            violations.push(format!(
                "{path} uses `{pattern}` {count} time(s), allowed {allowed}; add a reviewed workflow action runtime exception or upgrade the action"
            ));
        }
    }

    for ((allowed_path, pattern), allowed) in allowlist {
        if allowed_path != path {
            continue;
        }
        if !workflow_runtime_exception_patterns().contains(&pattern.as_str()) {
            violations.push(format!(
                "policy/workflow_action_runtime_allowlist.txt has unsupported exception `{pattern}` for {allowed_path}"
            ));
            continue;
        }
        let count = text.matches(pattern).count();
        if count > *allowed {
            violations.push(format!(
                "{path} uses `{pattern}` {count} time(s), allowed {allowed}"
            ));
        }
    }

    violations.sort();
    violations.dedup();
    violations
}

pub fn deprecated_workflow_action_refs() -> &'static [(&'static str, &'static str)] {
    &[
        ("actions/checkout@v4", "actions/checkout@v6"),
        ("actions/setup-node@v4", "actions/setup-node@v6"),
        ("actions/upload-artifact@v4", "actions/upload-artifact@v7"),
        (
            "actions/download-artifact@v4",
            "actions/download-artifact@v8",
        ),
        ("codecov/codecov-action@v4", "codecov/codecov-action@v6"),
    ]
}

pub fn workflow_runtime_exception_patterns() -> &'static [&'static str] {
    &["actions/dependency-review-action@v4"]
}

pub fn is_extension_node_workflow(path: &str) -> bool {
    matches!(
        path,
        ".github/workflows/ci.yml" | ".github/workflows/publish-extension.yml"
    )
}

pub fn json_string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(json_scalar_as_string)
}

pub fn json_usize_field(value: &Value, key: &str) -> Option<usize> {
    value.get(key).and_then(json_scalar_as_usize)
}

pub fn read_json_value(path: &Path) -> Result<Value, String> {
    let text = read_text_lossy(path)?;
    serde_json::from_str(&text)
        .map_err(|err| format!("failed to parse JSON from {}: {err}", normalize_path(path)))
}

pub fn json_scalar_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

pub fn json_scalar_as_usize(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

pub fn write_report(file_name: &str, body: &str) -> Result<(), String> {
    write_report_in(&reports_dir(), file_name, body)
}

/// `write_report` against an explicit report directory, so parameterized
/// pipelines (and their tests) can target a hermetic location.
pub fn write_report_in(directory: &Path, file_name: &str, body: &str) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|err| {
        format!(
            "failed to create {}: {err}\nrerun with `cargo xtask shape` after fixing directory permissions",
            directory.display()
        )
    })?;
    let path = directory.join(file_name);
    fs::write(&path, body).map_err(|err| {
        format!(
            "failed to write {}: {err}\nrerun with `cargo xtask shape` after fixing file permissions",
            path.display()
        )
    })
}

pub fn reports_dir() -> PathBuf {
    Path::new("target").join("ripr").join("reports")
}

pub fn read_count_allowlist(path: &str) -> Result<BTreeMap<(String, String), usize>, String> {
    let text = read_text_lossy(Path::new(path))?;
    parse_count_allowlist(path, &text)
}

pub fn parse_count_allowlist(
    path: &str,
    text: &str,
) -> Result<BTreeMap<(String, String), usize>, String> {
    let mut allowed = BTreeMap::new();
    let mut first_line = BTreeMap::new();
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts = trimmed.split('|').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(format!(
                "{path}:{} expected path|pattern|max_count|reason",
                line_number + 1
            ));
        }
        let max_count = parts[2]
            .parse::<usize>()
            .map_err(|err| format!("{path}:{} invalid max_count: {err}", line_number + 1))?;
        insert_unique_count_allowlist_row(
            &mut allowed,
            &mut first_line,
            path,
            line_number + 1,
            parts[0],
            parts[1],
            max_count,
        )?;
    }
    Ok(allowed)
}

pub fn insert_unique_count_allowlist_row(
    allowed: &mut BTreeMap<(String, String), usize>,
    first_line: &mut BTreeMap<(String, String), usize>,
    source_path: &str,
    line_number: usize,
    row_path: &str,
    pattern: &str,
    max_count: usize,
) -> Result<(), String> {
    let key = (normalize_slashes(row_path), pattern.to_string());
    if let Some(&first) = first_line.get(&key) {
        return Err(format!(
            "{source_path}:{line_number} path|pattern `{row_path}|{pattern}` is duplicated (first declared near line {first})"
        ));
    }
    first_line.insert(key.clone(), line_number);
    allowed.insert(key, max_count);
    Ok(())
}

pub fn read_workflow_budgets(path: &str) -> Result<BTreeMap<String, WorkflowBudget>, String> {
    let mut budgets = BTreeMap::new();
    let text = read_text_lossy(Path::new(path))?;
    for (line_number, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let parts = trimmed.split('|').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(format!(
                "{path}:{} expected path|max_non_empty_lines|reason",
                line_number + 1
            ));
        }
        let max_non_empty_lines = parts[1].parse::<usize>().map_err(|err| {
            format!(
                "{path}:{} invalid max_non_empty_lines: {err}",
                line_number + 1
            )
        })?;
        let budget = WorkflowBudget {
            path: normalize_slashes(parts[0]),
            max_non_empty_lines,
            reason: parts[2].trim().to_string(),
        };
        if budget.reason.is_empty() {
            return Err(format!(
                "{path}:{} reason must not be empty",
                line_number + 1
            ));
        }
        budgets.insert(budget.path.clone(), budget);
    }
    Ok(budgets)
}

pub fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_inner(root, root, &mut files)?;
    Ok(files)
}

pub fn collect_files_inner(
    root: &Path,
    path: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let normalized = normalize_path(path);
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_normalized = normalize_path(relative);
    if should_skip_path(&relative_normalized) {
        return Ok(());
    }
    let metadata =
        fs::metadata(path).map_err(|err| format!("failed to inspect {normalized}: {err}"))?;
    if metadata.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in
            fs::read_dir(path).map_err(|err| format!("failed to read {normalized}: {err}"))?
        {
            let entry = entry.map_err(|err| format!("failed to read {normalized}: {err}"))?;
            collect_files_inner(root, &entry.path(), files)?;
        }
    }
    Ok(())
}

pub fn should_skip_path(path: &str) -> bool {
    path == ".git"
        || path.starts_with(".git/")
        || path == ".claude"
        || path.starts_with(".claude/")
        || path == "target"
        || path.starts_with("target/")
        || path.ends_with("/target")
        || path.contains("/target/")
        || path == ".ripr/release"
        || path.starts_with(".ripr/release/")
        || path.ends_with("/.vscode-test")
        || path.contains("/.vscode-test/")
        || path.ends_with("/node_modules")
        || path.contains("/node_modules/")
        || path.ends_with("/out")
        || path.contains("/out/")
        || path.ends_with("/dist")
        || path.contains("/dist/")
}

pub fn read_text_lossy(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn normalize_path(path: &Path) -> String {
    normalize_slashes(&path.to_string_lossy())
        .trim_start_matches("./")
        .to_string()
}

pub fn normalize_slashes(value: &str) -> String {
    value.replace('\\', "/")
}

pub fn shell_fetch_tool_name() -> &'static str {
    concat!("cu", "rl")
}

pub fn extract_workflow_run_blocks(text: &str) -> Vec<RunBlock> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut idx = 0usize;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim_start();
        if let Some(rest) = workflow_run_value(trimmed) {
            let indent = line.len() - trimmed.len();
            let run_value = rest.trim();
            if run_value == "|" || run_value == ">" || run_value == "|-" || run_value == ">-" {
                let mut block_lines = Vec::new();
                let mut next_idx = idx + 1;
                while next_idx < lines.len() {
                    let next = lines[next_idx];
                    let next_trimmed = next.trim_start();
                    let next_indent = next.len() - next_trimmed.len();
                    if !next_trimmed.is_empty() && next_indent <= indent {
                        break;
                    }
                    block_lines.push(next_trimmed.to_string());
                    next_idx += 1;
                }
                let non_empty_lines = block_lines
                    .iter()
                    .filter(|value| !value.trim().is_empty())
                    .count();
                blocks.push(RunBlock {
                    line_number: idx + 1,
                    non_empty_lines,
                    text: block_lines.join("\n"),
                });
                idx = next_idx;
                continue;
            }
            blocks.push(RunBlock {
                line_number: idx + 1,
                non_empty_lines: usize::from(!run_value.is_empty()),
                text: run_value.to_string(),
            });
        }
        idx += 1;
    }
    blocks
}

pub fn workflow_run_value(trimmed_line: &str) -> Option<&str> {
    trimmed_line
        .strip_prefix("run:")
        .or_else(|| trimmed_line.strip_prefix("- run:"))
}

#[derive(Debug)]
pub struct WorkflowBudget {
    pub path: String,
    pub max_non_empty_lines: usize,
    pub reason: String,
}

#[derive(Debug)]
pub struct RunBlock {
    pub line_number: usize,
    pub non_empty_lines: usize,
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum FixKind {
    #[allow(dead_code, reason = "test-only variant")]
    AutoFixable,
    AuthorDecisionRequired,
    ReviewerDecisionRequired,
    PolicyExceptionRequired,
}

pub struct PolicyReportSpec<'a> {
    pub report_file: &'a str,
    pub check: &'a str,
    pub why_it_matters: &'a str,
    pub fix_kind: FixKind,
    pub recommended_fixes: &'a [&'a str],
    pub rerun_command: &'a str,
    pub exception_template: Option<&'a str>,
}

#[cfg(test)]
mod workflow_tests;
