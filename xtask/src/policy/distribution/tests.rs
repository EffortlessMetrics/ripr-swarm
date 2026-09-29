use super::*;

const CONTRACT_TEXT: &str = include_str!("../../../../policy/distribution.toml");
const WORKSPACE_TEXT: &str = include_str!("../../../../Cargo.toml");
const CRATE_TEXT: &str = include_str!("../../../../crates/ripr/Cargo.toml");
const ROOT_APACHE_TEXT: &str = include_str!("../../../../LICENSE-APACHE");
const ROOT_MIT_TEXT: &str = include_str!("../../../../LICENSE-MIT");
const NPM_MANIFEST_TEXT: &str = include_str!("../../../../packaging/npm/launcher/package.json");
const NPM_BIN_TEXT: &str = include_str!("../../../../packaging/npm/launcher/bin/ripr.cjs");
const NPM_LIBRARY_TEXT: &str = include_str!("../../../../packaging/npm/launcher/lib/launcher.cjs");
const NPM_TEST_TEXT: &str =
    include_str!("../../../../packaging/npm/launcher/test/launcher.test.cjs");
const NPM_APACHE_TEXT: &str = include_str!("../../../../packaging/npm/launcher/LICENSE-APACHE");
const NPM_MIT_TEXT: &str = include_str!("../../../../packaging/npm/launcher/LICENSE-MIT");

fn parsed_contract(text: &str) -> Result<DistributionContract, String> {
    parse_distribution_contract(CONTRACT_PATH, text)
}

fn evaluated(
    contract_text: &str,
    workspace_text: &str,
    crate_text: &str,
    npm_text: &str,
) -> Result<Vec<String>, String> {
    let contract = parsed_contract(contract_text)?;
    Ok(evaluate_contract(
        CONTRACT_PATH,
        &contract,
        WORKSPACE_MANIFEST_PATH,
        workspace_text,
        CRATE_MANIFEST_PATH,
        crate_text,
        npm_launcher::MANIFEST_PATH,
        npm_text,
        npm_launcher::BIN_PATH,
        NPM_BIN_TEXT,
        npm_launcher::LIBRARY_PATH,
        NPM_LIBRARY_TEXT,
        npm_launcher::TEST_PATH,
        NPM_TEST_TEXT,
    ))
}

#[test]
fn valid_contract_matches_workspace_crate_and_npm_launcher() -> Result<(), String> {
    let violations = evaluated(CONTRACT_TEXT, WORKSPACE_TEXT, CRATE_TEXT, NPM_MANIFEST_TEXT)?;
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn npm_license_copies_match_repository_notices() {
    let mut violations = Vec::new();
    npm_launcher::validate_license_copy(
        LICENSE_APACHE_PATH,
        ROOT_APACHE_TEXT,
        npm_launcher::LICENSE_APACHE_PATH,
        NPM_APACHE_TEXT,
        &mut violations,
    );
    npm_launcher::validate_license_copy(
        LICENSE_MIT_PATH,
        ROOT_MIT_TEXT,
        npm_launcher::LICENSE_MIT_PATH,
        NPM_MIT_TEXT,
        &mut violations,
    );
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn npm_license_copy_drift_is_rejected() {
    let mut violations = Vec::new();
    npm_launcher::validate_license_copy(
        LICENSE_APACHE_PATH,
        ROOT_APACHE_TEXT,
        npm_launcher::LICENSE_APACHE_PATH,
        &format!("{NPM_APACHE_TEXT}\nchanged\n"),
        &mut violations,
    );
    assert!(violations.iter().any(|violation| {
        violation.contains(npm_launcher::LICENSE_APACHE_PATH)
            && violation.contains(LICENSE_APACHE_PATH)
            && violation.contains("byte-identical")
    }));
}

#[test]
fn duplicate_target_and_wrong_payload_are_rejected() -> Result<(), String> {
    let mutated = CONTRACT_TEXT
        .replace("aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu")
        .replace(
            "@effortlessmetrics/ripr-win32-x64-msvc",
            "@effortlessmetrics/ripr-win32-arm64-msvc",
        );
    let violations = evaluated(&mutated, WORKSPACE_TEXT, CRATE_TEXT, NPM_MANIFEST_TEXT)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("duplicate target"))
    );
    assert!(violations.iter().any(|violation| {
        violation.contains("field npm_package") && violation.contains("ripr-win32-x64-msvc")
    }));
    Ok(())
}

