use super::*;
use std::collections::BTreeSet;

#[path = "source_regressions.rs"]
mod source_regressions;

const CI: &str = include_str!("../../../../.github/workflows/ci.yml");
const VSCODE: &str = include_str!("../../../../.github/workflows/vscode-e2e.yml");

#[path = "source_oracle.rs"]
mod source_oracle;
use source_oracle::{field, job, same, source_agrees};

#[test]
fn existing_source_has_four_children_twelve_proof_steps_and_five_prerequisites()
-> Result<(), String> {
    source_agrees(CI, VSCODE)?;
    let inventory = source_inventory();
    same(inventory.version, 1, "inventory format")?;
    same(inventory.authority, Authority::NotEstablished, "authority")?;
    same(inventory.children.len(), 4, "children")?;
    let commands = inventory
        .children
        .iter()
        .flat_map(|child| &child.commands)
        .collect::<Vec<_>>();
    same(
        commands
            .iter()
            .filter(|step| step.role == CommandRole::Proof)
            .count(),
        12,
        "proof commands",
    )?;
    same(
        commands
            .iter()
            .filter(|step| step.role == CommandRole::Prerequisite)
            .count(),
        5,
        "prerequisites",
    )?;
    same(
        commands
            .iter()
            .filter(|step| step.role == CommandRole::Prerequisite)
            .map(|step| step.run)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml",
            "cargo fetch --locked --manifest-path fixtures/owner_return_pin_json_real_literal/input/Cargo.toml",
            "cargo fetch --locked --manifest-path crates/ripr/tests/fixtures/proptest_singleton/input/Cargo.toml",
            "npm ci",
            "sudo apt-get update && sudo apt-get install -y xvfb",
        ]),
        "prerequisites cannot exchange proof roles",
    )?;
    let selection = inventory
        .children
        .iter()
        .map(|child| child.selection_input)
        .collect::<BTreeSet<_>>();
    same(selection.len(), 4, "distinct selection inputs")?;
    let caller = include_str!("../../../../.github/workflows/routed-rust.yml");
    let feature = job(caller, "ci-feature")?;
    for input in selection {
        same(field(feature, 6, input), Some("true".to_owned()), input)?;
    }
    Ok(())
}

#[test]
fn command_omissions_and_wrong_owners_cannot_hide_behind_success() -> Result<(), String> {
    source_agrees(CI, VSCODE)?;
    let perl_step = "      - name: Test Perl language analysis\n        run: cargo test -p ripr --features lang-perl --lib analysis::language::perl\n";
    assert!(CI.contains(perl_step));
    assert!(source_agrees(&CI.replace(perl_step, ""), VSCODE).is_err());
    assert!(source_agrees(CI, &VSCODE.replace("      - run: npm run package\n", "")).is_err());
    let wrong_owner = CI.replace(perl_step, "").replace(
        "      - run: cargo check --workspace --all-targets",
        &format!("{perl_step}      - run: cargo check --workspace --all-targets"),
    );
    assert!(source_agrees(&wrong_owner, VSCODE).is_err());
    for (old, new) in [
        ("cargo check -p ripr --features lang-perl", "true"),
        (
            "cargo test -p ripr --features lang-perl --lib analysis::language::perl",
            "echo success",
        ),
        (
            "cargo test -p xtask --features perl-identity reports::sarif::identity_tests",
            "cargo test -p xtask",
        ),
        ("cargo generate-lockfile", "echo success"),
        (
            "cargo xtask release-readiness --version \"$release_version\"",
            "echo success",
        ),
        (
            "cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml",
            "true",
        ),
        (
            "--no-default-features --features lang-rust --no-fail-fast",
            "--all-features",
        ),
        (
            "cargo check --workspace --all-targets",
            "cargo check -p ripr",
        ),
        ("  release-proof:\n", "  another-job:\n"),
    ] {
        assert!(CI.contains(old));
        assert!(
            source_agrees(&CI.replace(old, new), VSCODE).is_err(),
            "{old}"
        );
    }
    for command in [
        "npm ci",
        "npm run compile",
        "npm run package",
        "sudo apt-get update && sudo apt-get install -y xvfb",
        "xvfb-run -a npm run test:e2e",
    ] {
        assert!(
            source_agrees(CI, &VSCODE.replace(command, "echo success")).is_err(),
            "{command}"
        );
    }
    Ok(())
}

