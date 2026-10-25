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

A changed single-line `let [mut] NAME = INIT;` (one statement, ending in `;`,
sitting directly in the function's body block) has a **return flow** when all
of these hold:

1. After the `let`, no statement mentions `NAME` except the tail. That rules
   out a shadow, a reassignment, a borrow, a closure capture and a macro use.
2. The function's tail expression is exactly one of: `NAME`; `&PATH[..NAME]`
   or `&PATH[NAME..]`; `PATH.get(..NAME)` or `PATH.get_unchecked(..NAME)`
   (also the `NAME..` start form), where `PATH` is an identifier or field path.
   A single-line `unsafe { ... }` around the tail is read through.
3. No statement between the `let` and the tail contains `return`, `break` or
   `?`, and the statement before the tail is terminated (`;`, `{` or `}`), so
   the tail is not the continuation of a longer expression.

The probe stays at the changed line and keeps its `static_unknown` class. The
finding carries the relation and says which of the three flows applies, so the
limit reads as "changed binding reaches the returned expression; its value is
not evaluated" instead of the generic reason. Any condition that is in doubt
keeps today's reading. This stage never adds a relation to a test, raises a
class, or strengthens an oracle.

### Credit (stage 2, not implemented)

Stage 2 is deferred (#10259). Retargeting the probe to the returned
expression was prototyped and rejected: the tail slice reads
`propagation_unknown` because the direct-sink propagation witness is built
from a changed returned expression, and the three negatives read as silent.
Credit needs its own digest-validated witness kind for the binding-to-return
flow, built where `classifier/evidence.rs` builds the others. Until then the
retained row `ledger-sku-family-end` stays `limited`, an acceptable verdict.
The rules below are the design that witness must satisfy, not behavior that
ships.

A return flow would earn credit only when all of these also hold:

1. `INIT` is `RECEIVER.unwrap_or(D)` or `RECEIVER.unwrap_or_else(|| D)`, `D`
   has no side effect (a literal, a parameter, `PARAM.len()`), and the changed
   span of `INIT` is exactly `D`: an edit to `RECEIVER` is not credited.
2. The flow is injective in the binding: the whole binding, or a range
   bound over a string or slice with a fixed base (a different in-range bound
   returns a different value; an out-of-range bound is detected by a safe
   slice, and is not claimed for `get_unchecked`).
3. A related test calls the owner with an input for which the value evaluator
   (`classify/value_transfer.rs` is the only authority) proves `RECEIVER` is `None`, so the default is taken, and asserts the
   owner's return with an exact equality on that same call.

An edit that changes the default is then noticed by that assertion. If the
default branch is never taken under an assertion (not called, or called with
its result discarded or unchecked), the finding keeps its stage 1 reading.
A rebound, shadowed or captured binding never reaches stage 2.

## Required Evidence

- Stage 1: the binding-to-return relation is recorded for the whole-binding and
  range-bound flows; each blocker in conditions 1 and 3 (rebound, shadowed, intervening use, each early-exit form, unterminated previous line) keeps the generic reading.
- Stage 2 (when built): the credited case, plus one negative each for an unpinned default, a
  rebound binding, and a default that is called but not asserted.
- Corpus negatives (present before the relation lands): `default-flow-unpinned`,
  `default-flow-rebound`, `default-flow-smoked`.

## Known Limits

- Only `unwrap_or` and `unwrap_or_else`; `unwrap_or_default`, `map_or` and
  `match` defaults are not covered.
- One binding and one function; no flow through helper calls or struct fields.
- The value evaluator proves `None` only for the receivers it already
  evaluates (`find`, `rfind`, `strip_prefix`-style lookups on literals).

- The retained row rewrites the combinator (`unwrap_or(D)` to
  `unwrap_or_else(|| D)`) and leaves `D` and the receiver untouched, so a rule
  that requires the changed span to be exactly `D` would never credit it. A
  stage 2 design must instead require both branches (`None` and `Some`) to be
  pinned by exact equalities on the owner call.

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
- Stage 2: not implemented (see Credit).

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
