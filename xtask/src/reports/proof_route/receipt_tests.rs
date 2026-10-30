use super::*;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const OTHER: &str = "cccccccccccccccccccccccccccccccccccccccc";
const CHECK: &str = "cargo xtask check-spec-format";
const NEW_CHECK: &str = "cargo xtask check-doc-index";

fn route(commands: &[&str]) -> ProofRoute {
    ProofRoute {
        matched_packs: Vec::new(),
        unmatched_files: Vec::new(),
        full_proof: false,
        release_proof_required: false,
        routed_packs: vec![RoutedPack {
            id: "docs-spec".to_string(),
            required_commands: commands
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            advisory_commands: Vec::new(),
        }],
        required_lanes: Vec::new(),
        advisory_lanes: Vec::new(),
        skipped_lanes: Vec::new(),
        never_routed_lanes: Vec::new(),
    }
}

fn receipt(commands: Value, overall_status: &str) -> Value {
    json!({
        "schema_version": "0.1", "mode": "advisory-local-preflight",
        "base": {"rev": "origin/main", "sha": BASE},
        "head": {"rev": "HEAD", "sha": HEAD},
        "routed_packs": ["docs-spec"], "full_proof": false,
        "release_proof_required": false, "overall_status": overall_status,
        "commands": commands, "advisory_not_run": [],
    })
}

fn passed(command: &str) -> Value {
    json!({"command": command, "packs": ["docs-spec"], "status": "pass",
        "exit_code": 0, "duration_ms": 5, "output_tail": null})
}

fn render(value: &Value, head: &str, route: ProofRoute) -> String {
    let receipt = parse_preflight_receipt(&value.to_string(), head, &route);
    proof_route_section_markdown(&Ok(ProofRouteSummary {
        base: DEFAULT_BASE.to_string(),
        head: DEFAULT_HEAD.to_string(),
        changed_file_count: 1,
        route,
        receipt,
    }))
}

#[test]
fn empty_range_receipt_does_not_cover_current_required_commands() {
    // Valid producer receipt for --base HEAD --head HEAD. No command failed;
    // the current summary nevertheless has a nonempty obligation.
    let mut value = receipt(json!([]), "pass");
    value["base"] = json!({"rev": "HEAD", "sha": HEAD});
    value["routed_packs"] = json!([]);
    let section = render(&value, HEAD, route(&[CHECK]));
    assert!(
        section.contains("recorded overall status `pass`"),
        "{section}"
    );
    assert!(
        section.contains("Current-head command coverage: `unproven`"),
        "{section}"
    );
    assert!(section.contains("0/1 required commands recorded pass, 0 fail, 0 not run, 1 missing"));
    assert!(section.contains(&format!("- Missing: `{CHECK}`")));
    assert!(!section.contains("Current-head command coverage: `pass`"));
}

#[test]
fn matching_commands_cover_current_route_once_even_across_packs() {
    let value = receipt(json!([passed(CHECK), passed(NEW_CHECK)]), "pass");
    let mut current = route(&[NEW_CHECK, CHECK, CHECK]);
    current.routed_packs.push(current.routed_packs[0].clone());
    let section = render(&value, HEAD, current);
    assert!(
        section.contains("Current-head command coverage: `pass`"),
        "{section}"
    );
    assert!(section.contains("2/2 required commands recorded pass, 0 fail, 0 not run, 0 missing"));
    assert!(section.contains(&format!("recorded head `{HEAD}`, base `{BASE}`")));
    assert!(section.contains("does not authenticate the executed checkout"));
}

#[test]
fn added_obligation_preserves_earlier_pass_and_names_only_missing_command() {
    let value = receipt(json!([passed(CHECK)]), "pass");
    let section = render(&value, HEAD, route(&[CHECK, NEW_CHECK]));
    assert!(
        section.contains("Current-head command coverage: `unproven`"),
        "{section}"
    );
    assert!(section.contains("1/2 required commands recorded pass, 0 fail, 0 not run, 1 missing"));
    assert!(section.contains(&format!("- Missing: `{NEW_CHECK}`")));
    assert!(!section.contains(&format!("- Missing: `{CHECK}`")));
}

#[test]
fn failed_command_is_distinct_from_unrun_and_absent_obligations() {
    let value = receipt(
        json!([
            passed(CHECK),
            {"command": NEW_CHECK, "status": "fail", "exit_code": 101},
            {"command": "cargo xtask check-static-language", "status": "not_run", "exit_code": null},
        ]),
        "fail",
    );
    let section = render(
        &value,
        HEAD,
        route(&[
            CHECK,
            NEW_CHECK,
            "cargo xtask check-static-language",
            "cargo xtask check-local-context",
        ]),
    );
    assert!(section.contains("recorded overall status `fail`"));
    assert!(
        section.contains("Current-head command coverage: `fail`"),
        "{section}"
    );
    assert!(section.contains("1/4 required commands recorded pass, 1 fail, 1 not run, 1 missing"));
    assert!(section.contains(&format!("- Failed: `{NEW_CHECK}`")));
    assert!(section.contains("- Not run: `cargo xtask check-static-language`"));
    assert!(section.contains("- Missing: `cargo xtask check-local-context`"));
}

