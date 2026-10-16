//! Read the same static workflow-level jq program Actions supplies to each step.
pub fn program(workflow: &str) -> String {
    workflow
        .split_once("  REVIEW_DISPOSITION_JQ: |\n")
        .map(|(_, tail)| {
            tail.lines()
                .take_while(|line| line.is_empty() || line.starts_with("    "))
                .map(|line| line.strip_prefix("    ").unwrap_or(line))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
