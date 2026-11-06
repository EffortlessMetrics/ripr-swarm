use super::*;

pub(super) const CALLED: &str = "contains(toJSON(inputs), '\"subject_sha\"')";
pub(super) const SURFACE: &str = "steps.surface.outputs.rust == 'true'";
pub(super) const RELEASE: &str = "contains(toJSON(inputs), '\"subject_sha\"') || (github.event_name == 'push' && (github.ref == 'refs/heads/main' || github.ref == 'refs/heads/master')) || contains(github.event.pull_request.labels.*.name, 'release-check') || contains(github.event.pull_request.labels.*.name, 'full-ci')";
const RELEASE_UPLOAD: &str = "always() && (contains(toJSON(inputs), '\"subject_sha\"') || (github.event_name == 'push' && (github.ref == 'refs/heads/main' || github.ref == 'refs/heads/master')) || contains(github.event.pull_request.labels.*.name, 'release-check') || contains(github.event.pull_request.labels.*.name, 'full-ci'))";
const LEGACY: &str = "(github.event_name != 'pull_request' || (contains(github.event.pull_request.labels.*.name, 'merge-ready') && (github.event.action == 'labeled' && github.event.label.name == 'merge-ready')))";
const RUNNER: &str = "${{ fromJSON(inputs.runner_config || '\"ubuntu-latest\"') }}";

const BUDGET_INPUTS: &[&str] = &[
    "qualification_budget",
    "budget_artifact_id",
    "budget_verifier_sha256",
];

const RESULT_EDGES: &[ResultEdge] = &[
    ResultEdge {
        child_job: "rust-only-features",
        env: "RUST_RESULT",
        selection_env: "SELECT_RUST",
    },
    ResultEdge {
        child_job: "release-proof",
        env: "RELEASE_RESULT",
        selection_env: "SELECT_RELEASE",
    },
    ResultEdge {
        child_job: "msrv",
        env: "MSRV_RESULT",
        selection_env: "SELECT_MSRV",
    },
    ResultEdge {
        child_job: "vscode",
        env: "VSCODE_RESULT",
        selection_env: "SELECT_VSCODE",
    },
];

fn command(
    name: Option<&'static str>,
    run: &'static str,
    condition: Option<&'static str>,
    role: CommandRole,
) -> Command {
    Command {
        name,
        run,
        condition,
        role,
    }
}

fn child(
    job: &'static str,
    selection: &'static str,
    legacy: &str,
    commands: Vec<Command>,
) -> Child {
    Child {
        caller_job: job,
        selection_input: selection,
        condition: format!(
            "always() && !cancelled() && (({CALLED} && needs.source-identity.result == 'success' && inputs.{selection}) || (!{CALLED} && {legacy}))"
        ),
        producer: Owner {
            workflow: CI_WORKFLOW,
            job,
        },
        runner: RUNNER,
        working_directory: ".",
        commands,
    }
}

