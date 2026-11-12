#!/usr/bin/env python3
"""Apply the reviewed #13865/#14200 hosted queue source repair.

Temporary branch-only transaction helper. The publishing workflow removes this
file before proof and commit, so it never becomes part of the selected source.
"""
from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def replace_exact(path: str, old: str, new: str, expected: int = 1) -> None:
    target = ROOT / path
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(
            f"{path}: expected {expected} replacement(s), found {count}: {old!r}"
        )
    target.write_text(text.replace(old, new), encoding="utf-8")


def repair_workflow() -> None:
    path = ".github/workflows/routed-rust.yml"
    replace_exact(
        path,
        """# Immutable reusable control authority. The merge-queue entrypoint pins this
# entire workflow, not just its evaluator. em-ci-queue-control must allow only
# the reviewed path@commit and run clean single-job workers separate from builds.
# No event base, candidate workflow, input, or mutable branch selects control code.
""",
        """# Immutable reusable control authority. The merge-queue entrypoint pins this
# entire workflow, not just its evaluator. Required control and product jobs use
# deterministic GitHub-hosted execution; no runner inventory or candidate-selected
# host participates. No event base, candidate workflow, input, or mutable branch
# selects controller or policy code.
""",
    )
    replace_exact(
        path,
        """    runs-on:
      group: em-ci-queue-control
      labels: [self-hosted, linux, x64, ci-control]
""",
        "    runs-on: ubuntu-latest\n",
        3,
    )
    replace_exact(
        path,
        """    runs-on:
      group: em-ci-build
      labels: [self-hosted, linux, x64, rust-heavy]
""",
        "    runs-on: ubuntu-latest\n",
    )
    for old, new in [
        (
            "    name: Ripr Rust on owned heavy capability\n",
            "    name: Ripr Rust on hosted qualification\n",
        ),
        ("      execution-profile: owned-jit\n", "      execution-profile: legacy\n"),
        ("      artifact-name: ripr-rust-owned\n", "      artifact-name: ripr-rust-hosted\n"),
        (
            "      runner-config: '{\"group\":\"em-ci-build\",\"labels\":[\"self-hosted\",\"linux\",\"x64\",\"rust-heavy\"]}'\n",
            "      runner-config: '\"ubuntu-latest\"'\n",
        ),
        (
            "      runner_config: '{\"group\":\"em-ci-build\",\"labels\":[\"self-hosted\",\"linux\",\"x64\",\"rust-heavy\"]}'\n",
            "      runner_config: '\"ubuntu-latest\"'\n",
        ),
        (
            "      control_runner_config: '{\"group\":\"em-ci-build\",\"labels\":[\"self-hosted\",\"linux\",\"x64\",\"rust-heavy\"]}'\n",
            "      control_runner_config: '\"ubuntu-latest\"'\n",
        ),
    ]:
        replace_exact(path, old, new)

    source = (ROOT / path).read_text(encoding="utf-8")
    forbidden = [
        "self-hosted",
        "em-ci-queue-control",
        "em-ci-build",
        "rust-heavy",
        "ci-control",
        "EM_RUNNER_READ_TOKEN",
        "owned-jit",
    ]
    remaining = [token for token in forbidden if token in source]
    if remaining:
        raise SystemExit(f"hosted route still contains forbidden tokens: {remaining}")
    for row, expected in {
        "    runs-on: ubuntu-latest\n": 4,
        "      runner-config: '\"ubuntu-latest\"'\n": 1,
        "      runner_config: '\"ubuntu-latest\"'\n": 1,
        "      control_runner_config: '\"ubuntu-latest\"'\n": 1,
    }.items():
        actual = source.count(row)
        if actual != expected:
            raise SystemExit(f"hosted route row {row!r}: expected {expected}, found {actual}")


