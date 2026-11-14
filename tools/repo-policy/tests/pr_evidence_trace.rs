//! Hosted source contract only: telemetry is enabled for PR evidence, not globally.
//! This does not establish analyzer speed or a native queue measurement (#12940).
const WORKFLOW: &str = include_str!("../../../.github/workflows/rust-gates.yml");
const ENABLE: &str = "          RIPR_REPO_EXPOSURE_LATENCY_TRACE: \"1\"";

fn trace_is_step_scoped(workflow: &str) -> bool {
    let Some((before, tail)) = workflow.split_once("      - name: PR evidence\n") else {
        return false;
    };
    let (step, after) = tail.split_once("\n      - ").unwrap_or((tail, ""));
    let Some((_, env_and_body)) = step.split_once("        env:\n") else {
        return false;
    };
    let Some((env, _)) = env_and_body.split_once("        run: |\n") else {
        return false;
    };
    let active_trace = |line: &str| {
        line.trim_start()
            .starts_with("RIPR_REPO_EXPOSURE_LATENCY_TRACE:")
    };
    env.lines().filter(|line| *line == ENABLE).count() == 1
        && workflow.lines().filter(|line| active_trace(line)).count() == 1
        && !before.lines().chain(after.lines()).any(active_trace)
}

#[test]
fn pr_evidence_enables_existing_analyzer_trace_only_for_its_step() {
    assert!(trace_is_step_scoped(WORKFLOW));
}

#[test]
fn trace_contract_refuses_missing_noncanonical_commented_or_global_switch() {
    let enabled = format!(
        "      - name: PR evidence\n        env:\n{ENABLE}\n        run: |\n          cargo xtask ripr-pr\n      - name: Next\n"
    );
    assert!(trace_is_step_scoped(&enabled));
    for replacement in [
        "",
        "          # RIPR_REPO_EXPOSURE_LATENCY_TRACE: \"1\"",
        "          RIPR_REPO_EXPOSURE_LATENCY_TRACE: \"0\"",
    ] {
        assert!(!trace_is_step_scoped(&enabled.replace(ENABLE, replacement)));
    }
    let global = format!("env:\n  RIPR_REPO_EXPOSURE_LATENCY_TRACE: \"1\"\n{enabled}");
    assert!(!trace_is_step_scoped(&global));
}
