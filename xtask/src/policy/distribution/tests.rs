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
const PYTHON_MANIFEST_TEXT: &str = include_str!("../../../../packaging/python/pyproject.toml");
const PYTHON_README_TEXT: &str = include_str!("../../../../packaging/python/README.md");
const PYTHON_LICENSE_MIT_TEXT: &str = include_str!("../../../../packaging/python/LICENSE-MIT");
const PYTHON_LICENSE_APACHE_TEXT: &str =
    include_str!("../../../../packaging/python/LICENSE-APACHE");
const PYTHON_QUALIFICATION_WORKFLOW_TEXT: &str =
    include_str!("../../../../.github/workflows/python-wheel-qualification.yml");
const ROOT_LICENSE_MIT_TEXT: &str = include_str!("../../../../LICENSE-MIT");
const ROOT_LICENSE_APACHE_TEXT: &str = include_str!("../../../../LICENSE-APACHE");

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
fn valid_contract_matches_workspace_crate_and_npm_launcher() -> Result<(), String> {
    let violations = evaluated(CONTRACT_TEXT, WORKSPACE_TEXT, CRATE_TEXT, NPM_MANIFEST_TEXT)?;
    assert!(violations.is_empty(), "{violations:#?}");
    Ok(())
}

#[test]
fn valid_python_adapter_matches_distribution_contract() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let mut violations = python::validate_python_adapter(
        &contract,
        python_sources(
            PYTHON_MANIFEST_TEXT,
            PYTHON_README_TEXT,
            PYTHON_LICENSE_MIT_TEXT,
            PYTHON_LICENSE_APACHE_TEXT,
        ),
    );
    violations.extend(python_guidance::validate_package_readme_commands(
        PYTHON_README_PATH,
        PYTHON_README_TEXT,
    ));
    violations.extend(python_guidance::validate_qualification_workflow(
        PYTHON_QUALIFICATION_WORKFLOW_PATH,
        PYTHON_QUALIFICATION_WORKFLOW_TEXT,
    ));
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
    let mut violations = python::validate_python_adapter(
        &contract,
        python_sources(
            PYTHON_MANIFEST_TEXT,
            &mutated_readme,
            &mutated_license,
            PYTHON_LICENSE_APACHE_TEXT,
        ),
    );
    violations.extend(python_guidance::validate_package_readme_commands(
        PYTHON_README_PATH,
        &mutated_readme,
    ));
    assert!(has_violation(&violations, "unrelated `pip install ripr`"));
    assert!(has_violation(
        &violations,
        "selects unrelated PyPI distribution `ripr`"
    ));
    assert!(has_violation(
        &violations,
        "packaged license must be byte-identical"
    ));
    Ok(())
}

#[test]
fn python_guidance_rejects_unsafe_install_command_variants() {
    for command in [
        "pip install --upgrade ripr",
        "python -m pip install ripr==1.0",
        "python3.12 -m pip install 'ripr[cli]'",
        "pipx install ripr>=1",
        "uv tool install ripr~=1.0",
        "uvx ripr --version",
        "uvx --python 3.12 ripr check",
        "uvx --from ripr ripr check",
    ] {
        let readme = format!("{PYTHON_README_TEXT}\n```console\n{command}\n```\n");
        let violations =
            python_guidance::validate_package_readme_commands(PYTHON_README_PATH, &readme);
        assert!(
            has_violation(&violations, "selects unrelated PyPI distribution"),
            "accepted unsafe command `{command}`: {violations:#?}"
        );
    }
}

#[test]
fn python_guidance_accepts_explicit_ripr_rs_commands() {
    let readme = format!(
        "{PYTHON_README_TEXT}\n```console\npip install --upgrade ripr-rs\nuv tool install ripr-rs\nuvx --from ripr-rs ripr check\n```\n"
    );
    let violations = python_guidance::validate_package_readme_commands(PYTHON_README_PATH, &readme);
    assert!(violations.is_empty(), "{violations:#?}");
}

