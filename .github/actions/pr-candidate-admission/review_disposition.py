#!/usr/bin/env python3
"""Source-review disposition admission for merge-ready qualification (#8157).

Human prose, quoted examples, and historical observations are not disposition
fields. A current positive attestation is either:

- a column-zero readiness marker on the first substantive review line; or
- an explicit ``disposition: SOURCE_REVIEW_READY`` / ``disposition: REVIEW_READY``
  field on its own line outside fenced examples.

Native APPROVED / CHANGES_REQUESTED / DISMISSED reviews remain a separate path.

PR-body carrier (#4984): the same attestation may instead be one column-zero line in
the pull request description, ``Source review: REVIEW_READY <40-hex head>`` (or
``SOURCE_REVIEW_READY``). The full SHA must equal the candidate head, so a push makes
the line stale by construction. It is read only for a same-repository pull request
whose author association is trusted, which is the population that could already
self-attest with a COMMENT review. It adds a carrier, not a new authority: a native
CHANGES_REQUESTED or a trusted reviewer's current negative review still refuses.
The reason is operational: creating a review is throttled per account ("was submitted
too quickly"), which left reviewed candidates unable to request qualification.

Documented decision (#12814): free prose cannot be enumerated, so the marker line
is an allowlist built from the shapes of real review-of-record tails (full bodies
are fixtures under tools/repo-policy/tests/fixtures/review_bodies). An unfamiliar
tail refuses; a ready review refused for an unusual tail is the safe failure, and
the reviewer re-posts with the plain marker. Fenced blocks cannot hide a
retraction: retraction or blocker text inside a fence refuses. Later body prose is
screened by a best-effort negation list; the closed controls are the marker-line
allowlist and a later native review superseding the comment.
"""

from __future__ import annotations

import argparse
import html
import json
import re
import subprocess
import sys
from typing import Any

