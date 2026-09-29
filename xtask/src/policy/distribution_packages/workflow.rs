use super::{ARCHIVE_WORKFLOW_PATH, DistributionTarget, RULE_WORKFLOW, WorkflowTarget, rule};

pub(super) fn parse_workflow_targets(
    text: &str,
    violations: &mut Vec<String>,
) -> Vec<WorkflowTarget> {
    let mut rows = Vec::new();
    let mut current_target: Option<(String, usize)> = None;
    let mut current_executable: Option<String> = None;

    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(target) = trimmed.strip_prefix("- target: ") {
            flush_workflow_target(
                &mut rows,
                &mut current_target,
                &mut current_executable,
                violations,
            );
            current_target = Some((unquote(target).to_string(), index + 1));
        } else if current_target.is_some()
            && let Some(executable) = trimmed.strip_prefix("executable: ")
        {
            current_executable = Some(unquote(executable).to_string());
        }
    }
    flush_workflow_target(
        &mut rows,
        &mut current_target,
        &mut current_executable,
        violations,
    );

    if rows.is_empty() {
        violations.push(rule(
            RULE_WORKFLOW,
            ARCHIVE_WORKFLOW_PATH,
            "no `matrix.include` target rows were parsed",
        ));
    }
    rows
}

fn flush_workflow_target(
    rows: &mut Vec<WorkflowTarget>,
    current_target: &mut Option<(String, usize)>,
    current_executable: &mut Option<String>,
    violations: &mut Vec<String>,
) {
    let Some((rust_target, line)) = current_target.take() else {
        return;
    };
    match current_executable.take() {
        Some(executable) => rows.push(WorkflowTarget {
            rust_target,
            executable,
        }),
        None => violations.push(rule(
            RULE_WORKFLOW,
            ARCHIVE_WORKFLOW_PATH,
            &format!("target `{rust_target}` at line {line} has no executable field"),
        )),
    }
}

fn unquote(value: &str) -> &str {
    let trimmed = value.trim();
    trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(trimmed)
}

pub(super) fn validate_workflow_projection(
    manifest_targets: &[DistributionTarget],
    workflow_targets: &[WorkflowTarget],
    violations: &mut Vec<String>,
) {
    let manifest_rows = manifest_targets
        .iter()
        .map(|target| WorkflowTarget {
            rust_target: target.rust_target.clone(),
            executable: target.executable.clone(),
        })
        .collect::<Vec<_>>();
    if manifest_rows != workflow_targets {
        violations.push(rule(
            RULE_WORKFLOW,
            ARCHIVE_WORKFLOW_PATH,
            &format!(
                "archive workflow target/executable rows must equal the distribution manifest; expected {manifest_rows:?}, got {workflow_targets:?}"
            ),
        ));
    }
}