#[test]
fn python_qualification_keeps_native_and_pep440_versions_distinct() {
    let violations = python_guidance::validate_qualification_workflow(
        PYTHON_QUALIFICATION_WORKFLOW_PATH,
        PYTHON_QUALIFICATION_WORKFLOW_TEXT,
    );
    assert!(violations.is_empty(), "{violations:#?}");

    let mutated = PYTHON_QUALIFICATION_WORKFLOW_TEXT
        .replace(
            "ripr-rs==${RIPR_PYTHON_VERSION}",
            "ripr-rs==${RIPR_NATIVE_VERSION}",
        )
        .replace("\"0.11.0-rc.1\": \"0.11.0rc1\",", "");
    let violations = python_guidance::validate_qualification_workflow(
        PYTHON_QUALIFICATION_WORKFLOW_PATH,
        &mutated,
    );
    assert!(has_violation(
        &violations,
        "missing release-candidate mapping control"
    ));
    assert!(has_violation(
        &violations,
        "native SemVer used as a Python installer requirement"
    ));
}

#[test]
fn python_qualification_requires_record_integrity_negative_control() {
    let mutated = PYTHON_QUALIFICATION_WORKFLOW_TEXT
        .replace(
            "mutated .data/scripts/ripr without updating RECORD",
            "payload mutation control removed",
        )
        .replace("RECORD digest mismatch", "integrity error removed");
    let violations = python_guidance::validate_qualification_workflow(
        PYTHON_QUALIFICATION_WORKFLOW_PATH,
        &mutated,
    );
    assert!(has_violation(
        &violations,
        "missing stale RECORD negative control"
    ));
    assert!(has_violation(
        &violations,
        "missing RECORD digest rejection"
    ));
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
        "\"test\": \"node test/run-tests.test.cjs\"",
        "\"test\": \"node test/run-tests.test.cjs\",\n    \"install\": \"curl example.invalid\"",
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
            .replace("process.exitCode = signalExitCode", "process.exitCode = 0")
            .replace("SIGNALS_TO_OBSERVE", "removed-observer")
            .replace(
                "const effectiveSignal = receivedSignal || signal",
                "const effectiveSignal = signal",
            ),
        npm_launcher::TEST_PATH,
        NPM_TEST_TEXT,
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
            .any(|violation| violation.contains("signalExitCode"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("terminal process-group signals"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("first observed termination signal"))
    );
    assert!(!contract.npm.install_scripts);
    Ok(())
}

#[test]
fn npm_launcher_rejects_all_install_time_lifecycle_hooks() -> Result<(), String> {
    for script in [
        "preinstall",
        "install",
        "postinstall",
        "preprepare",
        "prepare",
        "postprepare",
    ] {
        let anchor = "\"test\": \"node test/run-tests.test.cjs\"";
        let replacement = format!("{anchor},\n    \"{script}\": \"node should-not-run.cjs\"");
        let mutated = NPM_MANIFEST_TEXT.replace(anchor, &replacement);
        let violations = evaluated(CONTRACT_TEXT, WORKSPACE_TEXT, CRATE_TEXT, &mutated)?;
        assert!(
            violations
                .iter()
                .any(|violation| { violation.contains(&format!("scripts.{script} is forbidden")) }),
            "{script}: {violations:#?}"
        );
    }
    Ok(())
}

#[test]
fn npm_launcher_guarded_runner_rejects_missing_controls() {
    let mut violations = Vec::new();
    npm_launcher::validate_test_runner(
        npm_launcher::TEST_RUNNER_PATH,
        &npm_launcher::TEST_RUNNER_TEXT.replace(
            "source bin missing-package failure never falls back to PATH or writes stdout",
            "removed required control",
        ),
        npm_launcher::TEST_PATH,
        NPM_TEST_TEXT,
        &mut violations,
    );
    assert!(violations.iter().any(|violation| {
        violation.contains("required executed-control admission")
            && violation.contains("source bin missing-package")
    }));
}

fn compiled_test_names(filter: &str) -> Result<std::collections::BTreeSet<String>, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let args = [filter, "--list", "--format", "terse"].map(str::to_string);
    let output = crate::run::capture_output_with_timeout(
        executable.to_str().ok_or("non-UTF-8 test executable")?,
        &args,
        &[],
        std::time::Duration::from_secs(30),
        "distribution source-gate test inventory",
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "test inventory failed: status={:?}, timed_out={}, stderr={}",
            output.status, output.timed_out, output.stderr
        ));
    }
    Ok(output
        .stdout
        .lines()
        .filter_map(|line| line.strip_suffix(": test").map(str::to_string))
        .collect())
}

