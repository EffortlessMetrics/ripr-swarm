use super::*;

const CONTRACT_TEXT: &str = include_str!("../../../../policy/distribution.toml");
const WORKSPACE_TEXT: &str = include_str!("../../../../Cargo.toml");
const CRATE_TEXT: &str = include_str!("../../../../crates/ripr/Cargo.toml");
const PYTHON_MANIFEST_TEXT: &str = include_str!("../../../../packaging/python/pyproject.toml");
const PYTHON_README_TEXT: &str = include_str!("../../../../packaging/python/README.md");
const PYTHON_LICENSE_MIT_TEXT: &str = include_str!("../../../../packaging/python/LICENSE-MIT");
const PYTHON_LICENSE_APACHE_TEXT: &str =
    include_str!("../../../../packaging/python/LICENSE-APACHE");
const ROOT_LICENSE_MIT_TEXT: &str = include_str!("../../../../LICENSE-MIT");
const ROOT_LICENSE_APACHE_TEXT: &str = include_str!("../../../../LICENSE-APACHE");

fn parsed_contract(text: &str) -> Result<DistributionContract, String> {
    parse_distribution_contract(CONTRACT_PATH, text)
}

fn python_sources<'a>(
    manifest_text: &'a str,
    readme_text: &'a str,
    packaged_mit_text: &'a str,
    packaged_apache_text: &'a str,
) -> python::PythonAdapterSources<'a> {
    python::PythonAdapterSources {
        manifest_path: PYTHON_MANIFEST_PATH,
        manifest_text,
        readme_path: PYTHON_README_PATH,
        readme_text,
        packaged_mit_path: PYTHON_LICENSE_MIT_PATH,
        packaged_mit_text,
        root_mit_path: ROOT_LICENSE_MIT_PATH,
        root_mit_text: ROOT_LICENSE_MIT_TEXT,
        packaged_apache_path: PYTHON_LICENSE_APACHE_PATH,
        packaged_apache_text,
        root_apache_path: ROOT_LICENSE_APACHE_PATH,
        root_apache_text: ROOT_LICENSE_APACHE_TEXT,
    }
}

fn has_violation(violations: &[String], fragment: &str) -> bool {
    violations
        .iter()
        .any(|violation| violation.contains(fragment))
}

#[test]
fn valid_contract_matches_workspace_and_crate() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let violations = evaluate_contract(
        CONTRACT_PATH,
        &contract,
        WORKSPACE_MANIFEST_PATH,
        WORKSPACE_TEXT,
        CRATE_MANIFEST_PATH,
        CRATE_TEXT,
    );
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn valid_python_adapter_matches_distribution_contract() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let violations = python::validate_python_adapter(
        &contract,
        python_sources(
            PYTHON_MANIFEST_TEXT,
            PYTHON_README_TEXT,
            PYTHON_LICENSE_MIT_TEXT,
            PYTHON_LICENSE_APACHE_TEXT,
        ),
    );
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn duplicate_target_and_wrong_payload_are_rejected() -> Result<(), String> {
    let mutated = CONTRACT_TEXT
        .replace("aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu")
        .replace(
            "@effortlessmetrics/ripr-win32-x64-msvc",
            "@effortlessmetrics/ripr-win32-arm64-msvc",
        );
    let contract = parsed_contract(&mutated)?;
    let violations = evaluate_contract(
        CONTRACT_PATH,
        &contract,
        WORKSPACE_MANIFEST_PATH,
        WORKSPACE_TEXT,
        CRATE_MANIFEST_PATH,
        CRATE_TEXT,
    );
    assert!(has_violation(&violations, "duplicate target"));
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
    let contract = parsed_contract(&mutated)?;
    let violations = evaluate_contract(
        CONTRACT_PATH,
        &contract,
        WORKSPACE_MANIFEST_PATH,
        WORKSPACE_TEXT,
        CRATE_MANIFEST_PATH,
        CRATE_TEXT,
    );
    assert!(has_violation(
        &violations,
        "missing target `aarch64-apple-darwin`"
    ));
    Ok(())
}

