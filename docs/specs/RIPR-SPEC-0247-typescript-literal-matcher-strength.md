# RIPR-SPEC-0247: TypeScript literal matcher strength

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

Linked PRs:

- #6686 (TypeScript verdict corpus)
- #7379 (RIPR-SPEC-0246)

Support-tier impact:

- No tier change. TypeScript stays Preview. All three rules add credit in
  every repository. Corpus mutation evidence (#6686) motivated them: in
  each shape the medium or weak reading was a false actionable verdict,
  and the expected value written in the test pins the changed value as an
  exact matcher does.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Amends RIPR-SPEC-0243 decisions 1 and 11, its `node:assert` / chai
  `assert` table, and acceptance example 30.
- Rust (RIPR-SPEC-0231 steps 6 and 9) is not changed.

## Problem

After RIPR-SPEC-0246 the TypeScript verdict corpus scores 17 of 101 cases
false. Eight of them are discriminated cases that RIPR-SPEC-0243 reads as
medium or weak although the test writes the expected value as a literal:

| Corpus case | Matcher | Runtime truth |
|---|---|---|
| `jest-notify-called-with` | `const send = jest.fn()` … `expect(send).toHaveBeenCalledWith("order 7 shipped")` | both mutants fail |
| `jest-spec0234-ex30` | `const db = { write: jest.fn() }` … `toHaveBeenCalledWith(2)` | the edit fails |
| `ts-vitest-checkout-notify` | `const send = vi.fn()` passed as `{ send }` to a parameter `notifier` … `toHaveBeenCalledWith("ann", "charged 360")` | every mutant fails; rule 1 refuses it (the names differ) |
| `vitest-orders-audit-logger-called-with` | `const log = vi.fn()` … `toHaveBeenCalledWith("shipped order 42")` | every mutant fails |
| `nodetest-receipt-legacy-deepequal` | legacy `assert.deepEqual(receiptLine(3), { count: 3, label: "items: 3" })` | the mutant fails |
| `ts-nodetest-silver-legacy-assert` | legacy `assert.equal(tierForPoints(1000), "silver")` | every mutant fails |
| `jest-receipt-inline-snapshot` | `toMatchInlineSnapshot(` with the object's text `)` | both mutants fail |
| `ts-vitest-summary-snapshot` | ``toMatchInlineSnapshot(`"2 items"`)`` | the mutant fails |

## Behavior

**Rule 1. A literal mock-call payload on the parameter's own mock pins
the changed call.** On a SideEffect or CallDeletion seam, a
`toHaveBeenCalledWith` assertion reads `mock_expectation` / strong when
all of these hold:

- every argument is a string, number, boolean or `null` literal (an
  object payload is refused: Jest's recursive equality treats a property
  set to `undefined` as absent);
- the changed line calls an owner parameter `p` as `p(...)`, or a method
  of it as `p.m(...)` (not through a computed receiver such as
  `getTable(p).m(...)` or `t[p].m(...)`), the owner body after its
  parameter list has no loop, nested function or call back into the owner
  (any of which could run the changed call more than once), and the owner body mentions `p` exactly twice: its
  declaration and this call;
- the asserted target is `p` (or `p.m`), the same name, bound at the
  test callback's top level (not inside a nested block, where it could
  shadow an outer `p`) with `const|let|var p = jest.fn(...)` or
  `vi.fn(...)`, or to a flat object literal whose `m` is such a call,
  whose keys are plain identifiers, which keys `m` once and which has no
  spread that could replace it;
- the test mentions `p` exactly twice outside `expect(p)` /
  `expect(p.m)`: its declaration and one argument of the test's only
  owner call, at `p`'s parameter position with no spread before it.
  `expect(p(...))` calls the mock and counts as a mention.

`toHaveBeenCalledWith` passes when any recorded call matches, so every
other way a call could reach the mock refuses: a second call in the
owner, `p.call(...)`, `arguments[i]`, an alias, passing `p` on, an inner
parameter of the same name, the test calling or passing the mock
elsewhere, a second owner call, or the mock at another argument position
or after a spread. A mock whose name differs
from the parameter's (`const send = vi.fn()` passed in `{ send }` to a
parameter `notifier`) earns nothing; that costs a false gap, never false
credit. The owner body is the source the owner was parsed from, so a
committed-history diff reads the committed owner, not the worktree.
Without that source, or for an owner whose parameters
the adapter does not record (destructuring, rest), nothing is credited.
Anything else keeps `mock_expectation` / medium, including a mock from
`jest.mock`, `jest.spyOn` or a helper, `toHaveBeenCalledTimes`, and every
value seam, which RIPR-SPEC-0104 already closes to mock matchers.