#[test]
fn python_source_gate_selects_complete_distribution_inventory() -> Result<(), String> {
    let step = PYTHON_QUALIFICATION_WORKFLOW_TEXT
        .split_once("      - name: Validate package source contract\n")
        .ok_or("missing package source-contract step")?
        .1
        .split("\n      - name:")
        .next()
        .ok_or("missing package source-contract body")?;
    let filter = step
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("cargo test -p xtask --locked ")?
                .strip_suffix(" -- --nocapture")
        })
        .ok_or("missing focused distribution test command")?;
    assert_eq!(filter.split_whitespace().count(), 1);

    // Use libtest's actual matching, not a second substring-selector model.
    let expected = compiled_test_names("policy::distribution::")?;
    for module in ["tests", "wheelhouse::tests", "workflow::tests"] {
        let prefix = format!("policy::distribution::{module}::");
        assert!(expected.iter().any(|name| name.starts_with(&prefix)));
    }
    #[cfg(unix)]
    assert!(expected.contains(
        "policy::distribution::python_guidance_tests::local_wheelhouse_recipe_selects_its_installed_executable"
    ));
    let selected = compiled_test_names(filter)?;
    println!(
        "workflow selected {} of {} distribution tests",
        selected.len(),
        expected.len()
    );
    assert_eq!(selected, expected, "source gate omits distribution tests");

    // Old omission, stale zero-match selector, and unrelated over-selection
    // must all be rejected by the same complete-inventory comparison.
    for wrong in [
        "policy::distribution::tests",
        "policy::distribution::renamed_away",
        "policy::",
    ] {
        let actual = compiled_test_names(wrong)?;
        println!("refused {wrong:?}: selected {} tests", actual.len());
        assert_ne!(actual, expected, "accepted wrong selector {wrong}");
        if wrong.ends_with("::tests") {
            assert!(!actual.is_empty() && actual.is_subset(&expected));
        } else if wrong.ends_with("::renamed_away") {
            assert!(actual.is_empty());
        } else {
            assert!(actual.is_superset(&expected));
        }
    }
    Ok(())
}

fn npm_with_version_and_tag(version: &str, tag: Option<&str>) -> String {
    let mut text = NPM_MANIFEST_TEXT.replace(
        "\"version\": \"0.11.0\"",
        &format!("\"version\": \"{version}\""),
    );
    if let Some(tag) = tag {
        text = text.replace(
            "\"access\": \"public\"",
            &format!("\"access\": \"public\", \"tag\": \"{tag}\""),
        );
    }
    text
}

fn publish_tag_violations(version: &str, tag: Option<&str>) -> Vec<String> {
    let mut violations = Vec::new();
    npm_launcher::validate_publish_tag(npm_launcher::MANIFEST_PATH, version, tag, &mut violations);
    violations
}

#[test]
fn npm_prerelease_publish_tag_positive_controls_pass() {
    assert!(publish_tag_violations("0.11.0-alpha.2", Some("next")).is_empty());
    assert!(publish_tag_violations("0.11.0", None).is_empty());
    assert!(publish_tag_violations("0.11.0", Some("latest")).is_empty());
}

