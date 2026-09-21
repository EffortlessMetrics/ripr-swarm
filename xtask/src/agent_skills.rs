use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::json;

const SKILLS: [&str; 7] = [
    "build-candidate",
    "deliver-goal",
    "deliver-pr",
    "finish-pr",
    "prepare-issue",
    "prepare-proof",
    "review-pr",
];

/// Product-surface module tokens that AGENTS.md, CLAUDE.md, and
/// docs/ARCHITECTURE.md must name. Pins the #1943/#3774 drift class:
/// those three maps cannot omit these backticked tokens. Adding a new
/// product-map surface means adding the token here and in all three maps.
/// This list is not parsed from `lib.rs`; `git`, `workspace_status`, and
/// private helpers stay off the product map.
const REQUIRED_ARCHITECTURE_MODULES: [&str; 10] = [
    "`domain`",
    "`app`",
    "`analysis`",
    "`output`",
    "`cli`",
    "`lsp`",
    "`agent`",
    "`config`",
    "`mcp`",
    "`provider_contract`",
];

const ARCHITECTURE_MAP_FILES: [&str; 3] = ["AGENTS.md", "CLAUDE.md", "docs/ARCHITECTURE.md"];

const ROOT_REVIEW_ROUTE_MARKER: &str = "review_route:root_to_review_pr";

const REVIEW_ROUTE_REQUIRED_MARKERS: [(&str, &str); 5] = [
    (
        "build-candidate",
        "review_route:build_candidate_to_review_pr",
    ),
    (
        "build-candidate",
        "review_route:repair_returns_to_same_candidate",
    ),
    ("deliver-pr", "review_route:deliver_pr_to_review_pr"),
    ("finish-pr", "review_route:finish_pr_requires_review_ready"),
    (
        "finish-pr",
        "review_route:finish_pr_resolves_repaired_threads",
    ),
];

const REVIEW_PR_REQUIRED_MARKERS: [&str; 17] = [
    "review_contract:exact_head_binding",
    "review_contract:non_mutating_inspection_workspace",
    "review_contract:semantic_owner_and_consumers",
    "review_contract:wrong_behavior_oracle_challenge",
    "review_contract:rendered_behavior",
    "review_contract:contract_parity",
    "review_contract:platform_relevance",
    "review_contract:exact_head_ci_receipts",
    "review_contract:denominator_honesty",
    "review_contract:mutation_or_removal_challenge",
    "review_contract:no_threads_is_not_review",
    "review_contract:green_ci_is_not_semantic_review",
    "review_contract:clean_review_record_not_lgtm",
    "review_contract:author_self_review_comment",
    "review_contract:review_ready_gate",
    "review_contract:repair_same_candidate",
    "review_contract:blocked_is_not_human_cause",
];

/// ZCode/Muse continuously consumes AGENTS.md and the `.agents/skills` tree.
/// Pin the operating contract that prevents session summaries, local progress,
/// base movement, or routine repository writes from silently changing delivery
/// semantics. Claude's separate provider tree remains governed by #3785's
/// provider-parity acceptance and is intentionally not generated from this tree.
const AGENTS_ROOT_OPERATING_MARKERS: [&str; 5] = [
    "operating_contract:primary_authority",
    "operating_contract:routine_repo_writes",
    "operating_contract:ordinary_squash_merge",
    "operating_contract:delivery_state_ladder",
    "operating_contract:host_shell_detection",
];

