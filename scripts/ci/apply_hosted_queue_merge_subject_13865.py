#!/usr/bin/env python3
"""Align the native-G portfolio consumer with the selected hosted route."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PATH = ROOT / "tools/repo-policy/tests/merge_group_subject.rs"


def replace_exact(old: str, new: str, expected: int = 1) -> None:
    text = PATH.read_text(encoding="utf-8")
    count = text.count(old)
    if count != expected:
        raise SystemExit(f"expected {expected} replacement(s), found {count}: {old!r}")
    PATH.write_text(text.replace(old, new), encoding="utf-8")


replace_exact(
    '        && source.contains("      execution-profile: owned-jit")\n',
    '        && source.contains("      execution-profile: legacy")\n'
    '        && source.contains("      runner-config: \'\\"ubuntu-latest\\"\'")\n'
    '        && source.contains("      runner_config: \'\\"ubuntu-latest\\"\'")\n'
    '        && source.contains("      control_runner_config: \'\\"ubuntu-latest\\"\'")\n',
)
replace_exact(
    "fn caller_requires_every_frozen_native_group_leaf_on_the_owned_route() {\n",
    "fn caller_requires_every_frozen_native_group_leaf_on_the_hosted_route() {\n",
)
replace_exact(
    '        CALLER.replace("proof_profile: merge-group", "proof_profile: qualification"),\n'
    '        CALLER.replace("      run_rust_only: true", "      run_rust_only: false"),\n',
    '        CALLER.replace("proof_profile: merge-group", "proof_profile: qualification"),\n'
    '        CALLER.replace(\n'
    '            "      execution-profile: legacy",\n'
    '            "      execution-profile: owned-jit",\n'
    '        ),\n'
    '        CALLER.replace(\n'
    '            "      runner-config: \'\\"ubuntu-latest\\"\'",\n'
    '            "      runner-config: \'\\"self-hosted\\"\'",\n'
    '        ),\n'
    '        CALLER.replace("      run_rust_only: true", "      run_rust_only: false"),\n',
)
