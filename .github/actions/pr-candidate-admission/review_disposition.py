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
import subprocess
import sys
from typing import Any

READINESS_MARKERS = ("SOURCE_REVIEW_READY", "REVIEW_READY")
# A marker line: the marker alone, or the marker followed by a dash, comma or "for"
# and free text ("REVIEW_READY \u2014 head <sha>", "disposition: REVIEW_READY, subject to
# CI"). The free text is screened separately so it cannot retract the marker.
POSITIVE_LINE = re.compile(
    r"^ {0,3}(?:disposition:[ \t]*)?(?:SOURCE_REVIEW_READY|REVIEW_READY)"
    r"(?P<tail>(?:[ \t]*[,\u2014\u2013]|[ \t]+-[ \t]|[ \t]+for\b)[^\n]*)?[ \t]*$",
    re.IGNORECASE,
)
# Extra words that turn a marker line into a hedge or retraction.
TAIL_BLOCK = re.compile(r"\b(?:but|however|although|except|unless)\b", re.IGNORECASE)
# Any line that speaks about disposition or readiness state. After the first line,
# such a line is only allowed when it is itself an exact positive line.
STATE_MENTION = re.compile(
    r"disposition|(?:SOURCE_)?REVIEW_(?:READY|INCOMPLETE|BLOCKED)|REPAIR_REQUIRED"
    r"|NOT_READY|WITHDRAWN|SUPERSEDED|CHANGES_REQUESTED",
    re.IGNORECASE,
)
# Negating phrases about readiness on a line after the marker. Deliberately narrow:
# ordinary review prose ("nothing blocking", "not run", "fail-closed", "missing") is
# fine, only phrases that retract or deny the disposition refuse.
NEGATION = re.compile(
    r"\bnot[ \t]+(?:yet[ \t]+)?(?:(?:source[ _])?review[ _])?(?:ready|approved|mergeable)\b"
    r"|\bno[ \t]+longer[ \t]+(?:(?:source[ _])?review[ _])?(?:ready|approved|mergeable)\b"
    r"|\b(?:repairs?|changes?|fixes)[ \t]+(?:(?:is|are)[ \t]+)?(?:still[ \t]+)?(?:required|needed)\b"
    r"|\bneeds?[ \t]+(?:repair|changes|rework)\b",
    re.IGNORECASE,
)
FENCE_OPEN = re.compile(r"^ {0,3}(`{3,}|~{3,})")
HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
DECISIVE_STATES = frozenset({"APPROVED", "CHANGES_REQUESTED", "DISMISSED"})
TRUSTED_ASSOCIATIONS = frozenset({"OWNER", "MEMBER", "COLLABORATOR"})


def normalize_body(body: str) -> str:
    return (body or "").replace("\r\n", "\n").replace("\r", "\n")


def visible_lines(body: str) -> list[str] | None:
    """Return review-body lines outside fenced examples and HTML comments.

    Returns ``None`` when the body is structurally ambiguous (unclosed fence or
    unclosed HTML comment); callers must refuse.
    """
    text = HTML_COMMENT.sub("", normalize_body(body))
    if "<!--" in text:
        return None
    visible: list[str] = []
    fence: tuple[str, int] | None = None
    for line in text.split("\n"):
        if fence is not None:
            char, size = fence
            if re.fullmatch(rf"^ {{0,3}}{re.escape(char)}{{{size},}}[ \t]*$", line):
                fence = None
            continue
        match = FENCE_OPEN.match(line)
        if match:
            fence = (match.group(1)[0], len(match.group(1)))
            continue
        visible.append(line)
    if fence is not None:
        return None
    return visible


def positive_line(line: str) -> bool:
    """Whether ``line`` is a marker line whose free-text tail does not retract it."""
    match = POSITIVE_LINE.match(line)
    if match is None:
        return False
    tail = match.group("tail") or ""
    return not (
        STATE_MENTION.search(tail) or NEGATION.search(tail) or TAIL_BLOCK.search(tail)
    )


def comment_body_ready(body: str) -> bool:
    """Whether a COMMENT review body carries a current positive source disposition.

    The first substantive line must be exactly a readiness marker (optionally as
    ``disposition: MARKER``). Any later line that mentions disposition or a readiness
    state must itself be an exact positive line; everything else refuses.
    """
    lines = visible_lines(body)
    if lines is None:
        return False
    substantive = [line for line in lines if line.strip()]
    if not substantive or not positive_line(substantive[0]):
        return False
    return all(
        positive_line(line)
        or not (STATE_MENTION.search(line) or NEGATION.search(line))
        for line in substantive[1:]
    )


def comment_body_negative(body: str) -> bool:
    """Whether a COMMENT body speaks about disposition without being exactly positive.

    Ordinary chatter with no state mention is neutral. Structurally ambiguous bodies
    count as negative so they can never silently drop a refusal.
    """
    if comment_body_ready(body):
        return False
    lines = visible_lines(body)
    if lines is None:
        return True
    return any(STATE_MENTION.search(line) for line in lines)


def _order(review: dict[str, Any]) -> tuple[str, int]:
    return (str(review.get("submitted_at") or ""), int(review.get("id") or 0))


