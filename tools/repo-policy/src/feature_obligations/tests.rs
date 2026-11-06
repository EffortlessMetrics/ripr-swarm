use super::*;
use std::collections::BTreeSet;

const CI: &str = include_str!("../../../../.github/workflows/ci.yml");
const VSCODE: &str = include_str!("../../../../.github/workflows/vscode-e2e.yml");

// A narrow test oracle for the checked-in YAML spelling, not a YAML parser or
// production policy/admission API. Ambiguous owners refuse instead of choosing
// whichever occurrence makes a mutation appear correct.
fn job<'a>(source: &'a str, name: &str) -> Result<&'a str, String> {
    let (_, jobs) = source.split_once("\njobs:\n").ok_or("missing jobs")?;
    let marker = format!("  {name}:\n");
    let mut matches = jobs.match_indices(&marker);
    let (offset, _) = matches
        .next()
        .ok_or_else(|| format!("missing job {name}"))?;
    if matches.next().is_some() {
        return Err(format!("duplicate job {name}"));
    }
    let tail = &jobs[offset + marker.len()..];
    let end = tail
        .match_indices('\n')
        .find_map(|(offset, _)| {
            let next = &tail[offset + 1..];
            (next.starts_with("  ") && !next.starts_with("   ") && !next.starts_with("  #"))
                .then_some(offset)
        })
        .unwrap_or(tail.len());
    Ok(&tail[..end])
}

fn steps(source: &str) -> Vec<String> {
    source
        .split("      - ")
        .skip(1)
        .map(|step| {
            // Normalize only the first line's sequence marker away.
            format!("        {step}")
        })
        .collect()
}

fn field(source: &str, indent: usize, key: &str) -> Option<String> {
    let prefix = format!("{}{key}: ", " ".repeat(indent));
    let lines = source.lines().collect::<Vec<_>>();
    let index = lines.iter().position(|line| line.starts_with(&prefix))?;
    let value = lines[index].strip_prefix(&prefix)?;
    if value != "|" && value != ">-" {
        return Some(value.to_owned());
    }
    let continuation = " ".repeat(indent + 2);
    let body = lines[index + 1..]
        .iter()
        .take_while(|line| line.starts_with(&continuation) || line.is_empty())
        .filter_map(|line| line.strip_prefix(&continuation))
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>();
    Some(
        body.join(if value == ">-" { " " } else { "\n" })
            .trim_end()
            .to_owned(),
    )
}

fn named_step(source: &str, name: &str) -> Result<String, String> {
    let matching = steps(source)
        .into_iter()
        .filter(|step| field(step, 8, "name").as_deref() == Some(name))
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [step] => Ok(step.clone()),
        _ => Err(format!("missing or ambiguous step {name}")),
    }
}

fn same<T: std::fmt::Debug + PartialEq>(actual: T, expected: T, owner: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{owner}: actual {actual:?}, expected {expected:?}"))
    }
}

