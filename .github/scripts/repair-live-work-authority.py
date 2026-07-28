#!/usr/bin/env python3
"""One-shot repair for goals-era live work guidance."""

from pathlib import Path


def replace_between(text: str, start: str, end: str, replacement: str, label: str) -> str:
    start_index = text.find(start)
    if start_index < 0:
        raise SystemExit(f"missing start marker for {label}: {start!r}")
    end_index = text.find(end, start_index)
    if end_index < 0:
        raise SystemExit(f"missing end marker for {label}: {end!r}")
    return text[:start_index] + replacement + text[end_index:]


path = Path("docs/swarm-development.md")
text = path.read_text(encoding="utf-8")
if "cargo xtask goals next" not in text:
    raise SystemExit("retired goals command not found in swarm operator loop")
text = text.replace(
    "cargo xtask goals next",
    "gh issue list --repo EffortlessMetrics/ripr-swarm --state open --limit 100",
    1,
)
text = replace_between(
    text,
    "When `cargo xtask goals next` reports",
    "If no aligned work is available",
    """The retired `.ripr/goals` scheduler is not live execution authority. Do not
continue a closed campaign or infer a successor from chat history. Select work
from repo-owned evidence in this order:

1. open `ripr-swarm` PRs, reviews, and required checks;
2. ordinary source-repo PRs that should be ported or redirected;
3. open issues with explicit ownership and current acceptance criteria;
4. the PR-local `ImplementationSliceV1` under `.allow/spec-system/slices/`;
5. accepted RIPR-SPEC requirements and linked proposals, ADRs, or plans;
6. historical campaign documents only as context, never as current authorization.

""",
    "swarm operator authority",
)
path.write_text(text, encoding="utf-8")

path = Path("docs/IMPLEMENTATION_CAMPAIGNS.md")
text = path.read_text(encoding="utf-8")
text = replace_between(
    text,
    "This is the campaign-level plan",
    "## Campaign 1:",
    """This document preserves historical campaign-level context for Codex Goals and
long-context contributor work. It is not live execution authority. The campaigns
below remain useful for chronology, objectives, and completed work-item context.

Live work selection and ownership come from GitHub issues, pull requests, checks,
reviews, and the local worktree. One PR's scope is its `ImplementationSliceV1`
under `.allow/spec-system/slices/`; normative behavior lives in RIPR-SPEC
requirements. Do not infer current work from a campaign status below.

""",
    "implementation campaigns header",
)
path.write_text(text, encoding="utf-8")

path = Path("docs/lanes/LANE_4_PR_CI_REVIEW.md")
text = path.read_text(encoding="utf-8")
old_row = "| Active goal manifest | current Codex/Droid execution state | `.ripr/goals/active.toml` or a lane manifest when Lane 4 is active |"
if old_row not in text:
    raise SystemExit("retired active-goal row not found")
text = text.replace(
    old_row,
    "| Live work evidence | current ownership, scope, and review state | GitHub issues, PRs, checks, reviews, the local worktree, and the PR-local `ImplementationSliceV1` |",
    1,
)
text = replace_between(
    text,
    "Proposal explains why.",
    "## Scope",
    """Proposal explains why. Specs define what must be true. ADRs record durable
architecture decisions. Plans sequence bounded work but do not authorize it.
Current GitHub and local-worktree evidence identifies what is live; the PR-local
implementation slice bounds the change. Policy ledgers own authority and
exceptions. Closeouts record what happened and what remains.

""",
    "Lane 4 authority paragraph",
)
path.write_text(text, encoding="utf-8")