def _trusted(review: dict[str, Any]) -> bool:
    return str(review.get("author_association") or "").upper() in TRUSTED_ASSOCIATIONS


def _latest_by_login(reviews: list[dict[str, Any]]) -> list[dict[str, Any]]:
    by_login: dict[str, dict[str, Any]] = {}
    for index, review in enumerate(reviews):
        login = str((review.get("user") or {}).get("login") or "")
        if not login:
            # A deleted account yields user: null. Keep each such review as its own
            # identity so a blocking review is never silently dropped.
            review_id = review.get("id")
            login = f"<anonymous:{review_id if review_id is not None else f'index-{index}'}>"
        current = by_login.get(login)
        if current is None or _order(review) >= _order(current):
            by_login[login] = review
    return list(by_login.values())


def _latest_decisive(reviews: list[dict[str, Any]]) -> list[dict[str, Any]]:
    decisive = [r for r in reviews if str(r.get("state") or "") in DECISIVE_STATES]
    return _latest_by_login(decisive)


def reviews_allow_source_attestation(
    reviews: list[dict[str, Any]], *, head: str, author: str | None = None
) -> bool:
    """Whether review evidence admits qualification for ``head``."""
    del author  # same-account COMMENT attestation is allowed; trust comes from author_association.
    if any(
        str(r.get("state") or "") != "PENDING" and not r.get("submitted_at")
        for r in reviews
    ):
        return False  # cannot be ordered, so it cannot be proven superseded
    native = _latest_decisive(reviews)
    if any(str(r.get("state") or "") == "CHANGES_REQUESTED" for r in native):
        return False
    if any(
        str(r.get("commit_id") or "") == head
        and str(r.get("state") or "") == "APPROVED"
        and _trusted(r)
        for r in native
    ):
        return True
    commented = [
        r
        for r in _latest_by_login(reviews)
        if str(r.get("commit_id") or "") == head
        and _trusted(r)
        and str(r.get("state") or "") == "COMMENTED"
    ]
    # Any trusted reviewer's current negative disposition refuses, even when another
    # trusted reviewer's current comment is ready.
    if any(comment_body_negative(str(r.get("body") or "")) for r in commented):
        return False
    return any(comment_body_ready(str(r.get("body") or "")) for r in commented)


def parse_paginated(text: str) -> list[dict[str, Any]]:
    """Concatenate the JSON arrays ``gh api --paginate`` prints, one per page."""
    decoder = json.JSONDecoder()
    reviews: list[dict[str, Any]] = []
    index = 0
    while index < len(text):
        if text[index].isspace():
            index += 1
            continue
        page, index = decoder.raw_decode(text, index)
        if not isinstance(page, list) or not all(isinstance(item, dict) for item in page):
            raise ValueError("review page is not a list of review objects")
        reviews.extend(page)
    return reviews


def fetch_reviews(repository: str, pr_number: str) -> list[dict[str, Any]]:
    """Read every native review page; any failure raises so callers fail closed."""
    out = subprocess.run(
        [
            "gh",
            "api",
            "--paginate",
            f"repos/{repository}/pulls/{pr_number}/reviews?per_page=100",
        ],
        check=True,
        capture_output=True,
        text=True,
        timeout=120,
    ).stdout
    return parse_paginated(out)


