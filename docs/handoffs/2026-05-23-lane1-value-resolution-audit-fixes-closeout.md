# Handoff: Lane 1 Value Resolution Audit Fixes Closeout

Date: 2026-05-23

Branch: `campaign-value-resolution-audit-closeout`

Current work item: `campaign/value-resolution-audit-closeout`

Archived manifest:
`.ripr/goals/archive/2026-05-23-lane1-value-resolution-audit-fixes.toml`

## Current State

Lane 1 Value Resolution Audit Fixes is closed. The rail selected one
`predicate_boundary` / `activation_value_unresolved` sub-shape from the
finding-alignment burn-down closeout and current scorecard, pinned it with
fixtures, confirmed the current analyzer already supports it, recorded the
zero-movement sampled audit delta, and added a checked dogfood receipt.

This closeout does not change analyzer behavior, PR/CI rendering, editor
behavior, gate policy, badge semantics, generated tests, provider calls, source
edits, release behavior, default blocking, or mutation execution.

`.ripr/goals/active.toml` now records `status = "closed"` and
`no_current_goal = true`. The next Lane 1 campaign should be selected from a
fresh audit, scorecard, dogfood receipt, downstream consumer issue, or explicit
spec-backed campaign rather than continued from this closed rail.

## What Landed

| Work item | Result |
| --- | --- |
| `docs/lane1-value-resolution-audit-fixes-stack` | Opened the issue-backed rail, active manifest, lane tracker, implementation plan, and docs indexes. |
| `fixtures/value-resolution-audit-corpus` | Added fixture-backed positive and must-not-claim cases for same-test struct literal field projection with source-order and mutation guards. |
| `analysis/value-resolution-supported-subshape` | Dispositioned the selected sub-shape as already supported by the existing value-resolution path; no production analyzer edit was needed. |
| `report/value-resolution-audit-delta` | Recorded a zero-movement sampled audit delta and kept the remaining `activation_value_unresolved` queue visible. |
| `dogfood/value-resolution-receipts` | Added `value_resolution_struct_literal_projection_zero_movement` to the finding-alignment dogfood corpus with canonical identity and non-claims. |
| `campaign/value-resolution-audit-closeout` | Closed the rail, archived the manifest, and left no successor active goal selected. |

## Selected Sub-Shape

Supported shape:

```text
same-test struct literal field projection
```

Supported when:

- a test constructs a same-test struct literal with concrete literal fields;
- an owner call later passes a field projection such as `case.amount`;
- source-order and same-line column scoping make the value visible at the owner
  call;
- mutation or shadowing before the owner call prevents stale values from being
  promoted.

Still unsupported and still named as limitations:

- helper-returned structs;
- non-literal fields;
- mutable bindings and mutations before the owner call;
- shadowed, pattern-bound, fixture, or parameterized values;
- cross-file constants;
- macros, generated inputs, and deeper semantic value resolution.

## Counts

Current sampled Lane 1 audit state:

| Metric | Count |
| --- | ---: |
| Sampled seams / canonical items | 5,000 |
| Raw headline gaps | 1,567 |
| Canonical gap groups | 1,383 |
| Duplicate-looking groups | 130 |
| Missing discriminators | 149 |
| Static limitations | 1,551 |
| Finding-alignment raw signals | 5,630 |
| Finding-alignment canonical items | 5,000 |
| Aligned raw findings | 5,630 |
| Unaligned raw findings | 0 |
| Actionable canonical items | 61 |
| Finding-alignment static limitations | 1,506 |
| Canonical items without repair route | 0 |
| Canonical items without verify command | 0 |

The run is bounded by:

```text
repo_exposure_seam_limit = limit_5000_of_40233
```

Current `predicate_boundary` state:

| Metric | Count |
| --- | ---: |
| Raw signals | 975 |
| Canonical items | 601 |
| Actionable items | 61 |
| Already observed items | 222 |
| Static limitations | 318 |
| Unknown items | 0 |

The remaining selected limitation queue is:

| Static limitation | Count | Repair route |
| --- | ---: | --- |
| `activation_value_unresolved` | 315 | `analysis/value-resolution-audit-fixes` |