**Rule 2. Loose equality against a coercion-safe literal is exact.**
Legacy `node:assert` `equal` and `deepEqual`, and chai's `assert.equal`,
compare with `==`, which only differs from `===` when coercion makes two
values of different types equal. When the expected (second) argument is
a literal that no boolean, `null`, `undefined`, empty string or number of
another spelling can equal, the assertion reads `exact_value` / strong:

- a string or interpolation-free template literal that is not empty, not
  whitespace (including U+FEFF) and not numeric (`"20" == 20`,
  `"0x1f" == 31`, `"1" == true`, `"1e3" == 1000`, `"Infinity"`);
- a finite number literal other than `0` and `1` (`0 == false`,
  `0 == ""`, `1 == true`), with an optional leading `-`;
- for `deepEqual` only, an object literal with plain static keys other
  than `__proto__`, or an array literal without holes or spread, whose
  every leaf is one of the above.

`true`, `false`, `null`, `undefined`, `NaN`, a variable, a call, a
computed key, a spread, and an object literal under `equal` (a reference
comparison) keep `relational_check` / weak.

Amended 2026-10-14 (#6654): an expected identifier reads as the literal
it is bound to when it names a `const` declared with a plain identifier
at the top level of the test body (or of an awaited `t.test` subtest
body), before the assertion, by the only top-level declaration of that
name, and the assertion itself is a top-level statement of that body.
The literal must be a string, template or number literal the rule above
accepts, optionally parenthesized or under `as`. Object and array
literals do not qualify even under `deepEqual`: `const` stops
reassignment, not mutation, so `want.push(...)` or `want.total = ...`
changes what the assertion compares. Any other variable keeps the weak
reading: a `let` or `var`, a destructured, call-bound, object or array
`const`, a `const` declared after the assertion or in an enclosing
scope, and an assertion nested in a block, branch, loop, `try` or
callback, where a nested declaration could shadow the name.

Known limit: `==` still equals a value of another type that converts to
the literal: a number's own numeric string (`20 == "20"`), a BigInt
(`20n == 20`), a one-element array (`["bronze"] == "bronze"`) or a
wrapper object whose `valueOf` or `toString` returns it, and the same
for each `deepEqual` leaf (`{ count: "3" }` against `{ count: 3 }`). Each
needs a mutant that changes the result's type while keeping its printed
value. Runners that strip types without checking them (Vitest, esbuild,
ts-jest `isolatedModules`) do not refuse such a mutant, so rule 2 accepts
this residual false credit as rare rather than impossible.

**Rule 3. A literal inline snapshot pins the value.**
`toMatchInlineSnapshot` with exactly one argument, a string literal or an
interpolation-free template literal whose text is not whitespace, reads
`snapshot` / strong and does not carry
`typescript_snapshot_discriminator_unresolved`. The serialized value is
compared with text written in the test, as `toEqual` compares with an
expected object. `-u` rewrites inline text as it rewrites snapshot files,
but the inline text sits in the test diff a reviewer reads, while a
committed file's content is not in the test. `toMatchSnapshot`, an inline
snapshot with no argument, with an
interpolation, or with a property-matcher object keeps `snapshot` /
medium. Amended by RIPR-SPEC-0258: a `toMatchSnapshot()`
whose committed entry the test file names reads `snapshot` / strong.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Each records the adopted option, why,
and the rejected alternative. Recorded on #6654.

1. **Mock payloads (rule 1; amends RIPR-SPEC-0243 decision 11).**
   Adopted: strong only for a primitive-literal payload on a test-local
   mock bound under the parameter's own name, passed at its position to
   the one owner call, with the changed call the owner's only use of the
   parameter. Independent review of the first draft found that a
   name-only match credited a mock passed to another parameter, a
   `p.call(...)` second call, and a mock the test also passed elsewhere;
   the adopted conditions refuse all three. Rejected: keep
   medium, which leaves four discriminated corpus cases false actionable
   with a repair packet asking for a test the user already wrote.
   Rejected: strong for every `toHaveBeenCalledWith` with literals, which
   credits module mocks and spies whose other callers can satisfy the
   match.
2. **Legacy loose equality (rule 2; amends the RIPR-SPEC-0243
   `node:assert` table).** Adopted: exact when the expected literal is
   outside coercion's reach. Rejected: keep every legacy `equal` /
   `deepEqual` weak, which reads the most common Node assertion style as
   unable to pin `"silver"`. Rejected: treat legacy `equal` as strict,
   which credits `assert.equal(isEmpty(x), 0)` against a boolean.
3. **Inline snapshots (rule 3; amends RIPR-SPEC-0243 decision 1 for the
   inline form).** Adopted: strong for a literal inline snapshot only.
   Rejected: strong for every snapshot, which also credits a committed
   file (`jest-spec0234-ex03` and `vitest-orders-label-snapshot` show the
   committed form can be either way, and `--ci` behaviour decides it, not
   the test text). Rejected: keep medium, which reads text the reviewer
   can see as unreviewed. Superseded for committed files by
   RIPR-SPEC-0258 decision 1: a committed entry the test file names reads
   strong, because whether a reviewer reads the expected text does not
   change whether the test fails.
4. **Where the reading lives.** Adopted: rules 2 and 3 read at assertion
   extraction, because they depend only on the assertion; rule 1 reads in
   the classifier, because it needs the changed line and owner body.
   Rejected: a new oracle kind, which would ripple through every renderer
   for no new class.
5. **`t.assert.*`, committed snapshots and custom matchers.** Not changed
   here. `nodetest-latefee-t-assert`, `jest-spec0234-ex03` and the other
   remaining false verdicts stay listed on #6654. Superseded for
   committed snapshots by RIPR-SPEC-0258, which credits
   `jest-spec0234-ex03`; for `t.assert.*` by RIPR-SPEC-0252, which
   credits `nodetest-latefee-t-assert`; and for a same-file
   strict-equality custom matcher by RIPR-SPEC-0262, which credits
   `jest-cents-custom-matcher`.
6. **Test-local literal `const` (rule 2, amended 2026-10-14).** Adopted:
   a `const` bound once to a primitive literal at the top level of the
   test body reads as that literal, because a primitive `const` can be
   neither reassigned nor mutated, so the runtime comparison is the
   inline literal's. Rejected after review: object and array `const`s,
   which a test can fill in before the assertion (`want.push(...)`).
   Rejected: keep every variable weak, which leaves
   `nodetest-t-assert-equal-variable` (a discriminated test) a false gap.
   Rejected: resolve any reachable binding (enclosing `describe`, module
   scope, nested blocks), which needs scope analysis ripr does not do and
   could read a shadowing declaration's value.

## Required Evidence

- Each rule moves a finding to a stronger class only through an assertion
  whose expected value is a literal in the test.
- On the TypeScript verdict corpus, false verdicts fall from 17/101 to
  10/101, seven Problem rows move to `credited` (all but
  `ts-vitest-checkout-notify`), and no other row moves.
- Amended 2026-10-14: `nodetest-t-assert-equal-variable` moves from `gap`
  to `credited` (false verdicts 7/127 to 5/127 together with the
  RIPR-SPEC-0252 amendment), and no other row moves.

## Non-Goals

- No credit for `toHaveBeenCalledWith` on a module mock, a spy, a mock
  made by a helper, an object payload, a mock the owner reaches other
  than through a parameter of the same name, or any mock another call
  could reach.
- No change to `toHaveBeenLastCalledWith`, `toHaveBeenNthCalledWith` or
  `toHaveBeenCalledTimes`.
- No change to committed snapshot strength, except the named committed
  entry RIPR-SPEC-0258 reads as strong; every other committed snapshot
  stays medium.
- No change to `t.assert.*` here (superseded by RIPR-SPEC-0252, which
  reads it as legacy `node:assert`).
- No change to Rust or Python oracle classification.

## Acceptance Examples

1. `src/notify.ts` is `export function announce(id: number, send: (m:
   string) => void) { send("order " + id); }`; test `const send =
   jest.fn(); announce(7, send); expect(send).toHaveBeenCalledWith("order
   7")`: `mock_expectation` / strong, `exposed`.
2. As example 1 with `vi.fn()`, or with `const db = { write: jest.fn() }`,
   `save(db, 1)` and `expect(db.write).toHaveBeenCalledWith(2)` for a
   changed `db.write(v + 2)` in `save(db, v)`: `exposed`.
3. As example 1 with any of these in the owner: a second
   `send("order 7")`, `send.call(null, "order 7")`, `const s = send`,
   `retry(send)`, `[1].forEach((send) => send(1))`, the call inside a
   `for` loop or a nested arrow, or a recursive `announce(...)`; or with no readable
   owner source: `mock_expectation` / medium, `weakly_exposed`.
4. As example 1 with any of these in the test: `send("order 7")`,
   `remind(send, 7)`, a second `announce(8, send)`,
   `send.call(null, "order 7")`, or `announce(send, 7)`; or with the
   mock declared in a nested block (`if (ready) { const send = jest.fn() }`)
   or, for example 2, `{ write: jest.fn(), ...defaults }` or a second
   `write` key: `weakly_exposed`.
5. As example 1 with `send` not an owner parameter, with
   `const send = jest.spyOn(mailer, "send")`, a `jest.mock` factory mock
   or `const send = makeSend()`, with an object payload `{ id: 7 }`, or
   for `ship(notifier, cb)` whose changed line is
   `notifier.send("shipped")` while the test passes its `send` mock as
   `cb`: `weakly_exposed`.
6. Legacy `import assert from "node:assert"` and
   `assert.equal(tier(999), "bronze")`, `assert.equal(fee(10), 20)`,
   `assert.equal(delta(), -3)`, or
   `assert.deepEqual(line(3), { count: 3, label: "items: 3" })`:
   `exact_value` / strong.
7. Legacy `assert.equal(x, 1)`, `0`, `""`, `" "`, `"20"`, `"0x1f"`,
   `"1e3"`, `"\uFEFF20"`, `"Infinity"`, `1.0`, `-0`, `true`, `null`,
   `undefined`, `{ a: "x" }` or `` `n=${n}` ``, and
   `assert.deepEqual(x, { count: 0, label: "x" })`, with a spread, with a
   computed key, with a `__proto__` key, or against a variable other than
   the test-local `const` of example 10: `relational_check` / weak.
   chai `assert.equal(x, "silver")` reads strong and
   `assert.equal(x, 1)` weak, as legacy `node:assert` does.
8. ``expect(summary(2)).toMatchInlineSnapshot(`"2 items"`)``, or a
   multi-line template holding an object's text: `snapshot` / strong,
   no `typescript_snapshot_discriminator_unresolved`.
9. `toMatchInlineSnapshot()`, an inline snapshot with `${n}`, an inline
   snapshot with a property-matcher object, or `toMatchSnapshot()`:
   `snapshot` / medium. Amended by RIPR-SPEC-0258: `toMatchSnapshot()`
   whose committed entry the test file names reads `snapshot` / strong.
10. `const want = 30;` (or `"silver"`, or `(30 as number)`) at the top
    level of the test body, then `assert.equal(graceFee(10), want)`,
    `assert.deepEqual(graceFee(10), want)` or
    `t.assert.equal(graceFee(10), want)`: `exact_value` / strong,
    expected value `30`. The same with `const want = 1`, `let want = 30`,
    `const want = price(30)`, `const want = { total: 30 }` or `[]` (even
    under `deepEqual`), `const want` declared after the assertion or
    inside a block, or the assertion inside an `if`, a `try`, a returned
    `.then` callback or a subtest that does not declare `want` itself:
    `relational_check` / weak.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/literal_matcher_tests.rs`
  pins examples 1 to 9 at the rule level.
- `crates/ripr/src/analysis/language/typescript/assertion_library_tests.rs`
  keeps the loose-equality grading with coercion-reachable literals, and
  `loose_equality_reads_a_test_local_literal_const` pins example 10.
- `fixtures/typescript-verdict-corpus` pins example 10:
  `nodetest-t-assert-equal-variable` reads `credited`.
- `fixtures/typescript-verdict-corpus` pins the eight Problem rows: seven
  `credited`, and `ts-vitest-checkout-notify` still `gap`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/mock_payload_guard.rs`:
  `MockCallFacts` (rule 1).
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `strongest_family_matching_oracle` and `ts_changed_value_is_observed`
  read the credited payload as strong, and `project_credited_mock_rows`
  shows it as strong on the related-test row (rule 1).
- `crates/ripr/src/analysis/language/typescript/literal_matcher_guard.rs`:
  `loose_equality_pins_literal` (rule 2), `loose_literal_consts` and
  `resolve_loose_literal_const` (rule 2's test-local `const`),
  `inline_snapshot_is_literal` (rule 3).
- `crates/ripr/src/analysis/language/typescript/oracle.rs`: assertion
  extraction applies rules 2 and 3; `mock_payload_from_assertion` records
  multi-argument literal payloads.
- `crates/ripr/src/analysis/language/typescript/static_limit.rs`: a
  strong snapshot carries no snapshot limitation.

## Metrics

- `typescript_literal_matcher_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
