# RIPR-SPEC-0249: TypeScript instance class pin

Status: proposed

Owner: product / analysis

Created: 2026-10-12

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6654 (TypeScript corpus cases scored false after RIPR-SPEC-0243)

Linked PRs:

- #6686 (TypeScript verdict corpus)
- #7419 (RIPR-SPEC-0248)

Support-tier impact:

- No tier change. TypeScript stays Preview.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Adds `toBeInstanceOf` to the RIPR-SPEC-0027 matcher list.
- Rust is not changed.

## Problem

The TypeScript verdict corpus reads `jest-mined-tobeinstanceof` as a false
gap:

```ts
// src/errors.ts
export class NotFound extends Error {}
export class Forbidden extends Error {}

export function errorFor(status: number): Error {
  if (status === 404) {          // before
  if (404 === status) {          // after
    return new NotFound("missing");
  }
  return new Forbidden("denied");
}

// test/errors.test.ts
expect(errorFor(404)).toBeInstanceOf(NotFound);
expect(errorFor(403)).toBeInstanceOf(Forbidden);
```

Every mutant of the comparison fails the test: a `Forbidden` is not a
`NotFound`, so sending 404 down the other path fails the first assertion
and sending 403 down it fails the second. ripr read `toBeInstanceOf` as an
unrecognised matcher, so the finding was `weakly_exposed` with
`typescript_custom_matcher_unresolved`.

## Behavior

**Matcher reading.** `expect(x).toBeInstanceOf(C)` (and
`.resolves.toBeInstanceOf(C)`) is a `relational_check` / weak oracle. It
passes for an instance of `C` or of any subclass, so on its own it is not an
exact discriminator. When `C` is one PascalCase identifier the assertion
records it as its expected value. `.rejects.toBeInstanceOf(C)` observes the
error, not the returned value, and records none.

**Rule 1. A class pin reads exact for a predicate change whose paths return
different classes.** On a `predicate` probe, a `toBeInstanceOf(C)`
assertion of an oracle relation reads `exact_value` / strong, both in the
ladder and in its related-test row, when all of these hold:

- it observes the whole result of one owner call: `owner(...)`,
  `ns.owner(...)`, optionally awaited, or a local bound once to that call
  (not `f(owner(...))` or `owner(...).field`), and the test does not
  shadow the owner;
- the owner's module declares `class C`;
- every `return` in the owner body is `return new X(...)`, the body has no
  arrow or nested function, and the returned classes include `C` and at
  least one other class;
- each other returned class can never be a `C`: walking its `extends`
  chain through classes the module declares reaches a builtin (`Error`,
  `RangeError`, `Map`, ...) or a class with no `extends` without meeting
  `C`. An imported class, a mixin or member parent (`extends mix(C)`,
  `extends ns.Base`) and a generic class header are not read, and fail the
  rule.

The predicate boundary witness (RIPR-SPEC-0027) then runs as for any
strong assertion: the pin must observe an owner call at the changed
boundary.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Recorded on #6654.

1. **Predicate changes only.** Adopted: the pin tells the branches apart
   only when the change chooses the branch. A change to a returned value
   (`new Forbidden("denied")` to `new Forbidden("refused")`) keeps the
   class, so the pin passes on both versions. Rejected: reading the pin
   strong everywhere, which credits that message change.
2. **Weak, not unknown, by default.** Adopted: `toBeInstanceOf` is a
   recognised matcher with known semantics (instance-of, subclass
   included), the same footing as `toHaveProperty`. Its default reading
   moves from `unknown` to `relational_check` / weak and the test's
   assertion admission from unresolved to recognised; the classification
   of a change it does not pin stays `weakly_exposed`.
3. **Kind `exact_value`.** Adopted: no new oracle kind; the credited pin
   fixes which value the owner returns among the classes it can return.
   Rejected: `exact_error_variant`, which value seams exclude
   (RIPR-SPEC-0104).

### Known limits

- The rule does not ask which return each branch reaches. An owner that
  returns `C` on both sides of the changed predicate and a different class
  on a third path still reads the pin strong.
- `typescript_oracle_confidence` in the oracle metadata evidence reads the
  assertion's default (weak) strength, so a credited pin shows `low`.
- Owners whose returns are not all `new X(...)` (a cached instance, a
  factory call, a ternary) keep the weak reading.

## Required Evidence

- On the TypeScript verdict corpus, `jest-mined-tobeinstanceof` moves from
  `gap` to `credited` (false verdicts 9/101 to 8/101 on top of
  RIPR-SPEC-0248) and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No `toBeInstanceOf` reading on error, side-effect or value seams.
- No cross-module class hierarchy reading.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. The problem's change and tests: `exposed`; the related row reads
   `exact_value` / strong.
2. As example 1 with `const missing = errorFor(404);
   expect(missing).toBeInstanceOf(NotFound);`: `exposed`.
3. As example 1 with `class Forbidden extends NotFound`: not `exposed`.
4. As example 1 asserting `toBeInstanceOf(Error)` twice: not `exposed`.
5. As example 1 with `expect(normalize(errorFor(404)))` or
   `expect(new NotFound("x"))`: not `exposed`.
6. As example 1 with `.rejects.toBeInstanceOf(...)`: not `exposed`.
7. `return new Forbidden("denied")` changed to
   `return new Forbidden("refused")` with the problem's tests: not
   `exposed`, and rule 1 reads no pin on that value seam.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/instance_class_tests.rs`
  pins examples 1 to 7 and the owner-side conditions.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `jest-mined-tobeinstanceof` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `oracle_for_matcher`, `instance_class_argument`.
- `crates/ripr/src/analysis/language/typescript/instance_class_guard.rs`:
  `ts_owner_returns_distinct_classes`, `ts_instance_class_facts`.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `strongest_family_matching_oracle`, `ts_predicate_boundary_row_activation`,
  `project_credited_instance_rows`, `ts_observes_whole_owner_result`.

## Metrics

- `typescript_instance_pin_mismatch`: acceptance examples whose observed
  reading differs from this spec. Target: zero.