#[test]
fn stale_feature_contract_is_rejected() -> Result<(), String> {
    let mutated = CONTRACT_TEXT.replace(
        "features = [\"lang-python\", \"lang-rust\", \"lang-typescript\"]",
        "features = [\"lang-rust\"]",
    );
    let contract = parsed_contract(&mutated)?;
    let violations = evaluate_contract(
        CONTRACT_PATH,
        &contract,
        WORKSPACE_MANIFEST_PATH,
        WORKSPACE_TEXT,
        CRATE_MANIFEST_PATH,
        CRATE_TEXT,
    );
    assert!(has_violation(&violations, "product.features"));
    Ok(())
}

#[test]
fn independent_crate_version_is_rejected() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let mutated_crate = CRATE_TEXT.replace("version.workspace = true", "version = \"9.9.9\"");
    let violations = evaluate_contract(
        CONTRACT_PATH,
        &contract,
        WORKSPACE_MANIFEST_PATH,
        WORKSPACE_TEXT,
        CRATE_MANIFEST_PATH,
        &mutated_crate,
    );
    assert!(has_violation(
        &violations,
        "package.version.workspace must be true"
    ));
    Ok(())
}

#[test]
fn python_adapter_rejects_independent_version_and_compatibility_claim() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let mutated = PYTHON_MANIFEST_TEXT
        .replace(
            "dynamic = [\"version\"]",
            "dynamic = [\"version\"]\nversion = \"9.9.9\"",
        )
        .replace(
            "bindings = \"bin\"",
            "bindings = \"bin\"\ncompatibility = \"manylinux_2_28\"",
        );
    let violations = python::validate_python_adapter(
        &contract,
        python_sources(
            &mutated,
            PYTHON_README_TEXT,
            PYTHON_LICENSE_MIT_TEXT,
            PYTHON_LICENSE_APACHE_TEXT,
        ),
    );
    assert!(has_violation(&violations, "project.version must be absent"));
    assert!(has_violation(&violations, "tool.maturin.compatibility"));
    Ok(())
}

#[test]
fn python_adapter_rejects_feature_backend_and_target_drift() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let mutated = PYTHON_MANIFEST_TEXT
        .replace("maturin==1.14.1", "maturin>=1")
        .replace(
            "targets = [{ name = \"ripr\", kind = \"bin\" }]",
            "targets = [{ name = \"other\", kind = \"bin\" }]",
        )
        .replace(
            "features = [\"lang-python\", \"lang-rust\", \"lang-typescript\"]",
            "features = [\"lang-rust\"]",
        );
    let violations = python::validate_python_adapter(
        &contract,
        python_sources(
            &mutated,
            PYTHON_README_TEXT,
            PYTHON_LICENSE_MIT_TEXT,
            PYTHON_LICENSE_APACHE_TEXT,
        ),
    );
    assert!(has_violation(&violations, "build-system.requires"));
    assert!(has_violation(&violations, "tool.maturin.targets"));
    assert!(has_violation(&violations, "tool.maturin.features"));
    Ok(())
}

#[test]
fn python_adapter_rejects_unsafe_guidance_and_license_drift() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let mutated_readme = format!("{PYTHON_README_TEXT}\n```console\npip install ripr\n```\n");
    let mutated_license = format!("{PYTHON_LICENSE_MIT_TEXT}\nchanged\n");
    let violations = python::validate_python_adapter(
        &contract,
        python_sources(
            PYTHON_MANIFEST_TEXT,
            &mutated_readme,
            &mutated_license,
            PYTHON_LICENSE_APACHE_TEXT,
        ),
    );
    assert!(has_violation(&violations, "unrelated `pip install ripr`"));
    assert!(has_violation(
        &violations,
        "packaged license must be byte-identical"
    ));
    Ok(())
}