fn source_agrees(ci: &str, vscode: &str) -> Result<(), String> {
    let inventory = source_inventory();
    for child in &inventory.children {
        let caller = job(ci, child.caller_job)?;
        same(
            field(caller, 4, "if"),
            Some(child.condition.clone()),
            child.caller_job,
        )?;
        let source = if child.producer.workflow == CI_WORKFLOW {
            ci
        } else {
            vscode
        };
        let producer = job(source, child.producer.job)?;
        same(
            field(producer, 4, "runs-on"),
            Some(child.runner.to_owned()),
            child.caller_job,
        )?;
        if child.working_directory != "." {
            same(
                field(producer, 8, "working-directory"),
                Some(child.working_directory.to_owned()),
                "nested cwd",
            )?;
        }
        let observed = steps(producer)
            .into_iter()
            .filter_map(|step| {
                let run = field(&step, 8, "run")?;
                let name = field(&step, 8, "name");
                if child.caller_job == "rust-only-features"
                    && name.as_deref() == Some("Detect Rust surface change")
                {
                    return None;
                }
                Some((name, run, field(&step, 8, "if")))
            })
            .collect::<Vec<_>>();
        let expected = child
            .commands
            .iter()
            .map(|command| {
                (
                    command.name.map(str::to_owned),
                    command.run.to_owned(),
                    command.condition.map(str::to_owned),
                )
            })
            .collect::<Vec<_>>();
        same(observed, expected, child.caller_job)?;
        if producer.contains("continue-on-error:")
            || producer.contains("strategy:")
            || producer.contains("outputs:")
        {
            return Err(format!(
                "unmodeled execution/result structure in {}",
                child.caller_job
            ));
        }
    }
    let surface = named_step(job(ci, "rust-only-features")?, "Detect Rust surface change")?;
    same(
        field(&surface, 8, "id"),
        Some("surface".to_owned()),
        "surface owner",
    )?;
    for retained in [
        "CALLED: ${{ contains(toJSON(inputs), '\"subject_sha\"') }}",
        "rust=true reason=not_pull_request",
        "[ \"${CALLED:-false}\" != true ] || return 0",
        "[ \"$EVENT_NAME\" = \"pull_request\" ] || return 0",
        "rust=false reason=no_rust_surface",
        "echo \"rust=$rust\" >> \"$GITHUB_OUTPUT\"",
    ] {
        if !surface.contains(retained) {
            return Err(format!("surface boundary changed: {retained}"));
        }
    }
    let call = &inventory.nested_call;
    let caller = job(ci, call.caller.job)?;
    same(
        field(caller, 4, "uses"),
        Some(format!("./{}", call.callee.workflow)),
        "nested workflow",
    )?;
    same(
        field(caller, 6, "ref"),
        Some(call.ref_expression.to_owned()),
        "nested ref",
    )?;
    for input in call.budget_inputs {
        same(
            field(caller, 6, input),
            Some(format!("${{{{ inputs.{input} }}}}")),
            input,
        )?;
    }
    same(
        field(&steps(job(vscode, "vscode")?)[0], 10, "ref"),
        Some("${{ inputs.ref || github.sha }}".to_owned()),
        "nested checkout",
    )?;
    for upload in &inventory.uploads {
        let source = if upload.owner.workflow == CI_WORKFLOW {
            ci
        } else {
            vscode
        };
        let step = named_step(job(source, upload.owner.job)?, upload.step)?;
        same(
            field(&step, 8, "if"),
            Some(upload.condition.to_owned()),
            upload.step,
        )?;
        same(
            field(&step, 10, "name"),
            Some(upload.name.to_owned()),
            upload.step,
        )?;
        same(
            field(&step, 10, "path"),
            Some(upload.paths.join("\n")),
            upload.step,
        )?;
        same(
            field(&step, 10, "if-no-files-found"),
            Some(upload.missing_files.to_owned()),
            upload.step,
        )?;
        same(
            field(&step, 10, "retention-days"),
            Some(upload.retention_days.to_string()),
            upload.step,
        )?;
        same(
            field(&step, 8, "id").is_some(),
            upload.exports_artifact_identity,
            upload.step,
        )?;
    }
    let aggregate = &inventory.aggregate;
    let result = job(ci, aggregate.owner.job)?;
    same(
        field(result, 4, "if"),
        Some(aggregate.condition.to_owned()),
        "result condition",
    )?;
    same(
        field(result, 4, "needs"),
        Some(aggregate.needs.to_owned()),
        "result needs",
    )?;
    let step = named_step(result, aggregate.step)?;
    same(
        aggregate.accepted_pairs,
        &["true:success", "false:skipped"][..],
        "aggregate accepted source pairs",
    )?;
    for edge in aggregate.edges {
        same(
            field(&step, 10, edge.env),
            Some(format!("${{{{ needs.{}.result }}}}", edge.child_job)),
            "result owner",
        )?;
        let child = inventory
            .children
            .iter()
            .find(|child| child.caller_job == edge.child_job)
            .ok_or("unknown result child")?;
        same(
            field(&step, 10, edge.selection_env),
            Some(format!("${{{{ inputs.{} }}}}", child.selection_input)),
            "selection owner",
        )?;
        if !step.contains(&format!(
            "require \"${}\" \"${}\"",
            edge.selection_env, edge.env
        )) {
            return Err("missing result obligation".to_owned());
        }
    }
    for required in [
        "[[ \"$IDENTITY_RESULT\" == success ]] || exit 1",
        "case \"$1:$2\" in true:success|false:skipped) ;; *) echo 'Missing/failed or unexpected CI child' >&2; exit 1 ;; esac",
    ] {
        if !step.contains(required) {
            return Err("changed aggregate result contract".to_owned());
        }
    }
    Ok(())
}

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
