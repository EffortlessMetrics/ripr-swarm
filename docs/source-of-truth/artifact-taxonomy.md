# Artifact taxonomy

The source-of-truth stack works only when each artifact has a narrow job. A
document may link to other layers, but it must not absorb their responsibilities.

The [repo tracking model](../REPO_TRACKING_MODEL.md) defines the current
authority boundaries. [Spec Format](../SPEC_FORMAT.md) separates document
disposition from implementation, evidence, support, and live work.

| Artifact | Owns | Does not own | Storage |
| --- | --- | --- | --- |
| Roadmap | Direction, milestones, product sequencing | Acceptance tests, proof commands, PR order | [`docs/ROADMAP.md`](../ROADMAP.md) |
| Proposal / PRD | Problem, user value, alternatives, risks, success criteria | Behavior schemas, output contracts, work queues | [`docs/proposals/`](../proposals/) |
| Spec | Behavior contract, evidence requirements, acceptance examples | Product rationale, durable architecture decisions, PR sequencing | [`docs/specs/`](../specs/) |
| ADR | Durable architecture or product decision | Full behavior contract, proposal rationale, task queue | [`docs/adr/`](../adr/) |
| Implementation plan | PR-sized sequence, dependencies, proof commands, rollback | Why, behavior contract, support-tier truth, policy exception truth | [`plans/`](../../plans/) |
| Live execution state | Current issue, PR, checks, reviews, and worktree state | Durable design history, behavior contracts, product support claims | GitHub and the local worktree |
| Scoped PR slice | One PR's scope, seams, claim boundary, and evidence obligations | Live work selection, repository-wide writer or scheduler state | [`.allow/spec-system/slices/`](../../.allow/spec-system/slices/) (`ImplementationSliceV1`) |
| Support tiers | Product claim to proof-command mapping | Specs, CI lane ownership, policy exceptions | [`docs/status/SUPPORT_TIERS.md`](../status/SUPPORT_TIERS.md) |
| Policy ledgers | Exceptions, owners, reasons, review posture, governed obligations | Product claims, behavior contracts, PR sequencing | [`policy/`](../../policy/) |
| CI and proof commands | Mechanical evidence for the current slice | Product promises without support-tier mapping | [`.github/workflows/`](../../.github/workflows/) and `cargo xtask` |
| Closeout / handoff | What landed, proof run, remaining work, next handoff | New behavior contracts or retroactive proposal changes | [`docs/handoffs/`](../handoffs/) |

## Layer rules

- A proposal answers "why should this exist?"
- A spec answers "what exact behavior must be true?"
- An ADR answers "what durable decision governs this?"
- A plan answers "what PR-sized step lands next?"
- Live GitHub/worktree state answers "what is executing now?"
- A PR-local slice answers "what scope and evidence does this PR claim?"
- A support-tier row answers "what may users believe, and what proves it?"
- A policy ledger answers "what exception or obligation exists, who owns it,
  and when is it reviewed?"
- A closeout answers "what happened and what remains?"

## What must stay out of each layer

- Do not put product support claims inside specs. Link to support tiers.
- Do not put CI lane ownership inside specs. Link to CI policy ledgers.
- Do not put PR order inside proposals. Link to plans.
- Do not put durable architecture decisions inside plans. Link to ADRs.
- Do not put active-agent state inside handoffs. Link to the relevant GitHub
  issue or PR; use checks, reviews, and the local worktree for current state.
- Do not use README or release copy as the first place a stable claim appears.
  Stable claims need support-tier proof.

## Repo-specific notes

Historical references to `.ripr/goals/active.toml` describe the retired
goal-manifest scheduler, deleted in #1701 PR 3. Retained handoffs and Git
history explain that model; they do not select current work. See
[Codex Goals](../CODEX_GOALS.md) for the historical explanation.

`ripr` keeps one published package, one binary, one library, and an
experimental sidecar LSP adapter. Source-of-truth work must not introduce
package splits, new public schema promises, or stronger enforcement language
unless the linked proposal, spec, and policy ledger authorize that change.
