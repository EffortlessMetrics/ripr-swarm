use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

const PACKET: &str = "docs/agent-context/delivery-continuation-scenarios.md";

// Independent route pins for this bounded guidance claim. This checks an
// accessible Markdown destination, not prose symmetry or model obedience.
const ENTRYPOINTS: [&str; 4] = [
    "AGENTS.md",
    "CLAUDE.md",
    ".agents/skills/deliver-pr/SKILL.md",
    ".claude/skills/deliver-pr/SKILL.md",
];

// Bounded to plain inline links used by the instruction entrypoints, rather
// than a full Markdown renderer. Inactive text cannot establish discovery.
fn markdown_destinations(text: &str) -> Vec<&str> {
    let mut destinations = Vec::new();
    let mut fenced = false;
    let mut comment = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        comment |= trimmed.contains("<!--");
        if comment {
            if trimmed.contains("-->") {
                comment = false;
            }
            continue;
        }
        for prose in line.split('`').step_by(2) {
            for link in prose.split('[').skip(1) {
                if let Some((label, rest)) = link.split_once("](")
                    && !label.trim().is_empty()
                    && !label.contains(']')
                    && let Some((target, _)) = rest.split_once(')')
                {
                    destinations.push(target);
                }
            }
        }
    }
    destinations
}

#[test]
fn discovery_oracle_rejects_nonlinks_and_inactive_links() -> Result<(), Box<dyn Error>> {
    for text in [
        "packet.md",
        "](packet.md)",
        "[](packet.md)",
        "`[packet](packet.md)`",
        "```md\n[packet](packet.md)\n```",
        "~~~md\n[packet](packet.md)\n~~~",
        "<!-- [packet](packet.md) -->",
        "<!--\n[packet](packet.md)\n-->",
    ] {
        if !markdown_destinations(text).is_empty() {
            return Err(format!("inactive/nonlink text counted as a route: {text}").into());
        }
    }
    if markdown_destinations("Read [packet](packet.md#inputs).") != vec!["packet.md#inputs"] {
        return Err("ordinary inline route was not retained".into());
    }
    Ok(())
}

#[test]
fn native_entrypoints_discover_continuation_scenarios() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let expected = root.canonicalize()?.join(PACKET);
    for entrypoint in ENTRYPOINTS {
        let source = root.join(entrypoint);
        let text = fs::read_to_string(&source)?;
        let parent = source
            .parent()
            .ok_or("entrypoint has no parent directory")?;
        let reachable = markdown_destinations(&text).into_iter().any(|destination| {
            let target = destination.split('#').next().unwrap_or_default();
            !target.is_empty()
                && !target.contains("://")
                && parent
                    .join(Path::new(target))
                    .canonicalize()
                    .is_ok_and(|path| path == expected)
        });
        if !reachable {
            return Err(format!("{entrypoint} has no resolving Markdown route to {PACKET}").into());
        }
    }
    Ok(())
}
