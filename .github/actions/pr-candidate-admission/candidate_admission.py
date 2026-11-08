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
import shutil
import subprocess
import sys
from datetime import datetime, timedelta, timezone
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
    # Fact packets a pinned producer wrote for a verdict-corpus case. Data only
    # (.json), sha256-pinned by the case rows, whose provenance and edits stay
    # in the counted cases/ files.
    re.compile(r"^fixtures/[a-z]+-verdict-corpus/packets/\*\*$"),
    # Repo-exposure captures the evidence-promotion honesty gate regenerates
    # from their Cargo subject and binds by `content_sha256`, so a hand edit
    # fails that gate (RIPR-SPEC-0108). Only the two capture families, one glob
    # each; the hand-written reports beside them stay counted (#12739).
    re.compile(
        r"^fixtures/evidence-promotion-honesty-corpus/reports/"
        r"rust-(?:inherent|repo-exposure)-\*\.json$"
    ),
)
# Families narrower than the global data extensions: only these file types in
# the family are ever exempt, so a note or log dropped beside the packets stays
# counted.
GENERATED_FAMILY_EXTENSIONS = (
    (re.compile(r"^fixtures/[a-z]+-verdict-corpus/packets/"), {".json"}),
    (
        re.compile(
            r"^fixtures/evidence-promotion-honesty-corpus/reports/"
            r"rust-(?:inherent|repo-exposure)-"
        ),
        {".json"},
    ),
)
DEFAULT_MAX_GENERATED_LINES = 40000
# Bounds on the hand-written blobs read to cap count-less files.
BLOB_READ_TIMEOUT_SECONDS = 30
# Resolved through PATHEXT so Windows finds gh.exe or a gh.cmd shim; a bare name
# only matches gh.exe there (#9272).
GH = shutil.which("gh") or "gh"
MAX_BLOB_BYTES = 2_000_000
SHA_RE = re.compile(r"[0-9a-f]{40}")
TEMPLATE_LINES = {
    "claim": "Governing issue or accepted claim:",
    "delta": "Acceptance delta (one sentence):",
}
# History-sync route (#12437). A true merge between the public ripr repository
# and this swarm repository cannot be split without losing the history it
# exists to keep, so a validated subject is admitted past the size caps (and
# only past them). Every condition below must hold; anything unread, malformed
# or ambiguous leaves the PR on the ordinary route.
HISTORY_SYNC_LABEL = "history-sync"
HISTORY_SYNC_REASON = "history_sync_admitted"
HISTORY_SYNC_REPOSITORIES = ("effortlessmetrics/ripr", "effortlessmetrics/ripr-swarm")
HISTORY_SYNC_ROLES = {"admin", "maintain"}
HISTORY_SYNC_SOURCE_LINE = "history-sync source:"
HISTORY_SYNC_BASE_LINE = "history-sync destination base:"
HISTORY_SYNC_SOURCE_RE = re.compile(r"([A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)@([0-9a-f]{40})")
# Ownership claims (#11736 Proposal B v1, observability-only). The machine
# contract is only the casefolded line prefix `Converging this PR:`; the prose
# timestamp after it is free text and is never parsed (#12404's malformed
# `started2026-11-02` example). Liveness comes from the API-attested comment
# `updated_at`, refreshed by the documented edit-at-transitions discipline.
CLAIM_MARKER_PREFIX = "converging this pr:"
CLAIM_WINDOW = timedelta(hours=4)
# Bound on the free-text holder rendered into outputs and summaries.
CLAIM_ACTOR_MAX_CHARS = 200


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


def claim_carrier(body: str) -> str:
    """The body line carrying the claim marker, or ""."""
    for raw_line in (body or "").splitlines():
        line = raw_line.strip()
        if line.casefold().startswith(CLAIM_MARKER_PREFIX):
            return line
    return ""


def claim_actor(body: str) -> str:
    """Free text after the claim prefix on its line. The prose timestamp is
    never interpreted (#12404): detection reads only the prefix, so any
    framework's claim line is understood."""
    carrier = claim_carrier(body)
    if not carrier:
        return ""
    return carrier[len(CLAIM_MARKER_PREFIX) :].strip()[:CLAIM_ACTOR_MAX_CHARS]


def claim_time(value: Any) -> datetime | None:
    """API-attested instant of a comment timestamp, or None when missing or
    malformed. GitHub emits `...Z`; normalize for older Pythons."""
    text = str(value or "").strip()
    if not text:
        return None
    try:
        parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
    except ValueError:
        return None
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed


def live_claims(
    comments: Any, now: datetime
) -> list[dict[str, Any]]:
    """Ownership claims still inside the 4-hour youth window, newest first.

    Reads conversation (issue) comments only. A review-comment payload means
    the fetch used the wrong endpoint (`pulls/{n}/comments`), which must
    detect nothing rather than half of something, so entries carrying
    review-comment-only keys are ignored. A comment without a usable
    API-attested `updated_at` cannot prove youth and is ignored: this field
    annotates, it never fabricates evidence."""
    found: list[dict[str, Any]] = []
    for comment in comments or []:
        if not isinstance(comment, dict):
            continue
        if any(
            comment.get(key) is not None
            for key in ("pull_request_review_id", "diff_hunk", "path")
        ):
            continue
        body = comment.get("body") or ""
        if not claim_carrier(body):
            continue
        updated = claim_time(comment.get("updated_at"))
        if updated is None:
            continue
        age = now - updated
        if age < timedelta(0):
            age = timedelta(0)  # clock skew: a future-dated edit is the youngest
        if age >= CLAIM_WINDOW:
            continue
        found.append(
            {
                "actor": claim_actor(body),
                "login": str((comment.get("user") or {}).get("login") or "unknown"),
                "updated_at": str(comment.get("updated_at") or ""),
                "age_minutes": int(age.total_seconds() // 60),
            }
        )
    found.sort(key=lambda claim: (claim["updated_at"], claim["login"]), reverse=True)
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
                f"{pattern}: not a declarable generated path; allowed shapes are package-lock.json, Cargo.lock, fixtures/**/expected/**, fixtures/<language>-verdict-corpus/packets/**, metrics/public-proof/** and the rust-inherent-*.json and rust-repo-exposure-*.json captures under fixtures/evidence-promotion-honesty-corpus/reports/ (extend GENERATED_DECLARATION_ALLOWED in reviewed code)"
            )
    return problems


def is_generated(path: str, rules: list[tuple[re.Pattern[str], bool]]) -> bool:
    name = path.rsplit("/", 1)[-1]
    suffix = ("." + name.rsplit(".", 1)[-1]).lower() if "." in name else ""
    if name in GENERATED_EXEMPT_NEVER or suffix not in GENERATED_DATA_EXTENSIONS:
        return False
    for family, extensions in GENERATED_FAMILY_EXTENSIONS:
        if family.match(path) and suffix not in extensions:
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
    concurrent_claims: list[dict[str, Any]] | None = None,
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
        "concurrent_claims": list(concurrent_claims or []),
        "changed_files": changed_files,
        "changed_lines": changed_lines,
        "max_changed_files": max_files,
        "max_changed_lines": max_lines,
    }


def history_sync_record(status: str, detail: str = "", declared: dict[str, str] | None = None) -> dict[str, str]:
    declared = declared or {}
    return {
        "status": status,
        "detail": detail,
        "source": declared.get("source", ""),
        "source_sha": declared.get("source_sha", ""),
        "destination_base": declared.get("destination_base", ""),
    }


HISTORY_SYNC_NOT_REQUESTED = history_sync_record("not_requested")


def history_sync_declaration(body: str) -> tuple[dict[str, str] | None, str]:
    """The declared source and destination base, each on exactly one line."""
    sources: list[str] = []
    bases: list[str] = []
    for raw in (body or "").splitlines():
        line = raw.strip()
        folded = line.casefold()
        for prefix, found in ((HISTORY_SYNC_SOURCE_LINE, sources), (HISTORY_SYNC_BASE_LINE, bases)):
            if folded.startswith(prefix):
                found.append(line[len(prefix) :].strip().strip("`").strip())
    if len(sources) != 1 or len(bases) != 1:
        return None, "the PR body must carry exactly one `History-sync source: <owner>/<repo>@<40-hex sha>` line and one `History-sync destination base: <40-hex sha>` line"
    source = HISTORY_SYNC_SOURCE_RE.fullmatch(sources[0])
    if not source:
        return None, "`History-sync source:` must be `<owner>/<repo>@<full lowercase 40-hex sha>`"
    if not SHA_RE.fullmatch(bases[0]):
        return None, "`History-sync destination base:` must be a full lowercase 40-hex sha"
    return {
        "source": source.group(1),
        "source_sha": source.group(2),
        "destination_base": bases[0],
    }, ""