/// Installed source denominator: four selectable children, not four equivalent
/// tests. There is no Actions strategy matrix in either workflow. All four are
/// selected by routed-rust.yml's native-G caller; manual ci.yml is different
/// (no source-identity/ci-result and no release-surface steps by default).
///
/// This returns 12 proof command steps and 5 prerequisites, plus the separately
/// described surface classifier. Internal xtask/test cases are NOT additional
/// workflow rows, and a shell success cannot establish their complete coverage.
pub fn source_inventory() -> Inventory {
    use CommandRole::{Prerequisite, Proof};
    let release = child(
        "release-proof",
        "run_release_proof",
        &format!(
            "{LEGACY} && (github.event_name != 'pull_request' || contains(github.event.pull_request.labels.*.name, 'release-check') || contains(github.event.pull_request.labels.*.name, 'full-ci'))"
        ),
        vec![
            command(
                Some("Check Perl feature adapter"),
                "cargo check -p ripr --features lang-perl",
                None,
                Proof,
            ),
            command(
                Some("Test Perl language analysis"),
                "cargo test -p ripr --features lang-perl --lib analysis::language::perl",
                None,
                Proof,
            ),
            command(
                Some("Test SARIF identity producer-to-policy controls"),
                "cargo test -p xtask --features perl-identity reports::sarif::identity_tests",
                None,
                Proof,
            ),
            command(
                Some("Package list (release surface)"),
                "cargo package -p ripr --list",
                Some(RELEASE),
                Proof,
            ),
            command(
                Some("Publish dry-run (release surface)"),
                "cargo publish -p ripr --dry-run",
                Some(RELEASE),
                Proof,
            ),
            command(
                Some("Unlocked install resolution (release surface)"),
                "cargo package -p ripr --no-verify\npackage_id=\"$(cargo pkgid -p ripr)\"\npackage_version=\"${package_id##*#}\"\ntest -n \"$package_version\"\ntar xzf \"target/package/ripr-$package_version.crate\" -C \"$RUNNER_TEMP\"\ncd \"$RUNNER_TEMP/ripr-$package_version\"\nrm Cargo.lock\ncargo generate-lockfile\ncargo check --target-dir \"$RUNNER_TEMP/unlocked-target\"",
                Some(RELEASE),
                Proof,
            ),
            command(
                Some("Release readiness (release surface)"),
                "release_version=\"$(cargo pkgid -p ripr | sed 's/.*#//')\"\ncargo xtask release-readiness --version \"$release_version\"",
                Some(RELEASE),
                Proof,
            ),
        ],
    );
    let rust = child(
        "rust-only-features",
        "run_rust_only",
        LEGACY,
        vec![
            command(
                Some("Prepare locked owner-return fixture dependencies"),
                "cargo fetch --locked --manifest-path fixtures/owner_return_pin_fallible_literal/input/Cargo.toml",
                Some(SURFACE),
                Prerequisite,
            ),
            command(
                Some("Prepare locked JSON owner-return fixture dependencies"),
                "cargo fetch --locked --manifest-path fixtures/owner_return_pin_json_real_literal/input/Cargo.toml",
                Some(SURFACE),
                Prerequisite,
            ),
            command(
                Some("Prepare locked proptest profile dependencies"),
                "cargo fetch --locked --manifest-path crates/ripr/tests/fixtures/proptest_singleton/input/Cargo.toml",
                Some(SURFACE),
                Prerequisite,
            ),
            command(
                Some("Test --no-default-features --features lang-rust"),
                "cargo test -p ripr --locked --no-default-features --features lang-rust --no-fail-fast",
                Some(SURFACE),
                Proof,
            ),
        ],
    );
    let msrv = child(
        "msrv",
        "run_msrv",
        &format!(
            "{LEGACY} && (github.event_name == 'workflow_dispatch' || (github.event_name == 'pull_request' && contains(github.event.pull_request.labels.*.name, 'full-ci')))"
        ),
        vec![command(
            None,
            "cargo check --workspace --all-targets",
            None,
            Proof,
        )],
    );
    let mut vscode = child(
        "vscode",
        "run_vscode",
        "github.event_name != 'pull_request'",
        vec![
            command(None, "npm ci", None, Prerequisite),
            command(None, "npm run compile", None, Proof),
            command(None, "npm run package", None, Proof),
            command(
                None,
                "sudo apt-get update && sudo apt-get install -y xvfb",
                None,
                Prerequisite,
            ),
            command(None, "xvfb-run -a npm run test:e2e", None, Proof),
        ],
    );
    vscode.producer = Owner {
        workflow: VSCODE_WORKFLOW,
        job: "vscode",
    };
    vscode.runner = "ubuntu-latest";
    vscode.working_directory = "editors/vscode";
    Inventory {
        version: 1,
        authority: Authority::NotEstablished,
        children: vec![release, rust, msrv, vscode],
        nested_call: CallEdge {
            caller: Owner {
                workflow: CI_WORKFLOW,
                job: "vscode",
            },
            callee: Owner {
                workflow: VSCODE_WORKFLOW,
                job: "vscode",
            },
            ref_expression: "${{ inputs.subject_sha || github.sha }}",
            budget_inputs: BUDGET_INPUTS,
        },
        uploads: vec![
            Upload {
                owner: Owner {
                    workflow: CI_WORKFLOW,
                    job: "release-proof",
                },
                step: "Upload release-readiness receipts",
                condition: RELEASE_UPLOAD,
                name: "release-readiness-${{ github.run_id }}",
                paths: &[
                    "target/ripr/reports/release-readiness.json",
                    "target/ripr/reports/release-readiness.md",
                    "target/ripr/reports/agent-receipt.json",
                    "target/ripr/reports/gap-decision-ledger.json",
                    "target/ripr/reports/lsp-cockpit.json",
                    "target/ripr/reports/lsp-cockpit.md",
                    "target/ripr/reports/repo-exposure-latency.json",
                    "target/ripr/reports/repo-exposure-latency.md",
                    "target/ripr/reports/start-here.md",
                    "target/ripr/release-readiness/",
                ],
                missing_files: "warn",
                retention_days: 7,
                exports_artifact_identity: false,
            },
            Upload {
                owner: Owner {
                    workflow: VSCODE_WORKFLOW,
                    job: "vscode",
                },
                step: "Upload VS Code test host diagnostics",
                condition: "failure()",
                name: "vscode-test-host-diagnostics-${{ github.run_id }}-${{ github.run_attempt }}",
                paths: &["target/ripr/vscode-test-host-diagnostics/"],
                missing_files: "ignore",
                retention_days: 5,
                exports_artifact_identity: false,
            },
        ],
        aggregate: Aggregate {
            owner: Owner {
                workflow: CI_WORKFLOW,
                job: "ci-result",
            },
            condition: "always() && contains(toJSON(inputs), '\"subject_sha\"')",
            step: "Require every selected child",
            needs: "[source-identity, rust-only-features, release-proof, msrv, vscode]",
            edges: RESULT_EDGES,
            accepted_pairs: &["true:success", "false:skipped"],
        },
        rust_surface_output: "surface.rust",
        gaps: &[
            Gap::ProducerVersionCustody,
            Gap::PerCommandExecutionCustody,
            Gap::ReadinessIsNotCompleteProof,
            Gap::ArtifactCompletenessAndCustody,
            Gap::NestedVscodeExecutionAndVersion,
            Gap::ReconcilerDenominator,
        ],
    }
}
