#!/usr/bin/env python3
"""Validate the temporary RIPR 0.11 hard-cut freeze before qualification.

The checked-in packet is inert while ``state`` is ``inactive``. After all
selected product PRs land, a packet-only control commit changes it to ``active``
and binds the exact product cut SHA/tree in that commit's sole first parent.
While active, ordinary PR qualification is refused. A packet-only control PR may
reopen the cut (``inactive``) or move it to ``candidate_pinned``; no product PR
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
PACKET_PATH = "policy/release-hard-cut-admission.json"
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
        # A rename or removal touches another path or deletes the packet; it
        # is never a packet-only update.
        previous = item.get("previous_filename")
        if isinstance(previous, str) and previous != item["filename"]:
            paths.append(previous)
        if item.get("status") in {"removed", "renamed"}:
            paths.append(f"<{item.get('status')}>")
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
    proposed_packet: Any = None,
) -> dict[str, Any]:
    state = packet["state"]
    base_sha, base_tree, base_parents = _commit_identity(base_commit, "base_commit")
    result = {
        "status": "admitted",
        "reason": "cut_inactive",
        "detail": "",
        "cut_state": state,
        "cut_generation": packet["generation"],
        "pr": pr_number,
        "head_sha": actual_head,
        "base_sha": base_sha,
        "base_tree": base_tree,
    }
    if _changed_paths(pr_files) == [PACKET_PATH]:
        # A control update is the only way through or out of a freeze, so it
        # must be sound before it can become the base every later PR reads. An
        # unreadable head copy fails closed: retry rather than admit unchecked.
        problem = (
            "the PR head's packet could not be read; refresh the exact PR/head evidence and retry"
            if proposed_packet is None
            else check_transition(
                packet, proposed_packet, base_sha=base_sha, base_tree=base_tree
            )
        )
        if problem is not None:
            return {
                **result,
                "status": "refused",
                "reason": "cut_packet_transition_invalid",
                "detail": problem,
            }
    if state == "inactive":
        return result
    if state == "candidate_pinned":
        result["reason"] = "cut_candidate_pinned"
        return result

    if (
        actual_head == expected_head
        and _changed_paths(pr_files) == [PACKET_PATH]
        and proposed_packet is not None
        and proposed_packet.get("state") == "inactive"
    ):
        # Reopening binds no cut, so it must not depend on the recorded cut
        # commit being readable: a freeze whose cut cannot be read has to stay
        # exitable through qualification.
        result["reason"] = "cut_packet_control_update"
        return result

    if actual_head != expected_head:
        return {
            **result,
            "status": "refused",
            "reason": "cut_candidate_head_moved",
            "detail": f"PR head is {actual_head or 'missing'} but admission was requested for {expected_head}; refresh the exact PR/head evidence and retry",
        }
    if cut_commit is None:
        return {
            **result,
            "status": "refused",
            "reason": "cut_commit_missing",
            "detail": "the active hard-cut packet binds a cut commit that could not be read; refresh the exact base evidence and retry",
        }
    cut_sha, cut_tree, _ = _commit_identity(cut_commit, "cut_commit")
    if packet["observed_main"]["sha"] != cut_sha:
        return {
            **result,
            "status": "refused",
            "reason": "cut_sha_mismatch",
            "detail": f"packet observed_main.sha is {packet['observed_main']['sha']} but the read cut commit is {cut_sha}",
        }
    if packet["observed_main"]["tree"] != cut_tree:
        return {
            **result,
            "status": "refused",
            "reason": "cut_tree_mismatch",
            "detail": f"packet observed_main.tree is {packet['observed_main']['tree']} but the read cut commit tree is {cut_tree}",
        }
    # A packet-only PR is the sole way out of a freeze, so it is admitted even
    # after main moved (a second control commit or an unadmitted push);
    # otherwise the freeze could only be cleared by bypassing protection.
    paths = _changed_paths(pr_files)
    if paths == [PACKET_PATH]:
        result["reason"] = "cut_packet_control_update"
        return result
    # The active packet lives in one reviewed control commit immediately after
    # the product cut. Any further main movement makes its first parent differ.
    if not base_parents or base_parents[0] != cut_sha:
        parent = base_parents[0] if base_parents else "missing"
        return {
            **result,
            "status": "refused",
            "reason": "cut_main_moved",
            "detail": f"base first parent is {parent} but the packet cut is {cut_sha}; main moved after the cut and the active freeze refuses ordinary qualification",
        }
    return {
        **result,
        "status": "refused",
        "reason": "cut_frozen",
        "detail": "the active hard-cut freeze refuses ordinary qualification; only a packet-only #5114/#2766 control update may qualify before #1609",
    }


def check_transition(
    current: dict[str, Any],
    proposed: Any,
    *,
    base_sha: str,
    base_tree: str,
) -> str | None:
    """Return a refusal detail when a packet-only PR proposes an unsound packet.

    The proposed packet is the PR head's copy. The action never trusts it for
    enforcement; this only keeps a malformed or mis-bound control commit from
    landing, because a bad packet on the base would refuse every later PR.
    """
    try:
        nxt = validate_packet(proposed)
    except PacketError as error:
        return f"the proposed packet is invalid: {error}"
    before, after = current["state"], nxt["state"]
    if after == "active" and before == "active":
        return (
            "an active cut cannot be replaced by another active cut; reopen "
            "(inactive), land the repair, then freeze a successor cut"
        )
    if after == "active":
        if (
            nxt["observed_main"]["sha"] != base_sha
            or nxt["observed_main"]["tree"] != base_tree
        ):
            return (
                "activation must bind the exact current main: packet observed_main is "
                f"{nxt['observed_main']['sha']}/{nxt['observed_main']['tree']} but main is "
                f"{base_sha}/{base_tree}"
            )
        if nxt["generation"] == current["generation"]:
            return "activation must name a new generation"
    if after == "candidate_pinned" and (
        before != "active" or nxt["observed_main"] != current["observed_main"]
    ):
        return "candidate_pinned may only follow an active cut and must keep its exact observed_main"
    return None


def watch_main(packet: dict[str, Any], *, compare: dict[str, Any]) -> dict[str, Any]:
    """Judge main's tip against the packet it carries.

    ``compare`` is the Git compare of the active cut to the tip. Judging the
    whole range rather than the pushed commit keeps a skipped or coalesced push
    run from hiding a violating commit: under an active cut the only legal
    range is exactly one packet-only control commit sitting directly on the
    cut. The PR path cannot stop a PR whose required check was green before the
    freeze (protection is not strict), so this records the movement instead of
    letting it extend the denominator silently.
    """
    result = {
        "status": "ok",
        "reason": "cut_inactive",
        "detail": "",
        "cut_state": packet["state"],
        "cut_generation": packet["generation"],
    }
    if packet["state"] != "active":
        return result
    cut_sha = packet["observed_main"]["sha"]
    commits = compare.get("commits")
    if compare.get("status") != "ahead" or not isinstance(commits, list):
        return {
            **result,
            "status": "violation",
            "reason": "cut_main_moved",
            "detail": f"main is not a descendant of the active cut {cut_sha} (compare status {compare.get('status')!r})",
        }
    paths = _changed_paths(compare.get("files") or [])
    parent = ((commits[0].get("parents") or [{}])[0].get("sha")) if commits else None
    if len(commits) == 1 and paths == [PACKET_PATH] and parent == cut_sha:
        result["reason"] = "cut_packet_control_update"
        return result
    return {
        **result,
        "status": "violation",
        "reason": "cut_main_moved",
        "detail": (
            f"main is {len(commits)} commit(s) past the active cut {cut_sha} touching "
            f"{', '.join(paths) or 'nothing'}; only one packet-only control commit may "
            "sit on the cut. The denominator is no longer the cut: reconcile via "
            "#5114/#2768 (revert, or reopen and freeze a successor cut)"
        ),
    }


def render_activation(
    *, cut_sha: str, cut_tree: str, generation: str, admitted: list[dict[str, Any]]
) -> dict[str, Any]:
    """Build the activation packet for the exact product cut (operator aid)."""
    packet = fixture_packet("active")
    packet["generation"] = generation
    packet["observed_main"] = {"sha": cut_sha, "tree": cut_tree}
    packet["admitted"] = admitted
    packet["non_claims"] = list(
        json.loads((Path(__file__).resolve().parents[3] / PACKET_PATH).read_text(encoding="utf-8"))["non_claims"]
    )
    return validate_packet(packet)


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
            "proposed_packet": fixture_packet(),
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
    assert "hard-cut freeze" in result(active)["detail"]
    assert "cut commit" in result(active, cut_commit=None)["detail"]
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
            base_commit=moved,
            pr_files=[{"filename": PACKET_PATH}],
        )["reason"]
        == "cut_packet_control_update"
    )
    for hostile in (
        {"filename": PACKET_PATH, "previous_filename": "src/lib.rs", "status": "renamed"},
        {"filename": PACKET_PATH, "status": "removed"},
    ):
        assert result(active, pr_files=[hostile])["reason"] == "cut_frozen"
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

    # Packet-only control updates must be sound before they become the base.
    inactive_p = validate_packet(fixture_packet())
    pinned_p = validate_packet(fixture_packet("candidate_pinned"))
    base_sha, base_tree = "d" * 40, "e" * 40

    def transition(current: dict[str, Any], proposed: Any) -> str | None:
        return check_transition(
            current, proposed, base_sha=base_sha, base_tree=base_tree
        )

    good = fixture_packet("active")
    good["generation"] = "cut-2"
    good["observed_main"] = {"sha": base_sha, "tree": base_tree}
    assert transition(inactive_p, good) is None
    stale = fixture_packet("active")
    stale["generation"] = "cut-2"
    assert "exact current main" in (transition(inactive_p, stale) or "")
    same_gen = dict(good, generation="cut-1")
    assert "new generation" in (transition(inactive_p, same_gen) or "")
    assert "invalid" in (transition(inactive_p, {"state": "active"}) or "")
    active_p = validate_packet(fixture_packet("active"))
    assert "reopen" in (transition(active_p, good) or "")
    assert transition(active_p, fixture_packet()) is None
    assert transition(active_p, fixture_packet("candidate_pinned")) is None
    moved_pin = fixture_packet("candidate_pinned")
    moved_pin["observed_main"] = {"sha": "f" * 40, "tree": "c" * 40}
    assert "exact observed_main" in (transition(active_p, moved_pin) or "")
    assert "only follow an active" in (
        transition(inactive_p, fixture_packet("candidate_pinned")) or ""
    )
    assert pinned_p["state"] == "candidate_pinned"
    refused = result(
        fixture_packet(),
        pr_files=[{"filename": PACKET_PATH}],
        proposed_packet=stale,
    )
    assert refused["reason"] == "cut_packet_transition_invalid"
    # An ordinary PR cannot use the transition path to skip the freeze.
    assert (
        result(active, proposed_packet=fixture_packet())["reason"] == "cut_frozen"
    )

    # Main watch judges the whole range cut..tip, not just the pushed commit.
    def compare(n: int, files: list[str], parent: str = "b" * 40) -> dict[str, Any]:
        return {
            "status": "ahead",
            "commits": [{"parents": [{"sha": parent}]}] + [{}] * (n - 1),
            "files": [{"filename": f} for f in files],
        }

    assert watch_main(inactive_p, compare=compare(5, ["src/lib.rs"]))["status"] == "ok"
    assert watch_main(pinned_p, compare=compare(5, ["src/lib.rs"]))["status"] == "ok"
    assert watch_main(active_p, compare=compare(1, [PACKET_PATH]))["status"] == "ok"
    for bad in (
        compare(1, ["src/lib.rs"]),
        compare(2, [PACKET_PATH]),
        compare(2, [PACKET_PATH, "src/lib.rs"]),
        compare(1, [PACKET_PATH], parent="f" * 40),
        {"status": "diverged", "commits": [], "files": []},
        {},
    ):
        verdict = watch_main(active_p, compare=bad)
        assert verdict["status"] == "violation" and verdict["reason"] == "cut_main_moved"

    # A reopen stays exitable when the recorded cut commit is unreadable or wrong.
    reopen = fixture_packet()
    for cut in (None, fixture_commit("f" * 40, "c" * 40, [])):
        assert (
            result(
                active,
                cut_commit=cut,
                pr_files=[{"filename": PACKET_PATH}],
                proposed_packet=reopen,
            )["reason"]
            == "cut_packet_control_update"
        )
    assert (
        result(
            active,
            cut_commit=None,
            actual_head="f" * 40,
            pr_files=[{"filename": PACKET_PATH}],
            proposed_packet=reopen,
        )["reason"]
        == "cut_candidate_head_moved"
    )
    # An unreadable head copy fails closed for a packet-only PR only.
    assert (
        result(active, pr_files=[{"filename": PACKET_PATH}], proposed_packet=None)[
            "reason"
        ]
        == "cut_packet_transition_invalid"
    )
    print("release-cut-admission self-test: all controls passed")


def load_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PacketError(f"cannot read {label} {path}: {error}") from error


def run_watch(args: argparse.Namespace) -> int:
    try:
        packet = validate_packet(load_json(args.packet, "packet"))
        compare = load_json(args.compare, "compare") if args.compare else {}
        result = watch_main(packet, compare=compare)
    except PacketError as error:
        result = {
            "status": "violation",
            "reason": "cut_packet_invalid",
            "detail": f"the packet on main is unusable and refuses every PR: {error}",
        }
    print(json.dumps(result, sort_keys=True))
    return 1 if result["status"] == "violation" else 0


def run_admit(args: argparse.Namespace) -> int:
    pr_files: Any = []
    proposed: Any = None
    try:
        if args.proposed_packet and args.proposed_packet.exists():
            try:
                proposed = load_json(args.proposed_packet, "proposed packet")
            except PacketError as error:
                # Judged only if the PR is packet-only; validate_packet then
                # reports it. A product PR is never refused for this.
                proposed = {"unreadable": str(error)}
        pr_files = load_json(args.pr_files, "PR files")
        try:
            packet = validate_packet(load_json(args.packet, "packet"))
        except PacketError as error:
            # An unusable base packet would otherwise refuse the very control
            # update that repairs it. Only a packet-only PR carrying a valid
            # packet is let through; everything else stays refused.
            if _changed_paths(pr_files) == [PACKET_PATH] and proposed is not None:
                head = ((load_json(args.current, "PR snapshot").get("head") or {}).get("sha"))
                if head != args.expected_head:
                    raise PacketError(
                        f"{error}; PR head {head} is not the admitted head {args.expected_head}"
                    )
                if validate_packet(proposed)["state"] != "inactive":
                    raise PacketError(
                        f"{error}; a repair of an unusable packet must land inactive"
                    )
                result = {
                    "status": "admitted",
                    "reason": "cut_packet_repair",
                    "detail": f"base packet unusable ({error}); packet-only repair admitted",
                }
                print(json.dumps(result, sort_keys=True))
                return 0
            raise
        current = load_json(args.current, "PR snapshot")
        base_commit = load_json(args.base_commit, "base commit")
        cut_commit = (
            load_json(args.cut_commit, "cut commit") if args.cut_commit else None
        )
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
            proposed_packet=proposed,
        )
    except PacketError as error:
        result = {
            "status": "refused",
            "reason": "cut_packet_invalid",
            "detail": str(error),
        }
    print(json.dumps(result, sort_keys=True))
    return 1 if result["status"] == "refused" else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--packet", type=Path)
    parser.add_argument("--current", type=Path)
    parser.add_argument("--base-commit", type=Path)
    parser.add_argument("--cut-commit", type=Path)
    parser.add_argument("--pr-files", type=Path)
    parser.add_argument("--proposed-packet", type=Path)
    parser.add_argument("--expected-head")
    parser.add_argument("--watch", action="store_true")
    parser.add_argument("--compare", type=Path)
    parser.add_argument("--render-activation", action="store_true")
    parser.add_argument("--cut-sha")
    parser.add_argument("--cut-tree")
    parser.add_argument("--generation")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.render_activation:
        if not (args.cut_sha and args.cut_tree and args.generation):
            parser.error("--cut-sha, --cut-tree and --generation are required")
        try:
            packet = render_activation(
                cut_sha=args.cut_sha,
                cut_tree=args.cut_tree,
                generation=args.generation,
                admitted=[],
            )
        except PacketError as error:
            print(f"cannot render activation packet: {error}", file=sys.stderr)
            return 2
        print(json.dumps(packet, indent=2))
        return 0
    if args.watch:
        if not args.packet:
            parser.error("--packet is required with --watch")
        return run_watch(args)
    if not all(
        (args.packet, args.current, args.base_commit, args.pr_files, args.expected_head)
    ):
        parser.error(
            "--packet, --current, --base-commit, --pr-files and --expected-head are required"
        )
    return run_admit(args)


if __name__ == "__main__":
    sys.exit(main())
