#!/usr/bin/env python3
"""Validate RIPR 0.11 hard-cut admission before expensive PR qualification.

The checked-in packet is inert while ``state`` is ``inactive``. During the
short hard-cut window, ``active`` binds the current main SHA/tree and an exact
PR/head allowlist. ``candidate_pinned`` releases rolling development without
retargeting the immutable candidate.

This is a cheap policy/currentness predicate. It does not decide release
scope, qualify product behavior, or authorize merge/publication.
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
    observed_sha = _sha(observed["sha"], "observed_main.sha", optional=state != "active")
    observed_tree = _sha(
        observed["tree"], "observed_main.tree", optional=state != "active"
    )
    if state == "active" and (observed_sha is None or observed_tree is None):
        raise PacketError("active packet requires observed main SHA and tree")

    rows = packet["admitted"]
    if not isinstance(rows, list):
        raise PacketError("admitted must be an array")
    if state == "active" and not rows:
        raise PacketError("active packet requires at least one admitted PR/head row")
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


def decide(
    packet: dict[str, Any],
    *,
    pr_number: int,
    expected_head: str,
    actual_head: str,
    base_sha: str,
    base_tree: str,
) -> dict[str, Any]:
    state = packet["state"]
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
    if packet["observed_main"]["sha"] != base_sha:
        return {**result, "status": "refused", "reason": "cut_main_sha_moved"}
    if packet["observed_main"]["tree"] != base_tree:
        return {**result, "status": "refused", "reason": "cut_main_tree_moved"}

    row = next((item for item in packet["admitted"] if item["pr"] == pr_number), None)
    if row is None:
        return {**result, "status": "refused", "reason": "cut_pr_not_admitted"}
    if row["head_sha"] != actual_head:
        return {**result, "status": "refused", "reason": "cut_admitted_head_moved"}
    result.update(
        reason="cut_exact_pr_head_admitted",
        disposition=row["disposition"],
        owner_issue=row["owner_issue"],
        release_effect=row["release_effect"],
    )
    return result


def fixture_packet(state: str = "inactive") -> dict[str, Any]:
    active = state == "active"
    return {
        "schema_version": SCHEMA_VERSION,
        "kind": KIND,
        "release": RELEASE,
        "state": state,
        "generation": "cut-1",
        "controller_issue": 5114,
        "selection_issue": 2766,
        "observed_main": {
            "sha": "b" * 40 if active else None,
            "tree": "c" * 40 if active else None,
        },
        "admitted": [
            {
                "pr": 10,
                "head_sha": "a" * 40,
                "admission": "admitted",
                "disposition": "release_required_product",
                "owner_issue": 99,
                "release_effect": "repairs a selected candidate defect",
                "proof_refs": ["#99"],
            }
        ]
        if active
        else [],
        "non_claims": [
            "Admission is not product qualification or merge authority."
        ],
    }


def self_test() -> None:
    def result(packet: dict[str, Any], **overrides: Any) -> dict[str, Any]:
        values = {
            "pr_number": 10,
            "expected_head": "a" * 40,
            "actual_head": "a" * 40,
            "base_sha": "b" * 40,
            "base_tree": "c" * 40,
        }
        values.update(overrides)
        return decide(validate_packet(packet), **values)

    assert result(fixture_packet())["reason"] == "cut_inactive"
    assert result(fixture_packet("candidate_pinned"))["reason"] == "cut_candidate_pinned"
    active = fixture_packet("active")
    assert result(active)["reason"] == "cut_exact_pr_head_admitted"
    assert result(active, pr_number=11)["reason"] == "cut_pr_not_admitted"
    assert result(active, actual_head="d" * 40)["reason"] == "cut_candidate_head_moved"
    assert (
        result(active, expected_head="d" * 40, actual_head="d" * 40)["reason"]
        == "cut_admitted_head_moved"
    )
    assert result(active, base_sha="d" * 40)["reason"] == "cut_main_sha_moved"
    assert result(active, base_tree="d" * 40)["reason"] == "cut_main_tree_moved"

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

    empty = fixture_packet("active")
    empty["admitted"] = []
    try:
        validate_packet(empty)
    except PacketError as error:
        assert "at least one" in str(error)
    else:
        raise AssertionError("empty active set must fail")

    print("release-cut-admission self-test: 11 controls passed")


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
    parser.add_argument("--expected-head")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not all((args.packet, args.current, args.base_commit, args.expected_head)):
        parser.error(
            "--packet, --current, --base-commit and --expected-head are required"
        )

    try:
        packet = validate_packet(load_json(args.packet, "packet"))
        current = load_json(args.current, "PR snapshot")
        base_commit = load_json(args.base_commit, "base commit")
        pr_number = current.get("number")
        actual_head = ((current.get("head") or {}).get("sha") or "")
        base_sha = ((current.get("base") or {}).get("sha") or "")
        base_tree = ((base_commit.get("tree") or {}).get("sha") or "")
        if not isinstance(pr_number, int) or pr_number < 1:
            raise PacketError("PR snapshot lacks a positive number")
        _sha(args.expected_head, "expected head")
        _sha(actual_head, "actual head")
        _sha(base_sha, "base SHA")
        _sha(base_tree, "base tree")
        result = decide(
            packet,
            pr_number=pr_number,
            expected_head=args.expected_head,
            actual_head=actual_head,
            base_sha=base_sha,
            base_tree=base_tree,
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