#[test]
fn npm_prerelease_without_explicit_tag_or_with_latest_is_rejected() {
    for tag in [None, Some("latest"), Some("")] {
        let violations = publish_tag_violations("0.11.0-alpha.2", tag);
        assert!(has_violation(&violations, "prerelease version"), "{tag:?}");
    }
    let violations = publish_tag_violations("0.11.0", Some("next"));
    assert!(has_violation(&violations, "stable version"));
}

#[test]
fn npm_manifest_prerelease_to_latest_fails_through_launcher_validator() -> Result<(), String> {
    let contract = parsed_contract(CONTRACT_TEXT)?;
    let run = |tag: Option<&str>| {
        let mut violations = Vec::new();
        npm_launcher::validate_launcher(
            npm_launcher::MANIFEST_PATH,
            &npm_with_version_and_tag("0.11.0-alpha.2", tag),
            "0.11.0-alpha.2",
            &contract,
            &mut violations,
        );
        violations
    };
    assert!(has_violation(&run(None), "publishConfig.tag"));
    assert!(has_violation(&run(Some("latest")), "publishConfig.tag"));
    assert!(!has_violation(&run(Some("next")), "publishConfig.tag"));
    Ok(())
}

fn workflow_violations(text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    npm_launcher::validate_workflow_npm_tags("wf.yml", text, &mut violations);
    violations
}

#[test]
fn workflow_npm_publish_and_dist_tag_require_explicit_non_latest_tag() {
    let check = workflow_violations;
    assert!(check("      - run: npm publish --tag next\n").is_empty());
    assert!(check("      - run: npm publish --tag=next --access public\n").is_empty());
    assert!(check("      - run: npm publish --tag \"next\"\n").is_empty());
    assert!(check("      # npm publish\n      - run: npm pack --dry-run\n").is_empty());
    assert!(has_violation(
        &check("      - run: npm publish\n"),
        "explicit non-`latest`"
    ));
    assert!(has_violation(
        &check("      - run: npm publish --tag latest\n"),
        "forbidden"
    ));
    assert!(has_violation(
        &check("      - run: npm dist-tag add pkg@1.0.0-rc.1 latest\n"),
        "forbidden"
    ));
    assert!(check("      - run: npm dist-tag add pkg@1.0.0-rc.1 next\n").is_empty());
}

#[test]
fn workflow_multiline_and_chained_npm_publish_is_not_skipped() {
    let multiline = "      - run: |\n          npm \\\n            publish --access public\n";
    assert!(has_violation(
        &workflow_violations(multiline),
        "explicit non-`latest`"
    ));
    let flag_split = "      - run: |\n          npm publish \\\n            --tag latest\n";
    assert!(has_violation(&workflow_violations(flag_split), "forbidden"));
    let tagged = "      - run: |\n          npm publish \\\n            --tag next\n";
    assert!(workflow_violations(tagged).is_empty());
    let chained = "      - run: npm pack && npm publish\n";
    assert!(has_violation(
        &workflow_violations(chained),
        "explicit non-`latest`"
    ));
    let leading_flag = "      - run: npm --silent publish\n";
    assert!(has_violation(
        &workflow_violations(leading_flag),
        "explicit non-`latest`"
    ));
}

#[test]
fn workflow_interpolated_or_repeated_tag_options_are_rejected() {
    for text in [
        "      - run: npm publish --tag \"$DIST_TAG\"\n",
        "      - run: npm publish --tag ${{ inputs.tag }}\n",
        "      - run: npm publish --tag $(echo latest)\n",
        "      - run: npm publish --tag=`echo latest`\n",
        "      - run: npm dist-tag add pkg@1.0.0 $TAG\n",
    ] {
        assert!(
            has_violation(&workflow_violations(text), "not a literal dist-tag"),
            "{text}"
        );
    }
    for text in [
        "      - run: npm publish --tag next --tag latest\n",
        "      - run: npm publish --tag next --tag=latest\n",
        "      - run: npm publish --tag=next --tag=next\n",
    ] {
        assert!(
            has_violation(&workflow_violations(text), "exactly one --tag"),
            "{text}"
        );
    }
}