const AGENTS_SKILL_OPERATING_MARKERS: [(&str, &str); 20] = [
    ("deliver-goal", "goal_contract:parent_end_state"),
    ("deliver-goal", "goal_contract:progress_denominator"),
    ("deliver-goal", "goal_contract:local_work_not_delivery"),
    (
        "deliver-goal",
        "goal_contract:waiting_lane_not_global_blocker",
    ),
    (
        "deliver-goal",
        "goal_contract:subgoal_does_not_close_parent",
    ),
    (
        "deliver-goal",
        "goal_contract:primary_sources_before_summary",
    ),
    ("deliver-pr", "pr_contract:current_claim_search"),
    ("deliver-pr", "pr_contract:duplicate_check_before_conflict"),
    ("deliver-pr", "pr_contract:behind_only_no_restack"),
    ("deliver-pr", "pr_contract:routine_repo_writes"),
    ("deliver-pr", "pr_contract:ordinary_squash_merge"),
    ("deliver-pr", "pr_contract:local_commit_not_delivery"),
    ("build-candidate", "candidate_contract:host_shell_detection"),
    ("build-candidate", "candidate_contract:focused_local_proof"),
    ("build-candidate", "candidate_contract:one_writer_worktree"),
    (
        "build-candidate",
        "candidate_contract:publish_for_remote_evidence",
    ),
    ("finish-pr", "finish_contract:routine_repo_writes"),
    ("finish-pr", "finish_contract:ordinary_squash_merge"),
    ("finish-pr", "finish_contract:duplicate_recheck"),
    ("finish-pr", "finish_contract:behind_only_no_restack"),
];

const PROVIDERS: [(&str, &str, &str, &str, Option<&str>); 3] = [
    (
        "codex",
        "AGENTS.md",
        ".agents/skills",
        ".claude/skills",
        Some("AGENTS.override.md"),
    ),
    (
        "claude",
        "CLAUDE.md",
        ".claude/skills",
        ".agents/skills",
        None,
    ),
    // ZCode reads AGENTS.md and uses the same .agents/skills tree as Codex.
    // It has no separate override file and no separate skill root; the
    // provider entry validates that the shared AGENTS.md carries the root
    // route marker and that the skill tree is complete for this provider too.
    (
        "zcode",
        "AGENTS.md",
        ".agents/skills",
        ".claude/skills",
        None,
    ),
];