def self_test() -> None:
    assert [r["n"] for r in parse_paginated('[{"n":1}]\n[{"n":2},{"n":3}]\n[]')] == [1, 2, 3]

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
        ("REVIEW_READY\n\nChecked the diff and the oracle.", True),
        ("REVIEW_READY was withdrawn", False),
        ("REVIEW_READY: no", False),
        ("SOURCE_REVIEW_READY, but repairs required", False),
        ("REVIEW_READY\nREPAIR_REQUIRED", False),
        ("REVIEW_READY\nREVIEW_INCOMPLETE", False),
        ("REVIEW_READY\n  disposition: REPAIR_REQUIRED", False),
        ("REVIEW_READY\n- disposition: REPAIR_REQUIRED", False),
        ("REVIEW_READY\n> disposition: REPAIR_REQUIRED", False),
        ("REVIEW_READY\n**disposition: REPAIR_REQUIRED**", False),
        ("REVIEW_READY\n```\ndisposition: REPAIR_REQUIRED", False),
        ("<!--\ndisposition: REVIEW_READY\n-->", False),
        ("<!-- unclosed\nREVIEW_READY", False),
        ("REVIEW_READY\n<!-- note -->\ndisposition: REVIEW_READY", True),
        ("", False),
        ("REVIEW_READY\nnot ready; repairs are required", False),
        ("REVIEW_READY\nThis is no longer ready", False),
        ("REVIEW_READY\nRepairs are required before merge", False),
        ("REVIEW_READY — head 7c50f64fa93c3e40533240fbe4fd0e9ac7398eab", True),
        ("REVIEW_READY \u2014 head 7c50f64fa\n\nAngles: correctness. Findings: nothing blocking; hosted CI not run; fail-closed on missing data.", True),
        ("REVIEW_READY - head 7c50f64\nNo findings.", True),
        ("REVIEW_READY \u2014 head xyz", True),
        ("REVIEW_READY for the current candidate.\n\ndisposition: REVIEW_READY, subject to final unchanged-head refresh.", True),
        ("REVIEW_READY \u2014 native qualification and integration review complete.\ndisposition: REVIEW_READY for the installed candidate.", True),
        ("REVIEW_READY \u2014 exact-head source, native-proof, current-findings and integration review complete.", True),
        ("REVIEW_READY \u2014 head b9a71b72\n**The blocker is fixed.** The case now adds `unsafe { x.unreachable_unchecked() }`.", True),
        ("REVIEW_READY \u2014 not ready", False),
        ("REVIEW_READY \u2014 REPAIR_REQUIRED", False),
        ("REVIEW_READY for the candidate, but repairs required", False),
        ("SOURCE_REVIEW_READY, but repairs required", False),
        ("REVIEW_READY\n```\nnot ready\n```\nChecked the oracle.", True),
    ]
    for body, expected in cases:
        actual = comment_body_ready(body)
        assert actual == expected, f"body={body!r} expected={expected} actual={actual}"

    head = "a" * 40
    other = "b" * 40
    review = lambda **kw: {
        "user": {"login": "reviewer"},
        "author_association": "MEMBER",
        "submitted_at": "2026-10-15T01:00:00Z",
        **kw,
    }
    assert not reviews_allow_source_attestation(
        [
            review(
                state="COMMENTED",
                commit_id=head,
                submitted_at="2026-10-15T01:00:01Z",
                body="SOURCE_REVIEW_READY",
                author_association="NONE",
            )
        ],
        head=head,
    )
    assert not reviews_allow_source_attestation(
        [review(state="APPROVED", commit_id=head, author_association="CONTRIBUTOR")],
        head=head,
    )
    assert reviews_allow_source_attestation(
        [review(state="APPROVED", commit_id=head)], head=head
    )
    # Another trusted reviewer's later negative comment refuses despite a ready comment.
    assert not reviews_allow_source_attestation(
        [
            review(state="COMMENTED", commit_id=head, id=1, body="REVIEW_READY",
                   submitted_at="2026-10-15T01:00:01Z"),
            review(state="COMMENTED", commit_id=head, id=2, body="REPAIR_REQUIRED",
                   submitted_at="2026-10-15T01:00:02Z", user={"login": "other"}),
        ],
        head=head,
    )
    # Plain chatter from another reviewer stays neutral.
    assert reviews_allow_source_attestation(
        [
            review(state="COMMENTED", commit_id=head, id=1, body="REVIEW_READY",
                   submitted_at="2026-10-15T01:00:01Z"),
            review(state="COMMENTED", commit_id=head, id=2, body="Looks fine to me",
                   submitted_at="2026-10-15T01:00:02Z", user={"login": "other"}),
        ],
        head=head,
    )
    # A decisive review without submitted_at cannot be ordered, so refuse.
    assert not reviews_allow_source_attestation(
        [review(state="APPROVED", commit_id=head, submitted_at=None)], head=head
    )
    for bad_page in ('{"message": "Bad credentials"}', '[1]', '[{"a":1}] {"b":2}'):
        try:
            parse_paginated(bad_page)
        except ValueError:
            continue
        raise AssertionError(f"accepted malformed page {bad_page!r}")
    # A CHANGES_REQUESTED from a deleted account (user: null) must still block.
    assert not reviews_allow_source_attestation(
        [
            review(state="APPROVED", commit_id=head, id=1),
            review(state="CHANGES_REQUESTED", commit_id=head, id=2, user=None),
        ],
        head=head,
    )

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
    parser.add_argument("--repository", help="owner/name; read reviews with gh instead of a file")
    parser.add_argument("--pr", help="Pull request number for --repository")
    parser.add_argument("--head", help="Exact candidate head SHA")
    parser.add_argument("--author", help="PR author login (reserved for future custody checks)")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    live = args.repository is not None and args.pr is not None
    if not args.head or (args.check_reviews is None and not live):
        parser.error(
            "--head plus --check-reviews or --repository/--pr are required unless --self-test is set"
        )
    if live:
        try:
            reviews = fetch_reviews(args.repository, args.pr)
        except (OSError, subprocess.SubprocessError, ValueError) as exc:
            print(
                f"::error::source review attestation refused: could not read reviews: {exc}",
                file=sys.stderr,
            )
            return 1
    else:
        try:
            reviews = parse_paginated(args.check_reviews.read())
        except ValueError as exc:
            print(
                f"::error::source review attestation refused: malformed review data: {exc}",
                file=sys.stderr,
            )
            return 1
    if not reviews_allow_source_attestation(reviews, head=args.head, author=args.author):
        print(
            "::error::source review attestation refused: a trusted reviewer "
            "(OWNER/MEMBER/COLLABORATOR) must post a current-head COMMENT whose first line is "
            "exactly SOURCE_REVIEW_READY or REVIEW_READY (or disposition: of either) with no "
            "contradicting disposition text, or a native APPROVED on the head; narrative "
            "tokens and quoted examples do not count (#8157)",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