#[test]
fn unchanged_obligation_survives_route_narrowing_and_different_recorded_base() {
    let value = receipt(
        json!([
            passed(CHECK), {"command": NEW_CHECK, "status": "fail", "exit_code": null},
        ]),
        "fail",
    );
    let section = render(&value, HEAD, route(&[CHECK]));
    assert!(section.contains("recorded overall status `fail`"));
    assert!(section.contains(&format!("base `{BASE}`")));
    assert!(
        section.contains("Current-head command coverage: `pass`"),
        "{section}"
    );
    assert!(section.contains("1/1 required commands recorded pass"));
}

#[test]
fn empty_current_route_claims_no_executed_proof() {
    let section = render(&receipt(json!([]), "pass"), HEAD, route(&[]));
    assert!(
        section.contains("Current-head command coverage: `not_required`"),
        "{section}"
    );
    assert!(!section.contains("Current-head command coverage: `pass`"));
}

#[test]
fn different_head_preserves_original_outcomes_without_claiming_applicability() {
    let value = receipt(json!([passed(CHECK)]), "pass");
    let section = render(&value, OTHER, route(&[CHECK]));
    assert!(section.contains(&format!("recorded head `{HEAD}`, base `{BASE}`")));
    assert!(section.contains("recorded overall status `pass`; commands: 1 pass"));
    assert!(section.contains(&format!(
        "Current applicability is not established for head `{OTHER}`"
    )));
    assert!(
        section.contains("assess material changes before reusing or refreshing affected proof")
    );
    assert!(!section.contains("Current-head command coverage:"));
}

#[test]
fn command_identity_matches_executed_arguments_not_prefixes_or_pack_names() {
    let value = receipt(
        json!([passed(" cargo\t xtask   check-spec-format ")]),
        "pass",
    );
    assert!(render(&value, HEAD, route(&[CHECK])).contains("1/1 required commands recorded pass"));
    let value = receipt(
        json!([passed("cargo xtask check-spec-format --help")]),
        "pass",
    );
    assert!(render(&value, HEAD, route(&[CHECK])).contains("0/1 required commands recorded pass"));
    let value = receipt(json!([passed(CHECK)]), "pass");
    assert!(
        render(&value, HEAD, route(&[NEW_CHECK])).contains("0/1 required commands recorded pass")
    );
}

#[test]
fn malformed_and_inconsistent_receipts_cannot_supply_current_coverage() -> Result<(), String> {
    let valid = receipt(json!([passed(CHECK)]), "pass");
    let mut invalid = Vec::new();
    for (pointer, replacement) in [
        ("/schema_version", json!("99")),
        ("/mode", json!("hosted")),
        ("/head/sha", json!("short")),
        ("/base/sha", json!(null)),
        ("/commands", json!(null)),
        ("/commands/0/command", json!(" \t")),
        ("/commands/0/status", json!("cancelled")),
        ("/commands/0/exit_code", json!(101)),
        ("/commands/0/exit_code", json!("0")),
        ("/overall_status", json!("fail")),
    ] {
        let mut value = valid.clone();
        let field = value
            .pointer_mut(pointer)
            .ok_or_else(|| format!("fixture lacks {pointer}"))?;
        *field = replacement;
        invalid.push(value.to_string());
    }
    invalid.extend([
        "not JSON".to_string(),
        "{}".to_string(),
        receipt(json!([passed(CHECK), passed(CHECK)]), "pass").to_string(),
        receipt(
            json!([passed(CHECK), passed("cargo  xtask check-spec-format")]),
            "pass",
        )
        .to_string(),
        receipt(json!([{"command": CHECK, "status": "pass"}]), "pass").to_string(),
        receipt(
            json!([{"command": CHECK, "status": "fail", "exit_code": 101}]),
            "pass",
        )
        .to_string(),
        receipt(
            json!([{"command": CHECK, "status": "not_run", "exit_code": null}]),
            "pass",
        )
        .to_string(),
        receipt(
            json!([{"command": CHECK, "status": "not_run", "exit_code": 0}]),
            "fail",
        )
        .to_string(),
    ]);
    for text in invalid {
        let observed = parse_preflight_receipt(&text, HEAD, &route(&[CHECK]));
        assert!(
            matches!(observed, PreflightReceiptStatus::Unreadable(_)),
            "{text}: {observed:?}"
        );
        assert!(!preflight_receipt_line(&observed).contains("Current-head command coverage:"));
    }
    Ok(())
}

#[test]
fn timeout_exit_zero_remains_failed_and_required_unrun_remains_unproven() {
    let value = receipt(
        json!([
            {"command": CHECK, "status": "fail", "exit_code": 0},
            {"command": NEW_CHECK, "status": "not_run", "exit_code": null},
        ]),
        "fail",
    );
    let section = render(&value, HEAD, route(&[NEW_CHECK]));
    assert!(section.contains("commands: 0 pass, 1 fail (1 not run)"));
    assert!(
        section.contains("Current-head command coverage: `unproven`"),
        "{section}"
    );
    assert!(section.contains("0/1 required commands recorded pass, 0 fail, 1 not run, 0 missing"));
}