pub(crate) fn check() -> Result<(), String> {
    let mut findings = Vec::new();
    validate_architecture_maps(&mut findings);
    validate_agents_operating_contract(&mut findings);
    for (provider, instructions, root, other_root, override_path) in PROVIDERS {
        let text = match fs::read_to_string(instructions) {
            Ok(text) => text,
            Err(error) => {
                findings.push(format!("{provider}: {instructions} unreadable: {error}"));
                String::new()
            }
        };
        if !text.contains(root) {
            findings.push(format!(
                "{provider}: {instructions} does not point at {root}"
            ));
        }
        if has_active_reference(&text, other_root) {
            findings.push(format!("{provider}: root imports {other_root}"));
        }
        let mut provider_text = text.clone();
        if let Some(override_path) = override_path {
            match fs::read_to_string(override_path) {
                Ok(override_text) => {
                    if !override_text.contains(root) {
                        findings.push(format!(
                            "{provider}: {override_path} does not point at {root}"
                        ));
                    }
                    if has_active_reference(&override_text, other_root) {
                        findings.push(format!("{provider}: {override_path} imports {other_root}"));
                    }
                    provider_text.push('\n');
                    provider_text.push_str(&override_text);
                }
                Err(error) => {
                    findings.push(format!("{provider}: {override_path} unreadable: {error}"))
                }
            }
        }
        let routing_text = provider_text.clone();
        validate_root_review_route(provider, &routing_text, &mut findings);
        for skill in SKILLS {
            let relative = format!("{root}/{skill}/SKILL.md");
            let path = Path::new(&relative);
            let skill_text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) => {
                    findings.push(format!("{provider}: {relative} unreadable: {error}"));
                    continue;
                }
            };
            let mut lines = skill_text.lines();
            if lines.next() != Some("---") {
                findings.push(format!("{provider}: {relative} has no frontmatter"));
                continue;
            }
            let mut name = None;
            let mut description = false;
            let mut closed = false;
            for line in lines {
                if line == "---" {
                    closed = true;
                    break;
                }
                if let Some(value) = line.strip_prefix("name:") {
                    name = Some(value.trim());
                }
                if let Some(value) = line.strip_prefix("description:") {
                    description = !value.trim().is_empty();
                }
            }
            if !closed || name != Some(skill) || !description {
                findings.push(format!("{provider}: {relative} has invalid frontmatter"));
            }
            if has_active_reference(&skill_text, other_root) {
                findings.push(format!("{provider}: {relative} imports {other_root}"));
            }
            for sibling in SKILLS {
                if skill_text.contains(sibling)
                    && !Path::new(&format!("{root}/{sibling}/SKILL.md")).is_file()
                {
                    findings.push(format!(
                        "{provider}: {relative} references missing {sibling}"
                    ));
                }
            }
            for finding in review_route_findings(skill, &skill_text) {
                findings.push(format!("{provider}: {relative} {finding}"));
            }
            if skill == "finish-pr" && !skill_text.contains("REVIEW_READY") {
                findings.push(format!(
                    "{provider}: {relative} can converge without REVIEW_READY"
                ));
            }
            if skill == "review-pr" {
                validate_review_pr_contract(provider, &relative, &skill_text, &mut findings);
            }
            provider_text.push('\n');
            provider_text.push_str(&skill_text);
        }
        if provider_text.contains("## Orchestration Operating Model")
            || provider_text.contains("Use role-specific workers")
            || provider_text.contains("### Wave discipline")
        {
            findings.push(format!(
                "{provider}: retired fixed-role orchestration is active"
            ));
        }
        let lines = provider_text.lines().collect::<Vec<_>>();
        for token in [
            "active-goal",
            "current-writer",
            "current-stage",
            "liveness",
            "candidate-frontier",
        ] {
            for (index, line) in lines.iter().enumerate() {
                let lower = line.to_ascii_lowercase();
                if lower.contains(token) && !negative_context(&lines, index) {
                    findings.push(format!(
                        "{provider}: active orchestration authority contains {token}: {line}"
                    ));
                }
            }
        }
        let has_active_kiro = lines.iter().enumerate().any(|(index, line)| {
            let lower = line.to_ascii_lowercase();
            lower.contains("kiro")
                && (lower.contains("route")
                    || lower.contains("lifecycle")
                    || lower.contains("overlay")
                    || lower.contains("skill"))
                && !negative_context(&lines, index)
        });
        if has_active_kiro {
            findings.push(format!(
                "{provider}: active Kiro lifecycle route is present"
            ));
        }
        for state in [
            "PR_IN_FLIGHT",
            "GOAL_IN_FLIGHT",
            "NEEDS_OWNER_DECISION",
            "NOT_ESTABLISHED",
            "REVIEW_READY",
            "REPAIR_REQUIRED",
        ] {
            if !provider_text.contains(state) {
                findings.push(format!("{provider}: required state absent: {state}"));
            }
        }
    }
    let status = if findings.is_empty() {
        "pass"
    } else {
        "failed"
    };
    let report = json!({
        "schema_version": "0.1",
        "status": status,
        "findings": findings,
        "not_enforced": [
            "prose identity", "section-order symmetry", "equal agent counts",
            "equal model choices", "one role per pass",
            "one provider as generated canonical source", "mandatory separate reviewer identity",
            "semantic truth of declared review and operating contract markers",
            "Claude provider prose parity with the AGENTS/ZCode operating contract"
        ]
    });
    crate::write_report(
        "agent-skills.json",
        &(serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize agent skills report: {error}"))?
            + "\n"),
    )?;
    let mut markdown = format!("# Agent skill structure\n\n- Status: {status}\n\n");
    if findings.is_empty() {
        markdown.push_str("## Findings\n\n- none\n");
    } else {
        markdown.push_str("## Findings\n\n");
        for finding in &findings {
            markdown.push_str(&format!("- {finding}\n"));
        }
    }
    crate::write_report("agent-skills.md", &markdown)?;
    if status == "pass" {
        println!("check-agent-skills: pass (target/ripr/reports/agent-skills.md)");
        Ok(())
    } else {
        Err(format!(
            "check-agent-skills found {} issue(s); see target/ripr/reports/agent-skills.md",
            report["findings"].as_array().map_or(0, Vec::len)
        ))
    }
}

