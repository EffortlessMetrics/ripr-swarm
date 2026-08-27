# Spec Format

Specs are behavior contracts for humans, tests, tools, and future agents. They
should be consistent enough to parse mechanically.

## Status Values

Allowed statuses:

- `proposed`
- `planned`
- `accepted`
- `deprecated`

## Required Sections

Every spec in `docs/specs/RIPR-SPEC-*.md` must include:

- `Status: ...`
- `## Problem`
- `## Behavior`
- `## Required Evidence`
- `## Non-Goals`
- `## Acceptance Examples`
- `## Test Mapping`
- `## Implementation Mapping`
- `## Metrics`

Accepted specs should have concrete acceptance examples and at least one current
or planned test mapping. Planned specs may point at planned tests and planned
modules, but they still need the same sections so agents can reason over the
gap.

## IDs

Spec filenames and titles use stable IDs:

```text
docs/specs/RIPR-SPEC-0004-predicate-boundary-activation.md
# RIPR-SPEC-0004: Predicate Boundary Activation
```

Use these IDs in tests, fixtures, traceability entries, metrics, and PR
summaries when behavior changes.

Before adding a new spec, ask the repo for the next live ID:

```bash
cargo xtask specs next
```

Spec IDs are source-of-truth identifiers, not generated runtime counts. The
helper only prevents stale numbering assumptions; humans still author the spec
contract.

## Lifecycle and evidence separation

`Status:` records the document's normative disposition only. It does not claim
that the requirement is implemented, that evidence is current, that a support
tier is earned, or that work is active. Those claims belong to the implementation
slice, traceability and evidence receipts, support-tier authority, and live
GitHub/worktree state respectively.

A spec does not expire because it has not changed recently. Correctness,
implementation, evidence, support, supersession, and maintenance attention are
separate questions with separate authorities. A document changes disposition
through an explicit semantic decision, not an elapsed-time threshold.
## Checks

Run:

```bash
cargo xtask check-spec-format
cargo xtask check-spec-numbering
```

The check verifies required sections, status values, and title/filename ID
consistency. The numbering guard verifies that every spec file appears in
`docs/specs/README.md` and that traceability/capability surfaces do not
reference missing spec IDs.
