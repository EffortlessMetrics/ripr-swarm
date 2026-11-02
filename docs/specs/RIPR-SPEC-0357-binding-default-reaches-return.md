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

The probe stays at the changed line. The finding carries the relation and says
which of the three flows applies. A rebound, shadowed, intervening, or
transformed tail keeps today's `static_unknown` reading. Stage 1 by itself
does not add a relation to a test or strengthen an oracle.

### Whole-binding evaluation (RIPR-SPEC-0096 example 2)

When the return flow is the **whole binding** (`tail` is exactly `NAME`) and
the initializer is one evaluable expression over the owner's parameters
(inlining at most unique same-file free helpers whose bodies are one
arithmetic expression), related tests that compare the owner call to a
literal are evaluated under the old initializer and the new one. An
assertion that holds before and fails after is `exposed`; every related
assertion holding both ways is `weakly_exposed`; anything unreadable stays
`static_unknown`. A reassignment, shadow, partial use, transformed return
(`NAME + 1`, a method, a range bound), a `return`/`break`/`?` before the
changed `let`, a foreign same-name import of the owner, or a renamed
binding is not evaluated.

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
   has no side effect (a literal, a parameter, `PARAM.len()`). The change is
   either to `D` alone or a rewrite of the combinator that leaves `RECEIVER`
   and `D` text-identical; an edit to `RECEIVER` is not credited.
2. The flow is injective in the binding: the whole binding, or a range
   bound over a string or slice with a fixed base (a different in-range bound
   returns a different value; an out-of-range bound is detected by a safe
   slice, and is not claimed for `get_unchecked`).
3. Related tests call the owner with inputs for which the value evaluator
   (`classify/value_transfer.rs` is the only authority) proves `RECEIVER` is
   `None` for one and `Some` for another, each asserting the owner's return
   with an exact equality on that same call. A change to `D` alone needs only
   the `None` case.

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
  `unwrap_or_else(|| D)`) and leaves `D` and the receiver untouched, so a
  rule limited to an edit of `D` would never credit it. Because the line's
  value is then what the tests must notice, credit condition 3 requires both
  branches: one exact equality on the owner call where `RECEIVER` is `None`
  and one where it is `Some`.

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
- Whole-binding evaluation: `analysis/classify/binding_return_flow/tests.rs`
  and corpus case `spec0096-named-binding-used`.

## Implementation Mapping

- Stage 1: `crates/ripr/src/analysis/probes/binding_return.rs`
  (`resolve_changed_binding_return`), seeded from
  `analysis/probes/diff.rs` and disclosed by
  `analysis/language/rust/probes.rs` (`attach_changed_binding_return_evidence`).
- Whole-binding evaluation: `crates/ripr/src/analysis/classify/binding_return_flow.rs`
  (`evaluate_binding_return_flow`), applied from `classifier.rs`.
- Stage 2: not implemented (see Credit).

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