fn validate_architecture_maps(findings: &mut Vec<String>) {
    for path in ARCHITECTURE_MAP_FILES {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                findings.push(format!("architecture-map: {path} unreadable: {error}"));
                continue;
            }
        };
        findings.extend(architecture_map_findings(path, &text));
    }
}

fn architecture_map_findings(path: &str, text: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for module in REQUIRED_ARCHITECTURE_MODULES {
        if !text.contains(module) {
            findings.push(format!(
                "architecture-map: {path} omits required module token {module}"
            ));
        }
    }
    findings
}

fn validate_agents_operating_contract(findings: &mut Vec<String>) {
    let agents = match fs::read_to_string("AGENTS.md") {
        Ok(text) => text,
        Err(error) => {
            findings.push(format!("operating-contract: AGENTS.md unreadable: {error}"));
            return;
        }
    };
    for finding in closed_marker_findings(
        &agents,
        "operating_contract:",
        &AGENTS_ROOT_OPERATING_MARKERS,
    ) {
        findings.push(format!("operating-contract: AGENTS.md {finding}"));
    }

    for skill in SKILLS {
        let required = AGENTS_SKILL_OPERATING_MARKERS
            .iter()
            .filter_map(|(owner, marker)| (*owner == skill).then_some(*marker))
            .collect::<Vec<_>>();
        if required.is_empty() {
            continue;
        }
        let relative = format!(".agents/skills/{skill}/SKILL.md");
        let skill_text = match fs::read_to_string(&relative) {
            Ok(text) => text,
            Err(error) => {
                findings.push(format!(
                    "operating-contract: {relative} unreadable: {error}"
                ));
                continue;
            }
        };
        let prefix = required[0]
            .split_once(':')
            .map_or("", |(prefix, _)| prefix)
            .to_string()
            + ":";
        for finding in closed_marker_findings(&skill_text, &prefix, &required) {
            findings.push(format!("operating-contract: {relative} {finding}"));
        }
    }
}

fn closed_marker_findings(text: &str, prefix: &str, required: &[&str]) -> Vec<String> {
    let counts = declared_marker_counts(text, prefix);
    let mut findings = Vec::new();
    for marker in required {
        match counts.get(*marker).copied().unwrap_or(0) {
            0 => findings.push(format!("is missing marker `{marker}`")),
            1 => {}
            count => findings.push(format!("declares marker `{marker}` {count} times")),
        }
    }
    for declared in counts.keys() {
        if !required.contains(&declared.as_str()) {
            findings.push(format!("declares unknown marker `{declared}`"));
        }
    }
    findings
}

fn validate_root_review_route(provider: &str, routing_text: &str, findings: &mut Vec<String>) {
    let counts = declared_marker_counts(routing_text, "review_route:");
    match counts.get(ROOT_REVIEW_ROUTE_MARKER).copied().unwrap_or(0) {
        0 => findings.push(format!(
            "{provider}: root instructions are missing `{ROOT_REVIEW_ROUTE_MARKER}`"
        )),
        1 => {}
        count => findings.push(format!(
            "{provider}: root instructions declare `{ROOT_REVIEW_ROUTE_MARKER}` {count} times"
        )),
    }
    for declared in counts.keys() {
        if declared.as_str() != ROOT_REVIEW_ROUTE_MARKER {
            findings.push(format!(
                "{provider}: root instructions declare unknown review route marker `{declared}`"
            ));
        }
    }
}

fn review_route_findings(skill: &str, skill_text: &str) -> Vec<String> {
    let counts = declared_marker_counts(skill_text, "review_route:");
    let mut findings = Vec::new();

    for (_, required) in REVIEW_ROUTE_REQUIRED_MARKERS
        .iter()
        .filter(|(owner, _)| *owner == skill)
    {
        match counts.get(*required).copied().unwrap_or(0) {
            0 => findings.push(format!("is missing review route marker `{required}`")),
            1 => {}
            count => findings.push(format!(
                "declares review route marker `{required}` {count} times"
            )),
        }
    }
    for declared in counts.keys() {
        if !REVIEW_ROUTE_REQUIRED_MARKERS
            .iter()
            .any(|(owner, marker)| *owner == skill && *marker == declared.as_str())
        {
            findings.push(format!("declares unknown review route marker `{declared}`"));
        }
    }
    findings
}

