# Linking model

The source-of-truth stack is a graph, not a pile of documents. Each layer should
link to the next layer that owns the more specific question.

## Canonical chain

```text
Roadmap
  -> Proposal
    -> Spec
      -> ADR when a durable decision is needed
        -> Implementation plan
          -> Active goal work item
            -> PR / issue
              -> Proof command
                -> Support-tier or policy update when applicable
                  -> Closeout
```

The chain is not mandatory for every small change. It is mandatory when a
feature, claim, policy, or architecture decision would otherwise outlive the PR.

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
- Active goal work items should reuse the plan work-item ID when practical.
- Closeouts should name the date and lane or goal ID.

IDs are repo contracts. Renaming an accepted ID should be treated as a
compatibility change for links, reports, and agent handoff.

## Status rules

Artifacts should use small, explicit status vocabularies:

- proposals, specs, and ADRs: `draft`, `proposed`, `accepted`,
  `implemented`, `superseded`, `rejected`;
- plan and goal work items: `ready`, `active`, `blocked`, `done`,
  `superseded`;
- active goals: `active`, `paused`, `closed`, `complete`, `archived`.

If a validator supports a smaller vocabulary, the validator is authoritative for
that artifact until the schema changes.

## Proof links

Done work must name proof, not vibes. A plan item or closeout should include:

- exact commands run;
- whether they passed, failed, or were not run;
- relevant output paths or receipts;
- claim boundary for what the proof does not establish;
- rollback path for undoing the PR.

Support-tier rows map product claims to proof commands. Specs may reference the
support-tier row, but the support-tier row owns the claim/proof mapping.

## Planned validator responsibilities

A future `check-doc-artifacts` style validator should be able to verify:

- artifact IDs are unique;
- linked IDs resolve;
- declared paths exist;
- accepted specs link to a proposal or record a standalone reason;
- plans link to at least one proposal or spec;
- active goal work items reference real artifacts;
- support-tier claims name proof commands;
- policy exceptions include owner, reason, scope, and review posture.

This PR does not implement that validator.
