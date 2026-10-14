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

- No tier change. TypeScript stays Preview. Rules 1 and 3 remove credit:
  a finding may move from `exposed` to `weakly_exposed`. Rule 2 adds credit
  in every repository, for three callback forms that run before the test
  settles. Corpus mutation evidence (#6686) motivated it: in those forms
  the uncredited reading was a false actionable verdict, and a collected
  assertion is the same evidence as a top-level assertion with the
  substituted value.
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
the change alters the identity of the returned object. For a plain object
or array a structural matcher (`toEqual`, `toStrictEqual`, `deepEqual`,
`deepStrictEqual`, chai `eql` or `deep.equal`) passes on both versions.
An `exposed` ladder result then reads `weakly_exposed` unless a related
test asserts on the owner call's result itself (`expect(owner(...))` with
nothing after the argument list, not a member, index or optional chain on
it, nor a local) with a matcher that compares objects by identity, against
a non-literal expected value. Built-in identity matchers are `toBe`,
`strictEqual` and AVA `t.is`. A same-file custom matcher that
RIPR-SPEC-0262 credits also compares by identity, because its `pass`
requires `received === expected`; the assertion carries that property
rather than inheriting it from `exact_value` (`toEqual` shares that kind)
or from a widened matcher-name list. A literal such as `toBe(null)` or
`toBeSame(null)` pins a branch, not the returned object. chai `equal`
does not count, because the recorded matcher does not carry the `.deep`
flag (fail closed). A refused or non-strict custom matcher does not
count. Any
other change on the line, such as an added or overridden field, is not
copy-only and keeps the ladder result. Rule 1 runs before RIPR-SPEC-0243
rule 8, so a copy-only change reads `weakly_exposed` even when rule 8
would read `static_unknown`.

Known limit: when the returned value is a class instance, a `Set` or a
`Map`, a copy also changes its prototype or type, so `toStrictEqual` can
tell the two apart. Static evidence does not know the runtime type, so
rule 1 still demotes. That costs a false gap, never false credit.

**Rule 2. Three nested callbacks run before the test settles.** Their
assertions are collected as the test's own:

- `[<literal>, ...].forEach((v) => body)`: an inline array of at most 64
  literals (the RIPR-SPEC-0243 rule 11 literal set), each on one line,
  and a callback that is neither `async` nor a generator, with exactly
  one plain parameter. Each element reads as one case, the body with `v`
  replaced by the element, exactly as a rule 11 table row. A synchronous
  `return expect(...)` still runs and is collected, but a promise returned
  from the body does not reach the runner, because `forEach` discards it,
  so a `.then` returned there is not collected.
- `return p.then((v) => body)` or `await p.then((v) => body)`, with a
  single argument, a receiver `p` on one line in which `then`, `catch`
  and `finally` never appear as a word and whose member chain has no
  computed step (a computed name such as `"th" + "en"` cannot be read),
  and a body that declares none of the names `p` reads: the runner waits
  for the returned or awaited promise, so the body runs before the test
  settles. With one plain parameter the body reads with `v` replaced by
  `(p)`, the expression whose resolved value the assertion observes. With
  no parameter the body is collected as written.
- `await t.test(name, [options,] (t2) => body)`, where `t` is the test
  callback's own receiver, `name` is present and is a string or template
  literal, and
  `options` is an object literal with no `skip` or `todo` key and no
  spread: node:test runs an awaited subtest inside its parent, and a
  failing subtest fails the parent. The body is collected with `t2` as its
  receiver.

Anything else collects nothing, as before. That includes a non-literal or
empty array; an `async` or generator `forEach` callback, or one without
exactly one parameter; a `.then` whose promise is neither returned to the
runner nor awaited (it may settle after the test); a `.then` with a
rejection handler (it can swallow the change), a chained or multi-line
receiver, or a body that re-declares a receiver name; a subtest that is
not awaited, not on the test's receiver, or skipped or todo (a todo
failure does not fail the parent); and any body the rule 11 row reader
refuses. A subtest with no name, a non-literal name, or options without a
name is also refused.