fn validate_review_pr_contract(
    provider: &str,
    relative: &str,
    skill_text: &str,
    findings: &mut Vec<String>,
) {
    for finding in review_pr_contract_findings(skill_text) {
        findings.push(format!("{provider}: {relative} {finding}"));
    }
}

fn review_pr_contract_findings(skill_text: &str) -> Vec<String> {
    let counts = declared_marker_counts(skill_text, "review_contract:");
    let mut findings = Vec::new();

    for required in REVIEW_PR_REQUIRED_MARKERS {
        match counts.get(required).copied().unwrap_or(0) {
            0 => findings.push(format!("is missing review contract marker `{required}`")),
            1 => {}
            count => findings.push(format!(
                "declares review contract marker `{required}` {count} times"
            )),
        }
    }
    for declared in counts.keys() {
        if !REVIEW_PR_REQUIRED_MARKERS.contains(&declared.as_str()) {
            findings.push(format!(
                "declares unknown review contract marker `{declared}`"
            ));
        }
    }
    findings
}

fn declared_marker_counts(skill_text: &str, prefix: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in skill_text.lines() {
        let trimmed = line.trim();
        let Some(marker) = trimmed
            .strip_prefix("- `")
            .and_then(|value| value.strip_suffix('`'))
        else {
            continue;
        };
        let marker = marker.to_ascii_lowercase();
        if marker.starts_with(prefix) {
            *counts.entry(marker).or_insert(0) += 1;
        }
    }
    counts
}

fn has_active_reference(text: &str, target: &str) -> bool {
    let lines = text.lines().collect::<Vec<_>>();
    lines
        .iter()
        .enumerate()
        .any(|(index, line)| line.contains(target) && !negative_context(&lines, index))
}

