//! Local composition of the nine independent source/Git policy checks (#5073).
use super::{
    check_allow_attributes, check_covered_by, check_executable_files, check_local_context,
    check_network_policy, check_no_panic_family, check_process_policy, check_static_language,
};

pub(crate) fn check_policy_all() -> Result<(), String> {
    // Evaluate every leaf, in precommit-relative order, before inspecting errors.
    // These leaves read source/policy/Git state independently; file-policy is
    // intentionally excluded because it can invoke Cargo test compilation.
    let outcomes = [
        ("check-static-language", check_static_language()),
        ("check-no-panic-family", check_no_panic_family()),
        ("check-allow-attributes", check_allow_attributes()),
        ("check-local-context", check_local_context()),
        ("check-covered-by", check_covered_by()),
        ("check-executable-files", check_executable_files()),
        ("check-process-policy", check_process_policy()),
        ("check-network-policy", check_network_policy()),
        ("check-lint-policy", crate::check_lint_policy()),
    ];
    let failures = outcomes
        .iter()
        .filter_map(|(command, result)| result.as_ref().err().map(|error| (*command, error)))
        .collect::<Vec<_>>();
    let status = if failures.is_empty() { "pass" } else { "fail" };
    let mut body = format!("# check-policy-all\n\nStatus: {status}\n\nChecks:\n\n");
    for (command, result) in &outcomes {
        let status = if result.is_ok() { "pass" } else { "fail" };
        body.push_str(&format!("- `cargo xtask {command}`: {status}\n"));
    }
    // Errors are authoritative, even when a leaf failed before writing its
    // report. Do not read old leaf reports or invent a violation count from an
    // I/O error. The leaf reports and error text retain their existing contract.
    for (command, error) in &failures {
        body.push_str(&format!(
            "\n## {command}\n\nRerun:\n\n```bash\ncargo xtask {command}\n```\n\n```text\n{error}\n```\n"
        ));
    }
    body.push_str("\nRerun all selected checks:\n\n```bash\ncargo xtask check-policy-all\n```\n");
    let report = crate::write_report("check-policy-all.md", &body);
    if failures.is_empty() {
        report?;
        println!("check-policy-all: pass (target/ripr/reports/check-policy-all.md)");
        return Ok(());
    }
    let mut error = format!(
        "check-policy-all: {} gate(s) failed: {}\n{}",
        failures.len(),
        failures
            .iter()
            .map(|(command, _)| *command)
            .collect::<Vec<_>>()
            .join(", "),
        failures
            .iter()
            .map(|(_, error)| error.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    if let Err(report_error) = report {
        error.push_str(&format!(
            "\nAdditionally failed to write aggregate report: {report_error}"
        ));
    }
    Err(error)
}
