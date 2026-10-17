# Linking model

The source-of-truth stack is a graph, not a pile of documents. Each layer should
link to the next layer that owns the more specific question.

## Current authority graph

Use the [repo tracking model](../REPO_TRACKING_MODEL.md) to resolve each
question to its owner. A typical set of links is:

```text
Roadmap
  -> Proposal
    -> Spec
      -> ADR when a durable decision is needed
        -> Implementation plan
          -> GitHub issue / scoped PR
            -> PR-local implementation slice when applicable
              -> Proof command and exact evidence
                -> Support-tier or policy update when applicable
                  -> Closeout
```

This is a graph of applicable authorities, not a mandatory document chain.
Durable behavior, claims, policy, and decisions need links to their owning
artifacts; a change does not need every layer. Live work selection comes from
GitHub issues, PRs, checks, reviews, and the local worktree. A PR-local slice
records scope for already-selected work; it does not select or authorize work.

## Link fields

Use stable IDs and paths so humans and validators can resolve the graph.

Recommended fields:

```text
Status:
Owner:
Created:
Linked proposal:
Linked spec:
Linked ADR:
Linked plan:
Linked issue:
Linked PR:
Support-tier impact:
Policy impact:
Required evidence:
Claim boundary:
Rollback:
```

Use `n/a` when a layer does not apply. Do not omit the field in templates merely
because the current slice has no impact.

## ID rules

- Proposal IDs use `RIPR-PROP-NNNN`.
- Spec IDs use `RIPR-SPEC-NNNN`.
- ADRs use the existing numeric ADR path under `docs/adr/`.
- Plan work items use stable kebab-case IDs.
- PRs and PR-local slices should reference the relevant plan work-item ID when
  applicable.
- Closeouts should name the date and lane or goal ID.

IDs are repo contracts. Renaming an accepted ID should be treated as a
compatibility change for links, reports, and agent handoff.

## Status rules

Status belongs to a specific object; there is no common lifecycle for every
artifact. Keep these axes separate:

| Concern | Authority |
| --- | --- |
| Spec document disposition | [Spec Format](../SPEC_FORMAT.md): `proposed`, `planned`, `accepted`, `deprecated` |
| Proposal and ADR disposition | The respective document, template, and registered artifact schema |
| Plan work-item progress | The plan's work items and dependencies; this does not select live work |
| One-PR implementation claim | `ImplementationSliceV1` under [`.allow/spec-system/slices/`](../../.allow/spec-system/slices/) |
| Evidence and proof currentness | Traceability/evidence edges and exact receipts |
| Support claim | [Support tiers](../status/SUPPORT_TIERS.md), projecting capability authorities |
| Live work | GitHub issues/PRs/checks/reviews and the local worktree |

An accepted spec does not imply implemented behavior, sufficient evidence,
promoted support, or active work. Validators enforce their registered schemas;
this guide does not change validator semantics or introduce a shared status
vocabulary. Retired active-goal lifecycle states belong to the historical
model described in [Codex Goals](../CODEX_GOALS.md), not current work selection.

## Proof links

Done work must name proof, not vibes. A plan item or closeout should include:

- exact commands run;
- whether they passed, failed, or were not run;
- relevant output paths or receipts;
- claim boundary for what the proof does not establish;
- rollback path for undoing the PR.

Support-tier rows map product claims to proof commands. Specs may reference the
support-tier row, but the support-tier row owns the claim/proof mapping.

## Validator responsibilities

The current validators split document graph, support-tier, and policy proof instead
of making one document or one check own every layer.

`cargo xtask check-doc-artifacts` verifies registered document graph facts:

- artifact IDs are unique;
- linked IDs resolve;
- declared paths exist;
- accepted specs link to a proposal or record a standalone reason;
- plans link to at least one proposal or spec;

`cargo xtask check-support-tiers` verifies support-tier proof-command references.
`cargo xtask check-doc-index` checks document indexes, and
`cargo xtask check-doc-roles` checks the supported planning-document roles.
`cargo xtask markdown-links` checks local Markdown links. Policy-specific ledgers and
workflows own exception scope, owner, reason, and review posture.
`cargo xtask repo-contract-report` composes these facts into an advisory graph
packet; it is not a gate and does not infer product correctness.

Historical `cargo xtask check-goals` instructions referred to the
`.ripr/goals/` scheduler deleted in #1701. That command is retired; use live
GitHub/worktree state and the current authorities above.
