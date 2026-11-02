#!/usr/bin/env python3
"""Source-review disposition admission for merge-ready qualification (#8157).

Human prose, quoted examples, and historical observations are not disposition
fields. A current positive attestation is either:

- a column-zero readiness marker on the first substantive review line; or
- an explicit ``disposition: SOURCE_REVIEW_READY`` / ``disposition: REVIEW_READY``
  field on its own line outside fenced examples.

Native APPROVED / CHANGES_REQUESTED / DISMISSED reviews remain a separate path.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from typing import Any

READINESS_MARKERS = ("SOURCE_REVIEW_READY", "REVIEW_READY")
FIRST_LINE_MARKER = re.compile(
    r"^ {0,3}(SOURCE_REVIEW_READY|REVIEW_READY)([^A-Za-z0-9_]|$)"
)
DISPOSITION_LINE = re.compile(r"^disposition:[ \t]*(.*)[ \t]*$", re.IGNORECASE)
FENCE_OPEN = re.compile(r"^ {0,3}(`{3,}|~{3,})")
DECISIVE_STATES = frozenset({"APPROVED", "CHANGES_REQUESTED", "DISMISSED"})


def normalize_body(body: str) -> str:
    return (body or "").replace("\r\n", "\n").replace("\r", "\n")


def _fence_char(opening: str) -> tuple[str, int]:
    return opening[0], len(opening)


def visible_lines(body: str) -> list[str]:
    """Return review-body lines that may carry disposition, masking fenced blocks."""
    lines = normalize_body(body).split("\n")
    visible: list[str] = []
    fence: tuple[str, int] | None = None
    for line in lines:
        if fence is not None:
            char, size = fence
            if re.fullmatch(rf"^ {{0,3}}{re.escape(char)}{{{size},}}[ \t]*$", line):
                fence = None
            continue
        if not line.strip():
            visible.append(line)
            continue
        match = FENCE_OPEN.match(line)
        if match:
            fence = _fence_char(match.group(1))
            continue
        if re.match(r"^( {4}|\t)", line):
            continue
        visible.append(line)
    return visible


def comment_body_ready(body: str) -> bool:
    """Whether a COMMENT review body carries a current positive source disposition."""
    dispositions: list[str] = []
    first = True
    for line in visible_lines(body):
        if not line.strip():
            continue
        if first and FIRST_LINE_MARKER.match(line):
            dispositions.append("positive")
        if DISPOSITION_LINE.match(line):
            token = DISPOSITION_LINE.match(line)
            assert token is not None
            value = token.group(1).strip().upper()
            if value in READINESS_MARKERS:
                dispositions.append("positive")
            else:
                dispositions.append("refuse")
        first = False
    return bool(dispositions) and all(item == "positive" for item in dispositions)


def _latest_by_login(reviews: list[dict[str, Any]]) -> list[dict[str, Any]]:
    by_login: dict[str, dict[str, Any]] = {}
    for review in reviews:
        login = str((review.get("user") or {}).get("login") or "")
        if not login:
            continue
        current = by_login.get(login)
        submitted = str(review.get("submitted_at") or "")
        if current is None or submitted >= str(current.get("submitted_at") or ""):
            by_login[login] = review
    return list(by_login.values())


def _latest_decisive(reviews: list[dict[str, Any]]) -> list[dict[str, Any]]:
    decisive = [r for r in reviews if str(r.get("state") or "") in DECISIVE_STATES]
    return _latest_by_login(decisive)


def reviews_allow_source_attestation(
    reviews: list[dict[str, Any]], *, head: str, author: str | None = None
) -> bool:
    """Whether review evidence admits qualification for ``head``."""
    del author  # same-account COMMENT attestation is allowed; identity is out of scope here.
    native = _latest_decisive(reviews)
    if any(str(r.get("state") or "") == "CHANGES_REQUESTED" for r in native):
        return False
    if any(
        str(r.get("commit_id") or "") == head and str(r.get("state") or "") == "APPROVED"
        for r in native
    ):
        return True
    for review in _latest_by_login(reviews):
        if (
            str(review.get("commit_id") or "") == head
            and str(review.get("state") or "") == "COMMENTED"
            and comment_body_ready(str(review.get("body") or ""))
        ):
            return True
    return False


def self_test() -> None:
    cases: list[tuple[str, bool]] = [
        ("SOURCE_REVIEW_READY", True),
        ("REVIEW_READY", True),
        ("REVIEW_INCOMPLETE", False),
        (
            "REVIEW_INCOMPLETE. This candidate is not SOURCE_REVIEW_READY; behavioral proof is missing.",
            False,
        ),
        ("REVIEW_INCOMPLETE. Example:\n```\nREVIEW_READY\n```", False),
        (
            "Previous result was REVIEW_READY; current disposition: REPAIR_REQUIRED.",
            False,
        ),
        ("No longer SOURCE_REVIEW_READY. REPAIR_REQUIRED.", False),
        ("disposition: SOURCE_REVIEW_READY", True),
        ("disposition: REVIEW_READY\ndisposition: REPAIR_REQUIRED", False),
        ("`SOURCE_REVIEW_READY`", False),
        ("> SOURCE_REVIEW_READY", False),
    ]
    for body, expected in cases:
        actual = comment_body_ready(body)
        assert actual == expected, f"body={body!r} expected={expected} actual={actual}"

    head = "a" * 40
    other = "b" * 40
    review = lambda **kw: {"user": {"login": "reviewer"}, **kw}

    assert reviews_allow_source_attestation(
        [
            review(
                state="COMMENTED",
                commit_id=head,
                submitted_at="2026-10-15T01:00:01Z",
                body="SOURCE_REVIEW_READY",
            )
        ],
        head=head,
    )
    assert not reviews_allow_source_attestation(
        [
            review(
                state="COMMENTED",
                commit_id=head,
                submitted_at="2026-10-15T01:00:01Z",
                body="REVIEW_INCOMPLETE. This candidate is not SOURCE_REVIEW_READY; behavioral proof is missing.",
            )
        ],
        head=head,
    )
    assert not reviews_allow_source_attestation(
        [
            review(
                state="COMMENTED",
                commit_id=head,
                submitted_at="2026-10-15T01:00:02Z",
                body="SOURCE_REVIEW_READY",
            ),
            review(
                state="COMMENTED",
                commit_id=head,
                submitted_at="2026-10-15T01:00:03Z",
                body="No longer SOURCE_REVIEW_READY. REPAIR_REQUIRED.",
            ),
        ],
        head=head,
    )
    assert not reviews_allow_source_attestation(
        [
            review(
                state="COMMENTED",
                commit_id=other,
                submitted_at="2026-10-15T01:00:01Z",
                body="SOURCE_REVIEW_READY",
            )
        ],
        head=head,
    )
    assert not reviews_allow_source_attestation(
        [
            review(
                state="CHANGES_REQUESTED",
                commit_id=head,
                submitted_at="2026-10-15T01:00:01Z",
                body="",
            ),
            review(
                state="COMMENTED",
                commit_id=head,
                submitted_at="2026-10-15T01:00:02Z",
                body="SOURCE_REVIEW_READY",
            ),
        ],
        head=head,
    )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--check-reviews", type=argparse.FileType("r", encoding="utf-8"))
    parser.add_argument("--head", help="Exact candidate head SHA")
    parser.add_argument("--author", help="PR author login (reserved for future custody checks)")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.check_reviews is None or not args.head:
        parser.error("--check-reviews and --head are required unless --self-test is set")
    raw = json.load(args.check_reviews)
    reviews = raw if isinstance(raw, list) else [raw]
    if not reviews_allow_source_attestation(reviews, head=args.head, author=args.author):
        print(
            "::error::source review attestation refused: post a current-head COMMENT with a "
            "column-zero SOURCE_REVIEW_READY/REVIEW_READY marker or an explicit "
            "disposition: SOURCE_REVIEW_READY field, or obtain native APPROVED on the head; "
            "narrative tokens and quoted examples do not count (#8157)",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