READINESS_MARKERS = ("SOURCE_REVIEW_READY", "REVIEW_READY")
# A marker line: the marker alone, or the marker followed by a dash, comma or "for" and
# a tail. The tail is admitted only from a closed vocabulary (see tail_allowed): any word
# outside it refuses, so hedges, conditions and retractions cannot be enumerated away.
POSITIVE_LINE = re.compile(
    r"^ {0,3}(?:disposition:[ \t]*)?(?:SOURCE_REVIEW_READY|REVIEW_READY)"
    r"(?P<tail>(?:[ \t]*[,\u2014\u2013]|[ \t]+-[ \t]|[ \t]+for\b)[^\n]*)?[ \t]*$",
    re.IGNORECASE,
)
# Vocabulary derived from the first lines and `disposition:` lines of the 11 real
# REVIEW_READY native reviews on merged PRs #12736..#12793, plus connectors. Words that
# can hedge or retract (not, but, if, only, pending, blocked, fix, wait, ...) are absent
# on purpose; "change" is absent too and appears only inside the two literal phrases
# stripped first. An unusual but genuine tail is refused: that is the safe failure.
TAIL_VOCABULARY = frozenset(
    """for the a an and or of to then with by current published diagnostics candidate selected
    installed ordinary final fresh normal landed tree expected exact hash layout head source
    native proof findings integration qualification review complete completed squash protected
    enforcement refresh reconciliation issue subject unchanged gate conversation pr check
    thread snapshot after""".split()
)
TAIL_LITERALS = (
    re.compile(r"\bno[ \t]+admin[ \t]+bypass,?[ \t]+settings[ \t]+change,?[ \t]+release[ \t]+or[ \t]+auto-merge\b", re.IGNORECASE),
    re.compile(r"\bhash-layout[ \t]+change\b", re.IGNORECASE),
)
# Any line that speaks about disposition or readiness state. After the first line,
# such a line is only allowed when it is itself an exact positive line.
STATE_TOKEN = re.compile(
    r"(?:SOURCE_)?REVIEW_(?:READY|INCOMPLETE|BLOCKED)|REPAIR_REQUIRED"
    r"|NOT_READY|WITHDRAWN|SUPERSEDED|CHANGES_REQUESTED",
    re.IGNORECASE,
)
STATE_MENTION = re.compile(r"disposition|" + STATE_TOKEN.pattern, re.IGNORECASE)
# A line that states a disposition: after optional list (bulleted, numbered or task),
# quote, heading, emphasis or code markers it opens with a ``disposition:`` field or a state token. Only such a line
# after the first one changes what the review says. A state word inside prose, such
# as a backticked quotation of a marker or "the earlier review is superseded", is
# narration and is not read: free text scanning ejected fully green queue entries
# after a whole run (#11566).
# A line that is nothing but a state token, however it is marked up (including
# backticks), still states it. The leading markup includes ordered-list numbers
# (``1.``, ``1)``) and task-list boxes (``[ ]``, ``[x]``).
STATE_LINE_PREFIX = r"^(?:[ \t>*\-+_#~`]|\d{1,9}[.)]|\[[ \txX]?\])*"
# A ``disposition`` field label anywhere on the line (``Final disposition:``, a
# ``| Disposition | ... |`` table row) and a table cell that holds only a state token
# also state one.
STATE_LINE = re.compile(
    STATE_LINE_PREFIX + r"(?:disposition[ \t*_`]*:|(?:" + STATE_TOKEN.pattern + r")\b)"
    r"|" + STATE_LINE_PREFIX + r"(?:" + STATE_TOKEN.pattern + r")[ \t*_`.:!]*$"
    r"|^.*?\bdisposition[ \t*_`]*[:|]"
    r"|^[ \t]*\|(?:.*\|)?[ \t*_`]*(?:" + STATE_TOKEN.pattern + r")[ \t*_`.]*\|",
    re.IGNORECASE,
)
# Retraction and blocker phrases for body prose after the marker (and for fenced text,
# which must not be able to hide one). Best effort: free prose cannot be enumerated, so
# the closed control is the marker line plus later native reviews superseding this one.
NEGATION = re.compile(
    r"\bnot[ \t]+(?:yet[ \t]+)?(?:(?:source[ _])?review[ _])?(?:ready|approved|mergeable)\b"
    r"|\bno[ \t]+longer[ \t]+(?:(?:source[ _])?review[ _])?(?:ready|approved|mergeable)\b"
    r"|\b(?:repairs?|changes?|fix(?:es)?)[ \t]+(?:(?:is|are)[ \t]+)?(?:still[ \t]+)?(?:required|needed)\b"
    r"|\bneeds?[ \t]+(?:work|a[ \t]+fix|fix|fixes|repair|changes|rework)\b"
    r"|\b(?:do(?:es)?[ \t]+not(?:[ \t]+yet)?|dont|don't|cannot|can't)[ \t]+(?:merge|approve|ship|land)\b"
    r"|\btake[ \t]+(?:it|that)[ \t]+back\b|\bretract(?:ing|ed|s)?\b|\bwithdr(?:aw|awn|awing)\b"
    r"|\b(?:is|are|remains?|still)[ \t]+blocked\b|\bblocked[ \t]+(?:on|by)\b"
    r"|\bblockers?[ \t]+(?:found|remains?|remaining|open|still|exists?)\b"
    r"|\btests?[ \t]+(?:fail|fails|failed|failing)\b|\bci[ \t]+(?:is[ \t]+)?red\b"
    r"|\b(?:build|ci)[ \t]+(?:is[ \t]+)?(?:red|broken|failing|fails?)(?![-\w])"
    r"|\b(?:a[ \t]+)?fix(?:es)?[ \t]+(?:is|are)?[ \t]*(?:needed|required)\b"
    r"|\bon[ \t]+hold\b|\bhold[ \t]+(?:the[ \t]+)?merge\b|\bchanges[ \t]+requested\b"
    r"|\b(?:never|won't|will[ \t]+not|should[ \t]+not|must[ \t]+not|shouldn't|mustn't)[ \t]+(?:merge|approve|ship|land)\b"
    r"|\bstatus:[ \t]*(?:blocked|not[ \t]+ready|incomplete|rejected)\b|\breview[ \t]+incomplete\b"
    r"|^\W*(?:wait|hold|stop|stale|blocked|revoked|superseded|reject(?:ed)?|nack)\b",
    re.IGNORECASE | re.MULTILINE,
)
# #14355: a negated change/repair phrase is not a blocker. "no code change needed"
# and "no changes needed" describe resolved work, not a required repair. The
# filler set is closed so an unrelated "no findings and code change needed" is
# not swallowed. The phrase is stripped only when it ends a sentence and no
# preceding cue refutes it; the rest of the line is then scanned separately, so
# "no code change needed. The build is red." still refuses.
NEGATED_CHANGE = re.compile(
    r"\b(?:no|without)\b[ \t]+"
    r"(?:(?:a|an|any|further|code|source|production)[ \t]+){0,2}"
    r"(?:repairs?|changes?|fix(?:es)?)\b[ \t]+"
    r"(?:(?:is|are)[ \t]+)?(?:still[ \t]+)?(?:required|needed)\b",
    re.IGNORECASE,
)
# Sentence position: the phrase is followed by '.', '!' or ';' (or the line ends).
NEGATED_CHANGE_TAIL = re.compile(r"^[ \t]*(?:[.!;]|$)")
# A recognized cue that refutes a negated change phrase, so it is not resolved
# work. The cue may sit anywhere earlier in the same sentence; a cue in a prior
# sentence does not reach across the '.' or ';'. The same cue set is checked
# after the phrase, but only when the remainder actually refutes it (see
# REFUTATION_AFTER), so "Nothing is wrong." is not a refutation. The set is a
# closed enumeration; a phrasing outside it is not detected.
REFUTATION_CUE = re.compile(
    r"\b(?:not[ \t]+true|untrue|false|incorrect|not[ \t]+the[ \t]+case"
    r"|isn't|is[ \t]+not|it's[ \t]+not|it[ \t]+is[ \t]+not"
    r"|not[ \t]+accurate|not[ \t]+correct|not[ \t]+right"
    r"|no[ \t]+truth[ \t]+to|a[ \t]+lie|wrong[ \t]+to[ \t]+say|wrong[ \t]+to[ \t]+claim"
    r"|would[ \t]+be[ \t]+wrong[ \t]+to|disagree|disagrees|disagreed"
    r"|(?:i|we)[ \t]+(?:don't|do[ \t]+not)[ \t]+believe|(?:i|we)[ \t]+doubt"
    r"|nobody[ \t]+could[ \t]+say|cannot[ \t]+say|can't[ \t]+say|deny|denies|denied)\b",
    re.IGNORECASE,
)
REFUTES_NEGATION = re.compile(
    REFUTATION_CUE.pattern + r"[^.;\n]*$",
    re.IGNORECASE,
)
# A refutation after the phrase counts only when the remainder starts with a
# demonstrative/pronoun subject ("That is false", "It is a lie", "I deny it"),
# a demonstrative plus a short noun ("That claim is false", "The above is
# false"), or the cue itself ("Incorrect.", "Wrong.", "Not so."). Ordinary
# prose that merely contains a cue word ("Nothing is wrong", "The wrong file")
# does not.
REFUTATION_AFTER = re.compile(
    r"^[ \t]*[.!;]?[ \t]*"
    r"(?:"
    r"(?:that|this|it|i)\b[ \t]*(?:is|'s|\u2019s|am|was|are)?[ \t]*"
    r"(?:not[ \t]+(?:true|correct|so|the[ \t]+case|accurate|right)|false|incorrect|wrong|a[ \t]+lie|untrue|denied|deny)"
    r"|(?:that|this)[ \t]+(?:claim|statement|assertion|report|summary|finding|comment|review)\b[ \t]*(?:is|'s|\u2019s|was|are)?[ \t]*"
    r"(?:not[ \t]+(?:true|correct|so|the[ \t]+case|accurate|right)|false|incorrect|wrong|a[ \t]+lie|untrue)"
    r"|the[ \t]+(?:above|statement|claim|assertion|report|summary|finding|comment|review)\b[ \t]*(?:is|'s|\u2019s|was|are)?[ \t]*"
    r"(?:not[ \t]+(?:true|correct|so|the[ \t]+case|accurate|right)|false|incorrect|wrong|a[ \t]+lie|untrue)"
    r"|(?:that|this|it)\b[ \t]*(?:is|are|was)n['\u2019]t[ \t]+(?:true|correct|so|the[ \t]+case|accurate|right)"
    r"|(?:incorrect|wrong|not[ \t]+(?:so|true|correct|the[ \t]+case|accurate|right)|false|untrue|a[ \t]+lie)\b"
    r")",
    re.IGNORECASE,
)
FENCE_OPEN = re.compile(r"^ {0,3}(`{3,}|~{3,})")
HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
DECISIVE_STATES = frozenset({"APPROVED", "CHANGES_REQUESTED", "DISMISSED"})
TRUSTED_ASSOCIATIONS = frozenset({"OWNER", "MEMBER", "COLLABORATOR"})


