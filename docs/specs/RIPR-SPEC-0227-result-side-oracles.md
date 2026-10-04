# RIPR-SPEC-0227: Result-side oracles (`is_err`, `is_ok`, `unwrap_err`, `should_panic`)

Status: proposed

Owner: product / analysis

Created: 2026-10-04

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- None yet

Linked PRs:

- #5333 (do not confirm ErrorPath from operand error lexemes)
- #5416 (unknown, not a gap: a weak oracle ripr read keeps the gap)

Support-tier impact:

- No tier change. Oracles that observe only which side of a `Result` a call
  returned are named as one oracle class with one rule: they never confirm a
  changed error variant or a changed `Ok` value, and they confirm a change
  that flips the side only when the same test feeds an input on which the
  side flips. Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. Oracle kinds stay `broad_error` and `smoke_only`;
  this spec names their contract.

## Problem

Tests often check only whether a call failed:

```rust
assert!(check(300).is_err());
assert!(matches!(check(300), Err(_)));
let _e = check(300).unwrap_err();
#[should_panic] fn rejects() { check(300).unwrap(); }
```

Each observes the side of the `Result`, not its contents. ripr reads them
inconsistently today (origin/main, measured with `ripr check --diff`):

- `is_err()` and `Err(_)` are `broad_error` / weak; `is_ok()`, `unwrap()` and
  `expect()` are `smoke_only` / smoke (`analysis/extract/oracles/classify.rs`).
- A bare `let e = f().unwrap_err();` is not an assertion line
  (`analysis/extract/oracles/scan.rs`), so it is no oracle at all and the
  finding reads `reachable_unrevealed`.
- `#[should_panic]` is never read.

RIPR-SPEC-0107 covers one shape: a changed error variant with an `is_err()`
test reads `weakly_exposed`. No spec covers `Err(_)`, `unwrap_err()`,
`should_panic` or `is_ok()`, and none covers the case where a broad oracle is
a real discriminator: a guard that decides between `Ok` and `Err`.

```rust
pub fn check(len: usize) -> Result<usize, E> {
    if len >= 256 { return Err(E::TooLong); }   // changed from `len > 256`
    Ok(len)
}

#[test]
fn boundary() {
    assert!(check(256).is_err());
    assert!(check(255).is_ok());
}
```

The mutant back to `len > 256` flips `check(256)` from `Err` to `Ok`, and
this test fails. ripr
reads the predicate `weakly_exposed` ("Only broad error oracle"), a false
actionable gap. The "Fill spec-defined corpus cases" ledger crate also
reports both of its error-path findings `exposed` when the only test asserts
`is_err()` and a wrong variant passes; that observation conflicts with the
`weakly_exposed` reading above and is a defect against this spec either way.

## Behavior

### Result-side oracles

A **result-side oracle** observes only whether an owner call returned `Ok`
or `Err` (or `Some` or `None`):

- `assert!(r.is_err())`, `assert!(r.is_ok())`, `assert!(!r.is_ok())` and the
  `Option` twins;
- `assert!(matches!(r, Err(_)))`, `assert!(matches!(r, Ok(_)))`;
- `r.unwrap_err()` or `r.expect_err(..)` whose value is never compared;
- `r.unwrap()` or `r.expect(..)` whose value is never compared;
- `#[should_panic]` without `expected`, around a body whose only panic source
  on the owner result is `unwrap()` or `expect()`.

`#[should_panic(expected = "..")]` with a message is not a result-side oracle;
it belongs to message-bound authorities.

Kinds keep their existing names: the `Err` side reads `broad_error`, the `Ok`
side reads `smoke_only`. Both are weak for every case below except rule 3.
A bare `unwrap_err()` and `should_panic` produce the oracle instead of nothing.

### Rule 1: variants and values

A result-side oracle never confirms a changed error variant
(`Err(E::A)` to `Err(E::B)`), a changed `Ok` or `Some` value, or a changed
payload. The finding reads at most `weakly_exposed`, and the gap is kept:
RIPR-SPEC-0221 does not withhold a gap whose only oracle is result-side,
because ripr read the oracle and it is weak. An `exposed` reading in these
shapes is a defect.

### Rule 2: far inputs

A result-side oracle on an input that does not sit where the side flips
(`check(300).is_err()` for the guard above) reads at most `weakly_exposed`.

