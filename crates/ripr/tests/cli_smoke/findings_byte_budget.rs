use super::{run_ripr, run_ripr_with_env, unique_temp_workspace};

/// Top-level findings-array byte budget (#5203): a tiny budget renders the
/// deterministic first-finding prefix with disclosed totals, `0` restores the
/// full document byte-identically, and an invalid value fails closed.
#[test]
fn check_findings_byte_budget_bounds_array_with_disclosed_totals() -> Result<(), String> {
    let root = unique_temp_workspace("findings-byte-budget");
    let result = (|| {
        let fixture = root.join("budget");
        std::fs::create_dir_all(fixture.join("src")).map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.join("Cargo.toml"),
            "[package]\nname = \"findings_byte_budget\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
        )
        .map_err(|error| error.to_string())?;
        std::fs::write(
            fixture.join("src/lib.rs"),
            "pub fn record_mark(values: &mut Vec<u32>) {\n    values.push(1);\n}\n#[cfg(test)]\nmod tests {\nuse super::record_mark;\n#[test]\nfn case_00() {\nlet mut values = Vec::new();\nrecord_mark(&mut values);\nassert_eq!(values, vec![1]);\n}\n}\n",
        )
        .map_err(|error| error.to_string())?;
        let diff = fixture.join("change.patch");
        std::fs::write(
            &diff,
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn record_mark(values: &mut Vec<u32>) {\n-    values.push(0);\n+    values.push(1);\n }\n",
        )
        .map_err(|error| error.to_string())?;
        let root_arg = fixture.to_string_lossy().into_owned();
        let diff_arg = diff.to_string_lossy().into_owned();
        let args = [
            "check",
            "--root",
            root_arg.as_str(),
            "--diff",
            diff_arg.as_str(),
            "--format",
            "json",
        ];

        // Baseline: the tiny fixture renders fully with no bound disclosure.
        let default = run_ripr(&args);
        assert!(
            default.status.success(),
            "default JSON: {}",
            String::from_utf8_lossy(&default.stderr)
        );
        let baseline: serde_json::Value =
            serde_json::from_slice(&default.stdout).map_err(|error| error.to_string())?;
        let baseline_findings = baseline["findings"]
            .as_array()
            .ok_or("baseline must carry a findings array")?;
        assert!(
            baseline_findings.len() >= 2,
            "fixture must yield >= 2 findings to discriminate truncation, got {}",
            baseline_findings.len()
        );
        assert!(
            baseline.get("run_limitations").is_none(),
            "unbounded baseline must not disclose a bound: {}",
            String::from_utf8_lossy(&default.stdout)
        );
        let total = baseline_findings.len();
        let first_id = baseline_findings[0]["id"]
            .as_str()
            .ok_or("baseline first finding must carry an id")?
            .to_string();

        // Tiny budget: exactly the first finding renders (first-always), with
        // reconciling totals and a named run status. Budget 1 is robust to
        // pretty-print width: no real finding fits in one byte.
        let bounded = run_ripr_with_env(&args, &[("RIPR_CHECK_FINDINGS_BYTES", "1")]);
        assert!(
            bounded.status.success(),
            "bounded JSON: {}",
            String::from_utf8_lossy(&bounded.stderr)
        );
        let bounded_report: serde_json::Value =
            serde_json::from_slice(&bounded.stdout).map_err(|error| error.to_string())?;
        let bounded_findings = bounded_report["findings"]
            .as_array()
            .ok_or("bounded run must carry a findings array")?;
        assert_eq!(
            bounded_findings.len(),
            1,
            "budget 1 must render exactly the first finding, got {}",
            bounded_findings.len()
        );
        assert_eq!(
            bounded_findings[0]["id"], first_id,
            "bounded prefix must be the deterministic first finding"
        );
        assert_eq!(
            bounded_report["summary"]["findings"], total,
            "summary keeps full analysis counts under a render bound"
        );
        let limitations = bounded_report["run_limitations"]
            .as_array()
            .ok_or("bounded run must disclose run_limitations")?;
        assert_eq!(limitations.len(), 1, "one bound, one entry");
        let entry = &limitations[0];
        assert_eq!(entry["run_status"], "limited_findings_bound");
        assert_eq!(entry["category"], "limited_findings_bound");
        assert_eq!(entry["downstream_consumable"], false);
        let message = entry["message"].as_str().ok_or("entry needs a message")?;
        assert!(
            message.contains(&format!("1 of {total}")),
            "message must reconcile rendered/total: {message}"
        );
        assert!(
            message.contains("RIPR_CHECK_FINDINGS_BYTES"),
            "message must name the applied limit: {message}"
        );
        assert_eq!(entry["repair_route"], "output/check-findings-budget");

        // Opt-out restores the full document byte-identically (removal shape).
        let unbounded = run_ripr_with_env(&args, &[("RIPR_CHECK_FINDINGS_BYTES", "0")]);
        assert!(
            unbounded.status.success(),
            "opt-out JSON: {}",
            String::from_utf8_lossy(&unbounded.stderr)
        );
        assert_eq!(
            unbounded.stdout, default.stdout,
            "=0 must restore the unbounded document byte-identically"
        );

        // Invalid values fail closed: an error naming the variable, never a
        // silent fallback to bounded or unbounded rendering.
        let invalid = run_ripr_with_env(&args, &[("RIPR_CHECK_FINDINGS_BYTES", "nope")]);
        assert!(
            !invalid.status.success(),
            "invalid budget must fail the run, got success with {} bytes",
            invalid.stdout.len()
        );
        let stderr = String::from_utf8_lossy(&invalid.stderr);
        assert!(
            stderr.contains("RIPR_CHECK_FINDINGS_BYTES"),
            "failure must name the variable and the repair: {stderr}"
        );

        // Binary gate refusal: the bounded document is refused as a gap-ledger
        // input with the bound run state named (fail-closed consumption).
        // Pre-implementation this feeds an unbounded doc, ledger parsing
        // fails instead, and the bound state is absent — red.
        std::fs::write(fixture.join("bounded.json"), &bounded.stdout)
            .map_err(|error| error.to_string())?;
        let gate_out = fixture.join("gate-decision.json");
        let gate_out_arg = gate_out.to_string_lossy().into_owned();
        let gate = run_ripr(&[
            "gate",
            "evaluate",
            "--root",
            root_arg.as_str(),
            "--gap-ledger",
            "bounded.json",
            "--mode",
            "visible-only",
            "--out",
            gate_out_arg.as_str(),
        ]);
        assert!(
            !gate.status.success(),
            "gate must fail on a bounded ledger, got success"
        );
        let gate_text = std::fs::read_to_string(&gate_out).map_err(|error| error.to_string())?;
        let gate_report: serde_json::Value =
            serde_json::from_str(&gate_text).map_err(|error| error.to_string())?;
        assert_eq!(gate_report["status"], "config_error");
        let gate_errors = gate_report["config_errors"]
            .as_array()
            .ok_or("gate refusal must carry config_errors")?;
        assert!(
            gate_errors.iter().any(|error| error
                .as_str()
                .is_some_and(|text| text.contains("limited_findings_bound"))),
            "gate refusal must name the bound run state: {gate_errors:?}"
        );
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
    result.and(cleanup)
}