# PR-body carrier. Any column-zero line that starts with this field name and has a
# value must be exactly the positive form; an empty value is the unfilled template.
BODY_FIELD = re.compile(r"^source review:[ \t]*(?P<value>.*?)[ \t]*$", re.IGNORECASE)
BODY_FIELD_VALUE = re.compile(r"^(?:SOURCE_REVIEW_READY|REVIEW_READY) (?P<sha>[0-9a-f]{40})$")
# A second, competing state field in the description. The review-body form is a
# ``disposition:`` line, so a description that carries one (whatever its value, and
# however it is quoted, listed, emphasised or fenced) is ambiguous and attests nothing.
# The field shapes match the review side's state lines (#11566): a ``disposition``
# label after any list (bulleted, numbered, task), quote or emphasis markup or later in
# the line (``Final disposition:``, ``| Disposition | ... |``), and a table row with a
# cell that holds only a state token.
BODY_COMPETING_FIELD = re.compile(
    STATE_LINE_PREFIX + r"disposition[ \t*_`]*:"
    r"|^.*?\bdisposition[ \t*_`]*[:|]"
    r"|^[ \t]*\|(?:.*\|)?[ \t*_`]*(?:" + STATE_TOKEN.pattern + r")[ \t*_`.]*\|",
    re.IGNORECASE,
)


def normalize_body(body: str) -> str:
    text = (body or "").replace("\r\n", "\n").replace("\r", "\n")
    text = html.unescape(text).replace("\u2019", "'").replace("\u2018", "'")
    text = re.sub(r"[^\S\n]", " ", text)  # nbsp and other unicode spaces
    return text.replace("**", "").replace("__", "")


def split_body(body: str) -> tuple[list[str], list[str]] | None:
    """Return (visible lines, fenced lines); HTML comments are dropped.

    Returns ``None`` when the body is structurally ambiguous (unclosed fence or
    unclosed HTML comment); callers must refuse.
    """
    text = HTML_COMMENT.sub("", normalize_body(body))
    if "<!--" in text:
        return None
    visible: list[str] = []
    fenced: list[str] = []
    fence: tuple[str, int] | None = None
    for line in text.split("\n"):
        if fence is not None:
            char, size = fence
            if re.fullmatch(rf"^ {{0,3}}{re.escape(char)}{{{size},}}[ \t]*$", line):
                fence = None
            else:
                fenced.append(line)
            continue
        match = FENCE_OPEN.match(line)
        if match:
            fence = (match.group(1)[0], len(match.group(1)))
            continue
        visible.append(line)
    if fence is not None:
        return None
    return visible, fenced


def tail_allowed(tail: str) -> bool:
    """Whether a marker-line tail uses only the closed vocabulary."""
    text = tail
    for literal in TAIL_LITERALS:
        text = literal.sub(" ", text)
    # ASCII charset gate: any other character (CJK, emoji, "?", "!") refuses. The
    # leading em/en dash is the only non-ASCII character a real tail uses.
    if not re.fullmatch(r"[A-Za-z0-9 \t,.;:\-`'()/_\u2014\u2013]*", text):
        return False
    previous = ""
    for word in re.findall(r"[A-Za-z0-9]+", text):
        lowered = word.lower()
        if re.fullmatch(r"[0-9a-f]{7,40}", lowered):
            if previous != "head":  # a commit sha, only as "head <sha>"
                return False
        elif lowered not in TAIL_VOCABULARY:
            return False
        previous = lowered
    return True


def positive_line(line: str) -> bool:
    """Whether ``line`` is a marker line whose tail stays inside the vocabulary."""
    match = POSITIVE_LINE.match(normalize_body(line))
    if match is None:
        return False
    tail = match.group("tail") or ""
    return not STATE_TOKEN.search(tail) and tail_allowed(tail)


def strip_negated_change(line: str) -> str:
    """Remove a negated change/repair phrase only when it is resolved work.

    #14355: the phrase is stripped only when it ends a sentence, no preceding
    cue refutes it, and the remainder of the line carries no refutation cue.
    A refuted or followed-by-blocker phrase stays in place, so the blocker scan
    still refuses it.
    """

    def replace(match: re.Match[str]) -> str:
        before = line[: match.start()]
        after = line[match.end() :]
        # #14355: a cue only refutes the phrase in its own clause. A comma
        # resets the clause, so "The Devin thread was a false positive, no code
        # change needed." is resolved work, not a refutation.
        before_clause = before.rsplit(",", 1)[-1]
        if (
            NEGATED_CHANGE_TAIL.match(after)
            and not REFUTES_NEGATION.search(before_clause)
            and not REFUTATION_AFTER.search(after)
        ):
            return " "
        return match.group(0)

    return NEGATED_CHANGE.sub(replace, line)


def negation_hit(line: str) -> bool:
    """Whether ``line`` carries a blocker phrase outside a negated change phrase.

    #14355: "no code change needed" is a resolved-work statement, not a blocker.
    Only an unrefuted statement-position phrase is removed, so a genuine blocker
    elsewhere on the same line still refuses.
    """
    return bool(NEGATION.search(strip_negated_change(line)))


