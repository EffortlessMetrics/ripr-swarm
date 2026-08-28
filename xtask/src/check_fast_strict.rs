use std::fs;

const BASE_REF: &str = "origin/main";
const FAST_REPORT_PATH: &str = "target/ripr/reports/check-fast.md";
const SELECTOR_REPORT: &str = "check-fast-selector.md";

pub(super) fn run() -> Result<(), String> {
    super::super::ensure_reports_dir()?;
    let mut discover = super::super::changed_files_vs_origin_main;
    let mut write_selector_report = |body: &str| super::super::write_report(SELECTOR_REPORT, body);
    run_with(
        &mut discover,
        super::super::check_fast,
        || {
            fs::read_to_string(FAST_REPORT_PATH)
                .map_err(|error| format!("read {FAST_REPORT_PATH}: {error}"))
        },
        &mut write_selector_report,
    )
}

fn run_with<D, F, R, W>(
    discover: &mut D,
    run_fast: F,
    read_fast_report: R,
    write_selector_report: &mut W,
) -> Result<(), String>
where
    D: FnMut() -> Result<Vec<String>, String>,
    F: FnOnce() -> Result<(), String>,
    R: FnOnce() -> Result<String, String>,
    W: FnMut(&str) -> Result<(), String>,
{
    let before = match discover() {
        Ok(files) => files,
        Err(error) => {
            let primary = format!("check-fast selector unavailable for {BASE_REF}: {error}");
            return fail_with_receipt(
                write_selector_report,
                &[],
                "selector_unavailable",
                &primary,
            );
        }
    };

    if let Err(error) = run_fast() {
        let primary = format!("check-fast gate failed after selector establishment: {error}");
        return fail_with_receipt(
            write_selector_report,
            &before,
            "fast_gate_failed",
            &primary,
        );
    }

    let after = match discover() {
        Ok(files) => files,
        Err(error) => {
            let primary = format!(
                "check-fast selector became unavailable after the gate run for {BASE_REF}: {error}"
            );
            return fail_with_receipt(
                write_selector_report,
                &before,
                "selector_unavailable_after_run",
                &primary,
            );
        }
    };

    if before != after {
        let primary = format!(
            "check-fast selector changed during the gate run: before={} file(s), after={} file(s)",
            before.len(),
            after.len()
        );
        return fail_with_receipt(
            write_selector_report,
            &before,
            "selector_changed_during_run",
            &primary,
        );
    }

    let fast_report = match read_fast_report() {
        Ok(report) => report,
        Err(error) => {
            let primary = format!("check-fast success report unavailable: {error}");
            return fail_with_receipt(
                write_selector_report,
                &before,
                "fast_report_unavailable",
                &primary,
            );
        }
    };

    if let Err(error) = validate_fast_report(&before, &fast_report) {
        let primary = format!("check-fast report does not match its selector: {error}");
        return fail_with_receipt(
            write_selector_report,
            &before,
            "fast_report_mismatch",
            &primary,
        );
    }

    write_selector_report(&selector_report(
        "pass",
        "stable",
        "matched",
        &before,
        "selector and conditional gate receipt agree",
    ))
    .map_err(|error| format!("write {SELECTOR_REPORT}: {error}"))
}

fn validate_fast_report(files: &[String], report: &str) -> Result<(), String> {
    if !report.contains("Status: pass") {
        return Err("success report does not declare pass".to_string());
    }
    let ran = report
        .split_once("Ran:\n")
        .and_then(|(_, remainder)| remainder.split_once("\n\nSkipped:\n"))
        .map(|(ran, _)| ran)
        .ok_or_else(|| "success report has no bounded Ran/Skipped sections".to_string())?;

    let categories = super::super::categorize_changed_files(files);
    let mut required = Vec::new();
    if categories.rust_src {
        required.extend([
            "check-no-panic-family",
            "check-allow-attributes",
            "check-file-policy",
            "clippy",
        ]);
    }
    if categories.workflow {
        required.push("check-workflows");
    }
    if categories.policy {
        required.extend(["check-process-policy", "check-network-policy"]);
    }
    if categories.fixture {
        required.push("check-fixture-contracts");
    }

    let missing = required
        .into_iter()
        .filter(|gate| {
            let expected = format!("- {gate}");
            !ran.lines().any(|line| line.trim() == expected)
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "required ran gate(s) absent: {}",
            missing.join(", ")
        ))
    }
}

fn fail_with_receipt<W>(
    write_selector_report: &mut W,
    files: &[String],
    failure: &str,
    primary: &str,
) -> Result<(), String>
where
    W: FnMut(&str) -> Result<(), String>,
{
    let report = selector_report("failed", failure, "not_accepted", files, primary);
    match write_selector_report(&report) {
        Ok(()) => Err(primary.to_string()),
        Err(report_error) => Err(format!(
            "{primary}; additionally failed to write {SELECTOR_REPORT}: {report_error}"
        )),
    }
}

