# RIPR-SPEC-0353: A pure bool `&&`/`||` owner is infected when its directly-called literal rows tell the changed chain from each one-step mutation

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

- #6488
- #9619

Linked PRs:

- None yet

Support-tier impact:

- No tier change. Rust static analysis stays advisory; this only stops one
  fully-read call shape from reading `infection_unknown`.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- No output schema change.

## Problem

`can_ship(paid, in_stock)` is `paid && in_stock`. An edit that reorders its
operands has no literal boundary, so the infection stage reads
`infection_unknown` ("no literal boundary was visible") even when the tests
call `can_ship(true, true)`, `can_ship(true, false)` and `can_ship(false,
true)`. For a bool chain the boundary is the truth table, and these calls
already pin it: the `||` swap, the constant `true`, the constant `false` and
a negated operand each disagree with the chain on one of those rows.

## Behavior

The infection stage reads `yes` for a changed predicate that has no literal
boundary when every item holds:

1. The owner's whole body is the changed chain: one `&&` chain or one `||`
   chain (never both) whose operands are bool parameters, each after any
   number of `!`. A comparison operand, call, field, cast, parenthesis or
   rebinding statement declines (the `ShortCircuitTail` grammar of
   RIPR-SPEC-0197 rule 3).
2. The edit keeps the connective sequence (`same_connectives`); an added,
   removed or re-connected operand declines.
3. No function other than the related tests calls the owner by name.
4. Every related test reaches the owner only by direct calls with plain
   boolean or decimal literal arguments, one owner call per line, with no
   table, builder, helper or user macro that could feed other inputs.
5. Every owner call's arguments are readable, and across the calls some row
   gives a different value for the chain than each of: the other connective,
   the constant `true`, the constant `false`, and the chain with any single
   operand negated.

Anything else keeps its existing reading. The rule decides only that the
rows reach and separate the mutations; it grants no oracle strength and no
propagation or reveal credit.

## Required Evidence

- Positive: `ships_only_when_paid_and_in_stock` (rows `(true,true)`,
  `(true,false)`, `(false,true)`) on `in_stock && paid` reads infection
  `yes`.
- Negatives: a single row, a test pinning only `(true, true)` and `(false,
  false)`, a literal-free call, a table-driven test, an owner with another
  caller, a comparison operand and an owner body with a leading statement
  each keep `infection_unknown`.

## Known Limits

- Table-driven and helper-fed rows (`for (a, b, want) in cases`) are not
  read here; they stay `infection_unknown`.
- The rows must separate the listed one-step mutations. Multi-step mutations
  are not claimed.
- Process success, oracle strength and propagation stay separate axes.

## Non-Goals

- Runtime mutation proof or sufficiency claims.
- Reading `if`/`match` bool logic, comparison chains or mixed operators.

## Acceptance Examples

1. `fn can_ship(paid: bool, in_stock: bool) -> bool { in_stock && paid }`
   with a test asserting `can_ship(true, true)`, `!can_ship(true, false)` and
   `!can_ship(false, true)` has infection `yes`.
2. The same owner pinned only at `can_ship(true, true)` and
   `can_ship(false, false)` keeps `infection_unknown`.
3. The same owner pinned by the three rows but also called by a wrapper in
   production code keeps `infection_unknown`.

## Test Mapping

- `crates/ripr/src/analysis/classify/owner_pin/short_circuit.rs`:
  `truth_table_rows_must_separate_every_one_step_mutant`.
- `crates/ripr/tests/bool_truth_table_infection.rs`.
- `fixtures/rust-verdict-corpus/cases/grid-bool-exact`.

## Implementation Mapping

- `crates/ripr/src/analysis/classify/infection.rs`:
  `boolean_truth_table_is_exercised`, `owner_literal_runs`.
- `crates/ripr/src/analysis/classify/owner_pin/short_circuit.rs`:
  `ShortCircuitTail::runs_tell_apart_one_step_mutants`.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
