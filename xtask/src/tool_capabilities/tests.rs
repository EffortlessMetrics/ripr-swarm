use std::path::Path;

use super::{DEFAULT_REGISTRY_PATH, evaluate, parse_options, render_json, validate_iso_date};

fn document(body: &str) -> String {
    format!("schema_version = 1\nkind = \"ripr.tool_capabilities\"\n\n{body}")
}

fn local_entry(id: &str, claim_id: &str) -> String {
    format!(
        r#"[[capability]]
id = "{id}"
claim_id = "{claim_id}"
kind = "ripr-specific"
decision_kind = "proof"
claim = "local RIPR proof for {id}"
scanner_scope = "named fixture corpus"
current_authority = "ripr"
destination_authority = "ripr"
state = "local"
source_commands = ["check-example"]
source_paths = ["xtask/src/example.rs"]
policy_paths = []
consumer_surfaces = ["none"]
rationale_refs = ["docs/LEARNINGS.md"]
owner = "repo-infra"
support_tier = "required"
control_semantics = "report-only"
exception_model = "none"
lifecycle_semantics = "review-only"
count_semantics = "not-applicable"
negative_proof_refs = []
"#
    )
}

fn deny_generic_dual_run(expires: &str) -> String {
    format!(
        r#"[[capability]]
id = "source-policy.no-panic"
claim_id = "ripr.source-policy.no-panic.v1"
kind = "generic"
decision_kind = "policy"
claim = "panic-family source exceptions match current scanned occurrences"
scanner_scope = "tracked Rust source syntax"
current_authority = "ripr.xtask"
destination_authority = "cargo-allow"
state = "dual-running"
source_commands = ["check-no-panic-family"]
source_paths = ["xtask/src/no_panic.rs"]
policy_paths = ["policy/no-panic-allowlist.toml"]
consumer_surfaces = ["precommit"]
rationale_refs = ["docs/CLIPPY_POLICY.md"]
owner = "repo-infra"
support_tier = "required"
control_semantics = "deny"
exception_model = "receipt"
lifecycle_semantics = "hard-expiry"
count_semantics = "exact"
negative_proof_refs = ["test:no_panic_expired_exception_fails"]
required_claim_id = "ripr.required.source-policy.no-panic"
engine_contract = "cargo-allow.check.v1"
destination_issue = "EffortlessMetrics/cargo-allow#2699"
consumer_issue = "EffortlessMetrics/ripr-swarm#2344"
comparison_artifact = "receipt:no-panic-dual-run-comparison"
dual_run_expires = "{expires}"
"#
    )
}

fn diagnostics_text(text: &str, as_of: Option<&str>) -> String {
    evaluate(text, as_of, "fixture.toml").diagnostics.join("\n")
}

fn seed_text() -> Result<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(DEFAULT_REGISTRY_PATH);
    std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))
}

#[test]
fn valid_seed_registry_parses_without_diagnostics() -> Result<(), String> {
    let outcome = evaluate(&seed_text()?, Some("2026-11-02"), DEFAULT_REGISTRY_PATH);
    if !outcome.diagnostics.is_empty() {
        return Err(outcome.diagnostics.join("\n"));
    }
    let kinds = outcome
        .entries
        .iter()
        .map(|entry| entry.kind.as_str())
        .collect::<Vec<_>>();
    if !kinds.contains(&"generic")
        || !kinds.contains(&"ripr-specific")
        || !kinds.contains(&"hybrid")
    {
        return Err(format!("seed kinds were {kinds:?}"));
    }
    if !outcome
        .entries
        .iter()
        .any(|entry| entry.state.as_str() == "dual-running")
    {
        return Err("seed is missing a dual-running example".to_string());
    }
    Ok(())
}

#[test]
fn unknown_schema_version_fails() {
    let text = "schema_version = 2\nkind = \"ripr.tool_capabilities\"\n";
    let report = diagnostics_text(text, None);
    assert!(
        report.contains("[schema]") && report.contains("unknown schema_version 2"),
        "{report}"
    );
}

#[test]
fn unknown_kind_fails() {
    let body = local_entry("ripr.local.example", "ripr.local.example.v1")
        .replace("kind = \"ripr-specific\"", "kind = \"magic\"");
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[schema]") && report.contains("magic"),
        "{report}"
    );
}

#[test]
fn unknown_state_fails() {
    let body = local_entry("ripr.local.example", "ripr.local.example.v1")
        .replace("state = \"local\"", "state = \"maybe\"");
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[schema]") && report.contains("maybe"),
        "{report}"
    );
}

#[test]
fn unknown_decision_kind_fails() {
    let body = local_entry("ripr.local.example", "ripr.local.example.v1")
        .replace("decision_kind = \"proof\"", "decision_kind = \"vibes\"");
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[schema]") && report.contains("vibes"),
        "{report}"
    );
}