#[test]
fn conditions_nested_identity_and_result_denominator_are_not_interchangeable() -> Result<(), String>
{
    source_agrees(CI, VSCODE)?;
    for (old, new) in [
        ("inputs.run_release_proof", "inputs.run_rust_only"),
        (inventory::SURFACE, "false"),
        (inventory::RELEASE, "true"),
        ("[ \"${CALLED:-false}\" != true ] || return 0", "return 0"),
        (
            "./.github/workflows/vscode-e2e.yml",
            "./.github/workflows/ci.yml",
        ),
        (
            "      ref: ${{ inputs.subject_sha || github.sha }}",
            "      ref: ${{ inputs.head_sha }}",
        ),
        (
            "      budget_artifact_id: ${{ inputs.budget_artifact_id }}",
            "      budget_artifact_id: stale",
        ),
        (
            "[source-identity, rust-only-features, release-proof, msrv, vscode]",
            "[source-identity, rust-only-features, release-proof, msrv]",
        ),
        (
            "VSCODE_RESULT: ${{ needs.vscode.result }}",
            "VSCODE_RESULT: ${{ needs.msrv.result }}",
        ),
        (
            "true:success|false:skipped",
            "true:success|true:skipped|false:skipped",
        ),
        (
            "require \"$SELECT_VSCODE\" \"$VSCODE_RESULT\"",
            "echo success",
        ),
    ] {
        assert!(CI.contains(old));
        assert!(
            source_agrees(&CI.replace(old, new), VSCODE).is_err(),
            "{old}"
        );
    }
    assert!(
        source_agrees(
            CI,
            &VSCODE.replace("${{ inputs.ref || github.sha }}", "${{ github.sha }}")
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn uploaded_files_are_not_complete_proof_or_authenticated_receipts() -> Result<(), String> {
    source_agrees(CI, VSCODE)?;
    for (old, new) in [
        ("if-no-files-found: warn", "if-no-files-found: error"),
        (
            "release-readiness-${{ github.run_id }}",
            "release-readiness-${{ github.run_id }}-${{ github.run_attempt }}",
        ),
        ("            target/ripr/reports/agent-receipt.json\n", ""),
    ] {
        assert!(source_agrees(&CI.replace(old, new), VSCODE).is_err());
    }
    for (old, new) in [
        ("if: failure()", "if: always()"),
        ("if-no-files-found: ignore", "if-no-files-found: error"),
        ("-${{ github.run_attempt }}", ""),
    ] {
        assert!(source_agrees(CI, &VSCODE.replace(old, new)).is_err());
    }
    let inventory = source_inventory();
    assert!(
        inventory
            .uploads
            .iter()
            .all(|upload| !upload.exports_artifact_identity)
    );
    assert!(inventory.gaps.contains(&Gap::ReconcilerDenominator));
    assert!(inventory.gaps.contains(&Gap::PerCommandExecutionCustody));
    assert!(
        inventory
            .gaps
            .contains(&Gap::ArtifactCompletenessAndCustody)
    );
    Ok(())
}

#[test]
fn nested_host_guards_and_readiness_warning_are_preserved_as_limits() -> Result<(), String> {
    let scripts: serde_json::Value =
        serde_json::from_str(include_str!("../../../../editors/vscode/package.json"))
            .map_err(|error| error.to_string())?;
    same(
        scripts["scripts"]["test:e2e"].as_str(),
        Some("cargo xtask vscode-test-e2e"),
        "npm e2e owner",
    )?;
    let xtask = include_str!("../../../../xtask/src/main.rs");
    for evidence in [
        "fn vscode_test_e2e()",
        "if build_server {",
        "stage_vscode_test_server_archive(&server_path)?",
        "RIPR_TEST_PACKAGED_SERVER_SHA256",
        "args: vec![\"out/test/runTest.js\".to_string()]",
    ] {
        assert!(xtask.contains(evidence), "{evidence}");
    }
    let host = include_str!("../../../../editors/vscode/test/runTest.ts");
    for evidence in [
        "downloadAndUnzipVSCode({ cachePath, extensionDevelopmentPath })",
        "const mode = value ?? 'trusted'",
        "workspaceTrustMode === 'untrusted'",
        "process.env.RIPR_TEST_MULTIROOT_ONLY !== '1'",
        "process.env.RIPR_TEST_GREP = '^Real multi-root server acceptance'",
        "delete process.env.RIPR_TEST_REQUIRE_RAN",
        "requiredJourneyTitles(",
    ] {
        assert!(host.contains(evidence), "{evidence}");
    }
    let suite = include_str!("../../../../editors/vscode/test/suite/index.ts");
    for evidence in [
        "if (executed === 0)",
        "if (missingRequired.length > 0)",
        "missingRequiredTitles(requiredTitles, passedTitles)",
    ] {
        assert!(suite.contains(evidence), "{evidence}");
    }
    let readiness = include_str!("../../../../xtask/src/reports/release.rs");
    for evidence in [
        "if report.status == \"fail\"",
        "check.status == \"warn\" || check.status == \"not_run\"",
        "\"schema_version\": \"0.1\"",
        "\"version\": report.version",
    ] {
        assert!(readiness.contains(evidence), "{evidence}");
    }
    // These source anchors do not run npm, test hosts, readiness, or CI. Even
    // intact guards cannot authenticate a supplied job result or success file.
    same(
        source_inventory().authority,
        Authority::NotEstablished,
        "source-only boundary",
    )
}
