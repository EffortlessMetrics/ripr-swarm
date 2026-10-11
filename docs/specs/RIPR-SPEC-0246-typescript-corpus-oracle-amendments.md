# RIPR-SPEC-0246: TypeScript corpus oracle amendments

Status: proposed

Owner: product / analysis

Created: 2026-10-11

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6654 (TypeScript corpus cases scored false after RIPR-SPEC-0243)
- #5506 (TypeScript custom matchers)

Linked PRs:

- #6686 (TypeScript verdict corpus)
- #6798 (RIPR-SPEC-0243 rules)

Support-tier impact:

- No tier change. TypeScript stays Preview. Rule 1 removes credit: a
  finding may move from `exposed` to `weakly_exposed`. Rule 2 adds credit
  only where the corpus mutation evidence (#6686) shows the uncredited
  reading was a false actionable verdict: the collected assertion is the
  same evidence a top-level assertion with the substituted value would be.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Amends RIPR-SPEC-0243: the Non-Goal "no collection of assertions inside
  nested callbacks or subtests" now excepts the three rule 2 forms, and
  acceptance example 10 reads `exposed`.

## Problem

After RIPR-SPEC-0243 the TypeScript verdict corpus scores 21 of 101 cases
false: 19 of 62 discriminated cases false actionable and 2 of 39
non-discriminated cases false exposed. Each remaining case follows a rule
RIPR-SPEC-0243 states, so the fix for each is a spec decision, not an
implementation bug.

| Corpus case | Today | Runtime truth |
|---|---|---|
| `jest-mined-toequal-copy-not-identity`: `return cfg` becomes `return { ...cfg }`, test `expect(readySettings(cfg)).toEqual(cfg)` | `exposed` | the copy passes `toEqual` |
| `jest-spec0234-ex10`: `[150].forEach((v) => { expect(priceEx10(v)).toBe(140); })` | `weakly_exposed` | the edit fails the test |
| `jest-order-assert-in-then`: `return orderTotal([10, 20]).then((total) => { expect(total).toBe(30); })` | `weakly_exposed` | both mutants fail the test |
| `nodetest-member-subtest`: `await t.test("...", () => { assert.strictEqual(memberPrice(30), 25); })` | `weakly_exposed` | both mutants fail the test |

## Behavior

**Rule 1. A copy-for-identity change needs an identity matcher.** When the
changed line's removed text is `return x` and its added text is
`return { ...x }`, `return [...x]`, `return Object.assign({}, x)` or
`return structuredClone(x)` for the same identifier `x`, or the reverse,
the change alters only the identity of the returned object. A structural
matcher (`toEqual`, `toStrictEqual`, `deepEqual`, `deepStrictEqual`, chai
`eql` or `deep.equal`) passes on both versions. An `exposed` ladder result
then reads `weakly_exposed` unless a related test asserts with `toBe`,
`strictEqual` or AVA `t.is`, which compare objects by identity. chai
`equal` does not count, because the recorded matcher does not carry the
`.deep` flag (fail closed). Any other change on the line, such as an
added or overridden field, is not copy-only and keeps the ladder result.

**Rule 2. Three nested callbacks run before the test settles.** Their
assertions are collected as the test's own:

- `[<literal>, ...].forEach((v) => body)`: an inline array of at most 64
  literals (the RIPR-SPEC-0243 rule 11 literal set) and a callback with one
  plain parameter. Each element reads as one case, the body with `v`
  replaced by the element, exactly as a rule 11 table row.
- `return p.then((v) => body)` or `await p.then((v) => body)`: the runner
  waits for the returned or awaited promise, so the body runs before the
  test settles. With one plain parameter the body reads with `v` replaced
  by `(p)`, the expression whose resolved value the assertion observes.
  With no parameter the body is collected as written.
- `await t.test(name, [options,] (t2) => body)`, where `t` is the test
  callback's own receiver: node:test runs an awaited subtest inside its
  parent, and a failing subtest fails the parent. The body is collected
  with `t2` as its receiver.

Anything else collects nothing, as before: a non-literal or empty array, a
`forEach` callback with more than one parameter, a `.then` whose promise is
neither returned nor awaited (it may settle after the test), a subtest that
is not awaited or not on the test's receiver, and any body the rule 11 row
reader refuses.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Each records the adopted option, why,
and the rejected alternative.

1. **Copy-for-identity (rule 1).** Adopted: demote unless an identity
   matcher is present. Rejected: demote only when the strongest matcher is
   structural, which keeps credit when a `toBe` exists anywhere but
   observes something else. That case is rare and the demotion never
   hides a real gap, so the simpler test-level check is kept.
2. **Nested callbacks (rule 2).** Adopted: the three forms whose callback
   provably runs before the test settles. Rejected: collect every nested
   callback, which would credit assertions in `setTimeout`, unawaited
   promises and callbacks that may never run.
3. **`.then` parameter.** Adopted: substitute `(p)`, so the observation
   guard and boundary witness read the owner call that produces the
   value. Rejected: treat the parameter as an opaque local, which fails
   the observation guard for the most common promise test shape.
4. **Snapshots and mock matchers (RIPR-SPEC-0243 decisions 1 and 11).**
   Not changed here. Seven corpus cases (`toMatchSnapshot`,
   `toMatchInlineSnapshot`, `toHaveBeenCalledWith`) stay false actionable
   until a follow-up decides whether an inline snapshot with a literal body
   and a `toHaveBeenCalledWith` whose arguments are all literals read
   strong. Recorded on #6654.
5. **Legacy `node:assert` `equal` / `deepEqual` and `t.assert.*`.** Not
   changed here. Three corpus cases stay false actionable until a
   follow-up decides whether a literal expected value makes loose equality
   exact. Recorded on #6654.

## Required Evidence

- Rule 1 moves no finding to a stronger class.
- Rule 2 moves a finding to a stronger class only through an assertion it
  collects, and each collected assertion keeps its original line.
- On the TypeScript verdict corpus, false verdicts fall from 21/101 to
  17/101 and no case regresses.

## Non-Goals

- No collection of assertions in other nested callbacks (`setTimeout`,
  `waitFor`, `act`, a `.then` that floats, a `forEach` over a variable).
- No change to snapshot, mock-matcher or legacy-assert strengths.
- No identity reasoning beyond the four copy forms of rule 1.

## Acceptance Examples

1. `return cfg` becomes `return { ...cfg }`, test
   `expect(ready(cfg)).toEqual(cfg)`: `weakly_exposed`.
2. The same change, test `expect(ready(cfg)).toBe(cfg)`: `exposed`.
3. `return { ...cfg, ok: true }` becomes `return { ...cfg }`, test
   `toEqual(cfg)`: the ladder result, `exposed`.
4. `[150, 200].forEach((v) => { expect(price(v)).toBe(140); })`: two
   assertions, observing `price(150)` and `price(200)`.
5. `cases.forEach(...)`, `[150, n].forEach(...)` or `[].forEach(...)`: no
   assertion.
6. `return total([10, 20]).then((sum) => { expect(sum).toBe(30); })`: one
   assertion observing `(total([10, 20]))`.
7. `total([10, 20]).then(...)` as a bare statement: no assertion.
8. `await t.test("x", () => { assert.strictEqual(memberPrice(30), 25); })`:
   one assertion; without `await`, or on another object, none.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/nested_callback_tests.rs`
  pins examples 1 to 8.
- `fixtures/typescript-verdict-corpus` pins the four Problem rows.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `ts_apply_post_ladder_rules`, `ts_identity_copy_subject`,
  `ts_test_pins_identity` (rule 1).
- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `collect_nested_callback_assertions` (rule 2).
- `crates/ripr/src/analysis/language/typescript/table_rows.rs`:
  `callback_case_assertions`, shared with RIPR-SPEC-0243 rule 11.

## Metrics

- `typescript_corpus_amendment_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
