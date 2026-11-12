//! #14200: the selected queue-only generation uses deterministic hosted execution.
const WORKFLOW: &str = include_str!("../../../.github/workflows/routed-rust.yml");

fn findings(source: &str) -> Vec<String> {
    let mut findings = Vec::new();
    for token in [
        "self-hosted",
        "em-ci-queue-control",
        "em-ci-build",
        "rust-heavy",
        "ci-control",
        "EM_RUNNER_READ_TOKEN",
        "owned-jit",
    ] {
        if source.contains(token) {
            findings.push(format!("forbidden queue-route token: {token}"));
        }
    }
    for (row, expected) in [
        ("    runs-on: ubuntu-latest\n", 4),
        ("      runner-config: '\"ubuntu-latest\"'\n", 1),
        ("      runner_config: '\"ubuntu-latest\"'\n", 1),
        ("      control_runner_config: '\"ubuntu-latest\"'\n", 1),
    ] {
        let actual = source.matches(row).count();
        if actual != expected {
            findings.push(format!(
                "hosted row {row:?}: expected {expected}, found {actual}"
            ));
        }
    }
    for required in [
        "    name: Ripr Rust on hosted qualification\n",
        "      execution-profile: legacy\n",
        "      use-scratch: false\n",
        "      test-shards: 1\n",
        "      artifact-name: ripr-rust-hosted\n",
        "      job-timeout-minutes: 20\n",
        "    needs: [intake, admission, rust-owned, windows-subset, python-wheel, ci-feature, printed-command]\n",
    ] {
        if !source.contains(required) {
            findings.push(format!("missing hosted queue contract row {required:?}"));
        }
    }
    if source.matches("persist-credentials: false").count() < 4 {
        findings.push("trusted/candidate checkouts do not all disable credentials".into());
    }
    if source
        .lines()
        .any(|line| line.trim_start().starts_with("ref:") && line.contains("merge_group.base_sha"))
    {
        findings.push("event base is executable authority".into());
    }
    if source
        .matches("ref: 1b6bae558bd5895057726544b9c6baca63effb97")
        .count()
        < 2
    {
        findings.push("immutable trusted evaluator/admission source is missing".into());
    }
    findings
}

#[test]
fn selected_queue_route_is_hosted_and_immutable() {
    assert_eq!(findings(WORKFLOW), Vec::<String>::new());
}

#[test]
fn self_hosted_and_mutable_authority_mutants_are_rejected() {
    for changed in [
        WORKFLOW.replacen("    runs-on: ubuntu-latest", "    runs-on: self-hosted", 1),
        WORKFLOW.replacen(
            "      execution-profile: legacy",
            "      execution-profile: owned-jit",
            1,
        ),
        WORKFLOW.replacen(
            "ref: 1b6bae558bd5895057726544b9c6baca63effb97",
            "ref: github.event.merge_group.base_sha",
            1,
        ),
        WORKFLOW.replacen("persist-credentials: false", "persist-credentials: true", 1),
    ] {
        assert_ne!(changed, WORKFLOW);
        assert!(!findings(&changed).is_empty());
    }
}