**Rule 3. A dropped spread override needs an input that lacks it.**
(Added 2026-10-14.) When the changed line's removed and added text are
`return { ...x, <entries> }` on one line, the first entry of both is the
same spread `...x` of an owner parameter `x`, and one side has exactly one
more entry `k: L`, a plain or quoted key with a literal value (a number,
a string with no escape and no quote character inside, `true`, `false`
or `null`; numbers compare by value and sign, so `-0` is not `0`), the change only sets
or drops that field. Other entries must be `key: value` pairs (a
shorthand entry stops the rule), match in order, and hold no second
spread and no second `k`; the object may hold no comment outside a
string. The owner must never rebind `x` (the RIPR-SPEC-0248 rule 1
stability check), write or delete a member of `x`, or pass `x` itself to
a call, since the object it spreads would then not be the input the test
wrote. An input whose `k` already holds `L` comes back equal on both
versions, however exact the matcher is.

The rule reads every owner call whose whole result a strong
`exact_value`, `whole_object_equality` or snapshot assertion observes (as
RIPR-SPEC-0248 rule 1 does). Its argument for `x` is read when it is an
object literal, inline or a name the test binds once by
`const name = { ... }` (a type annotation is allowed) and by no `let` or
`var`. A name whose member the test writes, deletes or hands to
`Object.assign`, or that a callback in the test also takes as a
parameter, cannot be read.
That input lacks the field when it has no `k` entry and no spread,
computed, method or accessor entry, or when its last `k` entry holds
another literal. A strong assertion on the `k` member of an owner call
(`owner(cfg).k`, `owner(cfg)?.k`, `owner(cfg)["k"]`) also counts when its
input lacks the field; on any other input it is not an observation. If
one such input lacks the field, the ladder result stands. Otherwise, when
at least one whole-result call is observed, an `exposed` ladder result
reads `weakly_exposed`, whether every input carries `k: L` or some input
cannot be read. With no observed whole-result call the ladder result
stands. Rule 3 runs after RIPR-SPEC-0243 rule 8 and before RIPR-SPEC-0248
rule 1, on the same value seams.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Each records the adopted option, why,
and the rejected alternative.

1. **Copy-for-identity (rule 1).** Adopted: demote unless an identity
   matcher observes the owner call's result directly. Rejected: any
   identity matcher in the test, which keeps credit for
   `expect(ready(cfg).ok).toBe(true)`, an assertion that cannot see the
   copy.
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
6. **Spread override (rule 3, 2026-10-14).** Adopted: demote when calls
   are observed and none passes an input shown to lack the field,
   including when an input cannot be read, matching RIPR-SPEC-0248 rule 1.
   With no observed call the ladder stands, so a test that only asserts a
   member (`expect(ready(cfg).ok).toBe(true)`) keeps its credit, even when
   its input already carries the field: that case remains a known false
   `exposed`. The input read is textual. A member written after the
   binding, or a callback parameter of the same name, makes the input
   unreadable (`weakly_exposed`), and so does an input it cannot read,
   such as an escaped string or `undefined`: both remove credit, never add
   it. A binding in an outer scope that an inner one shadows, or a member
   written through an alias, can still make an input read as lacking the
   field, which keeps the ladder result (no new credit). Member pins
   through a local (`const r = owner(cfg); expect(r.ok)`) and
   `toHaveProperty` / `toMatchObject` assertions are not read as witnesses,
   so an input they would separate can still be demoted. Rejected:
   leave example 3 at `exposed`, which credits a test that passes on both
   versions; and demote every dropped override, which would remove credit
   from tests whose input lacks the field.

## Required Evidence

- Rules 1 and 3 move no finding to a stronger class.
- Rule 2 moves a finding to a stronger class only through an assertion it
  collects, and each collected assertion keeps its original line.
- On the TypeScript verdict corpus, false verdicts fall from 21/101 to
  17/101 and no case regresses.

## Non-Goals

- No collection of assertions in other nested callbacks (`setTimeout`,
  `waitFor`, `act`, a `.then` that floats, a `forEach` over a variable),
  or in the refused forms rule 2 lists (an `async` or generator callback,
  a multi-line cell or receiver, an earlier promise step, a subtest
  without a literal name or on another receiver).
- No type reasoning about `.then`. A receiver whose own `then` never calls
  its callback (a hand-written thenable) is read as a promise. The same
  receiver already credits `expect(await p).toBe(...)`, so rule 2 adds no
  new false-credit path.
- No change to snapshot, mock-matcher or legacy-assert strengths.
- No identity reasoning beyond the four copy forms of rule 1.

## Acceptance Examples