#[test]
fn missing_target_is_rejected() -> Result<(), String> {
    let start = CONTRACT_TEXT
        .find("[[target]]\nrust_target = \"aarch64-apple-darwin\"")
        .ok_or_else(|| "missing macOS ARM64 fixture anchor".to_string())?;
    let end = CONTRACT_TEXT
        .find("[[target]]\nrust_target = \"x86_64-pc-windows-msvc\"")
        .ok_or_else(|| "missing Windows x64 fixture anchor".to_string())?;
    let mutated = format!("{}{}", &CONTRACT_TEXT[..start], &CONTRACT_TEXT[end..]);
    let violations = evaluated(&mutated, WORKSPACE_TEXT, CRATE_TEXT, NPM_MANIFEST_TEXT)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("missing target `aarch64-apple-darwin`"))
    );
    Ok(())
}

#[test]
fn stale_feature_contract_is_rejected() -> Result<(), String> {
    let mutated = CONTRACT_TEXT.replace(
        "features = [\"lang-python\", \"lang-rust\", \"lang-typescript\"]",
        "features = [\"lang-rust\"]",
    );
    let violations = evaluated(&mutated, WORKSPACE_TEXT, CRATE_TEXT, NPM_MANIFEST_TEXT)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("product.features"))
    );
    Ok(())
}

#[test]
fn independent_crate_version_is_rejected() -> Result<(), String> {
    let mutated_crate = CRATE_TEXT.replace("version.workspace = true", "version = \"9.9.9\"");
    let violations = evaluated(
        CONTRACT_TEXT,
        WORKSPACE_TEXT,
        &mutated_crate,
        NPM_MANIFEST_TEXT,
    )?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("package.version.workspace must be true"))
    );
    Ok(())
}

#[test]
fn npm_launcher_version_and_dependency_drift_are_rejected() -> Result<(), String> {
    let stale = NPM_MANIFEST_TEXT.replacen("\"version\": \"0.11.0\"", "\"version\": \"0.10.0\"", 1);
    let violations = evaluated(CONTRACT_TEXT, WORKSPACE_TEXT, CRATE_TEXT, &stale)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("version must be `0.11.0`"))
    );

    let ranged = NPM_MANIFEST_TEXT.replace(
        "\"@effortlessmetrics/ripr-linux-x64-gnu\": \"0.11.0\"",
        "\"@effortlessmetrics/ripr-linux-x64-gnu\": \"^0.11.0\"",
    );
    let violations = evaluated(CONTRACT_TEXT, WORKSPACE_TEXT, CRATE_TEXT, &ranged)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("optionalDependencies"))
    );
    Ok(())
}

#[test]
fn npm_launcher_platform_and_install_script_drift_are_rejected() -> Result<(), String> {
    let wrong_target = NPM_MANIFEST_TEXT.replace(
        "\"rustTarget\": \"x86_64-unknown-linux-gnu\"",
        "\"rustTarget\": \"x86_64-unknown-linux-musl\"",
    );
    let violations = evaluated(CONTRACT_TEXT, WORKSPACE_TEXT, CRATE_TEXT, &wrong_target)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("ripr.platforms"))
    );

    let with_install = NPM_MANIFEST_TEXT.replace(
        "\"test\": \"node --test test/*.test.cjs\"",
        "\"test\": \"node --test test/*.test.cjs\",\n    \"install\": \"curl example.invalid\"",
    );
    let violations = evaluated(CONTRACT_TEXT, WORKSPACE_TEXT, CRATE_TEXT, &with_install)?;
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("scripts.install is forbidden"))
    );
    Ok(())
}

#[test]
fn npm_launcher_source_guards_reject_removed_safety_rails() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let mut violations = Vec::new();
    npm_launcher::validate_launcher_sources(
        npm_launcher::BIN_PATH,
        NPM_BIN_TEXT,
        npm_launcher::LIBRARY_PATH,
        &NPM_LIBRARY_TEXT
            .replace("shell: false", "shell: true")
            .replace("process.exitCode = signalExitCode", "process.exitCode = 0"),
        npm_launcher::TEST_PATH,
        &NPM_TEST_TEXT
            .replace("PATH-FALLBACK", "removed-control")
            .replace("RIPR_UNKNOWN_SIGNAL", "removed-signal-control"),
        &mut violations,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("shell interpolation"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("PATH-FALLBACK"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("signalExitCode"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("RIPR_UNKNOWN_SIGNAL"))
    );
    assert!(!contract.npm.install_scripts);
    Ok(())
}