fn negative_context(lines: &[&str], index: usize) -> bool {
    (0..=2).any(|offset| {
        index
            .checked_sub(offset)
            .and_then(|candidate| lines.get(candidate))
            .map(|line| line.to_ascii_lowercase())
            .is_some_and(|line| {
                line.contains("do not") || line.contains("no ") || line.contains("without ")
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operating_contract_markers_are_closed_and_discriminating() -> Result<(), String> {
        let complete = AGENTS_ROOT_OPERATING_MARKERS
            .iter()
            .map(|marker| format!("- `{marker}`"))
            .collect::<Vec<_>>()
            .join("\n");
        let findings = closed_marker_findings(
            &complete,
            "operating_contract:",
            &AGENTS_ROOT_OPERATING_MARKERS,
        );
        if !findings.is_empty() {
            return Err(format!(
                "complete operating contract unexpectedly failed: {findings:?}"
            ));
        }

        let removed = AGENTS_ROOT_OPERATING_MARKERS[0];
        let incomplete = complete.replace(&format!("- `{removed}`"), "");
        let findings = closed_marker_findings(
            &incomplete,
            "operating_contract:",
            &AGENTS_ROOT_OPERATING_MARKERS,
        );
        let expected = vec![format!("is missing marker `{removed}`")];
        if findings != expected {
            return Err(format!(
                "operating contract omission should report only `{removed}`, got {findings:?}"
            ));
        }

        let unknown = format!("{complete}\n- `operating_contract:invented_permission`");
        let findings = closed_marker_findings(
            &unknown,
            "operating_contract:",
            &AGENTS_ROOT_OPERATING_MARKERS,
        );
        let expected =
            vec!["declares unknown marker `operating_contract:invented_permission`".to_string()];
        if findings != expected {
            return Err(format!(
                "unknown operating contract marker was not isolated: {findings:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn review_contract_markers_are_closed_and_discriminating() -> Result<(), String> {
        let complete = REVIEW_PR_REQUIRED_MARKERS
            .iter()
            .map(|marker| format!("- `{marker}`"))
            .collect::<Vec<_>>()
            .join("\n");
        let findings = review_pr_contract_findings(&complete);
        if !findings.is_empty() {
            return Err(format!(
                "complete review contract unexpectedly failed: {findings:?}"
            ));
        }

        let removed = REVIEW_PR_REQUIRED_MARKERS[3];
        let incomplete = complete.replace(&format!("- `{removed}`"), "");
        let findings = review_pr_contract_findings(&incomplete);
        let expected = vec![format!("is missing review contract marker `{removed}`")];
        if findings != expected {
            return Err(format!(
                "review contract omission should report only `{removed}`, got {findings:?}"
            ));
        }

        let duplicated = format!("{complete}\n- `{}`", REVIEW_PR_REQUIRED_MARKERS[0]);
        let findings = review_pr_contract_findings(&duplicated);
        let expected = vec![format!(
            "declares review contract marker `{}` 2 times",
            REVIEW_PR_REQUIRED_MARKERS[0]
        )];
        if findings != expected {
            return Err(format!(
                "review contract duplicate was not isolated: {findings:?}"
            ));
        }

        let unknown = format!("{complete}\n- `review_contract:invented_authority`");
        let findings = review_pr_contract_findings(&unknown);
        let expected = vec![
            "declares unknown review contract marker `review_contract:invented_authority`"
                .to_string(),
        ];
        if findings != expected {
            return Err(format!(
                "unknown review contract marker was not isolated: {findings:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn review_route_markers_are_closed_and_discriminating() -> Result<(), String> {
        let complete = [
            "- `review_route:build_candidate_to_review_pr`",
            "- `review_route:repair_returns_to_same_candidate`",
        ]
        .join("\n");
        let findings = review_route_findings("build-candidate", &complete);
        if !findings.is_empty() {
            return Err(format!(
                "complete build-candidate review route unexpectedly failed: {findings:?}"
            ));
        }

        let incomplete = complete.replace("- `review_route:build_candidate_to_review_pr`", "");
        let findings = review_route_findings("build-candidate", &incomplete);
        let expected = vec![
            "is missing review route marker `review_route:build_candidate_to_review_pr`"
                .to_string(),
        ];
        if findings != expected {
            return Err(format!(
                "review route omission did not isolate the required marker: {findings:?}"
            ));
        }

        let unknown = format!("{complete}\n- `review_route:parallel_reviewer_gate`");
        let findings = review_route_findings("build-candidate", &unknown);
        let expected = vec![
            "declares unknown review route marker `review_route:parallel_reviewer_gate`"
                .to_string(),
        ];
        if findings != expected {
            return Err(format!(
                "unknown review route marker was not isolated: {findings:?}"
            ));
        }
        Ok(())
    }

    #[test]
    fn architecture_map_tokens_are_closed_and_discriminating() -> Result<(), String> {
        let complete = REQUIRED_ARCHITECTURE_MODULES.join("\n");
        let findings = architecture_map_findings("AGENTS.md", &complete);
        if !findings.is_empty() {
            return Err(format!(
                "complete architecture map unexpectedly failed: {findings:?}"
            ));
        }

        let incomplete = complete.replace("`mcp`", "mcp");
        let findings = architecture_map_findings("AGENTS.md", &incomplete);
        let expected =
            vec!["architecture-map: AGENTS.md omits required module token `mcp`".to_string()];
        if findings != expected {
            return Err(format!(
                "architecture map omission should report only `mcp` without backticks, got {findings:?}"
            ));
        }

        if ARCHITECTURE_MAP_FILES != ["AGENTS.md", "CLAUDE.md", "docs/ARCHITECTURE.md"] {
            return Err(format!(
                "architecture map file set drifted: {ARCHITECTURE_MAP_FILES:?}"
            ));
        }
        for required in ["`mcp`", "`provider_contract`", "`agent`", "`config`"] {
            if !REQUIRED_ARCHITECTURE_MODULES.contains(&required) {
                return Err(format!("required architecture set omits {required}"));
            }
        }
        Ok(())
    }
}
