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
- Adds `toBeInstanceOf` to the RIPR-SPEC-0243 matcher table
  (`relational_check` / weak).
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
exact discriminator. When the argument is one identifier that starts with
an uppercase letter, the assertion records it as its expected value, so it
no longer carries `typescript_dynamic_assertion_unresolved` or
`typescript_custom_matcher_unresolved`, its test's assertion admission is
`recognised`, its oracle metadata names the class
(`typescript_oracle_expected: C`) and its `typescript_oracle_confidence` is
`low`. `.rejects.toBeInstanceOf(C)` observes the error, not the returned
value, and records no class; a lowercase or member argument
(`errors.NotFound`) records none either.

**Rule 1. A class pin reads exact for a predicate change whose branches
return different classes.** On a `predicate` probe, a `toBeInstanceOf(C)`
assertion of an oracle relation reads `exact_value` / strong, both in the
ladder and in its related-test row, when all of these hold:

- it observes the whole result of one owner call: `owner(...)`,
  `ns.owner(...)`, optionally awaited, or a local bound once to that call
  (not `f(owner(...))` or `owner(...).field`), and the test does not
  shadow the owner;
- the owner body has no arrow or nested `function`, and the changed line
  is an `if (...) {` found once in the owner that ends with `{` and leaves
  exactly one more brace open than it closes (a one-line
  `if (c) { return a; }` fails), and the
  RIPR-SPEC-0027 branch scan reads its true branch and its false branch
  (an `else` block or the fall-through) as a single `return` each, with
  string and comment text blanked first; another statement in a branch,
  an `else if`, a fall-through that is not a `return`, or a comment that
  spans lines fails the scan;
- each branch returns exactly `new X(...)`, with nothing after the
  argument list (`new X().as()` and `new X() || y` fail);
- one branch returns `C`, the other returns a different class `D`, and the
  owner's module declares `class C`;
- `D` can never be a `C`: walking its `extends` chain through classes the
  module declares reaches a class with no `extends`, or a name in the
  builtin list below that the module does not itself declare, without
  meeting `C`.

The module scan reads every `class Name` declaration; a module with any
generic (`class A<T>`), anonymous (`export default class {`) or repeated
class declaration is not read, and a mixin or member parent
(`extends mix(C)`, `extends ns.Base`) or an imported class stops the walk.
Builtins: `Error`, `TypeError`, `RangeError`, `SyntaxError`,
`ReferenceError`, `EvalError`, `URIError`, `AggregateError`, `Object`,
`Array`, `Map`, `Set`, `WeakMap`, `WeakSet`, `Date`, `RegExp`, `Promise`.

A credited pin is identified by its test file, line and observed
expression, so a pin that shares a line with a credited one does not
inherit its credit. The predicate boundary witness (RIPR-SPEC-0027) then
runs as for any strong assertion: the pin must observe an owner call at
the changed boundary.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Recorded on #6654.

1. **Predicate changes only.** Adopted: the pin tells the branches apart
   only when the change chooses the branch. A change to a returned value
   (`new Forbidden("denied")` to `new Forbidden("refused")`) keeps the
   class, so the pin passes on both versions. Rejected: reading the pin
   strong everywhere, which credits that message change.
2. **The changed `if`'s own branches, not every return.** Adopted: an
   independent review showed that an owner returning `C` on both sides of
   the changed predicate and another class on a third path would be
   credited by a whole-body reading. Rejected: requiring every `return` of
   the owner to construct a class, which also counted object-method
   returns and `new X().as()` tails.
3. **Weak, not unknown, by default.** Adopted: `toBeInstanceOf` is a
   recognised matcher with known semantics (instance-of, subclass
   included), on the same footing as `toHaveProperty` in the
   RIPR-SPEC-0243 matcher table. The classification of a change it does
   not pin stays `weakly_exposed`.
4. **Kind `exact_value`.** Adopted: no new oracle kind; the credited pin
   fixes which of the two branch values the owner returns. Rejected:
   `exact_error_variant`, which value seams exclude (RIPR-SPEC-0104).

### Known limits

- The asserted class is matched by name. The test's `C` is not resolved to
  the owner module's binding, so a test-local `const NotFound = Error`
  imported under that name could be credited.
- A renamed owner import (`import { errorFor as ef }`) is not read, and
  its pins keep the weak reading.
- `typescript_oracle_confidence` in the oracle metadata evidence reads the
  assertion's default (weak) strength, so a credited pin shows `low`.
- Single-line `if (...) return ...;`, ternaries and `switch` are not read.
- The boundary witness checks that a tested call carries the boundary
  input, not that control reaches the changed `if`: an earlier
  `if (admin) { return ...; }` can stop the tested call first. This
  RIPR-SPEC-0027 limit applies to every strong predicate assertion.

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
   `exact_value` / strong, and the finding carries no
   `typescript_custom_matcher_unresolved`.
2. As example 1 with `const missing = errorFor(404);
   expect(missing).toBeInstanceOf(NotFound);`: `exposed`.
3. As example 1 with `class Forbidden extends NotFound`, or through
   `class Forbidden extends Gone` and `class Gone extends NotFound`, or
   through a module `class Map extends NotFound`: not credited.
4. As example 1 asserting `toBeInstanceOf(Error)` twice: not `exposed`.
5. Pins on `new NotFound("x")`, on `wrap(errorFor(404))`, through
   `.rejects`, or through a test-local `errorFor`: not credited; on the
   same line as a credited pin, a wrapped pin is still not credited;
   `.resolves.toBeInstanceOf` is credited.
6. As example 1 where the false branch falls through to
   `if (status >= 400) { return new NotFound("client"); }`: not credited.
7. A branch returning `cached`, `new Forbidden("denied").asNotFound()`,
   an imported class, or holding another statement; a changed `if` inside
   an arrow; a one-line `if (c) { return ...; }`: not credited.
8. `return new Forbidden("denied")` changed to
   `return new Forbidden("refused")` with the problem's tests: not
   `exposed`, and rule 1 reads no pin on that value seam.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/instance_class_tests.rs`
  pins examples 1 to 8 and the owner-side conditions.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `jest-mined-tobeinstanceof` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `oracle_for_matcher`, `instance_class_argument`.
- `crates/ripr/src/analysis/language/typescript/instance_class_guard.rs`:
  `ts_owner_branches_split_class`, `ts_instance_class_facts`.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `branch_return_expressions` (shared with the RIPR-SPEC-0027 boundary
  witness).
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `strongest_family_matching_oracle`, `ts_predicate_boundary_row_activation`,
  `project_credited_instance_rows`, `ts_observes_whole_owner_result`.

## Metrics

- `typescript_instance_pin_mismatch`: acceptance examples whose observed
  reading differs from this spec. Target: zero.