1. `return cfg` becomes `return { ...cfg }`, test
   `expect(ready(cfg)).toEqual(cfg)`: `weakly_exposed`.
2. The same change, test `expect(ready(cfg)).toBe(cfg)`: `exposed`.
3. `return { ...cfg, ok: true }` becomes `return { ...cfg }`, test
   `const cfg = { name: "bo", ok: true }; expect(markOk(cfg)).toEqual(cfg)`:
   rule 1 does not apply; rule 3 reads `weakly_exposed` (it read `exposed`
   before rule 3).
4. `[150, 200].forEach((v) => { expect(price(v)).toBe(140); })`: two
   assertions, observing `price(150)` and `price(200)`.
5. `cases.forEach(...)`, `[150, n].forEach(...)` or `[].forEach(...)`: no
   assertion.
6. `return total([10, 20]).then((sum) => { expect(sum).toBe(30); })`: one
   assertion observing `(total([10, 20]))`.
7. `total([10, 20]).then(...)` as a bare statement, with a rejection
   handler, after another `.then` step, or with a body that re-declares a
   receiver name: no assertion.
8. `await t.test("x", () => { assert.strictEqual(memberPrice(30), 25); })`:
   one assertion, also with `{ timeout: 50 }` options; without `await`, on
   another object, or with `{ skip: true }` or `{ todo: true }`, none.
9. `[150].forEach(async (v) => { await tick(); expect(price(v)).toBe(140); })`
   or `[150].forEach((v) => { return load(v).then(...); })`: no assertion.
10. `return cfg` becomes `return { ...cfg }`, test
    `expect(ready(cfg).ok).toBe(true); expect(ready(cfg)).toEqual(cfg)`:
    `weakly_exposed`. So too with `ready(cfg)["ok"]`, `ready(cfg)?.ok`, or
    `expect(ready(cfg)).toBe(null)` in place of the member pin.
11. `return cfg` becomes `return { ...cfg }`, test registers
    `expect.extend({ toBeSame(r, e) { return { pass: r === e }; } })` and
    asserts `expect(ready(cfg)).toBeSame(cfg)`: `exposed`. The same change
    with `toEqual`, with `pass: r == e`, with `toBeSame(null)`, or with a
    comment between `expect` and `.extend`, stays `weakly_exposed`.
12. As example 3 with `cfg = { name: "bo", ok: false }`, or
    `expect(ready({ name: "bo" })).toEqual({ name: "bo", ok: true })`:
    `exposed`. With one input carrying `ok: true` and another `ok: false`:
    `exposed`.
13. As example 3 with `const cfg = load()` or `const cfg = { ...base }`:
    `weakly_exposed`. Dropping two fields (`{ ...cfg, ok: true, extra: 1 }`
    to `{ ...cfg }`) is not rule 3.
14. As example 3 with `const cfg: Settings = { name: "bo" }`, or with a
    second assertion `expect(markOk({ name: "bo" }).ok).toBe(true)`:
    `exposed`. With `cfg.ok = false` written after the binding:
    `weakly_exposed`. With an owner that runs `cfg.ok = false` or
    `normalize(cfg)` before the return: the ladder result, `exposed`.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/nested_callback_tests.rs`
  pins examples 1 to 14; `spread_override_guard.rs` unit tests pin the
  rule 3 shape, input reads and member-write detection.
- `fixtures/typescript-verdict-corpus` pins the four Problem rows.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `ts_apply_post_ladder_rules`, `ts_identity_copy_subject`,
  `ts_test_pins_identity` (rule 1; reads
  `TypeScriptAssertion::compares_reference_identity`).
- `crates/ripr/src/analysis/language/typescript/types.rs`:
  `compares_reference_identity` on the assertion.
- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `expect_assertion_from_expression` sets the property for a credited
  RIPR-SPEC-0262 matcher; `collect_nested_callback_assertions` (rule 2).
- `crates/ripr/src/analysis/language/typescript/spread_override_guard.rs`:
  `SpreadOverride`, `ts_test_local_object`; `dropped_term_guard.rs`:
  `ts_members_written`; `classifier.rs`: `ts_spread_override_unseen`,
  `ts_owner_call_member_arguments` (rule 3).
- `crates/ripr/src/analysis/language/typescript/table_rows.rs`:
  `callback_case_assertions`, shared with RIPR-SPEC-0243 rule 11.

## Metrics

- `typescript_corpus_amendment_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