fn scratch_dir(name: &str) -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("ripr-9302-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    Ok(dir)
}

#[test]
fn workflow_directory_scan_reports_multiline_publish_and_read_failures() -> Result<(), String> {
    let dir = scratch_dir("scan")?;
    fs::write(
        dir.join("publish.yml"),
        "jobs:\n  job:\n    steps:\n      - run: |\n          npm \\\n            publish\n",
    )
    .map_err(|err| err.to_string())?;
    // A workflow whose bytes are not UTF-8 cannot be read as text and must fail closed.
    fs::write(dir.join("binary.yml"), [0xff_u8, 0xfe, 0x00]).map_err(|err| err.to_string())?;
    let mut violations = Vec::new();
    scan_workflow_npm_tags(&dir, &mut violations);
    assert!(
        has_violation(&violations, "publish.yml:5"),
        "{violations:#?}"
    );
    assert!(
        has_violation(&violations, "failed to read workflow"),
        "{violations:#?}"
    );

    let mut missing = Vec::new();
    scan_workflow_npm_tags(&dir.join("absent"), &mut missing);
    assert!(has_violation(&missing, "run from the repository root"));
    let mut orphan = Vec::new();
    scan_workflow_npm_tags(&dir.join("no-parent").join("absent"), &mut orphan);
    assert!(orphan.is_empty());
    let mut not_a_dir = Vec::new();
    scan_workflow_npm_tags(&dir.join("publish.yml"), &mut not_a_dir);
    assert!(has_violation(
        &not_a_dir,
        "failed to read workflow directory"
    ));
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn workflow_global_flags_dry_run_comments_and_glued_separators() {
    for text in [
        "      - run: npm -w pkg publish\n",
        "      - run: npm --workspace x publish\n",
        "      - run: npm --otp 123 publish\n",
        "      - run: npm --tag latest publish\n",
        "      - run: echo \"a #b\"; npm publish\n",
        "      - run: foo;npm publish\n",
        "      - run: (npm publish)\n",
        "      - run: 'npm publish'\n",
        "      - run: npm publish --tag next;npm publish\n",
        "      - run: npm dist-tag add pkg@1.0.0-rc.1\n",
        "      - run: npm dist-tags add pkg@1.0.0-rc.1\n",
        "      - run: npm dist-tag add --otp 123 pkg@1.0.0 latest\n",
    ] {
        assert!(!workflow_violations(text).is_empty(), "{text}");
    }
    for text in [
        "      - run: npm publish --dry-run\n",
        "# npm publish\n",
        "      - run: echo done # npm publish\n",
        "      - run: npm --otp 123 publish --tag next;\n",
        "      - run: npm -w pkg publish --tag=next\n",
        "      - run: npm run publish\n",
        "      - run: npm dist-tag add --otp 123 pkg@1.0.0 next\n",
        "      - run: npm dist-tag ls pkg\n",
    ] {
        assert!(
            workflow_violations(text).is_empty(),
            "{text}: {:?}",
            workflow_violations(text)
        );
    }
}

#[test]
fn prerelease_detection_ignores_build_metadata_and_latest_is_case_insensitive() {
    assert!(publish_tag_violations("1.0.0+build-1", None).is_empty());
    assert!(!publish_tag_violations("1.0.0-rc.1+build", None).is_empty());
    assert!(!publish_tag_violations("1.0.0-rc.1", Some("LATEST")).is_empty());
    assert!(publish_tag_violations("1.0.0", Some("Latest")).is_empty());
}