def repair_disposition() -> None:
    path = ".github/actions/pr-candidate-admission/review_disposition.py"
    replace_exact(
        path,
        'BODY_COMPETING_FIELD = re.compile(r"^[ \\t>*\\-+_`#~]*disposition[ \\t*_`]*:", re.IGNORECASE)\n',
        'BODY_COMPETING_FIELD = re.compile(\n'
        '    r"^[ \\t>*\\-+_`#~]*(?:[0-9]+[.)][ \\t]*)?[ \\t*_`#~]*disposition[ \\t*_`]*:",\n'
        '    re.IGNORECASE,\n'
        ')\n',
    )
    replace_exact(
        path,
        '        (f"{line}\\n> disposition: REVIEW_READY", False),  # ambiguous even when positive\n',
        '        (f"{line}\\n> disposition: REVIEW_READY", False),  # ambiguous even when positive\n'
        '        (f"{line}\\n1. disposition: REPAIR_REQUIRED", False),\n'
        '        (f"{line}\\n2) disposition: REVIEW_READY", False),\n',
    )

    test = "tools/repo-policy/tests/review_disposition_admission.rs"
    replace_exact(
        test,
        '    let quoted = format!("> {line}");\n'
        '    let cases: [(&str, &serde_json::Value, serde_json::Value, bool); 17] = [\n',
        '    let quoted = format!("> {line}");\n'
        '    let ordered_dot = format!("{line}\\n1. disposition: REPAIR_REQUIRED");\n'
        '    let ordered_paren = format!("{line}\\n2) disposition: REVIEW_READY");\n'
        '    let cases: [(&str, &serde_json::Value, serde_json::Value, bool); 19] = [\n',
    )
    quoted = (
        '        (\n'
        '            "quoted line",\n'
        '            &none,\n'
        '            pull_object("MEMBER", "o/r", &head, &quoted),\n'
        '            false,\n'
        '        ),\n'
    )
    ordered = quoted + (
        '        (\n'
        '            "ordered-list competing disposition with period",\n'
        '            &none,\n'
        '            pull_object("MEMBER", "o/r", &head, &ordered_dot),\n'
        '            false,\n'
        '        ),\n'
        '        (\n'
        '            "ordered-list competing disposition with parenthesis",\n'
        '            &none,\n'
        '            pull_object("MEMBER", "o/r", &head, &ordered_paren),\n'
        '            false,\n'
        '        ),\n'
    )
    replace_exact(test, quoted, ordered)


