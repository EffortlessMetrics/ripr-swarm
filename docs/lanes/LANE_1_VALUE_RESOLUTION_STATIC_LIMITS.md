# Lane 1: Value Resolution Static Limits

Status: active

Opened: 2026-05-22

Issue: [swarm #285](https://github.com/EffortlessMetrics/ripr-swarm/issues/285)

## Goal

Burn down the next measured Lane 1 static-limitation bucket without reopening
closed release, editor, PR/CI, gate, or policy work.

The selected evidence queue is:

```text
predicate_boundary
-> activation_value_unresolved
-> analysis/value-resolution-audit-fixes
```

This tracker starts from repo-owned evidence. The closed finding-alignment
burn-down says future Lane 1 work must be selected from a fresh audit,
scorecard, dogfood receipt, downstream consumer issue, or spec-backed
campaign. The current closeout records `activation_value_unresolved` as the
largest remaining named limitation bucket, and the scorecard repair route for
that bucket is `analysis/value-resolution-audit-fixes`.

## Boundary

Lane 1 owns the analyzer evidence truth for this work:

- value-resolution fact quality;
- activation value identity;
- `predicate_boundary` missing discriminator evidence;
- `activation_value_unresolved` limitation classification;
- raw-finding to canonical-item alignment;
- fixture-backed movement out of named static limitations;
- audit and scorecard before/after proof.

This rail does not own downstream rendering. PR/CI, editor, policy, and
adoption lanes should consume the resulting canonical evidence after it lands.
They must not infer actionability from raw findings or from unresolved value
limitations.

## Current Baseline

The closed Lane 1 Finding Alignment Burn-Down recorded these relevant signals:

| Signal | Closeout value |
| --- | ---: |
| Live raw alignment signals | 47,626 |
| Live canonical evidence items | 38,564 |
| Live actionable canonical gaps | 162 |
| Live named static limitations | 26,250 |
| Top remaining named limitation bucket | `activation_value_unresolved` at 25,881 |

Earlier burn-down slices moved supported owner-call cases out of
`activation_value_unresolved`, but predicate-boundary value checks still require
concrete discriminator evidence. This campaign must not invent observed
activation values just to reduce a count.

## Source-Of-Truth Stack

- scorecard contract:
  [RIPR-SPEC-0034: Evidence Quality Scorecard](../specs/RIPR-SPEC-0034-evidence-quality-scorecard.md);
- alignment contract:
  [RIPR-SPEC-0045: Finding-To-Gap Alignment](../specs/RIPR-SPEC-0045-finding-to-gap-alignment.md);
- predecessor closeout:
  [Lane 1 Finding Alignment Burn-Down Closeout](../handoffs/2026-05-22-lane1-finding-alignment-burndown-closeout.md);
- predecessor tracker:
  [Lane 1 Finding Alignment Burn-Down](LANE_1_FINDING_ALIGNMENT_BURNDOWN.md);
- implementation rail:
  [Lane 1 Value Resolution Static Limits plan](../../plans/lane1-value-resolution-static-limits/implementation-plan.md);
- active manifest:
  [`.ripr/goals/active.toml`](../../.ripr/goals/active.toml).

## Burn-Down Queue

| Order | Issue | Slice | Intent | Status |
| ---: | --- | --- | --- | --- |
| 1 | [swarm #285](https://github.com/EffortlessMetrics/ripr-swarm/issues/285) | `fixtures/value-resolution-audit-plan` | Select the first supported value-resolution sub-shape and fixture/audit plan before analyzer behavior changes. | ready |
| 2 | future issue | `fixtures/value-resolution-benchmark-cases` | Add positive and must-not-claim benchmark cases for the selected supported shape. | blocked by slice 1 |
| 3 | future issue | `analysis/value-resolution-audit-fixes` | Move only fixture-backed supported cases out of `activation_value_unresolved`. | blocked by slice 2 |
| 4 | future issue | `campaign/value-resolution-static-limits-closeout` | Record movement, unsupported cases, and the next audit-driven selection rule. | blocked by slice 3 |

## Operating Rules

- Start from scorecard and audit data, not screenshots or anecdote.
- Fixture planning comes before analyzer behavior.
- Positive and must-not-claim cases travel together.
- Raw findings remain supporting evidence.
- Canonical items remain the countable unit.
- Unsupported value-resolution shapes remain named static limitations.
- Do not invent observed activation values.
- Predicate-boundary value checks require concrete discriminator evidence.
- Runtime-only signal does not create a static gap.

## Non-Goals

- No PR or CI rendering changes.
- No inline PR comment publishing.
- No LSP or editor polish.
- No gate-policy changes or default blocking.
- No public badge or score redefinition.
- No generated tests.
- No automatic source edits.
- No provider or model calls.
- No mutation execution.
- No release, packaging, platform, dependency, or MSRV cleanup.

## Validation Gates

Docs and planning slices should run:

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

Fixture slices should also run:

```bash
cargo test -p xtask evidence_quality_benchmark
cargo xtask check-fixture-contracts
cargo xtask check-output-contracts
```

Analyzer slices should also run:

```bash
cargo xtask lane1-evidence-audit
cargo xtask evidence-quality-scorecard
cargo xtask check-output-contracts
```
