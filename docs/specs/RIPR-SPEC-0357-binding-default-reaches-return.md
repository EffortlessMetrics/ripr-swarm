# RIPR-SPEC-0357: A changed value-default binding that reaches the returned text

Status: proposed

Owner: product / analysis

Created: 2026-10-25

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #10259

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No output schema change.

## Problem

`let end = sku.find('-').unwrap_or(sku.len()); &sku[..end]` returns text that
depends on the default. When a diff rewrites that default (`unwrap_or_else(||
sku.len())`) ripr has no probe it can read: the changed line is a `let`, not
the returned expression, so the finding is `static_unknown` with a generic
reason, even when a test pins both the dashed and the undashed input. The
retained row `ledger-sku-family-end` is that case. Retargeting the probe to the
returned expression would be wrong: the owner-return pin would then credit any
exact assertion on the return, including one that never takes the default
branch.

## Behavior

### Relation (stage 1, no new credit)

A changed `let [mut] NAME = INIT;` in a function body has a **return flow**
when all of these hold:

1. After the `let`, `NAME` is not shadowed, reassigned, compound-assigned,
   mutably borrowed, captured by a closure, or named inside a macro.
2. The function's tail expression (or the operand of a `return` that no
   statement can bypass) is exactly one of: `NAME`; a reference to an
   indexed range bounded by `NAME` (`&EXPR[..NAME]`, `&EXPR[NAME..]`); or a
   `get`/`get_unchecked` call with a range bounded by `NAME`.
3. No statement between the `let` and the tail returns early or branches on a
   value derived from `NAME`.

The probe stays at the changed line and keeps its `static_unknown` class. The
finding carries the relation and says which of the three flows applies, so the
limit reads as "changed binding reaches the returned expression; its value is
not evaluated" instead of the generic reason. Any condition that is in doubt
keeps today's reading. This stage never adds a relation to a test, raises a
class, or strengthens an oracle.

### Credit (stage 2)

A return flow earns credit only when all of these also hold:

1. `INIT` is `RECEIVER.unwrap_or(D)` or `RECEIVER.unwrap_or_else(|| D)`, and
   `D` has no side effect (a literal, a parameter, `PARAM.len()`).
2. The flow is injective in the binding: the whole binding, or a range
   bound over a string or slice (a different bound returns a different value
   or panics).
3. A related test calls the owner with an input for which the value evaluator
   proves `RECEIVER` is `None`, so the default is taken, and asserts the
   owner's return with an exact equality on that same call.

An edit that changes the default is then noticed by that assertion. If the
default branch is never taken under an assertion (not called, or called with
its result discarded or unchecked), the finding keeps its stage 1 reading.
A rebound, shadowed or captured binding never reaches stage 2.

## Required Evidence

- Stage 1: the binding-to-return relation is recorded for the whole-binding and
  range-bound flows; each blocker in condition 1 and 3 keeps the generic reading.
- Stage 2: the credited case, plus one negative each for an unpinned default, a
  rebound binding, and a default that is called but not asserted.
- Corpus negatives (present before the relation lands): `default-flow-unpinned`,
  `default-flow-rebound`, `default-flow-smoked`.

## Known Limits

- Only `unwrap_or` and `unwrap_or_else`; `unwrap_or_default`, `map_or` and
  `match` defaults are not covered.
- One binding and one function; no flow through helper calls or struct fields.
- The value evaluator proves `None` only for the receivers it already
  evaluates (`find`, `rfind`, `strip_prefix`-style lookups on literals).

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- A general dataflow engine or taint analysis.

## Acceptance Examples

1. `family_unpinned` pins only `family_unpinned("BOLT-M8")`; the default branch
   never runs under an assertion, so editing the default earns no credit.
2. `family_rebound` reassigns the binding (`end = end.min(4)`) before the
   slice; the relation does not apply and no credit follows.
3. `family_smoked` calls `family_smoked("WASHER")` with the result discarded
   and pins only the dashed input; no credit follows.

## Test Mapping

- `fixtures/rust-verdict-corpus` cases `default-flow-unpinned`,
  `default-flow-rebound`, `default-flow-smoked`.
- Unit tests in `analysis/probes/binding_return.rs`, the seeding test in
  `analysis/probes/diff.rs` and the evidence test in
  `analysis/language/rust/probes.rs`.

## Implementation Mapping

- Stage 1: `crates/ripr/src/analysis/probes/binding_return.rs`
  (`resolve_changed_binding_return`), seeded from
  `analysis/probes/diff.rs` and disclosed by
  `analysis/language/rust/probes.rs` (`attach_changed_binding_return_evidence`).
- Stage 2: `crates/ripr/src/analysis/classify/value_transfer.rs` (planned).

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
