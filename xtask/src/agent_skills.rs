use std::fs;
use std::path::Path;

use serde_json::json;

const SKILLS: [&str; 6] = [
    "build-candidate",
    "deliver-goal",
    "deliver-pr",
    "finish-pr",
    "prepare-issue",
    "prepare-proof",
];

const PROVIDERS: [(&str, &str, &str, &str); 2] = [
    ("codex", "AGENTS.md", ".agents/skills", ".claude/skills"),
    ("claude", "CLAUDE.md", ".claude/skills", ".agents/skills"),
];

pub(crate) fn check() -> Result<(), String> {
    let mut findings = Vec::new();
    for (provider, instructions, root, other_root) in PROVIDERS {
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
        if text.contains(&format!("{other_root}/"))
            && !text.contains("Do not route")
            && !text.contains("Do not import or route")
        {
            findings.push(format!("{provider}: root imports {other_root}"));
        }
        if text.contains("## Orchestration Operating Model")
            || text.contains("Use role-specific workers")
            || text.contains("### Wave discipline")
        {
            findings.push(format!(
                "{provider}: retired fixed-role orchestration is active"
            ));
        }
        for token in [
            "active-goal",
            "current-writer",
            "current-stage",
            "liveness",
            "candidate-frontier",
        ] {
            for line in text.lines() {
                let lower = line.to_ascii_lowercase();
                if lower.contains(token)
                    && !lower.contains("no ")
                    && !lower.contains("not ")
                    && !lower.contains("do not")
                    && !text.contains("Do not create repository-global")
                {
                    findings.push(format!(
                        "{provider}: active orchestration authority contains {token}: {line}"
                    ));
                }
            }
        }
        let has_active_kiro = text.lines().any(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("kiro")
                && (lower.contains("route")
                    || lower.contains("lifecycle")
                    || lower.contains("overlay")
                    || lower.contains("skill"))
                && !lower.contains("no kiro")
                && !lower.contains("without kiro")
        });
        if has_active_kiro
            && !(text.contains("Do not create fixed actor rosters")
                && text.contains("Kiro lifecycle routes"))
        {
            findings.push(format!(
                "{provider}: active Kiro lifecycle route is present"
            ));
        }
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
            if skill_text.contains(other_root) {
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
        }
    }
    let all_provider_text = PROVIDERS
        .iter()
        .flat_map(|(_, instructions, root, _)| {
            let mut paths = vec![instructions.to_string()];
            paths.extend(
                SKILLS
                    .iter()
                    .map(|skill| format!("{root}/{skill}/SKILL.md")),
            );
            paths
        })
        .filter_map(|path| fs::read_to_string(path).ok())
        .collect::<Vec<_>>()
        .join("\n");
    for state in [
        "PR_IN_FLIGHT",
        "GOAL_IN_FLIGHT",
        "NEEDS_OWNER_DECISION",
        "NOT_ESTABLISHED",
    ] {
        if !all_provider_text.contains(state) {
            findings.push(format!("required state absent: {state}"));
        }
    }
    let status = if findings.is_empty() { "pass" } else { "fail" };
    let report = json!({
        "schema_version": "0.1",
        "status": status,
        "findings": findings,
        "not_enforced": [
            "prose identity", "section-order symmetry", "equal agent counts",
            "equal model choices", "one role per pass",
            "one provider as generated canonical source"
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
