use super::{
    ARCHIVE_WORKFLOW_PATH, DISTRIBUTION_MANIFEST_PATH, DistributionOutcome, RULE_CRATE,
    RULE_IDENTITY, RULE_TARGET, RULE_VERSION, RULE_WORKFLOW, distribution_json,
    evaluate_distribution_contract, map_pep440_version,
};

const MANIFEST: &str = include_str!("../../../../policy/distribution-packages.toml");
const WORKSPACE: &str = "[workspace]\n[workspace.package]\nversion = \"0.11.0\"\n";
const RIPR: &str = concat!(
    "[package]\n",
    "name = \"ripr\"\n",
    "version.workspace = true\n\n",
    "[[bin]]\n",
    "name = \"ripr\"\n",
    "path = \"src/main.rs\"\n\n",
    "[features]\n",
    "default = [\"lang-rust\", \"lang-typescript\", \"lang-python\"]\n",
);
const WORKFLOW: &str = concat!(
    "matrix:\n  include:\n",
    "    - target: x86_64-pc-windows-msvc\n      executable: ripr.exe\n",
    "    - target: x86_64-unknown-linux-gnu\n      executable: ripr\n",
    "    - target: aarch64-unknown-linux-gnu\n      executable: ripr\n",
    "    - target: x86_64-apple-darwin\n      executable: ripr\n",
    "    - target: aarch64-apple-darwin\n      executable: ripr\n",
);

#[test]
fn current_distribution_contract_passes() {
    let outcome = evaluate_distribution_contract(MANIFEST, WORKSPACE, RIPR, WORKFLOW);
    assert!(outcome.violations.is_empty(), "{:#?}", outcome.violations);
    assert_eq!(outcome.cargo_version.as_deref(), Some("0.11.0"));
    assert_eq!(outcome.pypi_version.as_deref(), Some("0.11.0"));
    assert_eq!(outcome.npm_version.as_deref(), Some("0.11.0"));
}

#[test]
fn registry_version_mapping_is_closed_and_deterministic() -> Result<(), String> {
    let cases = [
        ("1.2.3", "1.2.3"),
        ("1.2.3-rc.4", "1.2.3rc4"),
        ("1.2.3-alpha.4", "1.2.3a4"),
        ("1.2.3-beta.4", "1.2.3b4"),
        ("1.2.3-dev.4", "1.2.3.dev4"),
    ];
    for (native, expected) in cases {
        assert_eq!(map_pep440_version(native)?, expected);
    }
    for unsupported in [
        "1.2.3-preview.1",
        "1.2.3+build.1",
        "1.2",
        "01.2.3",
        "1.2.3-rc",
        "1.2.3-rc.1.extra",
    ] {
        assert!(
            map_pep440_version(unsupported).is_err(),
            "unsupported version unexpectedly mapped: {unsupported}"
        );
    }
    Ok(())
}

#[test]
fn wrong_registry_identity_fails_the_production_evaluator() {
    let wrong = MANIFEST.replace(
        "pypi_distribution = \"ripr-rs\"",
        "pypi_distribution = \"ripr\"",
    );
    let outcome = evaluate_distribution_contract(&wrong, WORKSPACE, RIPR, WORKFLOW);
    assert!(
        outcome
            .violations
            .iter()
            .any(|violation| violation.starts_with(RULE_IDENTITY)),
        "{:#?}",
        outcome.violations
    );
}

#[test]
fn duplicate_or_missing_target_fails_before_packaging() {
    let duplicate = MANIFEST.replace(
        "rust_target = \"aarch64-apple-darwin\"",
        "rust_target = \"x86_64-apple-darwin\"",
    );
    let outcome = evaluate_distribution_contract(&duplicate, WORKSPACE, RIPR, WORKFLOW);
    assert!(
        outcome
            .violations
            .iter()
            .any(|violation| violation.starts_with(RULE_TARGET)),
        "{:#?}",
        outcome.violations
    );
}

#[test]
fn crate_feature_or_binary_drift_fails() {
    let wrong = RIPR
        .replace("name = \"ripr\"\npath", "name = \"other\"\npath")
        .replace(
            "[\"lang-rust\", \"lang-typescript\", \"lang-python\"]",
            "[\"lang-rust\"]",
        );
    let outcome = evaluate_distribution_contract(MANIFEST, WORKSPACE, &wrong, WORKFLOW);
    assert!(
        outcome
            .violations
            .iter()
            .any(|violation| violation.starts_with(RULE_CRATE)),
        "{:#?}",
        outcome.violations
    );
}

#[test]
fn unsupported_workspace_version_fails_without_mutating_inputs() {
    let workspace = WORKSPACE.replace("0.11.0", "0.11.0-preview.1");
    let outcome = evaluate_distribution_contract(MANIFEST, &workspace, RIPR, WORKFLOW);
    assert!(
        outcome
            .violations
            .iter()
            .any(|violation| violation.starts_with(RULE_VERSION)),
        "{:#?}",
        outcome.violations
    );
    assert!(workspace.contains("0.11.0-preview.1"));
}

#[test]
fn archive_workflow_drift_fails_the_same_gate() {
    let wrong = WORKFLOW.replace(
        "target: x86_64-unknown-linux-gnu\n      executable: ripr",
        "target: x86_64-unknown-linux-gnu\n      executable: other",
    );
    let outcome = evaluate_distribution_contract(MANIFEST, WORKSPACE, RIPR, &wrong);
    assert!(
        outcome
            .violations
            .iter()
            .any(|violation| violation.starts_with(RULE_WORKFLOW)),
        "{:#?}",
        outcome.violations
    );
}

#[test]
fn portable_report_is_deterministic_and_repo_relative() -> Result<(), String> {
    let first = evaluate_distribution_contract(MANIFEST, WORKSPACE, RIPR, WORKFLOW);
    let second = evaluate_distribution_contract(MANIFEST, WORKSPACE, RIPR, WORKFLOW);
    let first_json = distribution_json(&first)?;
    let second_json = distribution_json(&second)?;
    assert_eq!(first_json, second_json);
    assert!(first_json.contains(DISTRIBUTION_MANIFEST_PATH));
    assert!(first_json.contains(ARCHIVE_WORKFLOW_PATH));
    assert!(!first_json.contains("/home/"));
    assert!(!first_json.contains("C:\\\\"));
    Ok(())
}

#[test]
fn malformed_manifest_fails_closed() {
    let outcome = evaluate_distribution_contract("product = [", WORKSPACE, RIPR, WORKFLOW);
    assert!(!outcome.violations.is_empty());
    assert!(outcome.manifest.is_none());
}

#[test]
fn failed_input_has_a_deterministic_json_report() -> Result<(), String> {
    let outcome = DistributionOutcome {
        violations: vec!["distribution_schema :: missing".to_string()],
        ..DistributionOutcome::default()
    };
    let report = distribution_json(&outcome)?;
    assert!(report.contains("\"status\": \"fail\""));
    assert!(report.contains("no package, version, target, compatibility, or publication claim"));
    Ok(())
}
