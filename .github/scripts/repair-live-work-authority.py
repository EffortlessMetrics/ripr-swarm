#!/usr/bin/env python3
"""One-shot repair for goals-era live work guidance."""

from pathlib import Path

path = Path("docs/swarm-development.md")
text = path.read_text(encoding="utf-8")
text = text.replace(
    "gh pr list --repo EffortlessMetrics/ripr --state open\n"
    "cargo xtask goals next",
    "gh pr list --repo EffortlessMetrics/ripr --state open\n"
    "gh issue list --repo EffortlessMetrics/ripr-swarm --state open --limit 100",
)
old = """When `cargo xtask goals next` reports `no_current_goal = true`, do not continue
the closed campaign and do not infer a successor from chat history. Select work
from repo-owned state in this order:

1. open `ripr-swarm` PRs and required checks;
2. ordinary source-repo PRs that should be ported or redirected;
3. `docs/IMPLEMENTATION_CAMPAIGNS.md`;
4. `docs/IMPLEMENTATION_PLAN.md`;
5. accepted proposals, specs, ADRs, and campaign plans;
6. open issues that cite those repo artifacts.
"""
new = """The retired `.ripr/goals` scheduler is not live execution authority. Do not
continue a closed campaign or infer a successor from chat history. Select work
from repo-owned evidence in this order:

1. open `ripr-swarm` PRs, reviews, and required checks;
2. ordinary source-repo PRs that should be ported or redirected;
3. open issues with explicit ownership and current acceptance criteria;
4. the PR-local `ImplementationSliceV1` under `.allow/spec-system/slices/`;
5. accepted RIPR-SPEC requirements and linked proposals, ADRs, or plans;
6. historical campaign documents only as context, never as current authorization.
"""
if old not in text:
    raise SystemExit("expected swarm operator authority paragraph not found")
path.write_text(text.replace(old, new), encoding="utf-8")

path = Path("docs/IMPLEMENTATION_CAMPAIGNS.md")
text = path.read_text(encoding="utf-8")
old = """This is the campaign-level plan for Codex Goals and long-context contributor
work. Campaigns are larger than one PR. Each campaign has an objective, an end
state, and work items that should each follow the
[scoped PR contract](SCOPED_PR_CONTRACT.md).

The operational checklist remains in [Implementation plan](IMPLEMENTATION_PLAN.md).
The machine-readable active campaign is `.ripr/goals/active.toml`.
"""
new = """This document preserves historical campaign-level context for Codex Goals and
long-context contributor work. It is not live execution authority. The campaigns
below remain useful for chronology, objectives, and completed work-item context.

Live work selection and ownership come from GitHub issues, pull requests, checks,
reviews, and the local worktree. One PR's scope is its `ImplementationSliceV1`
under `.allow/spec-system/slices/`; normative behavior lives in RIPR-SPEC
requirements. Do not infer current work from a campaign status below.
"""
if old not in text:
    raise SystemExit("expected implementation campaigns header not found")
path.write_text(text.replace(old, new), encoding="utf-8")

path = Path("docs/lanes/LANE_4_PR_CI_REVIEW.md")
text = path.read_text(encoding="utf-8")
text = text.replace(
    "| Active goal manifest | current Codex/Droid execution state | `.ripr/goals/active.toml` or a lane manifest when Lane 4 is active |",
    "| Live work evidence | current ownership, scope, and review state | GitHub issues, PRs, checks, reviews, the local worktree, and the PR-local `ImplementationSliceV1` |",
)
old = """Proposal explains why. Specs define what must be true. ADRs record durable
architecture decisions. Plans sequence PRs. Active manifests tell agents what
to do now. Policy ledgers own authority and exceptions. Closeouts record what
happened and what remains.
"""
new = """Proposal explains why. Specs define what must be true. ADRs record durable
architecture decisions. Plans sequence bounded work but do not authorize it.
Current GitHub and local-worktree evidence identifies what is live; the PR-local
implementation slice bounds the change. Policy ledgers own authority and
exceptions. Closeouts record what happened and what remains.
"""
if old not in text:
    raise SystemExit("expected Lane 4 authority paragraph not found")
path.write_text(text.replace(old, new), encoding="utf-8")
