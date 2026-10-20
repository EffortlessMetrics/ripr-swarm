use std::fs;

const BASE_REF: &str = "origin/main";
const FAST_REPORT_PATH: &str = "target/ripr/reports/check-fast.md";
const SELECTOR_REPORT: &str = "check-fast-selector.md";

pub(super) fn run() -> Result<(), String> {
    super::super::ensure_reports_dir()?;
    let mut discover = super::super::changed_files_vs_origin_main;
    let mut write_receipt = |status: &str, reason: &str, count: usize, detail: &str| {
        super::super::write_report(
            SELECTOR_REPORT,
            &selector_report(status, reason, count, detail),
        )
    };
    run_transaction(
        &mut discover,
        super::super::check_fast,
        || {
            fs::read_to_string(FAST_REPORT_PATH)
                .map_err(|error| format!("read {FAST_REPORT_PATH}: {error}"))
        },
        &mut write_receipt,
    )
}

fn run_transaction<D, F, R, W>(
    discover: &mut D,
    run_fast: F,
    read_fast_report: R,
    write_receipt: &mut W,
) -> Result<(), String>
where
    D: FnMut() -> Result<Vec<String>, String>,
    F: FnOnce() -> Result<(), String>,
    R: FnOnce() -> Result<String, String>,
    W: FnMut(&str, &str, usize, &str) -> Result<(), String>,
{
    let before = match discover() {
        Ok(files) => files,
        Err(error) => {
            let primary = format!("check-fast selector unavailable for {BASE_REF}: {error}");
            return fail(write_receipt, "selector_unavailable", 0, &primary);
        }
    };

    if let Err(error) = run_fast() {
        let primary = format!("check-fast gate failed after selector establishment: {error}");
        return fail(write_receipt, "fast_gate_failed", before.len(), &primary);
    }

    let after = match discover() {
        Ok(files) => files,
        Err(error) => {
            let primary = format!(
                "check-fast selector became unavailable after the gate run for {BASE_REF}: {error}"
            );
            return fail(
                write_receipt,
                "selector_unavailable_after_run",
                before.len(),
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
        return fail(
            write_receipt,
            "selector_changed_during_run",
            before.len(),
            &primary,
        );
    }

    let report = match read_fast_report() {
        Ok(report) => report,
        Err(error) => {
            let primary = format!("check-fast success report unavailable: {error}");
            return fail(
                write_receipt,
                "fast_report_unavailable",
                before.len(),
                &primary,
            );
        }
    };
    if let Err(error) = validate_fast_report(&before, &report) {
        let primary = format!("check-fast report does not match its selector: {error}");
        return fail(
            write_receipt,
            "fast_report_mismatch",
            before.len(),
            &primary,
        );
    }

    write_receipt(
        "pass",
        "stable",
        before.len(),
        "selector and conditional gate receipt agree",
    )
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
    if categories.support_tiers {
        required.push("check-support-tiers");
    }
    if categories.cli_help {
        required.push(crate::check_fast_cli::GATE);
    }
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

fn fail<W>(write_receipt: &mut W, reason: &str, count: usize, primary: &str) -> Result<(), String>
where
    W: FnMut(&str, &str, usize, &str) -> Result<(), String>,
{
    match write_receipt("failed", reason, count, primary) {
        Ok(()) => Err(primary.to_string()),
        Err(report_error) => Err(format!(
            "{primary}; additionally failed to write {SELECTOR_REPORT}: {report_error}"
        )),
    }
}

fn selector_report(status: &str, reason: &str, count: usize, detail: &str) -> String {
    let detail = detail.lines().next().unwrap_or("none");
    format!(
        "# check-fast selector\n\nStatus: {status}\nBase ref: {BASE_REF}\nSelector: {reason}\nChanged files: {count}\nDetail: {detail}\n"
    )
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::{run_transaction, selector_report};

    #[test]
    fn child_diagnostics_do_not_change_the_ran_skipped_contract() -> Result<(), String> {
        let report = format!("{EMPTY_FAST_REPORT}{}", crate::check_fast_diagnostics(&[]));
        super::validate_fast_report(&[], &report)
    }

    const EMPTY_FAST_REPORT: &str = concat!(
        "# check-fast report\n\n",
        "Status: pass\n\n",
        "Ran:\n",
        "- fmt --check\n",
        "- check-static-language\n",
        "- check-command-catalog\n",
        "- check-generated\n",
        "- check-generated-clean\n",
        "- check-lint-policy\n\n",
        "Skipped:\n",
        "- check-no-panic-family\n",
        "- check-allow-attributes\n",
        "- check-file-policy\n",
        "- clippy\n",
        "- check-fixture-contracts\n",
    );

    #[test]
    fn selector_failure_stops_before_fast_gate() -> Result<(), String> {
        let fast_ran = Cell::new(false);
        let receipt = RefCell::new(String::new());
        let mut discover = || Err("origin/main unavailable".to_string());
        let mut write = |status: &str, reason: &str, count: usize, detail: &str| {
            *receipt.borrow_mut() = selector_report(status, reason, count, detail);
            Ok(())
        };

        let result = run_transaction(
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
        if !receipt.borrow().contains("Selector: selector_unavailable") {
            return Err(format!("unexpected selector receipt: {}", receipt.borrow()));
        }
        Ok(())
    }

    #[test]
    fn legitimate_empty_selection_is_distinct_from_failure() -> Result<(), String> {
        let receipt = RefCell::new(String::new());
        let mut discover = || Ok(Vec::new());
        let mut write = |status: &str, reason: &str, count: usize, detail: &str| {
            *receipt.borrow_mut() = selector_report(status, reason, count, detail);
            Ok(())
        };

        run_transaction(
            &mut discover,
            || Ok(()),
            || Ok(EMPTY_FAST_REPORT.to_string()),
            &mut write,
        )?;
        let receipt = receipt.borrow();
        if !receipt.contains("Status: pass") || !receipt.contains("Changed files: 0") {
            return Err(format!("empty selector receipt is ambiguous: {receipt}"));
        }
        Ok(())
    }

    const RUST_GATES: [&str; 4] = [
        "check-no-panic-family",
        "check-allow-attributes",
        "check-file-policy",
        "clippy",
    ];

    fn selected_receipt(paths: &[&str], report: &str) -> (Result<(), String>, String) {
        let selected = paths
            .iter()
            .map(|path| (*path).to_string())
            .collect::<Vec<_>>();
        let receipt = RefCell::new(String::new());
        let mut discover = || Ok(selected.clone());
        let mut write = |status: &str, reason: &str, count: usize, detail: &str| {
            *receipt.borrow_mut() = selector_report(status, reason, count, detail);
            Ok(())
        };
        let result = run_transaction(
            &mut discover,
            || Ok(()),
            || Ok(report.to_string()),
            &mut write,
        );
        let written = receipt.borrow().clone();
        (result, written)
    }

    fn require_rust_receipts(paths: &[&str]) -> Result<(), String> {
        let (result, receipt) = selected_receipt(paths, EMPTY_FAST_REPORT);
        let error = result
            .err()
            .ok_or_else(|| format!("missing Rust gates unexpectedly passed for {paths:?}"))?;
        for gate in RUST_GATES {
            if !error.contains(gate) {
                return Err(format!("missing gate {gate} not identified: {error}"));
            }
        }
        if !receipt.contains("Status: fail")
            || !receipt.contains("Selector: fast_report_mismatch")
            || !receipt.contains(&format!("Changed files: {}", paths.len()))
        {
            return Err(format!("wrong missing-gate receipt: {receipt}"));
        }

        let (always, _) = EMPTY_FAST_REPORT
            .split_once("\n\nSkipped:\n")
            .ok_or_else(|| "control report lacks Skipped boundary".to_string())?;
        for omitted in [
            None,
            Some(RUST_GATES[0]),
            Some(RUST_GATES[1]),
            Some(RUST_GATES[2]),
            Some(RUST_GATES[3]),
        ] {
            let rust = RUST_GATES
                .iter()
                .filter(|gate| Some(**gate) != omitted)
                .map(|gate| format!("- {gate}"))
                .collect::<Vec<_>>()
                .join("\n");
            let report = format!("{always}\n{rust}\n\nSkipped:\n- check-fixture-contracts\n");
            let (result, receipt) = selected_receipt(paths, &report);
            match omitted {
                None => {
                    result?;
                    if !receipt.contains("Status: pass") || !receipt.contains("Selector: stable") {
                        return Err(format!("complete Rust receipt failed: {receipt}"));
                    }
                }
                Some(gate) => {
                    let error = result
                        .err()
                        .ok_or_else(|| format!("omitted {gate} passed"))?;
                    if !error.contains(gate) || !receipt.contains("Selector: fast_report_mismatch")
                    {
                        return Err(format!(
                            "omitted {gate} was not rejected: {error}; {receipt}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn committed_policy_paths_require_literal_complete_receipts() -> Result<(), String> {
        // These paths and obligations are an independent oracle for the actual
        // check_fast_gates table, never derived from categorize_changed_files.
        const FLOOR: [&str; 6] = [
            "fmt --check",
            "check-static-language",
            "check-command-catalog",
            "check-generated",
            "check-generated-clean",
            "check-lint-policy",
        ];
        const POLICY: [&str; 3] = [
            "check-covered-by",
            "check-process-policy",
            "check-network-policy",
        ];
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-receipt-obligations-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(std::env::temp_dir()).map_err(|error| error.to_string())?;
        std::fs::create_dir(&root).map_err(|error| error.to_string())?;
        let git = |args: &[&str]| -> Result<(), String> {
            let args = args
                .iter()
                .map(|arg| (*arg).to_string())
                .collect::<Vec<_>>();
            let output = crate::run::capture_output_in_dir(
                "git",
                &args,
                &root,
                "receipt obligation fixture Git",
            )?;
            if output.status.success() {
                Ok(())
            } else {
                Err(format!("fixture Git {args:?}: {}", output.stderr))
            }
        };
        let result = (|| {
            git(&["init", "--initial-branch=main"])?;
            git(&["config", "user.name", "Receipt fixture"])?;
            git(&["config", "user.email", "receipt@example.invalid"])?;
            git(&[
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--allow-empty",
                "-m",
                "base",
            ])?;
            let mut accepted_omissions = Vec::new();
            for (path, policy) in [
                ("policy/process-allowlist.toml", true),
                (".ripr/traceability.toml", true),
                ("policy-not/process-allowlist.toml", false),
                (".ripr-not/traceability.toml", false),
                ("docs/policy/process-allowlist.toml", false),
            ] {
                git(&["update-ref", "refs/remotes/origin/main", "HEAD"])?;
                let file = root.join(path);
                std::fs::create_dir_all(file.parent().ok_or("fixture parent missing")?)
                    .map_err(|error| error.to_string())?;
                std::fs::write(&file, "# receipt control\n").map_err(|error| error.to_string())?;
                git(&["add", "--", path])?;
                git(&[
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "-m",
                    "selected path",
                ])?;
                let files = crate::changed_files_vs_base(&root)?;
                assert_eq!(files, [path], "native committed selector inventory");
                let mut gates = FLOOR.to_vec();
                if policy {
                    gates.extend(POLICY);
                }
                let report = |omitted: Option<&str>| {
                    let ran = gates
                        .iter()
                        .filter(|gate| Some(**gate) != omitted)
                        .map(|gate| format!("- {gate}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    // Put an omitted receipt under Skipped too: it is not execution.
                    format!(
                        "# check-fast report\n\nStatus: pass\n\nRan:\n{ran}\n\nSkipped:\n- {}\n",
                        omitted.unwrap_or("none")
                    )
                };
                let paths = files.iter().map(String::as_str).collect::<Vec<_>>();
                selected_receipt(&paths, &report(None)).0?;
                for gate in &gates {
                    let (result, receipt) = selected_receipt(&paths, &report(Some(gate)));
                    let Some(error) = result.err() else {
                        accepted_omissions.push(format!("{path}: {gate}"));
                        continue;
                    };
                    assert!(error.contains(gate), "{error}");
                    assert!(
                        receipt.contains("Selector: fast_report_mismatch"),
                        "{receipt}"
                    );
                }
            }
            assert!(
                accepted_omissions.is_empty(),
                "accepted missing receipts: {accepted_omissions:?}"
            );
            Ok(())
        })();
        let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
        result.and(cleanup)
    }

    #[test]
    fn empty_selector_requires_each_unconditional_receipt() -> Result<(), String> {
        selected_receipt(&[], EMPTY_FAST_REPORT).0?;
        let mut accepted = Vec::new();
        for gate in [
            "fmt --check",
            "check-static-language",
            "check-command-catalog",
            "check-generated",
            "check-generated-clean",
            "check-lint-policy",
        ] {
            let report = EMPTY_FAST_REPORT.replace(&format!("- {gate}\n"), "");
            match selected_receipt(&[], &report).0 {
                Ok(()) => accepted.push(gate),
                Err(error) => assert!(error.contains(gate), "{error}"),
            }
        }
        assert!(
            accepted.is_empty(),
            "empty selector accepted missing receipts: {accepted:?}"
        );
        Ok(())
    }

    #[test]
    fn selected_manifest_only_requires_rust_gate_receipts() -> Result<(), String> {
        for path in [
            "Cargo.toml",
            "xtask/Cargo.toml",
            "crates/ripr/Cargo.toml",
            "tools/repo-policy/Cargo.toml",
        ] {
            require_rust_receipts(&[path])?;
        }
        Ok(())
    }

    #[test]
    fn selected_lock_only_requires_rust_gate_receipts() -> Result<(), String> {
        require_rust_receipts(&["Cargo.lock"])
    }

    #[test]
    fn selected_manifest_and_lock_require_rust_gate_receipts() -> Result<(), String> {
        require_rust_receipts(&["tools/repo-policy/Cargo.toml", "Cargo.lock"])
    }

    #[test]
    fn selected_cli_parser_owners_require_runtime_parity_receipt() -> Result<(), String> {
        for path in [
            "crates/ripr/src/cli/commands/check.rs",
            "crates/ripr/src/cli/help.rs",
            "crates/ripr/src/cli.rs",
            "crates/ripr/src/app.rs",
            "crates/ripr/src/app/pr_summary/mod.rs",
            "crates/ripr/src/app/annotations.rs",
            "crates/ripr/src/app/pr_evidence.rs",
            "crates/ripr/src/app/impacted_evidence.rs",
            "crates/ripr/src/app/ripr_plus.rs",
            "crates/ripr/src/mcp.rs",
            "crates/ripr/src/mcp/mod.rs",
            "crates/ripr/src/output/first_pr.rs",
            "crates/ripr/src/output/first_pr/options.rs",
        ] {
            let rust = RUST_GATES.map(|gate| format!("- {gate}")).join("\n");
            let report =
                EMPTY_FAST_REPORT.replace("\n\nSkipped:\n", &format!("\n{rust}\n\nSkipped:\n"));
            for incomplete in [
                report.clone(),
                report.replace("Skipped:\n", "Skipped:\n- cli-help-parity\n"),
            ] {
                let (result, receipt) = selected_receipt(&[path], &incomplete);
                let error = result
                    .err()
                    .ok_or_else(|| format!("missing runtime parity receipt passed for {path}"))?;
                if !error.contains("cli-help-parity")
                    || !receipt.contains("Selector: fast_report_mismatch")
                {
                    return Err(format!(
                        "missing parity was not diagnosed: {error}; {receipt}"
                    ));
                }
            }
            let complete = report.replace("\n\nSkipped:\n", "\n- cli-help-parity\n\nSkipped:\n");
            selected_receipt(&[path], &complete).0?;
        }
        Ok(())
    }

    #[test]
    fn unrelated_and_near_match_paths_do_not_select_cli_parity() {
        for paths in [
            vec![],
            vec!["docs/guide.md"],
            vec!["crates/ripr/src/analysis/mod.rs"],
            vec!["crates/ripr/src/output/human.rs"],
            vec!["crates/ripr/src/cli-not/check.rs"],
            vec!["crates/ripr/src/app.rs.bak"],
            vec!["crates/ripr/src/mcp/help.txt"],
            vec!["crates/ripr/src/output/first_pr_not/options.rs"],
        ] {
            let paths = paths.into_iter().map(str::to_string).collect::<Vec<_>>();
            assert!(
                !crate::categorize_changed_files(&paths).cli_help,
                "{paths:?}"
            );
        }
    }

    #[test]
    fn selected_support_tier_inputs_require_leaf_receipt() -> Result<(), String> {
        for path in [
            "docs/specs/RIPR-SPEC-9999-control.md",
            "docs/specs/nested/control.md",
            "docs/status/SUPPORT_TIERS.md",
            "README.md",
            "metrics/rust-repair-trust/corpus.json",
            "xtask/src/main.rs",
            "xtask/src/command.rs",
            "xtask/src/command/front_door.rs",
            ".github/workflows/proof.yml",
            "scripts/proof.sh",
        ] {
            let categories = super::super::super::categorize_changed_files(&[path.to_string()]);
            if !categories.support_tiers {
                return Err(format!("support-tier input was not selected: {path}"));
            }
            let mut report = EMPTY_FAST_REPORT.to_string();
            let mut other_gates = String::new();
            if categories.rust_src {
                for gate in RUST_GATES {
                    other_gates.push_str(&format!("- {gate}\n"));
                }
            }
            if categories.workflow {
                other_gates.push_str("- check-workflows\n");
            }
            report = report.replace("\n\nSkipped:\n", &format!("\n{other_gates}\nSkipped:\n"));
            let (result, receipt) = selected_receipt(&[path], &report);
            let error = result
                .err()
                .ok_or_else(|| format!("missing support leaf passed: {path}"))?;
            if !error.contains("check-support-tiers")
                || !receipt.contains("Selector: fast_report_mismatch")
            {
                return Err(format!(
                    "missing leaf was not diagnosed: {path}; {error}; {receipt}"
                ));
            }
            let complete =
                report.replace("\n\nSkipped:\n", "\n- check-support-tiers\n\nSkipped:\n");
            selected_receipt(&[path], &complete).0?;
        }
        Ok(())
    }

    #[test]
    fn support_tier_near_matches_keep_noop_applicability() -> Result<(), String> {
        for path in [
            "docs/specs/control.txt",
            "docs/specs-not/control.md",
            "docs/status/SUPPORT_TIERS.md.bak",
            "README.md.bak",
            "metrics/rust-repair-trust/corpus.json.bak",
            "xtask/src-not/main.rs",
            "scripts-not/proof.sh",
            ".github/workflows-not/proof.yml",
            "docs/guide.md",
            "crates/ripr/src/lib.rs",
        ] {
            if super::super::super::categorize_changed_files(&[path.to_string()]).support_tiers {
                return Err(format!("unrelated input selected a support scan: {path}"));
            }
        }
        if super::super::super::categorize_changed_files(&[]).support_tiers {
            return Err("zero paths selected a support scan".to_string());
        }
        Ok(())
    }

    #[test]
    fn documentation_and_manifest_near_matches_keep_fast_receipts() -> Result<(), String> {
        for path in [
            "docs/guide.md",
            "docs/Cargo.toml.md",
            "OtherCargo.toml",
            "docs/Cargo.lock",
            "Cargo.lock.md",
        ] {
            let categories = super::super::super::categorize_changed_files(&[path.to_string()]);
            if categories.rust_src || categories.workflow || categories.policy || categories.fixture
            {
                return Err(format!("unrelated path widened conditional gates: {path}"));
            }
            let (result, receipt) = selected_receipt(&[path], EMPTY_FAST_REPORT);
            result?;
            if !receipt.contains("Status: pass") || !receipt.contains("Changed files: 1") {
                return Err(format!("fast path receipt changed for {path}: {receipt}"));
            }
        }
        Ok(())
    }

    #[test]
    fn fixture_manifest_retains_rust_and_fixture_obligations() -> Result<(), String> {
        let paths = ["fixtures/propagate_value_returned/input/Cargo.toml"];
        let categories = super::super::super::categorize_changed_files(&[paths[0].to_string()]);
        if !categories.rust_src || !categories.fixture {
            return Err("fixture manifest lost Rust or fixture category".to_string());
        }
        let (result, _) = selected_receipt(&paths, EMPTY_FAST_REPORT);
        let error = result
            .err()
            .ok_or_else(|| "fixture manifest accepted empty gates".to_string())?;
        if !error.contains("clippy") || !error.contains("check-fixture-contracts") {
            return Err(format!("fixture manifest lost required gates: {error}"));
        }
        Ok(())
    }

    #[test]
    fn selected_rust_path_requires_rust_gate_receipt() -> Result<(), String> {
        for path in [
            "xtask/src/check_fast_strict.rs",
            "tools/repo-policy/src/lib.rs",
            "tools/repo-policy/build.rs",
            "tools/repo-policy/source_identity.rs",
            "tools/repo-policy/tests/frontdoor.rs",
        ] {
            let receipt = RefCell::new(String::new());
            let selected = vec![path.to_string()];
            let mut discover = || Ok(selected.clone());
            let mut write = |status: &str, reason: &str, count: usize, detail: &str| {
                *receipt.borrow_mut() = selector_report(status, reason, count, detail);
                Ok(())
            };

            let result = run_transaction(
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
            if !receipt.borrow().contains("Selector: fast_report_mismatch") {
                return Err(format!("unexpected mismatch receipt: {}", receipt.borrow()));
            }
        }
        Ok(())
    }

    #[test]
    fn selector_change_during_gate_run_fails_closed() -> Result<(), String> {
        let receipt = RefCell::new(String::new());
        let mut calls = 0usize;
        let mut discover = || {
            calls += 1;
            if calls == 1 {
                Ok(vec!["crates/ripr/src/lib.rs".to_string()])
            } else {
                Ok(vec![
                    "crates/ripr/src/lib.rs".to_string(),
                    "xtask/src/main.rs".to_string(),
                ])
            }
        };
        let mut write = |status: &str, reason: &str, count: usize, detail: &str| {
            *receipt.borrow_mut() = selector_report(status, reason, count, detail);
            Ok(())
        };

        let result = run_transaction(
            &mut discover,
            || Ok(()),
            || Ok(EMPTY_FAST_REPORT.to_string()),
            &mut write,
        );
        if result.is_ok() {
            return Err("selector drift during the gate run unexpectedly passed".to_string());
        }
        if !receipt
            .borrow()
            .contains("Selector: selector_changed_during_run")
        {
            return Err(format!("unexpected drift receipt: {}", receipt.borrow()));
        }
        if !receipt.borrow().contains("Changed files: 1") {
            return Err(format!(
                "drift receipt must keep the before-run denominator: {}",
                receipt.borrow()
            ));
        }
        Ok(())
    }
}
