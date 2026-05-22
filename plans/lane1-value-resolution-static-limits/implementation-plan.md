# Lane 1 Value Resolution Static Limits Implementation Plan

Status: active

Owner: Lane 1 - Evidence Accuracy

Linked tracker:
`docs/lanes/LANE_1_VALUE_RESOLUTION_STATIC_LIMITS.md`

Linked specs: `RIPR-SPEC-0034`, `RIPR-SPEC-0045`

Active goal: `.ripr/goals/active.toml`

## Current State

Lane 1 Finding Alignment Burn-Down is closed. Its closeout selected successor
work from repo-owned proof instead of chat history:

- the live audit recorded 47,626 raw alignment signals, 38,564 canonical
  evidence items, 162 actionable canonical gaps, and 26,250 named static
  limitations;
- the top remaining named limitation bucket was
  `activation_value_unresolved` at 25,881;
- the current scorecard maps that bucket to
  `analysis/value-resolution-audit-fixes`;
- earlier supported owner-call slices moved some cases out of limitation, but
  predicate-boundary value checks still require concrete discriminator
  evidence.

This plan activates the next queue without changing analyzer behavior. The
first behavior-bearing work must be fixture and audit planning.

## Hard Boundaries

- fixture first;
- audit-driven repairs before heuristics;
- raw findings remain diagnostic evidence;
- canonical items are the countable unit;
- unsupported value-resolution shapes remain named limitations;
- do not invent observed activation values;
- predicate-boundary value checks require concrete discriminator evidence;
- no PR/CI rendering changes;
- no inline PR comment publishing;
- no LSP or editor polish;
- no gate policy or default blocking changes;
- no public badge or score redefinition;
- no generated tests;
- no automatic source edits;
- no provider or model calls;
- no mutation execution.

## Work Item 1: fixtures: plan value-resolution audit cases

Issue: [swarm #285](https://github.com/EffortlessMetrics/ripr-swarm/issues/285)

Status: ready

### Goal

Select the first supported `predicate_boundary` /
`activation_value_unresolved` sub-shape from audit and scorecard evidence before
changing analyzer behavior.

### Production Delta

No analyzer delta. Add the fixture/audit plan that names:

- the supported sub-shape;
- positive benchmark cases;
- must-not-claim benchmark cases;
- expected canonical item state;
- expected raw finding support;
- expected static limitation behavior for unsupported shapes;
- before/after audit or scorecard proof commands.

### Non-Goals

- no analyzer classification changes;
- no public score or badge changes;
- no downstream PR/CI/editor changes;
- no generated tests, provider calls, source edits, or mutation execution.

### Acceptance

- `.ripr/goals/active.toml` reports this item as the next ready work item.
- The selected sub-shape is justified by the closeout and scorecard route.
- Unsupported value-resolution shapes remain named static limitations.
- Raw findings remain supporting evidence and canonical items remain countable.
- The follow-up fixture PR can be executed without re-deciding scope.

### Proof Commands

```bash
cargo xtask goals status
cargo xtask goals next
cargo xtask check-goals
cargo xtask check-doc-index
cargo xtask markdown-links
cargo xtask check-static-language
cargo xtask check-pr
git diff --check
```

### Rollback

Revert the plan and active-manifest changes. The previous closed
`lane1-finding-alignment-burndown` manifest is archived at
`.ripr/goals/archive/2026-05-22-lane1-finding-alignment-burndown.toml`.

## Work Item 2: fixtures: add value-resolution benchmark cases

Status: blocked by `fixtures/value-resolution-audit-plan`

### Goal

Pin the selected value-resolution shape before implementation.

### Production Delta

Add benchmark cases for at least one supported value-resolution sub-shape and
the corresponding must-not-claim shapes.

### Acceptance

- Positive cases record raw findings, expected canonical item, gap state,
  actionability, and moved limitation state.
- Must-not-claim cases remain `activation_value_unresolved`.
- No analyzer behavior changes land in the fixture PR unless the benchmark
  harness requires a non-semantic adapter.

### Proof Commands

```bash
cargo test -p xtask evidence_quality_benchmark
cargo xtask check-fixture-contracts
cargo xtask check-output-contracts
cargo xtask check-pr
git diff --check
```

## Work Item 3: analysis: apply value-resolution audit fixes

Status: blocked by `fixtures/value-resolution-benchmark-cases`

### Goal

Move only fixture-backed supported cases out of
`activation_value_unresolved`.

### Production Delta

Update value-resolution evidence so the supported predicate-boundary shape
uses concrete discriminator evidence instead of remaining a static limitation.
Unsupported shapes stay named.

### Acceptance

- Supported fixture cases move out of `activation_value_unresolved`.
- Must-not-claim cases stay limitations.
- Raw findings remain attached as supporting evidence.
- Before/after scorecard or audit delta is recorded.
- No public badge, gate, PR/CI, editor, generated-test, provider, source-edit,
  or mutation behavior changes.

### Proof Commands

```bash
cargo test -p xtask evidence_quality_benchmark
cargo xtask lane1-evidence-audit
cargo xtask evidence-quality-scorecard
cargo xtask check-output-contracts
cargo xtask check-pr
git diff --check
```

## Work Item 4: campaign: close value-resolution static limits

Status: blocked by `analysis/value-resolution-audit-fixes`

### Goal

Close this campaign with the selected shape, movement, remaining limits, and
next audit-driven selection rule.

### Acceptance

- Closeout records the fixture-backed supported shape.
- Closeout records before/after audit or scorecard movement.
- Remaining unsupported value-resolution shapes are named.
- Downstream consumers and non-goals are explicit.
- `.ripr/goals/active.toml` is archived or moved to the next selected campaign.

### Proof Commands

```bash
cargo xtask goals status
cargo xtask goals next
cargo xtask check-doc-index
cargo xtask markdown-links
cargo xtask check-static-language
cargo xtask check-pr
git diff --check
```
