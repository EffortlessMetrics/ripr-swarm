# Handoff: Value Resolution Audit Delta

Date: 2026-05-22

Branch: `report-value-resolution-audit-delta`

Current work item: `report/value-resolution-audit-delta`

## Current State

The Lane 1 Value Resolution Audit Fixes rail has fixture-backed the selected
same-test struct literal field projection sub-shape and dispositioned the
analysis slice without a production analyzer edit. The current value-resolution
path already resolves that shape when the concrete activation value is
statically visible and still keeps unsupported value flows named as static
limitations.

This handoff records the audit, scorecard, and trend state after that
disposition. It does not change analyzer behavior, output schemas, PR/CI
rendering, editor behavior, gates, badges, release behavior, generated tests,
source edits, provider calls, or mutation execution.

## Selected Sub-Shape

The selected supported shape is:

```text
same-test struct literal field projection
```

Supported:

- a test constructs a same-test struct literal with concrete literal fields;
- an owner call later passes a field projection such as `case.amount`;
- source-order and same-line column scoping make the value visible at the
  owner call;
- mutation or shadowing before the owner call prevents the value from being
  promoted.

Still unsupported and still named as limitations:

- helper-returned structs;
- non-literal fields;
- mutable bindings and mutations before the owner call;
- shadowed, pattern-bound, fixture, or parameterized values;
- cross-file constants;
- macros, generated inputs, and deeper semantic value resolution.

## Audit Snapshot

Current sampled Lane 1 audit:

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

The audit is bounded by the sampled repo exposure limit:

```text
repo_exposure_seam_limit = limit_5000_of_40233
```

This is a bounded work-queue signal, not a full-repo exposure completion claim.

## Predicate Boundary Queue

Current `predicate_boundary` alignment state:

| Metric | Count |
| --- | ---: |
| Raw signals | 975 |
| Canonical items | 601 |
| Actionable items | 61 |
| Already observed items | 222 |
| Static limitations | 318 |
| Unknown items | 0 |

The dominant named static limitation under `predicate_boundary` remains:

| Static limitation | Count | Repair route |
| --- | ---: | --- |
| `activation_value_unresolved` | 315 | `analysis/value-resolution-audit-fixes` |

## Delta

This report records a zero-movement sampled delta for the selected sub-shape.
The fixture-backed shape was already handled by existing value-resolution code
before the fixture corpus landed, so the disposition PR did not move sampled
scorecard totals.

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| Static limitations | 1,551 | 1,551 | 0 |
| Finding-alignment static limitations | 1,506 | 1,506 | 0 |
| Missing discriminators | 149 | 149 | 0 |
| `activation_value_unresolved` | 315 | 315 | 0 |

The trend report is currently `unknown` because no previous comparable
scorecard or audit snapshot was available to the trend command. It reports the
current `activation_value_unresolved` value as 315 but cannot claim
improvement or regression.

## What This Proves

- The selected fixture-backed sub-shape is not missing production support in
  the current analyzer.
- Unsupported value flows remain named static limitations rather than user test
  debt.
- The next useful work is proof-oriented dogfood: show the material
  value-resolution receipt path and keep the current zero-movement result
  visible.

## What This Does Not Prove

- no runtime adequacy;
- no mutation proof;
- no coverage adequacy;
- no merge approval;
- no default gate authority;
- no badge semantic change;
- no downstream PR/CI or editor projection change;
- no full-repo exposure completion beyond the sampled 5,000-seam run.

## Validation

Commands run for this slice:

```bash
cargo xtask lane1-evidence-audit
cargo xtask evidence-quality-scorecard
cargo xtask evidence-quality-trend
```

The previous disposition slice also validated:

```bash
cargo test -p ripr value_resolution --lib
cargo xtask goals next
cargo xtask check-goals
cargo xtask lane1-evidence-audit
cargo xtask evidence-quality-scorecard
cargo xtask evidence-quality-trend
cargo xtask check-output-contracts
cargo xtask check-static-language
cargo xtask check-pr
cargo xtask goals status
cargo xtask check-doc-index
cargo xtask markdown-links
git diff --check
```

## Next Work

`dogfood/value-resolution-receipts` is now the next ready work item. It should
record material value-resolution dogfood receipts with canonical gap identity,
raw finding context, before/after limitation movement or no-movement state,
repair route or limitation state, verify command where applicable, and explicit
static-evidence non-claims.
