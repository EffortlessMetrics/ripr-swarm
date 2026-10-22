#!/usr/bin/env python3
"""Pure PR-candidate admission decision used by the composite action.

The decision binds the current PR to the event head, requires the repository's
claim/delta fields, refuses unresolved trusted competitors, and keeps ordinary
non-doc qualification within its declared size envelope. It allocates no
product work and performs no GitHub mutation.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any, Callable
from urllib.parse import quote

PLACEHOLDERS = {
    "",
    "n/a",
    "na",
    "none",
    "not applicable",
    "tbd",
    "todo",
    "unknown",
    "pending",
    "-",
    "—",
}
LABELS = {
    "claim": (
        "governing issue or accepted claim",
        "owner / governing claim",
    ),
    "delta": (
        "acceptance delta (one sentence)",
        "acceptance delta",
    ),
    "replacement": (
        "proposed replacement of pr (if any; not yet accepted)",
        "proposed replacement of pr",
    ),
    "winner": (
        "accepted winner and losing candidate disposition (when decided)",
        "accepted winner and losing candidate disposition",
    ),
}
# Generated files are declared once, in the base commit's .gitattributes, with
# `linguist-generated`. Their lines are verified by the tool that wrote them
# (package manager, blessing command, drift check), not by reading the diff, so
# they do not count toward the ordinary size envelope. Three guards keep the
# label from hiding hand-written code: rules come from the base commit so a PR
# cannot label its own files; only data extensions are ever exempt; and the
# exempt lines have their own cap.
GENERATED_ATTRIBUTE = "linguist-generated"
GENERATED_DATA_EXTENSIONS = {".json", ".jsonl", ".lock", ".md", ".txt", ".stdout"}
GENERATED_EXEMPT_NEVER = {".gitattributes"}
# Declarable shapes. A new generated family is added here, in reviewed code, so
# a .gitattributes edit alone can never widen the exemption.
GENERATED_DECLARATION_ALLOWED = (
    re.compile(r"^(?:\*\*/)?package-lock\.json$"),
    re.compile(r"^(?:\*\*/)?Cargo\.lock$"),
    re.compile(r"^fixtures/(?:[A-Za-z0-9_.\-*]+/)*expected/\*\*$"),
    re.compile(r"^metrics/public-proof/\*\*$"),
)
DEFAULT_MAX_GENERATED_LINES = 40000
# Bounds on the hand-written blobs read to cap count-less files.
BLOB_READ_TIMEOUT_SECONDS = 30
MAX_BLOB_BYTES = 2_000_000
SHA_RE = re.compile(r"[0-9a-f]{40}")
TEMPLATE_LINES = {
    "claim": "Governing issue or accepted claim:",
    "delta": "Acceptance delta (one sentence):",
}


def normalize(value: str) -> str:
    value = value.strip().strip("`")
    return " ".join(value.split()).casefold()


def extract_fields(body: str) -> dict[str, str]:
    found = {"claim": "", "delta": "", "replacement": "", "winner": ""}
    for raw_line in (body or "").splitlines():
        line = raw_line.strip()
        folded = line.casefold()
        for key, labels in LABELS.items():
            if found[key]:
                continue
            for label in labels:
                prefix = f"{label}:"
                if folded.startswith(prefix):
                    found[key] = line[len(prefix) :].strip()
                    break
    return found


def invalid(value: str) -> bool:
    normalized = normalize(value)
    return normalized in PLACEHOLDERS or normalized.startswith("<")


def pr_refs(value: str) -> set[int]:
    return {int(match) for match in re.findall(r"#(\d+)", value or "")}


def first_ref(value: str) -> int | None:
    found = re.search(r"#(\d+)", value or "")
    return int(found.group(1)) if found else None


def reconciled(
    current: dict[str, Any],
    peer: dict[str, Any],
    fields: dict[str, str],
    peer_fields: dict[str, str],
) -> bool:
    """Whether both PR bodies settle which trusted candidate proceeds."""

    number = int(current.get("number") or 0)
    peer_number = int(peer.get("number") or 0)
    winner = first_ref(fields.get("winner", ""))
    peer_winner = first_ref(peer_fields.get("winner", ""))
    if winner == peer_number:
        return False
    if peer_winner == number:
        return True
    if peer_winner == peer_number:
        return False
    if winner == number:
        return True
    return (
        peer_number in pr_refs(fields.get("replacement", ""))
        and number not in pr_refs(peer_fields.get("replacement", ""))
    )


def head_repository(pr: dict[str, Any]) -> str:
    return (((pr.get("head") or {}).get("repo") or {}).get("full_name") or "")


def base_ref(pr: dict[str, Any]) -> str:
    return ((pr.get("base") or {}).get("ref") or "")


def trusted_peer(current: dict[str, Any], peer: dict[str, Any]) -> bool:
    return (
        peer.get("state") == "open"
        and peer.get("draft") is False
        and head_repository(peer) != ""
        and head_repository(peer) == head_repository(current)
        and base_ref(peer) != ""
        and base_ref(peer) == base_ref(current)
    )


def dependabot_identity(current: dict[str, Any]) -> dict[str, str] | None:
    """Policy-derived claim for a same-repository Dependabot update PR.

    Dependabot writes its own PR body and cannot carry the template lines, so
    the claim is the dependency policy in docs/CI.md. The delta names the update
    branch. Peers are compared only on identity lines in their own bodies, so a
    derived identity never competes with another Dependabot PR. Only the GitHub-attested
    bot author on a dependabot/ branch of this repository qualifies; a fork or
    a human-authored PR must still write the lines itself.
    """

    user = current.get("user") or {}
    head = current.get("head") or {}
    base = current.get("base") or {}
    ref = head.get("ref") or ""
    repository = head_repository(current)
    if (
        user.get("login") != "dependabot[bot]"
        or user.get("type") != "Bot"
        or not ref.startswith("dependabot/")
        or repository == ""
        or repository != ((base.get("repo") or {}).get("full_name") or "")
    ):
        return None
    return {
        "claim": "docs/CI.md dependency update policy",
        "delta": f"dependency update {ref}",
    }


def effective_fields(pr: dict[str, Any]) -> dict[str, str]:
    """Body fields, with the Dependabot policy identity filling missing lines."""

    fields = extract_fields(pr.get("body") or "")
    if invalid(fields["claim"]) or invalid(fields["delta"]):
        derived = dependabot_identity(pr)
        if derived is not None:
            fields = {**fields, **derived}
    return fields


def _glob_regex(pattern: str) -> re.Pattern[str]:
    """Translate a gitattributes glob: `*` and `?` stay inside one path
    segment, `**/` spans directories, a trailing `/**` spans everything below."""
    anchored = "/" in pattern.rstrip("/")
    pattern = pattern.lstrip("/")
    out = []
    i = 0
    while i < len(pattern):
        char = pattern[i]
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
        elif pattern.startswith("/**", i) and i + 3 == len(pattern):
            out.append("/.+")
            i += 3
        elif char == "*":
            out.append("[^/]*")
            i += 1
        elif char == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(char))
            i += 1
    body = "".join(out)
    return re.compile(("^" if anchored else "^(?:.*/)?") + body + "$")


def declaration_allowed(pattern: str) -> bool:
    return any(shape.match(pattern) for shape in GENERATED_DECLARATION_ALLOWED)


def generated_state(attributes: list[str]) -> bool | None:
    """Final state of the attribute on one line (last token wins), or None."""
    state = None
    for attribute in attributes:
        if attribute in (GENERATED_ATTRIBUTE, f"{GENERATED_ATTRIBUTE}=true"):
            state = True
        elif attribute in (
            f"-{GENERATED_ATTRIBUTE}",
            f"!{GENERATED_ATTRIBUTE}",
            f"{GENERATED_ATTRIBUTE}=false",
        ):
            state = False
    return state


def parse_generated_rules(text: str) -> list[tuple[re.Pattern[str], bool]]:
    """Ordered (pattern, generated) rules; the last matching rule wins. A
    declaration outside the allowed shapes is ignored, never trusted."""
    rules: list[tuple[re.Pattern[str], bool]] = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        pattern, *attributes = line.split()
        state = generated_state(attributes)
        if state is None:
            continue
        if state and not declaration_allowed(pattern):
            continue
        rules.append((_glob_regex(pattern), state))
    return rules


def check_generated_declarations(text: str) -> list[str]:
    """Problems with the declared generated paths: every declaration must be
    one of the shapes in GENERATED_DECLARATION_ALLOWED."""
    problems = []
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        pattern, *attributes = line.split()
        if any(a.split("=")[0].lstrip("-!") == GENERATED_ATTRIBUTE for a in attributes) and (
            generated_state(attributes) is not False
        ) and not declaration_allowed(pattern):
            problems.append(
                f"{pattern}: not a declarable generated path; allowed shapes are package-lock.json, Cargo.lock, fixtures/**/expected/** and metrics/public-proof/** (extend GENERATED_DECLARATION_ALLOWED in reviewed code)"
            )
    return problems


def is_generated(path: str, rules: list[tuple[re.Pattern[str], bool]]) -> bool:
    name = path.rsplit("/", 1)[-1]
    suffix = ("." + name.rsplit(".", 1)[-1]).lower() if "." in name else ""
    if name in GENERATED_EXEMPT_NEVER or suffix not in GENERATED_DATA_EXTENSIONS:
        return False
    declared = False
    for pattern, generated in rules:
        if pattern.match(path):
            declared = generated
    return declared


def generated_totals(
    pr_files: list[dict[str, Any]],
    rules: list[tuple[re.Pattern[str], bool]],
    changed_files: int,
    total_lines: int | None = None,
    blob_lines: Callable[[dict[str, Any]], int | None] | None = None,
) -> tuple[int, int]:
    """(generated files, generated lines). A truncated file listing cannot
    prove which lines are generated, so it exempts nothing."""
    if len(pr_files) < changed_files:
        return 0, 0
    files = lines = 0
    for entry in pr_files:
        paths = [str(entry.get("filename") or "")]
        if entry.get("previous_filename"):
            paths.append(str(entry["previous_filename"]))
        # A rename counts as generated only if both ends are.
        changed = int(entry.get("additions") or 0) + int(entry.get("deletions") or 0)
        # A zero-line change (pure rename, empty file) saves no review lines, so
        # it stays in the file count and cannot be used to drain it.
        if changed and all(is_generated(path, rules) for path in paths):
            files += 1
            lines += changed
    return _add_unlisted_stats(pr_files, rules, total_lines, files, lines, blob_lines)


def _entry_paths(entry: dict[str, Any]) -> list[str]:
    paths = [str(entry.get("filename") or "")]
    if entry.get("previous_filename"):
        paths.append(str(entry["previous_filename"]))
    return paths


def _add_unlisted_stats(
    pr_files: list[dict[str, Any]],
    rules: list[tuple[re.Pattern[str], bool]],
    total_lines: int | None,
    files: int,
    lines: int,
    blob_lines: Callable[[dict[str, Any]], int | None] | None = None,
) -> tuple[int, int]:
    """GitHub drops per-file line counts for the files past its diff budget
    (they list as zero-line `modified`), while the PR totals still include
    them. Their combined lines are the PR total minus the listed ones. The
    hand-written ones among them are bounded by their own line counts at the
    base and head (`blob_lines`, None when unreadable), and the rest is
    generated. Any unreadable file keeps the whole residual counted."""
    if total_lines is None or blob_lines is None:
        return files, lines
    # Every zero-line entry is unlisted whatever its status, so an added,
    # renamed or removed hand-written file is bounded rather than exempted.
    unlisted = [
        e
        for e in pr_files
        if not (int(e.get("additions") or 0) + int(e.get("deletions") or 0))
    ]
    residual = total_lines - sum(
        int(e.get("additions") or 0) + int(e.get("deletions") or 0) for e in pr_files
    )
    declared = [
        e
        for e in unlisted
        if all(is_generated(path, rules) for path in _entry_paths(e))
    ]
    if residual <= 0 or not declared:
        return files, lines
    bound = 0
    for entry in unlisted:
        if entry in declared:
            continue
        counted = blob_lines(entry)
        if counted is None:
            return files, lines
        bound += counted
    return files + len(declared), lines + max(0, residual - bound)


def identity_detail(fields: dict[str, str]) -> str:
    missing = [
        TEMPLATE_LINES[key]
        for key in ("claim", "delta")
        if invalid(fields.get(key, ""))
    ]
    if not missing:
        return ""
    return (
        "missing or placeholder PR body line(s): "
        + ", ".join(missing)
        + "; add the line(s) from .github/pull_request_template.md with a real value"
    )


def refusal(
    reason: str,
    current: dict[str, Any],
    fields: dict[str, str],
    max_files: int,
    max_lines: int,
    peers: list[int] | None = None,
    detail: str = "",
    generated_files: int = 0,
    generated_lines: int = 0,
) -> dict[str, Any]:
    # Same reviewable totals as an admitted decision, so the published scope
    # fields mean one thing whatever the outcome.
    changed_files = max(0, int(current.get("changed_files") or 0) - generated_files)
    changed_lines = max(
        0,
        int(current.get("additions") or 0)
        + int(current.get("deletions") or 0)
        - generated_lines,
    )
    return {
        "status": "refused",
        "reason": reason,
        "detail": detail,
        "claim": normalize(fields.get("claim", "")),
        "acceptance_delta": normalize(fields.get("delta", "")),
        "competing_prs": peers or [],
        "changed_files": changed_files,
        "changed_lines": changed_lines,
        "max_changed_files": max_files,
        "max_changed_lines": max_lines,
    }


def decide(
    current: dict[str, Any],
    open_candidates: list[dict[str, Any]],
    expected_head: str,
    docs_only: bool,
    max_files: int,
    max_lines: int,
    generated_files: int = 0,
    generated_lines: int = 0,
    max_generated_lines: int = DEFAULT_MAX_GENERATED_LINES,
) -> dict[str, Any]:
    fields = effective_fields(current)
    actual_head = ((current.get("head") or {}).get("sha") or "")
    refuse = lambda reason, peers=None, detail="": refusal(  # noqa: E731
        reason, current, fields, max_files, max_lines, peers, detail,
        generated_files, generated_lines,
    )
    if not expected_head or not SHA_RE.fullmatch(expected_head):
        return refuse(
            "expected_head_missing",
            detail="expected head is missing or is not a full 40-character commit SHA; retry admission with the exact PR head",
        )
    if actual_head != expected_head:
        return refuse(
            "candidate_head_moved",
            detail=f"PR head is {actual_head or 'missing'} but admission was requested for {expected_head}; refresh the exact PR/head evidence and retry",
        )
    if current.get("state") != "open" or current.get("draft") is True:
        return refuse(
            "candidate_not_open_ready",
            detail=f"PR state is {current.get('state')!r} and draft is {current.get('draft')!r}; admission requires an open, ready (non-draft) PR",
        )
    if invalid(fields["claim"]) or invalid(fields["delta"]):
        return refuse("claim_identity_missing", detail=identity_detail(fields))

    claim = normalize(fields["claim"])
    delta = normalize(fields["delta"])
    exact_peers: list[int] = []
    for peer in open_candidates:
        if peer.get("number") == current.get("number") or not trusted_peer(current, peer):
            continue
        peer_fields = effective_fields(peer)
        if invalid(peer_fields["claim"]) or invalid(peer_fields["delta"]):
            continue
        if normalize(peer_fields["claim"]) == claim and normalize(
            peer_fields["delta"]
        ) == delta:
            if reconciled(current, peer, fields, peer_fields):
                continue
            exact_peers.append(int(peer["number"]))

    if exact_peers:
        peers = sorted(exact_peers)
        listed = ", ".join(f"#{peer}" for peer in peers)
        return refuse(
            "competing_candidate",
            peers,
            detail=f"trusted competing PR(s) {listed} share the same governing claim and acceptance delta; reconcile them and record the accepted winner/losing disposition naming the winning PR in one of the competing PR bodies (or name a listed peer as the proposed replacement in the winning PR body) before qualification",
        )

    if not docs_only and generated_lines > max_generated_lines:
        return refuse(
            "generated_scope_oversized",
            detail=f"candidate changes {generated_lines} generated lines, exceeding the limit of {max_generated_lines}; split the regeneration from the hand-written change",
        )
    changed_files = max(0, int(current.get("changed_files") or 0) - generated_files)
    changed_lines = max(
        0,
        int(current.get("additions") or 0)
        + int(current.get("deletions") or 0)
        - generated_lines,
    )
    if not docs_only and (changed_files > max_files or changed_lines > max_lines):
        return refuse(
            "qualification_scope_oversized",
            detail=f"candidate changes {changed_files} files / {changed_lines} changed lines outside declared generated paths, exceeding the ordinary limit of {max_files} files / {max_lines} changed lines; split or recompose the candidate before ordinary qualification",
        )

    return {
        "status": "admitted",
        "reason": "candidate_current_unique_bounded",
        "detail": "",
        "claim": claim,
        "acceptance_delta": delta,
        "competing_prs": [],
        "changed_files": changed_files,
        "changed_lines": changed_lines,
        "max_changed_files": max_files,
        "max_changed_lines": max_lines,
    }


def fixture(
    number: int = 10,
    body: str = "",
    head: str = "a" * 40,
    changed_files: int = 2,
    additions: int = 20,
    deletions: int = 5,
    draft: bool = False,
    repository: str = "EffortlessMetrics/ripr-swarm",
    target: str = "main",
    login: str = "contributor",
    user_type: str = "User",
    ref: str = "feature",
) -> dict[str, Any]:
    return {
        "number": number,
        "state": "open",
        "draft": draft,
        "body": body,
        "user": {"login": login, "type": user_type},
        "head": {"sha": head, "ref": ref, "repo": {"full_name": repository}},
        "base": {"ref": target, "repo": {"full_name": "EffortlessMetrics/ripr-swarm"}},
        "changed_files": changed_files,
        "additions": additions,
        "deletions": deletions,
    }


def fixture_body(
    claim: str = "#2022",
    delta: str = "refuse duplicate candidate qualification",
    extra: str = "",
) -> str:
    return (
        f"Governing issue or accepted claim: {claim}\n"
        f"Acceptance delta (one sentence): {delta}\n"
        f"{extra}"
    )


def self_test() -> None:
    max_files = 160
    max_lines = 3000
    head = "a" * 40

    def run(
        current: dict[str, Any],
        peers: list[dict[str, Any]],
        expected: str = head,
        docs: bool = False,
    ) -> dict[str, Any]:
        return decide(current, peers, expected, docs, max_files, max_lines)

    current = fixture(body=fixture_body(), head=head)
    duplicate = fixture(number=11, body=fixture_body(), head="b" * 40)
    assert run(current, [current, duplicate])["reason"] == "competing_candidate"
    distinct = fixture(
        number=12,
        body=fixture_body(delta="add a delivery-state projection"),
        head="c" * 40,
    )
    assert run(current, [current, distinct])["status"] == "admitted"
    incomplete = fixture(
        number=13,
        body="Governing issue or accepted claim: #2022\n",
        head="d" * 40,
    )
    assert run(current, [current, incomplete])["status"] == "admitted"
    assert (
        run(
            current,
            [current, fixture(number=14, body=fixture_body(), head="e" * 40, draft=True)],
        )["status"]
        == "admitted"
    )
    assert (
        run(
            current,
            [
                current,
                fixture(
                    number=15,
                    body=fixture_body(),
                    head="f" * 40,
                    repository="external/fork",
                ),
            ],
        )["status"]
        == "admitted"
    )
    assert (
        run(
            current,
            [
                current,
                fixture(
                    number=16,
                    body=fixture_body(),
                    head="1" * 40,
                    target="release",
                ),
            ],
        )["status"]
        == "admitted"
    )

    winner = fixture(
        body=fixture_body(
            extra="Accepted winner and losing candidate disposition (when decided): #10 wins, #11 loses\n"
        ),
        head=head,
    )
    assert run(winner, [winner, duplicate])["status"] == "admitted"
    loser = fixture(
        number=11,
        body=fixture_body(
            extra="Accepted winner and losing candidate disposition (when decided): #10 wins, #11 loses\n"
        ),
        head="b" * 40,
    )
    assert run(loser, [winner, loser], "b" * 40)["reason"] == "competing_candidate"
    replaces = fixture(
        body=fixture_body(extra="Proposed replacement of PR (if any; not yet accepted): #11\n"),
        head=head,
    )
    assert run(replaces, [replaces, duplicate])["status"] == "admitted"

    def won(
        number: int, winner_ref: str, head_char: str, extra: str = ""
    ) -> dict[str, Any]:
        return fixture(
            number=number,
            head=head_char * 40,
            body=fixture_body(
                extra=(
                    "Accepted winner and losing candidate disposition (when decided): "
                    f"{winner_ref}\n{extra}"
                )
            ),
        )

    both_self = [won(10, "#10", "a"), won(11, "#11", "b")]
    assert run(both_self[0], both_self)["reason"] == "competing_candidate"
    assert run(both_self[1], both_self, "b" * 40)["reason"] == "competing_candidate"
    third = [won(10, "#10", "a"), won(11, "#99", "b")]
    assert run(third[0], third)["status"] == "admitted"
    assert run(third[1], third, "b" * 40)["reason"] == "competing_candidate"
    conceded = [won(10, "", "a"), won(11, "#10", "b")]
    assert run(conceded[0], conceded)["status"] == "admitted"
    mutual_replace = [
        won(10, "", "a", "Proposed replacement of PR (if any; not yet accepted): #11\n"),
        won(11, "", "b", "Proposed replacement of PR (if any; not yet accepted): #10\n"),
    ]
    assert run(mutual_replace[0], mutual_replace)["reason"] == "competing_candidate"
    assert run(fixture(body="", head=head), [])["reason"] == "claim_identity_missing"
    empty_detail = run(fixture(body="", head=head), [])["detail"]
    assert "Governing issue or accepted claim:" in empty_detail
    assert "Acceptance delta (one sentence):" in empty_detail
    claim_only_detail = run(
        fixture(body="Governing issue or accepted claim: #2022\n", head=head), []
    )["detail"]
    assert "Acceptance delta (one sentence):" in claim_only_detail
    assert "Governing issue or accepted claim:" not in claim_only_detail
    placeholder_detail = run(
        fixture(
            body=fixture_body(delta="TBD"),
            head=head,
        ),
        [],
    )["detail"]
    assert "Acceptance delta (one sentence):" in placeholder_detail
    oversized = fixture(body=fixture_body(), head=head, changed_files=161)
    assert run(oversized, [oversized])["reason"] == "qualification_scope_oversized"
    assert "161 files" in run(oversized, [oversized])["detail"]
    assert "160 files" in run(oversized, [oversized])["detail"]
    assert "#11" in run(current, [current, duplicate])["detail"]
    competing_detail = run(current, [current, duplicate])["detail"]
    assert "one of the competing PR bodies" in competing_detail
    assert "both PR bodies" not in competing_detail
    assert run(oversized, [oversized], docs=True)["status"] == "admitted"
    rules = parse_generated_rules(
        "package-lock.json linguist-generated\n"
        "Cargo.lock linguist-generated=true\n"
        "fixtures/**/expected/** linguist-generated\n"
        "fixtures/**/expected/hand-written.json -linguist-generated\n"
        "fixtures/**/expected/unset.json !linguist-generated\n"
        "crates/** linguist-generated\n"
        "*.json linguist-generated\n"
        ".claude/** linguist-generated\n"
        "docs/*.md linguist-generated\n"
        "# comment linguist-generated\n"
        "* text=auto\n"
    )
    for path, expected in (
        ("package-lock.json", True),
        ("fixtures/a/package-lock.json", True),
        ("Cargo.lock", True),
        ("editors/zed/Cargo.lock", True),
        ("src/not-package-lock.json", False),
        ("fixtures/a/expected/rows/x.json", True),
        ("fixtures/a/b/expected/out.txt", True),
        ("fixtures/a/expected/hand-written.json", False),
        ("fixtures/a/expected/unset.json", False),
        ("fixtures/a/input/expected.json", False),
        ("fixtures/expected/x.json", True),
        ("crates/ripr/src/lib.rs", False),
        ("crates/ripr/data.json", False),
        ("package.json", False),
        ("policy/release-hard-cut-admission.json", False),
        (".claude/skills/x/SKILL.md", False),
        ("docs/CI.md", False),
        (".gitattributes", False),
    ):
        assert is_generated(path, rules) is expected, path
    listed = [
        {"filename": "fixtures/a/package-lock.json", "additions": 900, "deletions": 800},
        {"filename": "Cargo.lock", "additions": 40, "deletions": 10},
        {"filename": "fixtures/a/expected/rows/x.json", "additions": 100, "deletions": 0},
        {"filename": "fixtures/a/expected/run.sh", "additions": 70, "deletions": 0},
        {"filename": "fixtures/a/expected/empty.json", "additions": 0, "deletions": 0},
        {"filename": "crates/ripr/src/lib.rs", "additions": 3, "deletions": 0},
        {"filename": "new.json", "previous_filename": "Cargo.lock", "additions": 1, "deletions": 1},
    ]
    assert generated_totals(listed, rules, 7) == (3, 1850)
    assert generated_totals(listed, rules, 8) == (0, 0), "a truncated listing exempts nothing"
    # Past GitHub's diff budget a file lists with no counts; the residual of the
    # PR total over the counted lines is exempt after the hand-written stat-less
    # files are bounded by their own base+head line counts.
    stat_less = [
        {"filename": "fixtures/a/package-lock.json", "status": "modified", "additions": 900, "deletions": 800},
        {"filename": "fixtures/b/package-lock.json", "status": "modified"},
        {"filename": "fixtures/c/package-lock.json", "status": "modified"},
        {"filename": "crates/ripr/src/lib.rs", "status": "modified", "additions": 3, "deletions": 0},
        {"filename": "fixtures/b/package.json", "status": "modified"},
    ]
    sizes = {"fixtures/b/package.json": 60, "src/new.rs": 500, "src/gone.rs": 400, "src/moved.rs": 700}
    bound = lambda entry: sizes.get(entry["filename"])  # noqa: E731
    assert generated_totals(stat_less, rules, 5) == (1, 1700), "no total, no residual"
    assert generated_totals(stat_less, rules, 5, 1703 + 2900) == (1, 1700), "no reader, no residual"
    assert generated_totals(stat_less, rules, 5, 1703 + 2900, bound) == (3, 1700 + 2840)
    assert generated_totals(stat_less, rules, 5, 1703, bound) == (1, 1700), "no residual lines"
    assert generated_totals(stat_less, rules, 5, 1703 + 40, bound) == (3, 1700), "a bound above the residual exempts none"
    assert generated_totals(stat_less, rules, 5, 1703 + 2900, lambda entry: None) == (1, 1700), "an unreadable file keeps the residual counted"
    # Whatever the status, a count-less hand-written file is bounded, not exempted.
    for status, name, extra in (
        ("added", "src/new.rs", {}),
        ("removed", "src/gone.rs", {}),
        ("renamed", "src/moved.rs", {"previous_filename": "src/old.rs"}),
    ):
        listed_files = stat_less[:4] + [{"filename": name, "status": status, **extra}]
        assert generated_totals(listed_files, rules, 5, 1703 + 2900, bound) == (
            3, 1700 + 2900 - sizes[name]
        ), status
        assert generated_totals(listed_files, rules, 5, 1703 + 2900, lambda entry: None) == (1, 1700), status
    # A rename is generated only if both ends are.
    renamed_lock = stat_less[:3] + [
        {"filename": "fixtures/d/package-lock.json", "previous_filename": "src/old.rs", "status": "renamed"}
    ]
    assert generated_totals(renamed_lock, rules, 4, 1700 + 800, lambda entry: 0) == (3, 2500)
    assert generated_totals(renamed_lock, rules, 4, 1700 + 800, lambda entry: 900) == (3, 1700)
    # The live reader bounds a rename at the merge base under its previous name.
    calls: list[str] = []

    blobs = {
        "src/old.rs": b"a\nb\nc\n",
        "src/moved.rs": b"a\n",
        "img/logo.png": b"\x89PNG\xff\xfe\n",
    }
    sizes_by_path = {"src/huge.rs": 5_000_000}
    mode = {"fail": ""}
    raw_reads: list[str] = []

    def fake_gh(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[bytes]:
        assert kwargs.get("timeout"), "every read has a timeout"
        url = next(a for a in command if a.startswith("repos/"))
        calls.append(url)
        if mode["fail"] == "timeout":
            raise subprocess.TimeoutExpired(command, kwargs["timeout"])
        if mode["fail"] == "error":
            return subprocess.CompletedProcess(command, 1, b"", b"HTTP 502")
        if "/compare/" in url:
            return subprocess.CompletedProcess(command, 0, b"c" * 40 + b"\n", b"")
        path = url.split("contents/")[1].split("?")[0]
        if "--jq" not in command:
            raw_reads.append(path)
        if path in sizes_by_path:
            return subprocess.CompletedProcess(command, 0, str(sizes_by_path[path]).encode(), b"")
        if path not in blobs:
            return subprocess.CompletedProcess(command, 1, b"", b"HTTP 404")
        if "--jq" in command:
            return subprocess.CompletedProcess(command, 0, str(len(blobs[path])).encode(), b"")
        return subprocess.CompletedProcess(command, 0, blobs[path], b"")

    head_now = {"sha": "b" * 40}

    def head_gh(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[bytes]:
        assert kwargs.get("timeout"), "the head read has a timeout"
        if head_now["sha"] == "error":
            return subprocess.CompletedProcess(command, 1, b"", b"HTTP 502")
        return subprocess.CompletedProcess(command, 0, head_now["sha"].encode() + b"\n", b"")

    snapshot = {"head": {"sha": "b" * 40}}
    real_run = subprocess.run
    subprocess.run = head_gh  # type: ignore[assignment]
    try:
        assert head_unmoved("o/r", "7", snapshot)
        head_now["sha"] = "c" * 40
        assert not head_unmoved("o/r", "7", snapshot), "a moved head discounts nothing"
        head_now["sha"] = "error"
        assert not head_unmoved("o/r", "7", snapshot), "an unreadable head discounts nothing"
        head_now["sha"] = "b" * 40
        assert not head_unmoved("o/r", "", snapshot), "no PR number discounts nothing"
        assert not head_unmoved("o/r", "7", {"head": {"sha": "short"}}), "a malformed snapshot head discounts nothing"
    finally:
        subprocess.run = real_run
    real_run = subprocess.run
    subprocess.run = fake_gh  # type: ignore[assignment]
    try:
        reader = gh_blob_lines("o/r", {"base": {"sha": "a" * 40}, "head": {"sha": "b" * 40}})
        assert reader({"filename": "src/moved.rs", "previous_filename": "src/old.rs"}) == 4
        assert reader({"filename": "src/fresh.rs"}) == 0
        assert reader({"filename": "img/logo.png"}) is None, "a binary blob keeps the residual counted"
        assert reader({"filename": "src/huge.rs"}) is None, "an oversized blob is not downloaded"
        assert "src/huge.rs" not in raw_reads
        for failure in ("timeout", "error"):
            mode["fail"] = failure
            assert gh_blob_lines("o/r", {"base": {"sha": "a" * 40}, "head": {"sha": "b" * 40}})(
                {"filename": "src/moved.rs"}
            ) is None, failure
        mode["fail"] = ""
    finally:
        subprocess.run = real_run
    assert any(f"ref={'c' * 40}" in call for call in calls), "base read at the merge base"
    assert not any(f"ref={'a' * 40}" in call and "contents" in call for call in calls), "never at the base tip"
    big = fixture(body=fixture_body(), head=head, additions=3000, deletions=100, changed_files=170)
    assert run(big, [big])["reason"] == "qualification_scope_oversized"
    admitted_big = decide(big, [big], head, False, max_files, max_lines, 20, 1750)
    assert admitted_big["status"] == "admitted"
    assert admitted_big["changed_files"] == 150 and admitted_big["changed_lines"] == 1350
    assert (
        decide(big, [big], head, False, max_files, max_lines, 0, 50)["reason"]
        == "qualification_scope_oversized"
    )
    refused_big = decide(big, [big], head, False, max_files, max_lines, 20, 50)
    assert refused_big["reason"] == "qualification_scope_oversized"
    assert refused_big["changed_files"] == 150 and refused_big["changed_lines"] == 3050
    assert "150 files / 3050 changed lines" in refused_big["detail"]
    huge = fixture(body=fixture_body(), head=head, additions=90000, deletions=0)
    assert (
        decide(huge, [huge], head, False, max_files, max_lines, 2, 90000)["reason"]
        == "generated_scope_oversized"
    )
    assert decide(huge, [huge], head, False, max_files, max_lines, 2, 30000)["reason"] == (
        "qualification_scope_oversized"
    )
    assert check_generated_declarations(
        "package-lock.json linguist-generated\nfixtures/**/expected/** linguist-generated\n"
        "metrics/public-proof/** linguist-generated\ncrates/x/*.rs -linguist-generated\n"
    ) == []
    for bad in (
        "fixtures/notexpected/**", "fixtures/myexpected/**", "fixtures/a/xexpected/**",
        "**/*.rs", "*", "**", "src/lib.rs", "xtask/**/*.py", "scripts/run.sh", "*.json",
        "**/*.md", ".claude/**", ".github/**", "crates/**", "scripts/*", "package.json",
        "policy/**", "CLAUDE.md", "fixtures/**/input/**",
    ):
        assert check_generated_declarations(f"{bad} linguist-generated\n"), bad
    assert check_generated_declarations("src/lib.rs linguist-generated=false linguist-generated\n")
    assert run(current, [current], "b" * 40)["reason"] == "candidate_head_moved"
    assert (
        run(fixture(body=fixture_body(), head=head, draft=True), [], head)["reason"]
        == "candidate_not_open_ready"
    )
    assert run(current, [], "short")["reason"] == "expected_head_missing"
    bump = {
        "login": "dependabot[bot]",
        "user_type": "Bot",
        "ref": "dependabot/npm_and_yarn/editors/vscode/markdown-it-14.3.3",
    }
    admitted = run(fixture(body="Bumps markdown-it.", head=head, **bump), [])
    assert admitted["status"] == "admitted"
    assert admitted["claim"] == "docs/ci.md dependency update policy"
    assert admitted["acceptance_delta"] == f"dependency update {bump['ref']}".casefold()
    sibling = fixture(
        number=11,
        body="Bumps another.",
        head="b" * 40,
        **{**bump, "ref": "dependabot/cargo/serde-1.0.1"},
    )
    assert run(fixture(body="", head=head, **bump), [sibling])["status"] == "admitted"
    for spoof in (
        {**bump, "login": "contributor"},
        {**bump, "user_type": "User"},
        {**bump, "ref": "feature/dependabot/x"},
        {**bump, "repository": "external/fork"},
    ):
        refused = run(fixture(body="", head=head, **spoof), [])
        assert refused["reason"] == "claim_identity_missing", spoof
    matching = fixture(
        number=12,
        body=fixture_body(
            claim="docs/CI.md dependency update policy",
            delta=f"dependency update {bump['ref']}",
        ),
        head="c" * 40,
    )
    bot = fixture(number=10, body="", head=head, **bump)
    assert run(bot, [bot, matching])["reason"] == "competing_candidate"
    assert run(matching, [bot, matching], "c" * 40)["reason"] == "competing_candidate"
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        files_path = Path(tmp) / "files.json"
        attrs_path = Path(tmp) / "gitattributes"
        files_path.write_text(
            json.dumps(
                [
                    {"filename": "fixtures/x/package-lock.json", "additions": 2000, "deletions": 1000},
                    {"filename": "src/lib.rs", "additions": 10, "deletions": 0},
                ]
            ),
            encoding="utf-8",
        )
        attrs_path.write_text("package-lock.json linguist-generated\n", encoding="utf-8")
        wired = fixture(body=fixture_body(), head=head, additions=3010, deletions=1000, changed_files=2)
        assert run(wired, [wired])["reason"] == "qualification_scope_oversized"
        wired_files, wired_lines = generated_from_files(wired, files_path, attrs_path)
        assert (wired_files, wired_lines) == (1, 3000)
        assert decide(wired, [wired], head, False, max_files, max_lines, wired_files, wired_lines)["status"] == "admitted"
        assert generated_from_files(wired, files_path, Path(tmp) / "absent") == (0, 0)
        assert generated_from_files(wired, None, attrs_path) == (0, 0)
    print("candidate-admission self-test: all controls passed")


def gh_blob_lines(
    repository: str, current: dict[str, Any]
) -> Callable[[dict[str, Any]], int | None]:
    """Lines of a listed file at the merge base (under its previous name for a
    rename) plus at the PR head: an upper bound on what the file changed. A
    path absent at one end counts 0 there; any other read failure, including
    an unresolvable merge base, is None so the caller counts the file's lines
    instead."""
    head = str((current.get("head") or {}).get("sha") or "")
    tip = str((current.get("base") or {}).get("sha") or "")
    cache: dict[str, str | None] = {}

    def gh(args: list[str]) -> subprocess.CompletedProcess[bytes]:
        # Bytes, not text: a binary blob must not raise while decoding. A hung
        # or missing `gh` is a failed read, never an exception out of admission.
        try:
            return subprocess.run(
                ["gh", "api", *args], capture_output=True, check=False, timeout=BLOB_READ_TIMEOUT_SECONDS
            )
        except (subprocess.TimeoutExpired, OSError):
            return subprocess.CompletedProcess(args, 1, b"", b"unavailable")

    def merge_base() -> str | None:
        if "sha" not in cache:
            found = ""
            if SHA_RE.fullmatch(tip) and SHA_RE.fullmatch(head):
                done = gh([f"repos/{repository}/compare/{tip}...{head}", "--jq", ".merge_base_commit.sha"])
                found = done.stdout.decode("ascii", "replace").strip() if done.returncode == 0 else ""
            cache["sha"] = found if SHA_RE.fullmatch(found) else None
        return cache["sha"]

    def lines_at(path: str, ref: str) -> int | None:
        url = f"repos/{repository}/contents/{quote(path)}?ref={ref}"
        # Size first, so an oversized blob is never downloaded.
        sized = gh([url, "--jq", ".size"])
        if sized.returncode != 0:
            return 0 if b"HTTP 404" in sized.stderr else None
        try:
            if int(sized.stdout.strip()) > MAX_BLOB_BYTES:
                return None
        except ValueError:
            return None
        done = gh(["-H", "Accept: application/vnd.github.raw", url])
        if done.returncode != 0:
            return 0 if b"HTTP 404" in done.stderr else None
        data = done.stdout
        try:
            data.decode("utf-8")
        except UnicodeDecodeError:
            return None  # a binary blob has no line count to bound by
        return data.count(b"\n") + (1 if data and not data.endswith(b"\n") else 0)

    def count(entry: dict[str, Any]) -> int | None:
        base = merge_base()
        if base is None or not SHA_RE.fullmatch(head):
            return None
        filename = str(entry.get("filename") or "")
        before = lines_at(str(entry.get("previous_filename") or filename), base)
        after = lines_at(filename, head)
        return None if before is None or after is None else before + after

    return count


def head_unmoved(repository: str, number: str, current: dict[str, Any]) -> bool:
    """The file listing is read after the PR snapshot and is mutable. Its
    generated-file discounts apply only if the PR head still is the snapshot's;
    any read problem counts as moved, so nothing is discounted."""
    snapshot = str((current.get("head") or {}).get("sha") or "")
    if not SHA_RE.fullmatch(snapshot) or not number.isdigit():
        return False
    try:
        done = subprocess.run(
            ["gh", "api", f"repos/{repository}/pulls/{number}", "--jq", ".head.sha"],
            capture_output=True, check=False, timeout=BLOB_READ_TIMEOUT_SECONDS,
        )
    except (subprocess.TimeoutExpired, OSError):
        return False
    return done.returncode == 0 and done.stdout.decode("ascii", "replace").strip() == snapshot


def generated_from_files(
    current: dict[str, Any],
    pr_files: Path | None,
    attributes: Path | None,
    blob_lines: Callable[[dict[str, Any]], int | None] | None = None,
) -> tuple[int, int]:
    """(generated files, generated lines) from the fetched PR file listing and
    the base commit's .gitattributes; (0, 0) unless both were supplied."""
    if not pr_files or not attributes or not attributes.exists():
        return 0, 0
    return generated_totals(
        load_json(pr_files) or [],
        parse_generated_rules(attributes.read_text(encoding="utf-8")),
        int(current.get("changed_files") or 0),
        int(current.get("additions") or 0) + int(current.get("deletions") or 0),
        blob_lines,
    )


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--current", type=Path)
    parser.add_argument("--open-prs", type=Path)
    parser.add_argument("--expected-head")
    parser.add_argument("--pr-files", type=Path)
    parser.add_argument(
        "--generated-attributes",
        type=Path,
        help=".gitattributes text from the PR's base commit; the only source of generated paths",
    )
    parser.add_argument("--max-generated-lines", type=int, default=DEFAULT_MAX_GENERATED_LINES)
    parser.add_argument("--check-attributes", type=Path)
    parser.add_argument("--docs-only", action="store_true")
    parser.add_argument("--max-changed-files", type=int, default=160)
    parser.add_argument("--max-changed-lines", type=int, default=3000)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.check_attributes:
        if not args.check_attributes.exists():
            print(f"::error::{args.check_attributes} is missing")
            return 1
        problems = check_generated_declarations(
            args.check_attributes.read_text(encoding="utf-8")
        )
        for problem in problems:
            print(f"::error::{problem}")
        return 1 if problems else 0
    if not args.current or not args.open_prs or args.expected_head is None:
        parser.error("--current, --open-prs and --expected-head are required")
    if args.max_changed_files < 1 or args.max_changed_lines < 1:
        parser.error("qualification limits must be positive")
    current = load_json(args.current)
    repository = os.environ.get("ADMISSION_REPOSITORY", "")
    generated_files, generated_lines = generated_from_files(
        current,
        args.pr_files,
        args.generated_attributes,
        gh_blob_lines(repository, current) if repository else None,
    )
    if generated_files and not head_unmoved(
        repository, os.environ.get("ADMISSION_PR_NUMBER", ""), current
    ):
        generated_files = generated_lines = 0
    result = decide(
        current,
        load_json(args.open_prs),
        args.expected_head,
        args.docs_only,
        args.max_changed_files,
        args.max_changed_lines,
        generated_files,
        generated_lines,
        args.max_generated_lines,
    )
    print(json.dumps(result, sort_keys=True))
    return 1 if result["status"] == "refused" else 0


if __name__ == "__main__":
    sys.exit(main())