#[test]
fn unknown_control_semantics_fails() {
    let body = local_entry("ripr.local.example", "ripr.local.example.v1").replace(
        "control_semantics = \"report-only\"",
        "control_semantics = \"block-maybe\"",
    );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[schema]") && report.contains("block-maybe"),
        "{report}"
    );
}

#[test]
fn duplicate_capability_ids_fail() {
    let body = format!(
        "{}{}",
        local_entry("ripr.local.example", "ripr.local.example.v1"),
        local_entry("ripr.local.example", "ripr.local.example.v2")
    );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[uniqueness]") && report.contains("duplicate capability id"),
        "{report}"
    );
}

#[test]
fn duplicate_claim_ids_fail() {
    let body = format!(
        "{}{}",
        local_entry("ripr.local.one", "ripr.local.shared.v1"),
        local_entry("ripr.local.two", "ripr.local.shared.v1")
    );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[uniqueness]") && report.contains("duplicate claim id"),
        "{report}"
    );
}

#[test]
fn dual_running_names_missing_dual_run_expires() {
    let body =
        deny_generic_dual_run("2027-12-31").replace("dual_run_expires = \"2027-12-31\"\n", "");
    let report = diagnostics_text(&document(&body), Some("2026-11-02"));
    assert!(
        report.contains("[state_required]") && report.contains("dual_run_expires"),
        "{report}"
    );
}

#[test]
fn deny_without_negative_proof_and_required_claim_fails() {
    let body = local_entry("source-policy.example", "ripr.source-policy.example.v1").replace(
        "control_semantics = \"report-only\"",
        "control_semantics = \"deny\"",
    );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[deny_honesty]")
            && report.contains("negative proof")
            && report.contains("required_claim_id"),
        "{report}"
    );
}

#[test]
fn deny_with_docs_path_as_negative_proof_fails() {
    let body = local_entry("source-policy.example", "ripr.source-policy.example.v1")
        .replace(
            "control_semantics = \"report-only\"",
            "control_semantics = \"deny\"",
        )
        .replace(
            "negative_proof_refs = []",
            "negative_proof_refs = [\"docs/CLIPPY_POLICY.md\"]\nrequired_claim_id = \"ripr.required.example\"",
        );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[deny_honesty]")
            && report.contains("docs/CLIPPY_POLICY.md")
            && report.contains("not execution evidence"),
        "{report}"
    );
}

#[test]
fn deny_with_empty_test_prefix_fails() {
    let body = local_entry("source-policy.example", "ripr.source-policy.example.v1")
        .replace(
            "control_semantics = \"report-only\"",
            "control_semantics = \"deny\"",
        )
        .replace(
            "negative_proof_refs = []",
            "negative_proof_refs = [\"test:\"]\nrequired_claim_id = \"ripr.required.example\"",
        );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[deny_honesty]") && report.contains("named negative proof"),
        "{report}"
    );
}

#[test]
fn deny_with_whitespace_only_test_suffix_fails() {
    let body = local_entry("source-policy.example", "ripr.source-policy.example.v1")
        .replace(
            "control_semantics = \"report-only\"",
            "control_semantics = \"deny\"",
        )
        .replace(
            "negative_proof_refs = []",
            "negative_proof_refs = [\"test:   \"]\nrequired_claim_id = \"ripr.required.example\"",
        );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[deny_honesty]") && report.contains("named negative proof"),
        "{report}"
    );
}

#[test]
fn deny_with_empty_receipt_prefix_fails() {
    let body = local_entry("source-policy.example", "ripr.source-policy.example.v1")
        .replace(
            "control_semantics = \"report-only\"",
            "control_semantics = \"deny\"",
        )
        .replace(
            "negative_proof_refs = []",
            "negative_proof_refs = [\"receipt:\"]\nrequired_claim_id = \"ripr.required.example\"",
        );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[deny_honesty]") && report.contains("named negative proof"),
        "{report}"
    );
}

#[test]
fn cutover_ready_empty_receipt_prefix_fails() {
    let body = local_entry("source-policy.example", "ripr.source-policy.example.v1")
        .replace(
            "control_semantics = \"report-only\"",
            "control_semantics = \"deny\"",
        )
        .replace("state = \"local\"", "state = \"cutover-ready\"")
        .replace(
            "negative_proof_refs = []",
            "negative_proof_refs = [\"test:named_negative\"]\nrequired_claim_id = \"ripr.required.example\"\nparity_receipt = \"receipt:\"\ncurrentness_receipt = \"receipt:current\"\nrequired_route_receipt = \"receipt:route\"",
        );
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[evidence_honesty]")
            && report.contains("parity_receipt")
            && report.contains("not execution evidence"),
        "{report}"
    );
}