def comment_body_ready(body: str) -> bool:
    """Whether a COMMENT review body carries a current positive source disposition.

    The first substantive line must be a marker line with an allowed tail. Any later
    line that states a disposition (see STATE_LINE) must itself be a positive line;
    retraction or blocker text refuses on later lines and inside fenced blocks.
    """
    parts = split_body(body)
    if parts is None:
        return False
    visible, fenced = parts
    substantive = [line for line in visible if line.strip()]
    if not substantive or not positive_line(substantive[0]):
        return False
    if any(STATE_TOKEN.search(line) or negation_hit(line) for line in fenced):
        return False
    return all(
        positive_line(line)
        or not (STATE_LINE.match(line) or negation_hit(line))
        for line in substantive[1:]
    )


def comment_body_negative(body: str) -> bool:
    """Whether a COMMENT body states a disposition without being exactly positive.

    Ordinary chatter with no state mention is neutral. Structurally ambiguous bodies
    count as negative so they can never silently drop a refusal.
    """
    if comment_body_ready(body):
        return False
    parts = split_body(body)
    if parts is None:
        return True
    visible, fenced = parts
    # A prose line that names a state and retracts or blocks it still speaks: another
    # reviewer's "the earlier REVIEW_READY is withdrawn" must not read as chatter.
    return any(
        STATE_LINE.match(line) or (STATE_MENTION.search(line) and negation_hit(line))
        for line in visible
    ) or any(STATE_TOKEN.search(line) or negation_hit(line) for line in fenced)


def body_attests_head(body: str, head: str) -> bool:
    """Whether a pull request description carries the exact-head attestation line.

    Exactly one filled ``Source review:`` line may exist outside fences and HTML
    comments, its value must be a marker, one space and the full lowercase head SHA,
    and no fenced ``Source review:`` line may exist to be mistaken for it.
    """
    if not re.fullmatch(r"[0-9a-f]{40}", head or ""):
        return False
    parts = split_body(body)
    if parts is None:
        return False
    visible, fenced = parts
    if any(BODY_FIELD.match(line.strip()) for line in fenced):
        return False
    if any(BODY_COMPETING_FIELD.match(line) for line in (*visible, *fenced)):
        return False
    values = []
    for line in visible:
        match = BODY_FIELD.match(line)
        if match is not None and match.group("value"):
            values.append(match.group("value"))
    if len(values) != 1:
        return False
    match = BODY_FIELD_VALUE.match(values[0])
    return match is not None and match.group("sha") == head


def pull_allows_source_attestation(pull: dict[str, Any] | None, *, head: str) -> bool:
    """Whether the pull request itself carries a trusted exact-head attestation."""
    if not isinstance(pull, dict):
        return False
    if str(pull.get("author_association") or "").upper() not in TRUSTED_ASSOCIATIONS:
        return False
    head_ref = pull.get("head") if isinstance(pull.get("head"), dict) else {}
    base_ref = pull.get("base") if isinstance(pull.get("base"), dict) else {}
    head_repo = str((head_ref.get("repo") or {}).get("full_name") or "")
    base_repo = str((base_ref.get("repo") or {}).get("full_name") or "")
    if not head_repo or head_repo != base_repo:
        return False
    # The live head must be the head being admitted: a description written for an
    # earlier head never attests a later one.
    if str(head_ref.get("sha") or "") != head:
        return False
    return body_attests_head(str(pull.get("body") or ""), head)


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
    reviews: list[dict[str, Any]],
    *,
    head: str,
    author: str | None = None,
    pull: dict[str, Any] | None = None,
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
    approvals = [
        r for r in native
        if str(r.get("commit_id") or "") == head
        and str(r.get("state") or "") == "APPROVED"
        and _trusted(r)
    ]
    # Every positive carrier is subject to the same unresolved-negative veto.
    # Neutral replies cannot erase a statement. A later explicit ready COMMENT
    # or still-current native approval resolves only that reviewer's objection.
    # Use the selected native approvals, not historical ones: a dismissed or
    # superseded approval cannot silently clear a negative COMMENT.
    stated = [
        r for r in reviews
        if str(r.get("commit_id") or "") == head
        and _trusted(r)
        and str(r.get("state") or "") == "COMMENTED"
        and (
            comment_body_ready(str(r.get("body") or ""))
            or comment_body_negative(str(r.get("body") or ""))
        )
    ]
    if any(
        str(r.get("state") or "") == "COMMENTED"
        and comment_body_negative(str(r.get("body") or ""))
        for r in _latest_by_login(stated + approvals)
    ):
        return False
    if approvals:
        return True
    commented = [
        r
        for r in _latest_by_login(reviews)
        if str(r.get("commit_id") or "") == head
        and _trusted(r)
        and str(r.get("state") or "") == "COMMENTED"
    ]
    if any(comment_body_ready(str(r.get("body") or "")) for r in commented):
        return True
    return pull_allows_source_attestation(pull, head=head)


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
        encoding="utf-8",
        timeout=120,
    ).stdout
    return parse_paginated(out)