def gh_api_json(path: str, paginate: bool = False) -> Any:
    """One read-only GitHub API read; None on any failure, never an exception."""
    args = [GH, "api", path] + (["--paginate", "--slurp"] if paginate else [])
    try:
        done = subprocess.run(args, capture_output=True, check=False, timeout=BLOB_READ_TIMEOUT_SECONDS)
    except (subprocess.TimeoutExpired, OSError):
        return None
    if done.returncode != 0:
        return None
    try:
        data = json.loads(done.stdout.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        return None
    if paginate:
        if not isinstance(data, list) or not all(isinstance(page, list) for page in data):
            return None
        return [item for page in data for item in page]
    return data


def history_sync_check(
    current: dict[str, Any],
    repository: str,
    event_name: str,
    base_tip: str,
    api: Callable[..., Any],
) -> dict[str, str]:
    """Whether the PR is a validated history-sync subject (#12437).

    `api(path, paginate=False)` returns parsed JSON or None on any failure. No
    read is made unless the label is present. Every unread, malformed or
    unexpected fact rejects; a rejection only means the ordinary route applies.
    """
    labels = [label.get("name") for label in current.get("labels") or [] if isinstance(label, dict)]
    if HISTORY_SYNC_LABEL not in labels:
        return HISTORY_SYNC_NOT_REQUESTED
    declared: dict[str, str] | None = None

    def reject(detail: str) -> dict[str, str]:
        return history_sync_record("rejected", detail, declared)

    if event_name != "pull_request":
        return reject("history-sync is honored only on a merge-ready pull_request request, never a merge-queue or other event")
    base_repository = (((current.get("base") or {}).get("repo") or {}).get("full_name") or "")
    if (
        not repository
        or head_repository(current).casefold() != repository.casefold()
        or base_repository.casefold() != repository.casefold()
    ):
        return reject("head and base must both be this repository (no fork)")
    declared, why = history_sync_declaration(current.get("body") or "")
    if declared is None:
        return reject(why)
    source = declared["source"].casefold()
    if source not in HISTORY_SYNC_REPOSITORIES or source == repository.casefold():
        return reject(f"source repository {declared['source']} is not the other repository of the sync pair")
    head = str((current.get("head") or {}).get("sha") or "")
    number = current.get("number")
    if not SHA_RE.fullmatch(head) or not SHA_RE.fullmatch(base_tip or "") or not isinstance(number, int):
        return reject("the PR head, number or live base tip is not available")

    events = api(f"repos/{repository}/issues/{number}/events?per_page=100", paginate=True)
    if not isinstance(events, list):
        return reject("the PR label events could not be read")
    toggles = [
        event
        for event in events
        if isinstance(event, dict)
        and event.get("event") in ("labeled", "unlabeled")
        and ((event.get("label") or {}).get("name") == HISTORY_SYNC_LABEL)
    ]
    if not toggles or any(not isinstance(event.get("created_at"), str) for event in toggles):
        return reject("no readable `history-sync` label event")
    last = max(toggles, key=lambda event: (event["created_at"], int(event.get("id") or 0)))
    actor = str((last.get("actor") or {}).get("login") or "")
    if last.get("event") != "labeled" or not actor:
        return reject("the latest `history-sync` label event is not a labeling with a known actor")
    permission = api(f"repos/{repository}/collaborators/{quote(actor)}/permission")
    if not isinstance(permission, dict) or permission.get("role_name") not in HISTORY_SYNC_ROLES:
        return reject(f"`history-sync` was last applied by {actor}, who does not hold admin or maintain on {repository}")

    # Identity model: S (declared source), B (declared destination base), H (the
    # exact PR head). The head's own shape is not authority: repair commits may
    # follow the sync merge. What makes it a sync is that H brings S's history
    # in while B does not already contain S.
    def compare_status(repo: str, older: str, newer: str) -> str | None:
        """`ahead`/`identical` when `older` is reachable from `newer`; another
        GitHub status otherwise; None when unread or inconsistent."""
        compared = api(f"repos/{repo}/compare/{older}...{quote(newer, safe='')}?per_page=1")
        if not isinstance(compared, dict) or (compared.get("base_commit") or {}).get("sha") != older:
            return None
        status = compared.get("status")
        reachable = (compared.get("merge_base_commit") or {}).get("sha") == older
        if status in ("ahead", "identical") and reachable:
            return str(status)
        if status in ("behind", "diverged") and not reachable:
            return str(status)
        return None

    def ancestor(repo: str, older: str, newer: str, allow_identical: bool) -> bool:
        allowed = ("ahead", "identical") if allow_identical else ("ahead",)
        return compare_status(repo, older, newer) in allowed

    source_repo = api(f"repos/{declared['source']}")
    default_branch = source_repo.get("default_branch") if isinstance(source_repo, dict) else None
    if not isinstance(default_branch, str) or not default_branch:
        return reject(f"the default branch of {declared['source']} could not be read")
    if not ancestor(declared["source"], declared["source_sha"], default_branch, True):
        return reject(f"source {declared['source_sha']} is not on the {declared['source']} default branch {default_branch}")
    if not ancestor(repository, declared["source_sha"], head, False):
        return reject(f"source {declared['source_sha']} is not an ancestor of the PR head {head}")
    if not ancestor(repository, declared["destination_base"], head, False):
        return reject(f"destination base {declared['destination_base']} is not an ancestor of the PR head {head}")
    if not ancestor(repository, declared["destination_base"], base_tip, True):
        return reject(f"destination base {declared['destination_base']} is not an ancestor of the live base tip {base_tip}")
    # S must be new history for B: reachable from H, not from B. An unread
    # comparison rejects, never counts as "not reachable".
    if compare_status(repository, declared["source_sha"], declared["destination_base"]) not in ("behind", "diverged"):
        return reject(f"source {declared['source_sha']} is already contained in destination base {declared['destination_base']} (or that could not be read); nothing new is synced")
    return history_sync_record("validated", "", declared)


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
    concurrent_claims: list[dict[str, Any]] | None = None,
    history_sync: dict[str, str] | None = None,
) -> dict[str, Any]:
    fields = effective_fields(current)
    actual_head = ((current.get("head") or {}).get("sha") or "")
    sync = history_sync or HISTORY_SYNC_NOT_REQUESTED
    claims = list(concurrent_claims or [])

    def refuse(reason: str, peers: list[int] | None = None, detail: str = "") -> dict[str, Any]:
        result = refusal(
            reason, current, fields, max_files, max_lines, peers, detail,
            generated_files, generated_lines, claims,
        )
        result["history_sync"] = sync
        return result

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

    changed_files = max(0, int(current.get("changed_files") or 0) - generated_files)
    changed_lines = max(
        0,
        int(current.get("additions") or 0)
        + int(current.get("deletions") or 0)
        - generated_lines,
    )
    # Only the size caps yield to a validated history-sync subject (#12437);
    # every refusal above has already applied.
    validated_sync = sync.get("status") == "validated"
    unhonored = (
        f"; the `{HISTORY_SYNC_LABEL}` label was not honored: {sync.get('detail')}"
        if sync.get("status") == "rejected"
        else ""
    )
    if not validated_sync and not docs_only and generated_lines > max_generated_lines:
        return refuse(
            "generated_scope_oversized",
            detail=f"candidate changes {generated_lines} generated lines, exceeding the limit of {max_generated_lines}; split the regeneration from the hand-written change{unhonored}",
        )
    if not validated_sync and not docs_only and (changed_files > max_files or changed_lines > max_lines):
        return refuse(
            "qualification_scope_oversized",
            detail=f"candidate changes {changed_files} files / {changed_lines} changed lines outside declared generated paths, exceeding the ordinary limit of {max_files} files / {max_lines} changed lines; split or recompose the candidate before ordinary qualification{unhonored}",
        )

    return {
        "status": "admitted",
        "reason": HISTORY_SYNC_REASON if validated_sync else "candidate_current_unique_bounded",
        "history_sync": sync,
        "detail": "",
        "claim": claim,
        "acceptance_delta": delta,
        "competing_prs": [],
        "concurrent_claims": claims,
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
        "fixtures/perl-verdict-corpus/packets/** linguist-generated\n"
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-*.json linguist-generated\n"
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-repo-exposure-*.json linguist-generated\n"
    ) == []
    for bad in (
        "fixtures/notexpected/**", "fixtures/myexpected/**", "fixtures/a/xexpected/**",
        "**/*.rs", "*", "**", "src/lib.rs", "xtask/**/*.py", "scripts/run.sh", "*.json",
        "**/*.md", ".claude/**", ".github/**", "crates/**", "scripts/*", "package.json",
        "policy/**", "CLAUDE.md", "fixtures/**/input/**",
        "fixtures/perl-verdict-corpus/**", "fixtures/perl-verdict-corpus/subjects/**",
        "fixtures/perl-verdict-corpus/cases/**", "fixtures/packets/**",
        "fixtures/perl-corpus/packets/**", "fixtures/Perl-verdict-corpus/packets/**",
        "fixtures/a/b-verdict-corpus/packets/**", "fixtures/perl-verdict-corpus/packets/*.rs",
        "fixtures/perl-verdict-corpus/xpackets/**", "xtask/fixtures/perl-verdict-corpus/packets/**",
        "fixtures/evidence-promotion-honesty-corpus/reports/**",
        "fixtures/evidence-promotion-honesty-corpus/reports/*.json",
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-*.json",
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-*",
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-*.md",
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-closure-*.json",
        "fixtures/evidence-promotion-honesty-corpus/reports/perl-*.json",
        "fixtures/evidence-promotion-honesty-corpus/**",
        "fixtures/evidence-promotion-honesty-corpus/corpus.json",
        "fixtures/other-corpus/reports/rust-inherent-*.json",
        "xtask/fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-*.json",
    ):
        assert check_generated_declarations(f"{bad} linguist-generated\n"), bad
    corpus_rules = parse_generated_rules("fixtures/perl-verdict-corpus/packets/** linguist-generated\n")
    assert is_generated("fixtures/perl-verdict-corpus/packets/a.json", corpus_rules)
    # A data file of another type beside the packets stays counted in the totals.
    beside = [
        {"filename": "fixtures/perl-verdict-corpus/packets/a.json", "status": "added", "additions": 900, "deletions": 0},
        {"filename": "fixtures/perl-verdict-corpus/packets/notes.md", "status": "added", "additions": 3500, "deletions": 0},
        {"filename": "fixtures/perl-verdict-corpus/packets/run.stdout", "status": "added", "additions": 10, "deletions": 0},
    ]
    assert generated_totals(beside, corpus_rules, 3) == (1, 900)
    for kept in (
        "fixtures/perl-verdict-corpus/packets/notes.md", "fixtures/perl-verdict-corpus/packets/a.jsonl",
        "fixtures/perl-verdict-corpus/cases/a.json", "fixtures/perl-verdict-corpus/packets/a.pm",
        "fixtures/perl-verdict-corpus/subjects/s/lib/A.pm", "fixtures/python-verdict-corpus/packets/a.json",
    ):
        assert not is_generated(kept, corpus_rules), kept
    honesty_rules = parse_generated_rules(
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-inherent-*.json linguist-generated\n"
        "fixtures/evidence-promotion-honesty-corpus/reports/rust-repo-exposure-*.json linguist-generated\n"
    )
    reports = "fixtures/evidence-promotion-honesty-corpus/reports"
    for captured in (
        f"{reports}/rust-inherent-async-fn-entry.json",
        f"{reports}/rust-repo-exposure-uncalled-sibling.json",
    ):
        assert is_generated(captured, honesty_rules), captured
    # The exemption follows the capture families only: hand-written reports,
    # the case file that carries the provenance, other types and other trees
    # beside them stay counted.
    for kept in (
        f"{reports}/rust-closure-callback-benign-macro-operand.json",
        f"{reports}/rust-matches-panic-message.json",
        f"{reports}/perl_token_substring.json",
        f"{reports}/rust-inherent-async-fn-entry.md",
        f"{reports}/rust-inherent-async-fn-entry.jsonl",
        f"{reports}/rust-inherent-x/y.json",
        "fixtures/evidence-promotion-honesty-corpus/corpus.json",
        "fixtures/evidence-promotion-honesty-corpus/repo-exposure-subjects/inherent-x/src/lib.rs",
        "fixtures/other/reports/rust-inherent-x.json",
    ):
        assert not is_generated(kept, honesty_rules), kept
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

    # Ownership claims (#11736 Proposal B v1): detection reads only the
    # casefolded line prefix; youth comes from the API-attested updated_at,
    # never the prose timestamp.
    now = datetime(2026, 11, 2, 8, 0, 0, tzinfo=timezone.utc)

    def claim(
        body: str = "Converging this PR: rival lane",
        login: str = "rival",
        created: str = "2026-11-02T07:00:00Z",
        updated: str = "2026-11-02T07:50:00Z",
        **extra: Any,
    ) -> dict[str, Any]:
        entry = {
            "body": body,
            "user": {"login": login},
            "created_at": created,
            "updated_at": updated,
        }
        entry.update(extra)
        return entry

    # The #12404 real-world line: malformed prose timestamp, intact prefix.
    malformed = claim(
        "Converging this PR: root delivery session, started2026-11-02 03:25 UTC.",
        login="EffortlessSteven",
        updated="2026-11-02T07:50:00Z",
    )
    claims = live_claims([malformed], now)
    assert len(claims) == 1, claims
    assert claims[0]["actor"] == "root delivery session, started2026-11-02 03:25 UTC."
    assert claims[0]["login"] == "EffortlessSteven"
    assert claims[0]["age_minutes"] == 10
    annotated = run(fixture(body=fixture_body(), head=head), [])
    assert annotated["status"] == "admitted"
    assert annotated["concurrent_claims"] == []
    annotated = decide(fixture(body=fixture_body(), head=head), [], head, False, max_files, max_lines, 0, 0, DEFAULT_MAX_GENERATED_LINES, claims)
    assert annotated["status"] == "admitted", "a live claim annotates, it never refuses"
    assert annotated["concurrent_claims"] == claims
    refused_annotated = run(
        fixture(body="", head=head), [], head
    )  # refusal result carries the same additive field
    assert refused_annotated["concurrent_claims"] == []
    # Youth is windowed: at and past 4 hours the claim is no longer live.
    assert live_claims([claim(updated="2026-11-02T04:00:00Z")], now) == [], "at the boundary"
    assert live_claims([claim(updated="2026-11-02T03:59:59Z")], now) == []
    # The edit-at-transitions discipline refreshes youth: an old comment
    # edited minutes ago is live.
    refreshed = live_claims(
        [claim(created="2026-10-27T07:00:00Z", updated="2026-11-02T07:58:00Z")], now
    )
    assert len(refreshed) == 1 and refreshed[0]["age_minutes"] == 2, refreshed
    # Prose without the prefix is not a claim, even mid-line or near-miss.
    for prose in (
        "I plan on converging this PR: soon",
        "see Converging this PR: above",
        "Converging the PR: other work",
        "",
    ):
        assert live_claims([claim(prose)], now) == [], prose
    # The prefix is case-insensitive; the actor stays free text.
    folded = live_claims([claim("converging this PR: lane B")], now)
    assert len(folded) == 1 and folded[0]["actor"] == "lane B", folded
    # A review-comment payload is the wrong endpoint and detects nothing,
    # even when its body carries the prefix.
    review_shaped = claim(
        "Converging this PR: wrong endpoint",
        pull_request_review_id=7,
        diff_hunk="@@ -1 +1 @@",
    )
    assert live_claims([review_shaped], now) == []
    assert live_claims([claim("Converging this PR: x", path="src/lib.rs")], now) == []
    # A comment without a usable API-attested updated_at cannot prove youth.
    assert live_claims([claim("Converging this PR: x", updated="")], now) == []
    assert live_claims([claim("Converging this PR: x", updated="not-a-time")], now) == []
    assert live_claims([{"user": {"login": "rival"}}], now) == []
    assert live_claims([], now) == []
    # Two live claims: both listed, newest first, each with its holder.
    pair = live_claims(
        [
            claim("Converging this PR: lane A", login="actor-a", updated="2026-11-02T07:40:00Z"),
            claim("Converging this PR: lane B", login="actor-b", updated="2026-11-02T07:55:00Z"),
        ],
        now,
    )
    assert [c["actor"] for c in pair] == ["lane B", "lane A"], pair
    assert {c["login"] for c in pair} == {"actor-a", "actor-b"}

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
    history_sync_self_test(max_files, max_lines)
    print("candidate-admission self-test: all controls passed")