fn selector_report(
    status: &str,
    selector: &str,
    fast_report: &str,
    files: &[String],
    detail: &str,
) -> String {
    let mut body = format!(
        "# check-fast selector\n\n- Status: {status}\n- Base ref: {BASE_REF}\n- Selector: {selector}\n- Fast report: {fast_report}\n- Changed files: {}\n- Detail: {}\n\n## Paths\n\n",
        files.len(),
        bounded_detail(detail)
    );
    if files.is_empty() {
        body.push_str("- none\n");
    } else {
        for path in files {
            body.push_str(&format!("- `{}`\n", path.replace('`', "\\`")));
        }
    }
    body
}

fn bounded_detail(detail: &str) -> String {
    let flattened = detail.lines().collect::<Vec<_>>().join(" ");
    let mut bounded = flattened.chars().take(400).collect::<String>();
    if flattened.chars().count() > 400 {
        bounded.push('…');
    }
    bounded
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::run_with;

    const EMPTY_FAST_REPORT: &str = "# check-fast report\n\nStatus: pass\n\nRan:\n- fmt --check\n- check-static-language\n- check-command-catalog\n- check-generated\n- check-generated-clean\n- check-lint-policy\n\nSkipped:\n- check-no-panic-family\n- check-allow-attributes\n- check-file-policy\n- clippy\n- check-fixture-contracts\n";

    #[test]
    fn selector_failure_stops_before_fast_gate() -> Result<(), String> {
        let fast_ran = Cell::new(false);
        let receipts = RefCell::new(Vec::new());
        let mut discover = || Err("origin/main unavailable".to_string());
        let mut write = |body: &str| {
            receipts.borrow_mut().push(body.to_string());
            Ok(())
        };

        let result = run_with(
            &mut discover,
            || {
                fast_ran.set(true);
                Ok(())
            },
            || Ok(EMPTY_FAST_REPORT.to_string()),
            &mut write,
        );
        if result.is_ok() {
            return Err("selector failure unexpectedly passed".to_string());
        }
        if fast_ran.get() {
            return Err("fast gate ran without selector authority".to_string());
        }
        let receipts = receipts.borrow();
        let Some(receipt) = receipts.first() else {
            return Err("selector failure emitted no receipt".to_string());
        };
        if !receipt.contains("selector_unavailable") {
            return Err(format!("unexpected selector failure receipt: {receipt}"));
        }
        Ok(())
    }

    #[test]
    fn legitimate_empty_selection_is_distinct_from_failure() -> Result<(), String> {
        let receipts = RefCell::new(Vec::new());
        let mut discover = || Ok(Vec::new());
        let mut write = |body: &str| {
            receipts.borrow_mut().push(body.to_string());
            Ok(())
        };

        run_with(
            &mut discover,
            || Ok(()),
            || Ok(EMPTY_FAST_REPORT.to_string()),
            &mut write,
        )?;
        let receipts = receipts.borrow();
        let Some(receipt) = receipts.first() else {
            return Err("empty selector emitted no receipt".to_string());
        };
        if !receipt.contains("- Status: pass") || !receipt.contains("- Changed files: 0") {
            return Err(format!("empty selector receipt is ambiguous: {receipt}"));
        }
        Ok(())
    }

    #[test]
    fn selected_rust_path_requires_rust_gate_receipt() -> Result<(), String> {
        let receipts = RefCell::new(Vec::new());
        let selected = vec!["xtask/src/check_fast_strict.rs".to_string()];
        let mut discover = || Ok(selected.clone());
        let mut write = |body: &str| {
            receipts.borrow_mut().push(body.to_string());
            Ok(())
        };

        let result = run_with(
            &mut discover,
            || Ok(()),
            || Ok(EMPTY_FAST_REPORT.to_string()),
            &mut write,
        );
        let error = match result {
            Ok(()) => return Err("missing Rust gate receipt unexpectedly passed".to_string()),
            Err(error) => error,
        };
        if !error.contains("check-no-panic-family") || !error.contains("clippy") {
            return Err(format!("missing Rust gates were not identified: {error}"));
        }
        let receipts = receipts.borrow();
        let Some(receipt) = receipts.first() else {
            return Err("Rust gate mismatch emitted no selector receipt".to_string());
        };
        if !receipt.contains("fast_report_mismatch") {
            return Err(format!("unexpected Rust mismatch receipt: {receipt}"));
        }
        Ok(())
    }
}