def repair_canonical_policy() -> None:
    path = "tools/repo-policy/src/queue_workflow.rs"
    replace_exact(
        path,
        'const HEAVY: &str =\n    "{\\"group\\":\\"em-ci-build\\",\\"labels\\":[\\"self-hosted\\",\\"linux\\",\\"x64\\",\\"rust-heavy\\"]}";\n',
        'const HOSTED: &str = "\\\"ubuntu-latest\\\"";\n',
    )
    replace_exact(
        path,
        '        "workflow_dispatch:",\n        "ubuntu-latest",\n',
        '        "workflow_dispatch:",\n'
        '        "self-hosted",\n'
        '        "em-ci-queue-control",\n'
        '        "em-ci-build",\n'
        '        "rust-heavy",\n'
        '        "ci-control",\n'
        '        "owned-jit",\n',
    )
    replace_exact(
        path,
        '''        let runner = if name == "admission" {
            "    runs-on:\\n      group: em-ci-queue-control\\n      labels: [self-hosted, linux, x64, ci-control]"
        } else {
            "    runs-on:\\n      group: em-ci-build\\n      labels: [self-hosted, linux, x64, rust-heavy]"
        };
        require(
            header.contains(runner),
            &format!("{name} must preserve control/build isolation"),
        );
''',
        '''        require(
            header.contains("    runs-on: ubuntu-latest"),
            &format!("{name} must use deterministic hosted execution"),
        );
''',
    )
    replace_exact(
        path,
        '        "labels: [self-hosted, linux, x64, ci-control]",\n',
        '        "    runs-on: ubuntu-latest",\n',
    )
    replace_exact(
        path,
        '''    require(
        !intake.contains("rust-heavy") && !intake.contains("ref: ${{ github.sha }}"),
        "intake must not execute or allocate candidate capacity",
    );
''',
        '''    require(
        ![
            "self-hosted",
            "em-ci-queue-control",
            "em-ci-build",
            "rust-heavy",
            "ci-control",
            "owned-jit",
        ]
        .iter()
        .any(|token| intake.contains(token))
            && !intake.contains("ref: ${{ github.sha }}"),
        "intake must use hosted capacity without candidate-selected authority",
    );
''',
    )
    replace_exact(
        path,
        '        "      execution-profile: owned-jit",\n',
        '        "      execution-profile: legacy",\n',
    )
    replace_exact(
        path,
        '''    require(
        rust.contains(&format!("      runner-config: '{HEAVY}'")),
        "owned Rust must select the governed heavy capability",
    );
''',
        '''    require(
        rust.contains(&format!("      runner-config: '{HOSTED}'")),
        "Rust qualification must use the deterministic hosted runner",
    );
''',
    )
    replace_exact(
        path,
        '''    require(
        ci.contains(&format!("      runner_config: '{HEAVY}'")),
        "generic Linux CI proof must use owned heavy capacity",
    );
''',
        '''    require(
        ci.contains(&format!("      runner_config: '{HOSTED}'"))
            && ci.contains(&format!("      control_runner_config: '{HOSTED}'")),
        "generic Linux CI proof must use deterministic hosted runners",
    );
''',
    )
    replace_exact(
        path,
        '        "    runs-on:\\n      group: em-ci-queue-control\\n      labels: [self-hosted, linux, x64, ci-control]",\n',
        '        "    runs-on: ubuntu-latest",\n',
    )
    replace_exact(
        path,
        '''        require(
            body.contains("group: em-ci-queue-control"),
            &format!("{name} needs restricted control capacity"),
        );
''',
        '''        require(
            body.contains("    runs-on: ubuntu-latest"),
            &format!("{name} needs deterministic hosted execution"),
        );
''',
    )
    replace_exact(
        path,
        '''/// The only event subscriber is a fixed, input-free call to an immutable workflow.
/// Runner-group selected_workflows is the independent runtime enforcement: a
/// malicious candidate can edit this checker, but cannot grant its job access.
''',
        '''/// The only event subscriber is a fixed, input-free call to an immutable workflow.
/// The caller and its evaluator are pinned independently of event B/H/G; hosted
/// execution removes mutable runner inventory from the qualification authority.
''',
    )
    replace_exact(
        path,
        '''            assert!(
                body.contains("group: em-ci-queue-control"),
                "{name} needs isolated control group"
            );
''',
        '''            assert!(
                body.contains("    runs-on: ubuntu-latest"),
                "{name} needs deterministic hosted execution"
            );
''',
    )
    replace_exact(
        path,
        '''        let hosted = CALLER.replace(HEAVY, "\\\"ubuntu-latest\\\"");
        assert_ne!(hosted, CALLER);
        assert!(!violations(&hosted, Some(RUST)).is_empty());
''',
        '''        let self_hosted = CALLER.replace(
            "    runs-on: ubuntu-latest",
            "    runs-on: self-hosted",
        );
        assert_ne!(self_hosted, CALLER);
        assert!(!violations(&self_hosted, Some(RUST)).is_empty());
''',
    )


def write_hosted_contract() -> None:
    path = ROOT / "tools/repo-policy/tests/hosted_queue_route.rs"
    path.write_text(
        r'''//! #14200: the selected queue-only generation uses deterministic hosted execution.
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
            findings.push(format!("hosted row {row:?}: expected {expected}, found {actual}"));
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
    if source.lines().any(|line| {
        line.trim_start().starts_with("ref:") && line.contains("merge_group.base_sha")
    }) {
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
        WORKFLOW.replacen("      execution-profile: legacy", "      execution-profile: owned-jit", 1),
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
''',
        encoding="utf-8",
    )


def write_changelog() -> None:
    (ROOT / "changelog.d/14200-hosted-queue-route.md").write_text(
        "<!-- section: Internal / CI -->\n"
        "- Route the immutable native-queue qualification generation through deterministic GitHub-hosted jobs while preserving authenticated H intake, exact G composition, mandatory proof fan-in and final currentness checks (#14200, #13865).\n",
        encoding="utf-8",
    )


def main() -> None:
    repair_workflow()
    repair_disposition()
    repair_canonical_policy()
    write_hosted_contract()
    write_changelog()


if __name__ == "__main__":
    main()