The selected sub-shape recorded zero sampled movement because it was already
implemented before this rail started:

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Static limitations | 1,551 | 1,551 | 0 |
| Finding-alignment static limitations | 1,506 | 1,506 | 0 |
| Missing discriminators | 149 | 149 | 0 |
| `activation_value_unresolved` | 315 | 315 | 0 |

The trend report status is `unknown` because no previous comparable scorecard
or audit snapshot was available to the trend command.

## Dogfood Receipt

`fixtures/finding-alignment-dogfood/corpus.json` now includes
`value_resolution_struct_literal_projection_zero_movement`.

The receipt records:

- canonical gap id:
  `gap:value_resolution_struct_literal_projection_zero_movement`;
- raw finding context for the same-test struct literal projection;
- zero before/after movement from the already-supported implementation;
- `cargo xtask lane1-evidence-audit` as the verify command;
- no-action outcome for the selected supported sub-shape;
- explicit non-claims for unsupported value flows, runtime proof, mutation
  proof, coverage, badge, and gate authority.

## What This Proves

- The selected fixture-backed sub-shape is not missing production support in
  the current analyzer.
- Unsupported value flows remain named static limitations rather than user test
  debt.
- Raw findings remain supporting evidence and canonical items remain the
  countable unit.
- Audit, scorecard, trend, and dogfood proof now carry the zero-movement state
  forward for future closeout and successor selection.

## What This Does Not Prove

- no runtime adequacy;
- no mutation proof;
- no coverage adequacy;
- no merge approval;
- no default gate authority;
- no badge semantic change;
- no PR/CI or editor projection change;
- no full-repo exposure completion beyond the sampled 5,000-seam run.

## Validation

Closeout validation for this PR:

```bash
cargo xtask goals status
cargo xtask goals next
cargo xtask check-doc-index
cargo xtask markdown-links
cargo xtask check-static-language
cargo xtask check-pr
git diff --check
```

The closed rail also used these proof commands across implementation slices:

```bash
cargo test -p xtask evidence_quality_benchmark
cargo test -p ripr value_resolution --lib
cargo xtask lane1-evidence-audit
cargo xtask evidence-quality-scorecard
cargo xtask evidence-quality-trend
cargo xtask dogfood
cargo xtask check-fixture-contracts
cargo xtask check-output-contracts
cargo xtask check-goals
```

## Artifacts

- `.ripr/goals/active.toml`
- `.ripr/goals/archive/2026-05-23-lane1-value-resolution-audit-fixes.toml`
- `docs/handoffs/2026-05-23-lane1-value-resolution-audit-fixes-closeout.md`
- `docs/handoffs/2026-05-22-value-resolution-audit-delta.md`
- `docs/handoffs/README.md`
- `docs/IMPLEMENTATION_CAMPAIGNS.md`
- `docs/IMPLEMENTATION_PLAN.md`
- `docs/lanes/LANE_1_VALUE_RESOLUTION_AUDIT_FIXES.md`
- `plans/lane1-value-resolution-audit-fixes/implementation-plan.md`
- `fixtures/evidence-quality-benchmark/corpus.json`
- `fixtures/finding-alignment-dogfood/corpus.json`

## Next Recommended Goal

No successor campaign is selected. The active manifest intentionally records
`no_current_goal = true`.

The next Lane 1 campaign should be selected from fresh repo-owned evidence. The
current scorecard still points at:

1. `analysis/related-test-ranking-audit-fixes` for
   `call_presence` / `activation_owner_call_unresolved`;
2. `analysis/value-resolution-audit-fixes` for the remaining unsupported
   `predicate_boundary` / `activation_value_unresolved` cases;
3. `analysis/static-limitation-taxonomy` for broader limitation categories.

Choose the next campaign by opening a fresh issue-backed rail, not by extending
this closed campaign.

## What Not To Do

- Do not keep adding value-resolution changes under this closed rail.
- Do not treat the remaining 315 `activation_value_unresolved` limitations as
  user test debt.
- Do not promote helper-built, cross-file, generated, macro-expanded, shadowed,
  pattern-bound, non-literal, or opaque value flows without a fixture-first
  successor.
- Do not claim runtime, mutation, coverage, gate, badge, or downstream
  projection movement from this rail.
