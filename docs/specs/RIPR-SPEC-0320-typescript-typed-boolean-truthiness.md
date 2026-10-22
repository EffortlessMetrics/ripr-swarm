# RIPR-SPEC-0320: TypeScript typed boolean truthiness

Status: proposed

Owner: product / analysis

Created: 2026-10-22

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #9532 (bare boolean assert on a boolean predicate owner reads as smoke)
- #9271 (TypeScript operator grid)

Linked PRs:

- #9499 (operator-grid cases for boundary, equality and logical)

Support-tier impact:

- No tier change. TypeScript stays Preview.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Extends the typed owner amendment of RIPR-SPEC-0247 rule 2 (an owner
  declared `number` pins a loose number literal) to truthiness checks on an
  owner declared `boolean`.
- Rust and Python are not changed.

## Problem

The operator grid (#9499) found three discriminated cases that read as a
false `weakly_exposed` gap:

```ts
// src/boundary.ts
export function isAdult(age: number): boolean {
  return age >= 18;             // before
  return 18 <= age;             // after
}

// test/boundary.test.ts
assert.ok(isAdult(18));
assert.ok(!isAdult(17));
```

The diff is a behavior-preserving rewrite; the corpus truth comes from
mutants such as `age > 18` and `age >= 19`, which fail those two assertions.
`assert.ok` is a truthiness check, so RIPR-SPEC-0243 reads it
`smoke_only`, and ripr asks the user for an exact-value assertion that
already exists. The same shape appears for `===` equality
(`assert.ok(isAdmin("admin"))`) and for `&&` (`assert.ok(canShip(true, true))`).

## Behavior

**Rule 1. A truthiness check on the whole result of a `boolean` owner is an
exact pin.** An assertion pins the owner's result as `=== true` (or
`=== false` under one leading `!`) when all of these hold:

- the matcher is a pure truthiness check: `assert.ok` / `assert(value)`,
  chai `assert.isTrue` / `assert.isFalse`, or Jest/Vitest `toBeTruthy()` /
  `toBeFalsy()` (read `smoke_only` today);
- the owner is a function or arrow function whose declared return type is
  exactly `boolean` (no `async`, generator, generic list, union, `any`,
  type predicate or missing annotation);
- the observed expression is one call to the owner, optionally awaited and
  optionally under one `!`, reached through a binding the test imports from
  the owner's module, as for the typed loose-number amendment;
- every argument of that call is a number, string, boolean or `null`
  literal.

A credited assertion is treated as an exact-value oracle by the boundary
witness, which still decides whether its inputs reach the changed boundary.
Unit tests pin the refusals no corpus case labels: an owner declared
`number`, `any`, `boolean | undefined` or not annotated keeps the smoke
reading, and so do `!!call` and a call wrapped in another call.

Everything else keeps its reading: an argument that is a variable or an
expression (a property check over `x, x` pins one side of the boundary), a
method owner, an owner without the exact annotation, `!!call`, and a call
wrapped in another call.

### Decisions

Recorded under the owner's standing instruction to make reasonable
documented decisions (2026-10-04).

1. **Literal arguments only.** Adopted: with a loop variable, the
   assertion can be true for every input the loop reaches while the mutant
   `return true` survives (corpus cases `ts-grid-equality-property` and
   `ts-grid-logical-property`). Rejected: crediting any argument and
   leaving the boundary witness to sort it out, which moved both of those
   rows from `gap` to a false `credited`.
2. **Declared type, not inferred.** Adopted: the same annotation test the
   loose-number amendment uses. Rejected: inferring `boolean` from a
   comparison body, which would credit an owner whose annotation widens
   later.

### Known limits

- An owner without a `: boolean` annotation keeps the smoke reading
  (under-credit).
- A truthiness check on a local bound to the owner call, or through a
  helper such as `checkAdult(18)`, is not read (under-credit).
- `assert.equal(isAdult(18), true)` is already exact and unchanged.

## Required Evidence

- On the TypeScript verdict corpus `ts-grid-boundary-exact`,
  `ts-grid-equality-exact` and `ts-grid-logical-exact` move from `gap` to
  `credited` and no other row moves.
- `cargo test -p ripr --lib typed_boolean` passes.

## Non-Goals

- No change to Python, Rust or other languages.
- No change to the table-loop boundary reading (#9533).
- No inference of a return type that is not written in the source.

## Acceptance Examples

1. `assert.ok(isAdult(18)); assert.ok(!isAdult(17));` against
   `isAdult(age: number): boolean`: both assertions are exact pins; the
   corpus case `ts-grid-boundary-exact` reads `credited`.
2. `assert.ok(isAdult(age))` inside a loop over `age`: no pin; the
   assertion stays `smoke_only`.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/typed_boolean_assert_tests.rs`:
  `boolean_return_annotation_reads_only_an_exact_boolean`,
  `truthiness_on_a_boolean_owner_with_literal_arguments_is_credited` pins
  examples 1 and 2, and
  `truthiness_stays_smoke_unless_the_owner_declares_boolean` pins the
  annotation rule.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `ts-grid-boundary-exact`, `ts-grid-equality-exact` and
  `ts-grid-logical-exact` read `credited`; `ts-grid-equality-property` and
  `ts-grid-logical-property` stay `gap`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/typed_boolean_assert_guard.rs`:
  `ts_typed_boolean_assert_facts`.
- `crates/ripr/src/analysis/language/typescript/typed_loose_equality_guard.rs`:
  `ts_typed_owner_pin_facts` (shared with the loose-number amendment) and
  `ts_owner_declares_return_type`.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`: the pin
  facts joined before `project_credited_pin_rows`.

## Metrics

- `typescript_typed_boolean_truthiness_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