def history_sync_self_test(max_files: int, max_lines: int) -> None:
    """#12437: each history-sync condition, missing on its own, leaves the PR
    on the ordinary route; other refusals still win for a validated subject."""
    repo = "EffortlessMetrics/ripr-swarm"
    source_repo = "EffortlessMetrics/ripr"
    head, source, dest, tip = "a" * 40, "5" * 40, "d" * 40, "e" * 40
    sync_body = fixture_body(
        claim="#12009",
        delta="sync ripr main into swarm",
        extra=f"History-sync source: {source_repo}@{source}\nHistory-sync destination base: `{dest}`\n",
    )

    def sync_pr(**overrides: Any) -> dict[str, Any]:
        labels = overrides.pop("labels", ["history-sync", "merge-ready"])
        pr = fixture(body=sync_body, head=head, changed_files=334, additions=60000, deletions=12000, **overrides)
        pr["labels"] = [{"name": name} for name in labels]
        return pr

    def compare(older: str, status: str) -> dict[str, Any]:
        merge_base = older if status in ("ahead", "identical") else "0" * 40
        return {"status": status, "base_commit": {"sha": older}, "merge_base_commit": {"sha": merge_base}}

    def responses() -> dict[str, Any]:
        return {
            f"repos/{repo}/issues/10/events?per_page=100": [
                {"id": 1, "event": "labeled", "created_at": "2026-11-01T00:00:00Z", "label": {"name": "history-sync"}, "actor": {"login": "contributor"}},
                {"id": 2, "event": "unlabeled", "created_at": "2026-11-01T01:00:00Z", "label": {"name": "history-sync"}, "actor": {"login": "contributor"}},
                {"id": 3, "event": "labeled", "created_at": "2026-11-01T02:00:00Z", "label": {"name": "history-sync"}, "actor": {"login": "owner"}},
                {"id": 4, "event": "labeled", "created_at": "2026-11-01T03:00:00Z", "label": {"name": "merge-ready"}, "actor": {"login": "contributor"}},
            ],
            f"repos/{repo}/collaborators/owner/permission": {"permission": "admin", "role_name": "admin"},
            f"repos/{repo}/collaborators/contributor/permission": {"permission": "write", "role_name": "write"},
            f"repos/{repo}/collaborators/maintainer/permission": {"permission": "write", "role_name": "maintain"},
            f"repos/{source_repo}": {"default_branch": "main"},
            f"repos/{source_repo}/compare/{source}...main?per_page=1": compare(source, "ahead"),
            f"repos/{repo}/compare/{source}...{head}?per_page=1": compare(source, "ahead"),
            f"repos/{repo}/compare/{dest}...{head}?per_page=1": compare(dest, "ahead"),
            f"repos/{repo}/compare/{dest}...{tip}?per_page=1": compare(dest, "ahead"),
            # S is new history for B: B does not reach S.
            f"repos/{repo}/compare/{source}...{dest}?per_page=1": compare(source, "diverged"),
        }

    def check(
        pr: dict[str, Any],
        table: dict[str, Any],
        event: str = "pull_request",
        repository: str = repo,
        base_tip: str = tip,
    ) -> dict[str, str]:
        calls: list[tuple[str, bool]] = []

        def api(path: str, paginate: bool = False) -> Any:
            calls.append((path, paginate))
            return json.loads(json.dumps(table[path])) if path in table else None

        sync = history_sync_check(pr, repository, event, base_tip, api)
        if HISTORY_SYNC_LABEL not in [label["name"] for label in pr.get("labels") or []]:
            assert calls == [], "no history-sync read without the label"
        # The head's own parent shape is never read or relied on: a single-parent
        # repair commit on top of the sync merge is as valid as the merge itself.
        assert not any("/git/commits/" in path for path, _ in calls), calls
        assert all(paginate == ("/events" in path) for path, paginate in calls), calls
        return sync

    def admit(pr: dict[str, Any], sync: dict[str, str], peers: list[dict[str, Any]] | None = None) -> dict[str, Any]:
        return decide(pr, [pr] if peers is None else peers, head, False, max_files, max_lines, history_sync=sync)

    validated = check(sync_pr(), responses())
    assert validated == {
        "status": "validated", "detail": "", "source": source_repo, "source_sha": source, "destination_base": dest,
    }, validated
    admitted = admit(sync_pr(), validated)
    assert admitted["status"] == "admitted" and admitted["reason"] == HISTORY_SYNC_REASON, admitted
    assert admitted["history_sync"]["source_sha"] == source and admitted["history_sync"]["destination_base"] == dest
    assert admitted["changed_files"] == 334 and admitted["changed_lines"] == 72000
    # A live ownership claim is observability only, including on the privileged
    # history-sync route. The two admission profiles must not overwrite each
    # other's typed fields during source/main reconciliation.
    sample_claims = [{"actor": "existing writer", "login": "maintainer", "age_minutes": 5}]
    combined = decide(
        sync_pr(), [sync_pr()], head, False, max_files, max_lines,
        concurrent_claims=sample_claims, history_sync=validated,
    )
    assert combined["reason"] == HISTORY_SYNC_REASON and combined["history_sync"] == validated
    assert combined["concurrent_claims"] == sample_claims, combined
    refused_combined = decide(
        sync_pr(draft=True), [], head, False, max_files, max_lines,
        concurrent_claims=sample_claims, history_sync=validated,
    )
    assert refused_combined["reason"] == "candidate_not_open_ready"
    assert refused_combined["history_sync"] == validated and refused_combined["concurrent_claims"] == sample_claims
    huge_generated = decide(sync_pr(), [sync_pr()], head, False, max_files, max_lines, 1, 50000, history_sync=validated)
    assert huge_generated["reason"] == HISTORY_SYNC_REASON, "the generated cap is a size cap too"
    maintainer = responses()
    maintainer[f"repos/{repo}/issues/10/events?per_page=100"][2]["actor"]["login"] = "maintainer"
    assert check(sync_pr(), maintainer)["status"] == "validated", "maintain is privileged"
    identical = responses()
    identical[f"repos/{source_repo}/compare/{source}...main?per_page=1"] = compare(source, "identical")
    identical[f"repos/{repo}/compare/{dest}...{tip}?per_page=1"] = compare(dest, "identical")
    assert check(sync_pr(), identical)["status"] == "validated", "the source may be the default tip; the base may be the tip"

    def must_fall_back(name: str, sync: dict[str, str], pr: dict[str, Any] | None = None, why: str = "") -> None:
        pr = pr or sync_pr()
        assert sync["status"] != "validated", (name, sync)
        assert why in sync["detail"], (name, why, sync)
        decision = admit(pr, sync)
        assert decision["status"] == "refused" and decision["reason"] == "qualification_scope_oversized", (name, decision)
        if sync["status"] == "rejected":
            assert "history-sync` label was not honored" in decision["detail"], (name, decision)

    must_fall_back("label absent", check(sync_pr(labels=["merge-ready"]), responses()), sync_pr(labels=["merge-ready"]))
    assert check(sync_pr(labels=["History-Sync"]), responses())["status"] == "not_requested", "the label name is exact"
    unprivileged = responses()
    unprivileged[f"repos/{repo}/issues/10/events?per_page=100"][2]["actor"]["login"] = "contributor"
    must_fall_back("non-privileged labeler", check(sync_pr(), unprivileged))
    legacy_only = responses()
    legacy_only[f"repos/{repo}/collaborators/owner/permission"] = {"permission": "admin"}
    must_fall_back("permission without a role name", check(sync_pr(), legacy_only))
    unreadable_permission = responses()
    del unreadable_permission[f"repos/{repo}/collaborators/owner/permission"]
    must_fall_back("unreadable permission", check(sync_pr(), unreadable_permission))
    relabeled_off = responses()
    relabeled_off[f"repos/{repo}/issues/10/events?per_page=100"].append(
        {"id": 5, "event": "unlabeled", "created_at": "2026-11-01T04:00:00Z", "label": {"name": "history-sync"}, "actor": {"login": "owner"}}
    )
    must_fall_back("latest label event is a removal", check(sync_pr(), relabeled_off))
    no_events = responses()
    del no_events[f"repos/{repo}/issues/10/events?per_page=100"]
    must_fall_back("unreadable label events", check(sync_pr(), no_events))
    fork = sync_pr(repository="external/fork")
    must_fall_back("fork head", check(fork, responses()), fork)
    must_fall_back("wrong current repository", check(sync_pr(), responses(), repository="other/repo"))
    for line in (f"History-sync source: {source_repo}@{source}\n", f"History-sync destination base: `{dest}`\n"):
        body_pr = sync_pr()
        body_pr["body"] = sync_body.replace(line, "")
        must_fall_back(f"missing {line.split(':')[0]}", check(body_pr, responses()), body_pr)
    for name, bad in (
        ("duplicate source line", f"History-sync source: {source_repo}@{source}\nHistory-sync source: {source_repo}@{source}\n"),
        ("short source sha", f"History-sync source: {source_repo}@{source[:12]}\n"),
        ("39-hex source sha", f"History-sync source: {source_repo}@{source[:39]}\n"),
        ("upper-case source sha", f"History-sync source: {source_repo}@{'A' * 40}\n"),
        ("self repository", f"History-sync source: {repo}@{source}\n"),
        ("unknown repository", f"History-sync source: someone/ripr@{source}\n"),
    ):
        body_pr = sync_pr()
        body_pr["body"] = sync_body.replace(f"History-sync source: {source_repo}@{source}\n", bad)
        why = "not the other repository" if "repository" in name else "History-sync source"
        must_fall_back(name, check(body_pr, responses()), body_pr, why)
    for name, short in (("short destination base", dest[:12]), ("41-hex destination base", dest + "d")):
        body_pr = sync_pr()
        body_pr["body"] = sync_body.replace(f"`{dest}`", short)
        must_fall_back(name, check(body_pr, responses()), body_pr, "History-sync destination base")
    # S already contained in B: the PR brings no new source history.
    for status in ("ahead", "identical"):
        contained = responses()
        contained[f"repos/{repo}/compare/{source}...{dest}?per_page=1"] = compare(source, status)
        must_fall_back(f"source {status} in the destination base", check(sync_pr(), contained), why="already contained")
    unread_containment = responses()
    del unread_containment[f"repos/{repo}/compare/{source}...{dest}?per_page=1"]
    must_fall_back("unreadable containment", check(sync_pr(), unread_containment), why="already contained")
    inconsistent = responses()
    inconsistent[f"repos/{repo}/compare/{source}...{dest}?per_page=1"] = {
        "status": "diverged", "base_commit": {"sha": source}, "merge_base_commit": {"sha": source},
    }
    must_fall_back("inconsistent containment read", check(sync_pr(), inconsistent), why="already contained")
    for name, key, status in (
        ("source off the source default branch", f"repos/{source_repo}/compare/{source}...main?per_page=1", "diverged"),
        ("source not an ancestor of the head", f"repos/{repo}/compare/{source}...{head}?per_page=1", "diverged"),
        ("source identical to the head", f"repos/{repo}/compare/{source}...{head}?per_page=1", "identical"),
        ("destination base not an ancestor of the head", f"repos/{repo}/compare/{dest}...{head}?per_page=1", "behind"),
        ("destination base not on the live base", f"repos/{repo}/compare/{dest}...{tip}?per_page=1", "diverged"),
    ):
        broken = responses()
        broken[key] = compare(key.split("/compare/")[1][:40], status)
        must_fall_back(name, check(sync_pr(), broken))
    missing_source = responses()
    del missing_source[f"repos/{source_repo}/compare/{source}...main?per_page=1"]
    must_fall_back("source sha absent from the source repository", check(sync_pr(), missing_source))
    must_fall_back("merge-queue event", check(sync_pr(), responses(), event="merge_group"))
    must_fall_back("no live base tip", check(sync_pr(), responses(), base_tip=""), why="live base tip is not available")
    # Other refusals still win for a validated subject; a non-sync PR is unaffected.
    no_claim = sync_pr()
    no_claim["body"] = sync_body.replace("Governing issue or accepted claim: #12009\n", "")
    assert admit(no_claim, check(no_claim, responses()))["reason"] == "claim_identity_missing"
    rival = fixture(number=11, body=fixture_body(claim="#12009", delta="sync ripr main into swarm"), head="b" * 40)
    competing = admit(sync_pr(), validated, [sync_pr(), rival])
    assert competing["reason"] == "competing_candidate", competing
    assert decide(sync_pr(), [sync_pr()], "b" * 40, False, max_files, max_lines, history_sync=validated)["reason"] == "candidate_head_moved"
    assert decide(sync_pr(draft=True), [], head, False, max_files, max_lines, history_sync=validated)["reason"] == "candidate_not_open_ready"
    ordinary = fixture(body=fixture_body(), head=head)
    assert decide(ordinary, [ordinary], head, False, max_files, max_lines)["reason"] == "candidate_current_unique_bounded"
    assert decide(ordinary, [ordinary], head, False, max_files, max_lines)["history_sync"]["status"] == "not_requested"
    oversized = fixture(body=fixture_body(), head=head, changed_files=334)
    assert decide(oversized, [oversized], head, False, max_files, max_lines)["reason"] == "qualification_scope_oversized"
    # The live reader: pagination is flattened, and any failure is None.
    pages = {"out": b"[[1,2],[3]]", "rc": 0}

    def fake_run(command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[bytes]:
        assert kwargs.get("timeout"), "every read has a timeout"
        if pages["rc"] == -1:
            raise subprocess.TimeoutExpired(command, kwargs["timeout"])
        return subprocess.CompletedProcess(command, pages["rc"], pages["out"], b"")

    real_run = subprocess.run
    subprocess.run = fake_run  # type: ignore[assignment]
    try:
        assert gh_api_json("repos/o/r/issues/1/events", paginate=True) == [1, 2, 3]
        pages["out"] = b"[1,2]"
        assert gh_api_json("repos/o/r/issues/1/events", paginate=True) is None, "an unpaged body is not trusted"
        assert gh_api_json("repos/o/r") == [1, 2]
        pages["out"] = b"not json"
        assert gh_api_json("repos/o/r") is None
        pages["rc"], pages["out"] = 1, b"{}"
        assert gh_api_json("repos/o/r") is None
        pages["rc"] = -1
        assert gh_api_json("repos/o/r") is None
    finally:
        subprocess.run = real_run


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
                [GH, "api", *args], capture_output=True, check=False, timeout=BLOB_READ_TIMEOUT_SECONDS
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
            [GH, "api", f"repos/{repository}/pulls/{number}", "--jq", ".head.sha"],
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
        "--comments",
        type=Path,
        help="PR conversation (issue) comments JSON; live ownership claims annotate the decision without refusing it",
    )
    parser.add_argument(
        "--generated-attributes",
        type=Path,
        help=".gitattributes text from the PR's base commit; the only source of generated paths",
    )
    parser.add_argument("--max-generated-lines", type=int, default=DEFAULT_MAX_GENERATED_LINES)
    parser.add_argument("--base-sha", help="live base branch tip the wrapper resolved; history-sync needs it")
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
    claims = live_claims(
        load_json(args.comments) if args.comments else [], datetime.now(timezone.utc)
    )
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
    # The event name comes from the runner, not from a candidate-supplied input.
    sync = history_sync_check(
        current, repository, os.environ.get("GITHUB_EVENT_NAME", ""), args.base_sha or "", gh_api_json
    )
    if sync["status"] == "validated" and not head_unmoved(
        repository, os.environ.get("ADMISSION_PR_NUMBER", ""), current
    ):
        sync = history_sync_record("rejected", "the PR head moved while the history-sync facts were read", sync)
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
        concurrent_claims=claims,
        history_sync=sync,
    )
    print(json.dumps(result, sort_keys=True))
    return 1 if result["status"] == "refused" else 0


if __name__ == "__main__":
    sys.exit(main())