def fetch_pull(repository: str, pr_number: str) -> dict[str, Any]:
    """Read the pull request object; any failure raises."""
    out = subprocess.run(
        ["gh", "api", f"repos/{repository}/pulls/{pr_number}"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=120,
    ).stdout
    pull = json.loads(out)
    if not isinstance(pull, dict):
        raise ValueError("pull request response is not an object")
    return pull


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
        # #14355: a negated change/repair phrase is resolved work, not a blocker.
        # "Resolved with no code change needed." admits only because the strip ran.
        ("REVIEW_READY\nResolved with no code change needed.", True),
        ("REVIEW_READY\nThree Devin threads resolved, no code changes needed.", True),
        ("REVIEW_READY\nResolved without a code change.", True),
        # #14355: a comma resets the clause, so a benign cue earlier in the
        # sentence does not suppress the strip.
        (
            "REVIEW_READY\nThe Devin thread was a false positive, no code change needed.",
            True,
        ),
        (
            "REVIEW_READY\nThe finding is not applicable, no changes needed.",
            True,
        ),
        ("REVIEW_READY\nGemini isn't available, no changes needed.", True),
        # #14355: a hyphenated build/ci idiom is not a blocker.
        (
            "REVIEW_READY\nHosted CI fail-closed on this change; no code change needed.",
            True,
        ),
        (
            "REVIEW_READY\nBuild fail-fast behaviour verified; no changes needed.",
            True,
        ),
        ("REVIEW_READY\nCode change needed.", False),
        ("REVIEW_READY\nA change is needed.", False),
        ("REVIEW_READY\nChanges required.", False),
        ("REVIEW_READY\nRepairs required.", False),
        ("REVIEW_READY\nNo code change needed, but repairs required.", False),
        # #14355: the real #14117 line 6 is mid-line, followed by other sentences.
        (
            "REVIEW_READY\n- Review input: three Devin threads, each answered in the PR "
            "conversation and resolved with no code change needed. Gemini had no findings. "
            "CodeRabbit was rate limited and ub-review was cancelled, so neither supplied a "
            "review; the owning session's inline source review stands in for them.",
            True,
        ),
        ("REVIEW_READY\nNo code change needed. All threads resolved.", True),
        ("REVIEW_READY\nNo code change needed. Gemini had no findings.", True),
        ("REVIEW_READY\nNo changes are needed. The build is red.", False),
        ("REVIEW_READY\nNo code change needed. Tests fail.", False),
        ("REVIEW_READY\nNo code change needed. Repairs required.", False),
        ("REVIEW_READY\nNo changes are needed. The build fails.", False),
        ("REVIEW_READY\nNo changes are needed. The build is broken.", False),
        ("REVIEW_READY\nNo changes are needed. The build is failing.", False),
        ("REVIEW_READY\nNo code change needed. A fix is needed.", False),
        # #14355: a refutation after the phrase, in the same sentence, refuses.
        ("REVIEW_READY\nNo changes are needed. That is false.", False),
        ("REVIEW_READY\nNo changes are needed; that is false.", False),
        ("REVIEW_READY\nNo changes are needed. That is not true.", False),
        ("REVIEW_READY\nNo changes are needed. I deny it.", False),
        ("REVIEW_READY\nNo changes are needed. That is a lie.", False),
        ("REVIEW_READY\nNo changes are needed. Incorrect.", False),
        ("REVIEW_READY\nNo changes are needed. Wrong.", False),
        ("REVIEW_READY\nNo code change needed. Not so.", False),
        ("REVIEW_READY\nNo changes are needed. Not true.", False),
        ("REVIEW_READY\nNo changes are needed. That isn't true.", False),
        ("REVIEW_READY\nNo changes are needed. This isn't correct.", False),
        ("REVIEW_READY\nNo changes are needed. That claim is false.", False),
        ("REVIEW_READY\nNo changes are needed. The above is false.", False),
        ("REVIEW_READY\nNo changes are needed. That assertion is false.", False),
        ("REVIEW_READY\nNo changes are needed. This claim is false.", False),
        ("REVIEW_READY\nNo changes are needed. The statement is false.", False),
        ("REVIEW_READY\nNo changes are needed. The claim is false.", False),
        ("REVIEW_READY\nNo changes are needed. The assertion is false.", False),
        ("REVIEW_READY\nNo changes are needed. The finding is false.", False),
        ("REVIEW_READY\nNo changes are needed. The report is false.", False),
        ("REVIEW_READY\nNo changes are needed. This statement is untrue.", False),
        ("REVIEW_READY\nNo changes are needed. All green.", True),
        # #14355: ordinary prose with a cue word is not a refutation.
        ("REVIEW_READY\nNo changes are needed. Nothing is wrong.", True),
        ("REVIEW_READY\nNo changes are needed. No incorrect claims.", True),
        (
            "REVIEW_READY\nNo code change needed. The wrong file was not touched.",
            True,
        ),
        (
            "REVIEW_READY\nNo changes are needed. All good, false alarm earlier.",
            True,
        ),
        # #14355: an unlisted refutation ahead of the phrase is not resolved work.
        ("REVIEW_READY\nIt's not accurate that no changes are needed.", False),
        ("REVIEW_READY\nIt's not correct that no changes are needed.", False),
        (
            "REVIEW_READY\nIt would be wrong to claim no changes are needed.",
            False,
        ),
        ("REVIEW_READY\nI disagree that no changes are needed.", False),
        ("REVIEW_READY\nI don't believe no changes are needed.", False),
        ("REVIEW_READY\nI doubt no changes are needed.", False),
        # Graphite thread examples.
        (
            "REVIEW_READY\nNo code change needed. The Devin thread was a false positive.",
            True,
        ),
        ("REVIEW_READY\nNo code change needed. Gemini isn't available.", True),
        # #14355: a refuted phrase is not resolved work.
        ("REVIEW_READY\nIt is not true that no changes are needed.", False),
        ("REVIEW_READY\nIt is false that no change is needed.", False),
        ("REVIEW_READY\nNo changes are needed is false.", False),
        ("REVIEW_READY\nNo code change needed is not correct.", False),
        ("REVIEW_READY\nNo changes are needed -- the build is red.", False),
        ("REVIEW_READY\nThere is no truth to the claim that no changes are needed.", False),
        ("REVIEW_READY\nIt is a lie that no changes are needed.", False),
        ("REVIEW_READY\nIt is wrong to say that no changes are needed.", False),
        ("REVIEW_READY\nNobody could say that no changes are needed.", False),
        ("REVIEW_READY\nno findings, code change needed", False),
        ("REVIEW_READY — head 7c50f64fa93c3e40533240fbe4fd0e9ac7398eab", True),
        ("REVIEW_READY \u2014 head 7c50f64fa\n\nAngles: correctness. Findings: nothing blocking; hosted CI not run; fail-closed on missing data.", True),
        ("REVIEW_READY - head 7c50f64\nNo findings.", True),
        ("REVIEW_READY \u2014 head xyz", False),
        ("REVIEW_READY for the current candidate.\n\ndisposition: REVIEW_READY, subject to final unchanged-head refresh.", True),
        ("REVIEW_READY \u2014 native qualification and integration review complete.\ndisposition: REVIEW_READY for the installed candidate.", True),
        ("REVIEW_READY \u2014 exact-head source, native-proof, current-findings and integration review complete.", True),
        ("REVIEW_READY \u2014 head b9a71b72\n**The blocker is fixed.** The case now adds `unsafe { x.unreachable_unchecked() }`.", True),
        ("REVIEW_READY \u2014 not ready", False),
        ("REVIEW_READY \u2014 REPAIR_REQUIRED", False),
        ("REVIEW_READY \u2014 pending fix", False),
        ("REVIEW_READY, blocked on CI", False),
        ("REVIEW_READY \u2014 blocking issue found", False),
        ("REVIEW_READY \u2014 do not merge", False),
        ("REVIEW_READY \u2014 changes requested", False),
        ("REVIEW_READY \u2014 failing tests remain", False),
        ("REVIEW_READY \u2014 stale", False),
        ("REVIEW_READY, if CI passes", False),
        ("REVIEW_READY \u2014 only after fix", False),
        ("REVIEW_READY \u2014 awaiting CI", False),
        ("REVIEW_READY\nDo not merge", False),
        ("REVIEW_READY\nI take it back", False),
        ("REVIEW_READY\nThe change is blocked on CI", False),
        ("REVIEW_READY \u2014 no findings", False),
        ("REVIEW_READY for the candidate, but repairs required", False),
        ("SOURCE_REVIEW_READY, but repairs required", False),
        ("REVIEW_READY\n```\nnot ready\n```\nChecked the oracle.", False),
        ("REVIEW_READY\n~~~\nRetracting this review\n~~~", False),
        ("REVIEW_READY \u2014 needs a fix", False),
        ("REVIEW_READY \u2014 cannot merge", False),
        ("REVIEW_READY \u2014 CI red", False),
        ("REVIEW_READY \u2014 wait for CI", False),
        ("REVIEW_READY \u2014 unverified", False),
        ("REVIEW_READY \u2014 WIP", False),
        ("REVIEW_READY \u2014 contingent on CI", False),
        ("REVIEW_READY \u2014 subject to change", False),
        ("REVIEW_READY\nDont merge", False),
        ("REVIEW_READY\nDon\u2019t merge", False),
        ("REVIEW_READY\nWait", False),
        ("REVIEW_READY\nRetracting", False),
        ("REVIEW_READY\nBlocker found", False),
        ("REVIEW_READY\nTests fail", False),
        ("REVIEW_READY\nNeeds work", False),
        ("REVIEW_READY\nBlocked.", False),
        ("REVIEW_READY\nStale", False),
        ("REVIEW_READY\nI do not approve", False),
        ("REVIEW_READY\n`REPAIR_REQUIRED`", False),
        ("REVIEW_READY\n## SUPERSEDED", False),
        ("REVIEW_READY\nDisposition: repair the oracle first", False),
        ("REVIEW_READY\n`REPAIR_REQUIRED`: fix the oracle", False),
        ("REVIEW_READY\n- `Disposition`: repair the oracle first", False),
        ("REVIEW_READY\n1. REPAIR_REQUIRED: repair oracle", False),
        ("REVIEW_READY\n1) REPAIR_REQUIRED: repair oracle", False),
        ("REVIEW_READY\n- [ ] REPAIR_REQUIRED: repair oracle", False),
        ("REVIEW_READY\n- [x] Disposition: REPAIR_REQUIRED", False),
        ("REVIEW_READY\n```\n1. REPAIR_REQUIRED: repair oracle\n```", False),
        ("REVIEW_READY\n1. The earlier `SOURCE_REVIEW_READY` was for the previous head.", True),
        # Fences cannot hide a state token, whatever shape the fenced text has.
        ('REVIEW_READY\n```json\n{"disposition":"REPAIR_REQUIRED"}\n```', False),
        ('REVIEW_READY\n```toml\nstate = "REPAIR_REQUIRED"\n```', False),
        # Declared field and table shapes state a disposition.
        ("REVIEW_READY\nFinal disposition: REPAIR_REQUIRED", False),
        ("REVIEW_READY\nCurrent disposition: REVIEW_INCOMPLETE", False),
        ("REVIEW_READY\n| Disposition | REPAIR_REQUIRED |", False),
        ("REVIEW_READY\n| PR | State |\n| --- | --- |\n| 14421 | REPAIR_REQUIRED |", False),
        # Any cell position, including the first and a sole cell.
        ("REVIEW_READY\n| State | Detail |\n| --- | --- |\n| REPAIR_REQUIRED | fix oracle |", False),
        ("REVIEW_READY\n| a | REPAIR_REQUIRED | b |", False),
        ("REVIEW_READY\n| REPAIR_REQUIRED |", False),
        ("REVIEW_READY\n| **`REPAIR_REQUIRED`** | fix oracle |", False),
        ("REVIEW_READY\n| Check | Result |\n| --- | --- |\n| fmt | pass |", True),
        # Narration that only mentions a state word is not a state line (#11566).
        ("SOURCE_REVIEW_READY - head 314f6893124dc719aabe2a4da3df6d5bf5f33ef5\n\n- One-line delta.\n\nThe earlier `SOURCE_REVIEW_READY` at c0d7341941 is superseded by this exact-head attestation.", True),
        ("REVIEW_READY\nThe previous run quoted `SOURCE_REVIEW_READY` in a later line and was refused.", True),
        ("REVIEW_READY\nThe admission script reads the review disposition from the first line.", True),
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
    # #14578: no positive carrier bypasses another reviewer's unresolved
    # negative, even if the objecting reviewer later posts a neutral reply.
    negative = review(state="COMMENTED", commit_id=head, id=2,
                      submitted_at="2026-10-15T01:00:02Z", body="REPAIR_REQUIRED")
    neutral = review(state="COMMENTED", commit_id=head, id=3,
                     submitted_at="2026-10-15T01:00:03Z", body="")
    for state, body in [("APPROVED", ""), ("COMMENTED", "SOURCE_REVIEW_READY")]:
        positive = review(state=state, commit_id=head, id=1, body=body,
                          user={"login": "other"})
        assert not reviews_allow_source_attestation(
            [positive, negative, neutral], head=head
        )
    assert reviews_allow_source_attestation(
        [negative, neutral, review(state="APPROVED", commit_id=head, id=4,
                                   submitted_at="2026-10-15T01:00:04Z")],
        head=head,
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
    # Another reviewer's prose that names a state and retracts it still refuses, while
    # prose that only quotes a state word is narration (#11566).
    for body, allowed in (
        ("Tests fail here, so the earlier REVIEW_READY is withdrawn.", False),
        ("`REPAIR_REQUIRED`: fix the oracle", False),
        ("1. REPAIR_REQUIRED: repair oracle", False),
        ("1) REPAIR_REQUIRED: repair oracle", False),
        ("- [ ] REPAIR_REQUIRED: repair oracle", False),
        ("- [ ] Disposition: REPAIR_REQUIRED", False),
        ('```json\n{"disposition":"REPAIR_REQUIRED"}\n```', False),
        ("Final disposition: REPAIR_REQUIRED", False),
        ("| Disposition | REPAIR_REQUIRED |", False),
        ("| REPAIR_REQUIRED | fix oracle |", False),
        ("| REPAIR_REQUIRED |", False),
        ("The earlier `SOURCE_REVIEW_READY` was for the previous head.", True),
    ):
        assert reviews_allow_source_attestation(
            [
                review(state="COMMENTED", commit_id=head, id=1, body="REVIEW_READY",
                       submitted_at="2026-10-15T01:00:01Z"),
                review(state="COMMENTED", commit_id=head, id=2, body=body,
                       submitted_at="2026-10-15T01:00:02Z", user={"login": "other"}),
            ],
            head=head,
        ) is allowed, body
    # A decisive review without submitted_at cannot be ordered, so refuse.
    assert not reviews_allow_source_attestation(
        [review(state="APPROVED", commit_id=head, submitted_at=None)], head=head
    )
    line = f"Source review: REVIEW_READY {head}"
    body_cases: list[tuple[str, bool]] = [
        (line, True),
        (f"Source review: SOURCE_REVIEW_READY {head}", True),
        (f"## Summary\n\n- change\n\n{line}\n\n## Proof\n", True),
        (f"source review:   REVIEW_READY {head}  ", True),
        ("Source review:", False),  # unfilled template
        (f"Source review:\n{line}", True),  # the empty template line is not a value
        (f"Source review: REVIEW_READY {other}", False),  # another head
        (f"Source review: REVIEW_READY {head[:12]}", False),  # abbreviated
        (f"Source review: REVIEW_READY {head.upper()}", False),
        ("Source review: REVIEW_READY", False),
        (f"Source review: REVIEW_INCOMPLETE {head}", False),
        (f"Source review: REVIEW_READY {head} pending CI", False),
        (f"Source review: not REVIEW_READY {head}", False),
        (f"{line}\nSource review: REVIEW_INCOMPLETE", False),  # contradicting second line
        (f"{line}\n{line}", False),  # exactly one filled line
        (f" {line}", False),  # not column zero
        (f"> {line}", False),
        (f"- {line}", False),
        (f"`{line}`", False),
        (f"```\n{line}\n```", False),
        (f"{line}\n```\nSource review: REVIEW_INCOMPLETE\n```", False),
        (f"<!-- {line} -->", False),
        (f"<!-- unclosed\n{line}", False),
        (f"Prior source review: REVIEW_READY {head}", False),
        (f"{line}\ndisposition: REPAIR_REQUIRED", False),  # competing state field
        (f"{line}\n- **Disposition:** REVIEW_INCOMPLETE", False),
        (f"{line}\n> disposition: REVIEW_READY", False),  # ambiguous even when positive
        (f"{line}\n```\ndisposition: REPAIR_REQUIRED\n```", False),
        (f"{line}\n1. disposition: REPAIR_REQUIRED", False),
        (f"{line}\n1) disposition: REPAIR_REQUIRED", False),
        (f"{line}\n- [ ] disposition: REPAIR_REQUIRED", False),
        (f"{line}\n- [x] Disposition: REVIEW_READY", False),  # ambiguous even when positive
        (f"{line}\n1. Recorded the earlier disposition in the issue.", True),
        (f"{line}\nFinal disposition: REPAIR_REQUIRED", False),
        (f"{line}\nCurrent disposition: REVIEW_INCOMPLETE", False),
        (f"{line}\n| Disposition | REPAIR_REQUIRED |", False),
        (f"{line}\n| PR | State |\n| --- | --- |\n| 14421 | REPAIR_REQUIRED |", False),
        (f"{line}\n| REPAIR_REQUIRED | fix oracle |", False),
        (f"{line}\n| REPAIR_REQUIRED |", False),
        (f"{line}\n| a | REVIEW_READY | b |", False),  # ambiguous even when positive
        (f"{line}\n| Check | Result |\n| --- | --- |\n| fmt | pass |", True),
        (f"{line}\nAccepted winner and losing candidate disposition (when decided): none", True),
        ("", False),
    ]
    for body, expected in body_cases:
        actual = body_attests_head(body, head)
        assert actual == expected, f"pr body={body!r} expected={expected} actual={actual}"
    assert not body_attests_head(line, "")
    assert not body_attests_head(f"Source review: REVIEW_READY {head[:7]}", head[:7])

    repo = {"repo": {"full_name": "owner/name"}}
    pull = lambda **kw: {
        "author_association": "MEMBER",
        "head": {"sha": head, **repo},
        "base": dict(repo),
        "body": line,
        **kw,
    }
    assert reviews_allow_source_attestation([], head=head, pull=pull())
    assert not reviews_allow_source_attestation([], head=head, pull=None)
    # A trusted reviewer's current negative state line refuses the description carrier
    # whatever list markup leads it (#11566); narration does not.
    for body, allowed in (
        ("1. REPAIR_REQUIRED: repair oracle", False),
        ("1) REPAIR_REQUIRED: repair oracle", False),
        ("- [ ] REPAIR_REQUIRED: repair oracle", False),
        ("- [ ] Disposition: REPAIR_REQUIRED", False),
        ("- REPAIR_REQUIRED: repair oracle", False),
        ('```json\n{"disposition":"REPAIR_REQUIRED"}\n```', False),
        ('```toml\nstate = "REPAIR_REQUIRED"\n```', False),
        ("Current disposition: REVIEW_INCOMPLETE", False),
        ("| Disposition | REPAIR_REQUIRED |", False),
        ("| REPAIR_REQUIRED | fix oracle |", False),
        ("| REPAIR_REQUIRED |", False),
        ("The earlier `SOURCE_REVIEW_READY` was for the previous head.", True),
    ):
        assert reviews_allow_source_attestation(
            [review(state="COMMENTED", commit_id=head, id=3, body=body,
                    submitted_at="2026-10-15T01:00:03Z", user={"login": "other"})],
            head=head,
            pull=pull(),
        ) is allowed, body
    assert not reviews_allow_source_attestation([], head=head)
    for association in ("NONE", "CONTRIBUTOR", "FIRST_TIME_CONTRIBUTOR", ""):
        assert not reviews_allow_source_attestation(
            [], head=head, pull=pull(author_association=association)
        ), association
    # A fork head is never read, whatever its description says.
    assert not reviews_allow_source_attestation(
        [], head=head, pull=pull(head={"sha": head, "repo": {"full_name": "fork/name"}})
    )
    assert not reviews_allow_source_attestation(
        [], head=head, pull=pull(head={"sha": head, "repo": None})
    )
    # The live head moved after the description was written.
    assert not reviews_allow_source_attestation(
        [], head=head, pull=pull(head={"sha": other, **repo})
    )
    assert not reviews_allow_source_attestation([], head=other, pull=pull())
    # The description is a carrier only: every review refusal still applies.
    assert not reviews_allow_source_attestation(
        [review(state="CHANGES_REQUESTED", commit_id=head, body="")], head=head, pull=pull()
    )
    assert not reviews_allow_source_attestation(
        [review(state="COMMENTED", commit_id=head, body="REPAIR_REQUIRED")],
        head=head,
        pull=pull(),
    )
    assert not reviews_allow_source_attestation(
        [review(state="APPROVED", commit_id=head, submitted_at=None)], head=head, pull=pull()
    )
    # A neutral current-head comment (a thread reply creates an empty one) no longer
    # hides the attestation, and an older-head ready review does not supply one.
    assert reviews_allow_source_attestation(
        [review(state="COMMENTED", commit_id=head, body="")], head=head, pull=pull()
    )
    assert not reviews_allow_source_attestation(
        [review(state="COMMENTED", commit_id=other, body="REVIEW_READY")],
        head=head,
        pull=pull(body="Source review:"),
    )
    # A reviewer's negative statement outlives their own later neutral reply: the
    # description cannot stand in for the retraction they never made.
    stamp = lambda n: f"2026-10-15T01:00:0{n}Z"
    negative_then_reply = [
        review(id=1, state="COMMENTED", commit_id=head, body="REPAIR_REQUIRED", submitted_at=stamp(1)),
        review(id=2, state="COMMENTED", commit_id=head, body="", submitted_at=stamp(2)),
    ]
    assert not reviews_allow_source_attestation(negative_then_reply, head=head, pull=pull())
    assert not reviews_allow_source_attestation(negative_then_reply, head=head)
    # The same reviewer replacing it with a ready statement lifts it; a negative one on
    # an earlier head never applied to this head.
    assert reviews_allow_source_attestation(
        negative_then_reply
        + [review(id=3, state="COMMENTED", commit_id=head, body="REVIEW_READY", submitted_at=stamp(3))]
        + [review(id=4, state="COMMENTED", commit_id=head, body="", submitted_at=stamp(4))],
        head=head,
        pull=pull(),
    )
    assert reviews_allow_source_attestation(
        [
            review(id=1, state="COMMENTED", commit_id=other, body="REPAIR_REQUIRED", submitted_at=stamp(1)),
            review(id=2, state="COMMENTED", commit_id=head, body="", submitted_at=stamp(2)),
        ],
        head=head,
        pull=pull(),
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
    # FileType returns the existing sys.stdin for '-', ignoring its encoding
    # argument. Read bytes so piped GitHub JSON also has an explicit UTF-8 boundary.
    parser.add_argument("--check-reviews", type=argparse.FileType("rb"))
    parser.add_argument(
        "--check-pull",
        type=argparse.FileType("rb"),
        help="pull request object (JSON) to read the Source review line from, with --check-reviews",
    )
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
        try:
            pull = fetch_pull(args.repository, args.pr)
        except (OSError, subprocess.SubprocessError, ValueError) as exc:
            # The description is an optional carrier: without it only reviews count.
            print(
                f"::warning::could not read the pull request description ({exc}); "
                "only native reviews are considered for the source review attestation",
                file=sys.stderr,
            )
            pull = None
    else:
        try:
            reviews = parse_paginated(args.check_reviews.read().decode("utf-8"))
            pull = None
            if args.check_pull is not None:
                pull = json.loads(args.check_pull.read().decode("utf-8"))
                if not isinstance(pull, dict):
                    raise ValueError("pull request data is not an object")
        except ValueError as exc:
            print(
                f"::error::source review attestation refused: malformed review data: {exc}",
                file=sys.stderr,
            )
            return 1
    if not reviews_allow_source_attestation(
        reviews, head=args.head, author=args.author, pull=pull
    ):
        print(
            "::error::source review attestation refused: a trusted reviewer "
            "(OWNER/MEMBER/COLLABORATOR) must post a current-head COMMENT whose first line is "
            "exactly SOURCE_REVIEW_READY or REVIEW_READY (or disposition: of either) with no "
            "contradicting disposition text, or a native APPROVED on the head; narrative "
            "tokens and quoted examples do not count (#8157). When GitHub throttles review "
            "writes, a same-repository pull request may instead carry one description line "
            f"`Source review: REVIEW_READY {args.head}` with the full current head SHA (#4984)",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