### Rule 3: side flips (proposal)

A result-side oracle confirms a change when all of these hold:

1. the changed expression decides the side: a predicate guarding a
   `return Err(..)` / `return Ok(..)` (or `None` / `Some`), or an added or
   removed `?`;
2. the same test calls the owner with a resolved input on which the side
   differs between the original and the changed code: the boundary inputs of
   RIPR-SPEC-0186 for a predicate, or an input reaching the `?` call's error
   for `?`;
3. the oracle on that call observes the side.

Then discrimination is confirmed and the finding may read `exposed`. When
the input is unresolved or the side flip cannot be established, rule 2
applies.

### Decisions for the owner

1. **Rule 3 credit.** Recommended: adopt it. Without it, the common
   `is_err`/`is_ok` boundary test stays a false gap. Alternative: keep every
   result-side oracle weak.
2. **Bare `unwrap_err()` and `should_panic`.** Recommended: read them as
   result-side oracles (weak), moving their findings from
   `reachable_unrevealed` to `weakly_exposed` and letting rule 3 apply.
   Alternative: keep them unread.

## Required Evidence

- A variant change with an `is_err()` test, a `matches!(r, Err(_))` test, a
  bare `unwrap_err()` test and a `should_panic` test each reads
  `weakly_exposed` for `error_path` and `return_value`, never `exposed`.
- The boundary reproduction above reads `exposed` for the predicate.
- A far-input-only control (`check(300).is_err()`) reads `weakly_exposed`.
- A split control (`let _ = check(256)` in one test, `check(300).is_err()` in
  another) reads at most `weakly_exposed` (RIPR-SPEC-0186).
- `Ok(len)` changed to `Ok(len + 1)` with an `is_ok()` test reads
  `weakly_exposed`.

## Non-Goals

- No credit for message text in `should_panic(expected = ..)`.
- No change to exact-variant authorities (RIPR-SPEC-0106, 0107, 0175, 0197).
- No change to how a `?` probe is confirmed by an exact `Err(E::X)`
  assertion; that under-credit is tracked separately.

## Acceptance Examples

Source: `check` as in Problem.

1. Guard changed to `len >= 256` (from `len > 256`), test
   `assert!(check(256).is_err()); assert!(check(255).is_ok());`: predicate
   `exposed` (rule 3).
2. Same change, test `assert!(check(256).is_err())` alone: predicate `exposed`.
3. Same change, test `assert!(check(300).is_err())` only: `weakly_exposed`.
4. Same change, tests `let _ = check(256);` and `assert!(check(300).is_err())`
   in different tests: at most `weakly_exposed`.
5. `Err(E::TooLong)` changed to `Err(E::Bad)`, test
   `assert!(check(300).is_err())`: `error_path` and `return_value`
   `weakly_exposed`, gap kept under RIPR-SPEC-0221.
6. Same change, `#[should_panic] fn t() { check(300).unwrap(); }`:
   `weakly_exposed`.
7. Same change, `let _e = check(300).unwrap_err();`: `weakly_exposed`.
8. `Ok(len)` changed to `Ok(len + 1)`, test `assert!(check(3).is_ok())`:
   `weakly_exposed`.
9. `let d = digit(c)?;` added in `total`, test `assert!(total("x").is_err())`
   where `digit('x')` errors: `exposed` (rule 3).

## Test Mapping

- Existing: `fixtures/weak_error_oracle`, `fixtures/unwrap_err_generic_is_err`,
  `fixtures/strong_error_oracle`.
- Planned: one fixture or verdict-corpus case per acceptance example.
- Planned: oracle-scan unit tests for bare `unwrap_err()` and `should_panic`.

## Implementation Mapping

- `crates/ripr/src/analysis/extract/oracles/scan.rs` and `classify.rs`: read
  bare `unwrap_err()` and `should_panic` as result-side oracles.
- `crates/ripr/src/analysis/classify/boundary_pairing.rs`: side-flip pairing
  for rule 3.
- `crates/ripr/src/analysis/classify/reveal.rs`: rule 1 and rule 3 gates.

## Metrics

- `result_side_oracle_variant_credit`: findings confirmed by a result-side
  oracle on a variant or value change; must be zero.
- `result_side_oracle_side_flip_credit`: predicate and `?` findings confirmed
  by a same-test side flip.