#[test]
fn metadata_only_cannot_satisfy_parity_or_cutover() {
    let body = local_entry("ripr.local.meta", "ripr.local.meta.v1")
        .replace(
            "control_semantics = \"report-only\"",
            "control_semantics = \"metadata-only\"",
        )
        .replace("state = \"local\"", "state = \"cutover-ready\"")
        + "parity_receipt = \"receipt:parity\"\ncurrentness_receipt = \"receipt:current\"\nrequired_route_receipt = \"receipt:route\"\n";
    let report = diagnostics_text(&document(&body), None);
    assert!(
        report.contains("[evidence_honesty]")
            && report.contains("cannot satisfy blocking state cutover-ready")
            && report.contains("cannot satisfy parity_receipt"),
        "{report}"
    );
}

#[test]
fn expired_dual_run_reported_for_injected_as_of() {
    let report = diagnostics_text(
        &document(&deny_generic_dual_run("2026-01-01")),
        Some("2026-11-02"),
    );
    assert!(
        report.contains("[lifecycle]")
            && report.contains("dual run expired as of 2026-11-02")
            && report.contains("2026-01-01"),
        "{report}"
    );
}

#[test]
fn same_dual_run_is_valid_on_expiry_date() -> Result<(), String> {
    let outcome = evaluate(
        &document(&deny_generic_dual_run("2026-11-02")),
        Some("2026-11-02"),
        "fixture.toml",
    );
    if !outcome.diagnostics.is_empty() {
        return Err(outcome.diagnostics.join("\n"));
    }
    Ok(())
}

#[test]
fn dual_run_expires_the_day_after_expiry_date() {
    let report = diagnostics_text(
        &document(&deny_generic_dual_run("2026-11-02")),
        Some("2026-11-03"),
    );
    assert!(
        report.contains("[lifecycle]")
            && report.contains("dual run expired as of 2026-11-03")
            && report.contains("2026-11-02"),
        "{report}"
    );
}

#[test]
fn output_is_byte_stable_for_identical_inputs() -> Result<(), String> {
    let text = document(&local_entry("ripr.local.example", "ripr.local.example.v1"));
    let first = render_json(&evaluate(&text, Some("2026-11-02"), "fixture.toml"))?;
    let second = render_json(&evaluate(&text, Some("2026-11-02"), "fixture.toml"))?;
    if first != second {
        return Err("tool-capabilities JSON drifted across identical inputs".to_string());
    }
    if !first.contains("\"network_access\": \"none\"") {
        return Err(first);
    }
    Ok(())
}

#[test]
fn parser_source_has_no_network_or_github_client() {
    let src = include_str!("../tool_capabilities.rs");
    assert!(!src.contains("std::net"));
    assert!(!src.contains(concat!("req", "west")));
    assert!(!src.contains(concat!("u", "req")));
    assert!(!src.contains("api.github.com"));
    assert!(!src.contains("octocrab"));
}

#[test]
fn issue_refs_are_syntax_only() {
    let body = deny_generic_dual_run("2027-12-31").replace(
        "destination_issue = \"EffortlessMetrics/cargo-allow#2699\"",
        "destination_issue = \"https://github.com/EffortlessMetrics/cargo-allow/issues/2699\"",
    );
    let report = diagnostics_text(&document(&body), Some("2026-11-02"));
    assert!(
        report.contains("[issue_ref]") && report.contains("authored owner/repo#n syntax"),
        "{report}"
    );
}

#[test]
fn review_after_is_not_hard_expiry() -> Result<(), String> {
    let body = local_entry("ripr.local.example", "ripr.local.example.v1")
        + "review_after = \"2020-01-01\"\n";
    let outcome = evaluate(&document(&body), Some("2026-11-02"), "fixture.toml");
    if !outcome.diagnostics.is_empty() {
        return Err(format!(
            "warning-only review_after must not hard-invalidate: {}",
            outcome.diagnostics.join("\n")
        ));
    }
    Ok(())
}

#[test]
fn parse_options_reject_unknown_and_require_dates() -> Result<(), String> {
    let options = parse_options(&[
        "--as-of".to_string(),
        "2026-11-02".to_string(),
        "--registry".to_string(),
        "fixture.toml".to_string(),
    ])?;
    if options_as_of(&options) != Some("2026-11-02") {
        return Err("as_of was not parsed".to_string());
    }
    if parse_options(&["--as-of".to_string(), "11/02/2026".to_string()]).is_ok() {
        return Err("invalid as_of was accepted".to_string());
    }
    if parse_options(&["--help".to_string()]).is_ok() {
        return Err("unknown argument was accepted".to_string());
    }
    validate_iso_date("2026-11-02")?;
    Ok(())
}

fn options_as_of(options: &super::Options) -> Option<&str> {
    options.as_of.as_deref()
}
