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

## Separate concerns

`Status` records the document's disposition in the legacy spec dialect. It does
not assert implementation, evidence, support, or live work. Requirement-level
lifecycle and ancestry belong to the RIPR-SPEC v2 dialect; one-PR implementation
claims belong to `ImplementationSliceV1`; evidence currentness belongs to
traceability and exact receipts; support claims belong to
`docs/status/SUPPORT_TIERS.md`; and maintenance attention belongs to the
advisory maintenance report and review receipts.

A document may deserve maintenance attention without being invalid. The
maintenance report is advisory and must not change this format check's status
validation or merge eligibility.
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
