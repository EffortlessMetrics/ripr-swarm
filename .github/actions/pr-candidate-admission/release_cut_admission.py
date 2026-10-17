#!/usr/bin/env python3
"""Validate the temporary RIPR 0.11 hard-cut freeze before qualification.

The checked-in packet is inert while ``state`` is ``inactive``. After all
selected product PRs land, a packet-only control commit changes it to ``active``
and binds the exact product cut SHA/tree in that commit's sole first parent.
While active, ordinary PR qualification is refused. A packet-only control PR may
move the state to a successor generation or ``candidate_pinned``; no product PR
can self-admit by editing its checkout copy of the packet because the action
reads the packet from the exact base commit.

This is a cheap policy/currentness predicate. It does not select scope, qualify
product behavior, authorize merge, or create the immutable candidate ref.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = "1"
KIND = "ripr_release_cut_admission"
RELEASE = "0.11.0"
PACKET_PATH = "docs/release-candidates/0.11.0-hard-cut-admission.json"
STATES = {"inactive", "active", "candidate_pinned"}
DENIED_DISPOSITIONS = {
    "accepted_defer_with_non_claim",
    "candidate_exclusion_with_manifest",
    "unrelated_to_selected_candidate",
    "superseded",
}
SHA_RE = re.compile(r"[0-9a-f]{40}")


class PacketError(ValueError):
    """The repository-owned packet is malformed or internally inconsistent."""


def _sha(value: Any, field: str, *, optional: bool = False) -> str | None:
    if value is None and optional:
        return None
    if not isinstance(value, str) or not SHA_RE.fullmatch(value):
        raise PacketError(f"{field} must be a lowercase full commit/tree SHA")
    return value


def _nonempty(value: Any, field: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise PacketError(f"{field} must be a non-empty string")
    return value.strip()


def validate_packet(packet: Any) -> dict[str, Any]:
    if not isinstance(packet, dict):
        raise PacketError("packet must be a JSON object")
    expected = {
        "schema_version",
        "kind",
        "release",
        "state",
        "generation",
        "controller_issue",
        "selection_issue",
        "observed_main",
        "admitted",
        "non_claims",
    }
    unknown = sorted(set(packet) - expected)
    missing = sorted(expected - set(packet))
    if unknown:
        raise PacketError(f"packet has unknown fields: {', '.join(unknown)}")
    if missing:
        raise PacketError(f"packet is missing fields: {', '.join(missing)}")
    if packet["schema_version"] != SCHEMA_VERSION:
        raise PacketError(f"schema_version must be {SCHEMA_VERSION}")
    if packet["kind"] != KIND:
        raise PacketError(f"kind must be {KIND}")
    if packet["release"] != RELEASE:
        raise PacketError(f"release must be {RELEASE}")
    state = packet["state"]
    if state not in STATES:
        raise PacketError(f"state must be one of {sorted(STATES)}")
    generation = _nonempty(packet["generation"], "generation")
    if packet["controller_issue"] != 5114 or packet["selection_issue"] != 2766:
        raise PacketError("controller_issue/selection_issue must remain #5114/#2766")

    observed = packet["observed_main"]
    if not isinstance(observed, dict) or set(observed) != {"sha", "tree"}:
        raise PacketError("observed_main must contain exactly sha and tree")
    bound = state in {"active", "candidate_pinned"}
    observed_sha = _sha(observed["sha"], "observed_main.sha", optional=not bound)
    observed_tree = _sha(observed["tree"], "observed_main.tree", optional=not bound)
    if bound and (observed_sha is None or observed_tree is None):
        raise PacketError(f"{state} packet requires the exact product cut SHA/tree")

    rows = packet["admitted"]
    if not isinstance(rows, list):
        raise PacketError("admitted must be an array")
    seen_prs: set[int] = set()
    normalized: list[dict[str, Any]] = []
    row_fields = {
        "pr",
        "head_sha",
        "admission",
        "disposition",
        "owner_issue",
        "release_effect",
        "proof_refs",
    }
    for index, row in enumerate(rows):
        if not isinstance(row, dict) or set(row) != row_fields:
            raise PacketError(
                f"admitted[{index}] must contain exactly {sorted(row_fields)}"
            )
        pr = row["pr"]
        if not isinstance(pr, int) or isinstance(pr, bool) or pr < 1:
            raise PacketError(f"admitted[{index}].pr must be a positive integer")
        if pr in seen_prs:
            raise PacketError(f"admitted PR #{pr} is duplicated")
        seen_prs.add(pr)
        head_sha = _sha(row["head_sha"], f"admitted[{index}].head_sha")
        if row["admission"] != "admitted":
            raise PacketError(f"admitted[{index}].admission must be admitted")
        disposition = _nonempty(
            row["disposition"], f"admitted[{index}].disposition"
        )
        if disposition in DENIED_DISPOSITIONS:
            raise PacketError(f"PR #{pr} uses non-admissible disposition {disposition}")
        owner_issue = row["owner_issue"]
        if (
            not isinstance(owner_issue, int)
            or isinstance(owner_issue, bool)
            or owner_issue < 1
        ):
            raise PacketError(
                f"admitted[{index}].owner_issue must be a positive integer"
            )
        release_effect = _nonempty(
            row["release_effect"], f"admitted[{index}].release_effect"
        )
        proof_refs = row["proof_refs"]
        if not isinstance(proof_refs, list) or any(
            not isinstance(ref, str) or not ref.strip() for ref in proof_refs
        ):
            raise PacketError(
                f"admitted[{index}].proof_refs must be an array of non-empty strings"
            )
        normalized.append(
            {
                "pr": pr,
                "head_sha": head_sha,
                "admission": "admitted",
                "disposition": disposition,
                "owner_issue": owner_issue,
                "release_effect": release_effect,
                "proof_refs": proof_refs,
            }
        )

    non_claims = packet["non_claims"]
    if not isinstance(non_claims, list) or any(
        not isinstance(item, str) or not item.strip() for item in non_claims
    ):
        raise PacketError("non_claims must be an array of non-empty strings")

    return {
        **packet,
        "generation": generation,
        "observed_main": {"sha": observed_sha, "tree": observed_tree},
        "admitted": normalized,
    }


def _commit_identity(commit: Any, label: str) -> tuple[str, str, list[str]]:
    if not isinstance(commit, dict):
        raise PacketError(f"{label} must be a Git commit object")
    sha = _sha(commit.get("sha"), f"{label}.sha")
    tree = _sha(((commit.get("tree") or {}).get("sha")), f"{label}.tree")
    parents = commit.get("parents")
    if not isinstance(parents, list):
        raise PacketError(f"{label}.parents must be an array")
    parent_shas = [
        _sha((parent or {}).get("sha"), f"{label}.parents[{index}].sha")
        for index, parent in enumerate(parents)
    ]
    return sha, tree, [parent for parent in parent_shas if parent is not None]


def _changed_paths(pr_files: Any) -> list[str]:
    if not isinstance(pr_files, list):
        raise PacketError("PR files must be an array")
    paths: list[str] = []
    for index, item in enumerate(pr_files):
        if not isinstance(item, dict) or not isinstance(item.get("filename"), str):
            raise PacketError(f"PR files[{index}] lacks filename")
        paths.append(item["filename"])
    return sorted(set(paths))


def decide(
    packet: dict[str, Any],
    *,
    pr_number: int,
    expected_head: str,
    actual_head: str,
    base_commit: dict[str, Any],
    cut_commit: dict[str, Any] | None,
    pr_files: list[dict[str, Any]],
) -> dict[str, Any]:
    state = packet["state"]
    base_sha, base_tree, base_parents = _commit_identity(base_commit, "base_commit")
    result = {
        "status": "admitted",
        "reason": "cut_inactive",
        "cut_state": state,
        "cut_generation": packet["generation"],
        "pr": pr_number,
        "head_sha": actual_head,
        "base_sha": base_sha,
        "base_tree": base_tree,
    }
    if state == "inactive":
        return result
    if state == "candidate_pinned":
        result["reason"] = "cut_candidate_pinned"
        return result

    if actual_head != expected_head:
        return {**result, "status": "refused", "reason": "cut_candidate_head_moved"}
    if cut_commit is None:
        return {**result, "status": "refused", "reason": "cut_commit_missing"}
    cut_sha, cut_tree, _ = _commit_identity(cut_commit, "cut_commit")
    if packet["observed_main"]["sha"] != cut_sha:
        return {**result, "status": "refused", "reason": "cut_sha_mismatch"}
    if packet["observed_main"]["tree"] != cut_tree:
        return {**result, "status": "refused", "reason": "cut_tree_mismatch"}
    # The active packet lives in one reviewed control commit immediately after
    # the product cut. Any further main movement makes its first parent differ.
    if not base_parents or base_parents[0] != cut_sha:
        return {**result, "status": "refused", "reason": "cut_main_moved"}

    paths = _changed_paths(pr_files)
    if paths == [PACKET_PATH]:
        result["reason"] = "cut_packet_control_update"
        return result
    return {**result, "status": "refused", "reason": "cut_frozen"}


def fixture_packet(state: str = "inactive") -> dict[str, Any]:
    bound = state in {"active", "candidate_pinned"}
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": KIND,
        "release": RELEASE,
        "state": state,
        "generation": "cut-1",
        "controller_issue": 5114,
        "selection_issue": 2766,
        "observed_main": {
            "sha": "b" * 40 if bound else None,
            "tree": "c" * 40 if bound else None,
        },
        "admitted": [
            {
                "pr": 9,
                "head_sha": "9" * 40,
                "admission": "admitted",
                "disposition": "release_required_product",
                "owner_issue": 99,
                "release_effect": "landed before the final cut",
                "proof_refs": ["#99"],
            }
        ]
        if bound
        else [],
        "non_claims": [
            "Admission is not product qualification or merge authority."
        ],
    }


def fixture_commit(sha: str, tree: str, parents: list[str]) -> dict[str, Any]:
    return {
        "sha": sha,
        "tree": {"sha": tree},
        "parents": [{"sha": parent} for parent in parents],
    }


def self_test() -> None:
    def result(packet: dict[str, Any], **overrides: Any) -> dict[str, Any]:
        values = {
            "pr_number": 10,
            "expected_head": "a" * 40,
            "actual_head": "a" * 40,
            "base_commit": fixture_commit("d" * 40, "e" * 40, ["b" * 40]),
            "cut_commit": fixture_commit("b" * 40, "c" * 40, ["1" * 40]),
            "pr_files": [{"filename": "src/lib.rs"}],
        }
        values.update(overrides)
        return decide(validate_packet(packet), **values)

    assert result(fixture_packet())["reason"] == "cut_inactive"
    assert (
        result(fixture_packet("candidate_pinned"))["reason"]
        == "cut_candidate_pinned"
    )
    active = fixture_packet("active")
    assert result(active)["reason"] == "cut_frozen"
    assert (
        result(active, pr_files=[{"filename": PACKET_PATH}])["reason"]
        == "cut_packet_control_update"
    )
    assert result(active, actual_head="f" * 40)["reason"] == "cut_candidate_head_moved"
    assert result(active, cut_commit=None)["reason"] == "cut_commit_missing"
    assert (
        result(active, cut_commit=fixture_commit("f" * 40, "c" * 40, []))[
            "reason"
        ]
        == "cut_sha_mismatch"
    )
    assert (
        result(active, cut_commit=fixture_commit("b" * 40, "f" * 40, []))[
            "reason"
        ]
        == "cut_tree_mismatch"
    )
    moved = fixture_commit("d" * 40, "e" * 40, ["f" * 40])
    assert result(active, base_commit=moved)["reason"] == "cut_main_moved"
    assert (
        result(
            active,
            pr_files=[{"filename": PACKET_PATH}, {"filename": "src/lib.rs"}],
        )["reason"]
        == "cut_frozen"
    )

    duplicate = fixture_packet("active")
    duplicate["admitted"].append(dict(duplicate["admitted"][0]))
    try:
        validate_packet(duplicate)
    except PacketError as error:
        assert "duplicated" in str(error)
    else:
        raise AssertionError("duplicate PR row must fail")

    excluded = fixture_packet("active")
    excluded["admitted"][0]["disposition"] = "accepted_defer_with_non_claim"
    try:
        validate_packet(excluded)
    except PacketError as error:
        assert "non-admissible" in str(error)
    else:
        raise AssertionError("deferred row must fail")

    print("release-cut-admission self-test: 12 controls passed")


def load_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PacketError(f"cannot read {label} {path}: {error}") from error


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--packet", type=Path)
    parser.add_argument("--current", type=Path)
    parser.add_argument("--base-commit", type=Path)
    parser.add_argument("--cut-commit", type=Path)
    parser.add_argument("--pr-files", type=Path)
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not all(
        (args.packet, args.current, args.base_commit, args.pr_files, args.expected_head)
    ):
        parser.error(
            "--packet, --current, --base-commit, --pr-files and --expected-head are required"
        )

    try:
        packet = validate_packet(load_json(args.packet, "packet"))
        current = load_json(args.current, "PR snapshot")
        base_commit = load_json(args.base_commit, "base commit")
        cut_commit = (
            load_json(args.cut_commit, "cut commit") if args.cut_commit else None
        )
        pr_files = load_json(args.pr_files, "PR files")
        pr_number = current.get("number")
        actual_head = ((current.get("head") or {}).get("sha") or "")
        if not isinstance(pr_number, int) or pr_number < 1:
            raise PacketError("PR snapshot lacks a positive number")
        _sha(args.expected_head, "expected head")
        _sha(actual_head, "actual head")
        result = decide(
            packet,
            pr_number=pr_number,
            expected_head=args.expected_head,
            actual_head=actual_head,
            base_commit=base_commit,
            cut_commit=cut_commit,
            pr_files=pr_files,
        )
    except PacketError as error:
        result = {
            "status": "refused",
            "reason": "cut_packet_invalid",
            "detail": str(error),
        }
    print(json.dumps(result, sort_keys=True))
    return 1 if result["status"] == "refused" else 0


if __name__ == "__main__":
    sys.exit(main())
