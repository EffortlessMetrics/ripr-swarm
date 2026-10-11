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
| `ts-vitest-checkout-notify` | `const send = vi.fn()` … `toHaveBeenCalledWith("ann", "charged 360")` | every mutant fails |
| `vitest-orders-audit-logger-called-with` | `const log = vi.fn()` … `toHaveBeenCalledWith("shipped order 42")` | every mutant fails |
| `nodetest-receipt-legacy-deepequal` | legacy `assert.deepEqual(receiptLine(3), { count: 3, label: "items: 3" })` | the mutant fails |
| `ts-nodetest-silver-legacy-assert` | legacy `assert.equal(tierForPoints(1000), "silver")` | every mutant fails |
| `jest-receipt-inline-snapshot` | `toMatchInlineSnapshot(` with the object's text `)` | both mutants fail |
| `ts-vitest-summary-snapshot` | ``toMatchInlineSnapshot(`"2 items"`)`` | the mutant fails |

## Behavior

**Rule 1. A literal mock-call payload on a test-local mock pins the
changed call.** On a SideEffect or CallDeletion seam, a
`toHaveBeenCalledWith` assertion reads `mock_expectation` / strong when
all of these hold:

- every argument is a literal: a string, number, boolean or `null`
  literal, or an object literal of those (the RIPR-SPEC-0243 mock payload
  set);
- the asserted target is a name `m` the test binds with
  `const|let|var m = jest.fn(...)` or `vi.fn(...)`, or a member `o.p` where
  the test binds `o` to a flat object literal whose `p` property is such a
  call;
- the test body never calls the target's last name itself;
- the changed line calls the target's last name through an owner
  parameter (`m(` for a parameter `m`, `x.p(` for a parameter `x`) that
  the owner body does not declare again, and the owner's body, read after
  the change, calls that name exactly once.

`toHaveBeenCalledWith` passes when any recorded call matches, so a second
call to the same name, from the owner or from the test, could still match
after the changed call is mutated; the "exactly once" and "never calls"
conditions refuse that. Requiring a parameter keeps a same-named import
(`log(` from a logger module while the test passes its own `log` mock
elsewhere) from borrowing the mock's credit. A parameter whose name
differs from the mock's (`const send = jest.fn()` passed as `notify`)
earns nothing; that costs a false gap, never false credit. Without a
readable owner source, or for an owner whose parameters the adapter does
not record (destructuring, rest), nothing is credited. Anything else keeps
`mock_expectation` / medium: a payload with a variable or spread, a mock
from `jest.mock`, `jest.spyOn` or a helper, a nested object literal, a
target the changed line does not call, `toHaveBeenCalledTimes`, and every
value seam, which RIPR-SPEC-0104 already closes to mock matchers.

**Rule 2. Loose equality against a coercion-safe literal is exact.**
Legacy `node:assert` `equal` and `deepEqual`, and chai's `assert.equal`,
compare with `==`, which only differs from `===` when coercion makes two
values of different types equal. When the expected (second) argument is
a literal no value of another type can equal, the assertion reads
`exact_value` / strong:

- a string or interpolation-free template literal that is not empty, not
  whitespace and not numeric (`"20" == 20`, `"0x1f" == 31`,
  `"1" == true`);
- a finite number literal other than `0` and `1` (`0 == false`,
  `0 == ""`, `1 == true`), with an optional leading `-`;
- for `deepEqual` only, an object literal with plain static keys, or an
  array literal without holes or spread, whose every leaf is one of the
  above.

`true`, `false`, `null`, `undefined`, `NaN`, a variable, a call, a
computed key, a spread, and an object literal under `equal` (a reference
comparison) keep `relational_check` / weak.

Known limit: a number literal still equals its own numeric string
(`20 == "20"`). A mutant that turns a numeric result into the same digits
as a string escapes. TypeScript's types make that mutant unusual, and it
would fail the type check before it ran.

**Rule 3. A literal inline snapshot pins the value.**
`toMatchInlineSnapshot` with exactly one argument, a string literal or an
interpolation-free template literal whose text is not whitespace, reads
`snapshot` / strong and does not carry
`typescript_snapshot_discriminator_unresolved`. The serialized value is
compared with text written in the test, as `toEqual` compares with an
expected object. `toMatchSnapshot` (a committed snapshot file that may be
stale or regenerated), an inline snapshot with no argument, with an
interpolation, or with a property-matcher object keeps `snapshot` /
medium.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Each records the adopted option, why,
and the rejected alternative. Recorded on #6654.

1. **Mock payloads (rule 1; amends RIPR-SPEC-0243 decision 11).**
   Adopted: strong only for an all-literal payload on a test-local mock
   that the changed line calls and the owner calls once. Rejected: keep
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
   can see as unreviewed.
4. **Where the reading lives.** Adopted: rules 2 and 3 read at assertion
   extraction, because they depend only on the assertion; rule 1 reads in
   the classifier, because it needs the changed line and owner body.
   Rejected: a new oracle kind, which would ripple through every renderer
   for no new class.
5. **`t.assert.*`, committed snapshots and custom matchers.** Not changed
   here. `nodetest-latefee-t-assert`, `jest-spec0234-ex03` and the other
   remaining false verdicts stay listed on #6654.

## Required Evidence

- Each rule moves a finding to a stronger class only through an assertion
  whose expected value is a literal in the test.
- On the TypeScript verdict corpus, false verdicts fall from 17/101 to
  9/101, the eight Problem rows move to `credited`, and no other row
  moves.

## Non-Goals

- No credit for `toHaveBeenCalledWith` on a module mock, a spy, a mock
  made by a helper, a mock the owner reaches other than through a
  parameter of the same name, or a mock the owner calls more than once.
- No change to `toHaveBeenLastCalledWith`, `toHaveBeenNthCalledWith` or
  `toHaveBeenCalledTimes`.
- No change to committed snapshot strength.
- No change to `t.assert.*`, which still yields no assertion.
- No change to Rust or Python oracle classification.

## Acceptance Examples

1. `src/notify.ts` is `export function announce(id: number, send: (m:
   string) => void) { send("order " + id); }`; test `const send =
   jest.fn(); announce(7, send); expect(send).toHaveBeenCalledWith("order
   7")`: `mock_expectation` / strong, `exposed`.
2. As example 1 with `vi.fn()`, or with `const db = { write: jest.fn() }`
   and `expect(db.write).toHaveBeenCalledWith(2)` for a changed
   `db.write(v + 2)`: `exposed`.
3. As example 1 with a second `send("order 7")` in the owner body, with
   no readable owner source, with `send` not an owner parameter, with
   `const send = realSend` in the owner body, or with the test calling
   `send("order 7")` itself: `mock_expectation` / medium,
   `weakly_exposed`.
4. As example 1 with `const send = jest.spyOn(mailer, "send")`, a
   `jest.mock` factory mock, `const send = makeSend()`, or `const db = {
   write: { inner: jest.fn() } }`: `weakly_exposed`.
5. As example 1 with `toHaveBeenCalledWith(message)` or
   `toHaveBeenCalledWith("order 7", status)`: no payload,
   `weakly_exposed`.
6. Legacy `import assert from "node:assert"` and
   `assert.equal(tier(999), "bronze")`, `assert.equal(fee(10), 20)`,
   `assert.equal(delta(), -3)`, or
   `assert.deepEqual(line(3), { count: 3, label: "items: 3" })`:
   `exact_value` / strong.
7. Legacy `assert.equal(x, 1)`, `0`, `""`, `" "`, `"20"`, `"0x1f"`,
   `true`, `null`, `undefined`, `{ a: "x" }` or `` `n=${n}` ``, and
   `assert.deepEqual(x, { count: 0, label: "x" })`, with a spread, with a
   computed key, or against a variable: `relational_check` / weak.
8. ``expect(summary(2)).toMatchInlineSnapshot(`"2 items"`)``, or a
   multi-line template holding an object's text: `snapshot` / strong,
   no `typescript_snapshot_discriminator_unresolved`.
9. `toMatchInlineSnapshot()`, an inline snapshot with `${n}`, an inline
   snapshot with a property-matcher object, or `toMatchSnapshot()`:
   `snapshot` / medium.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/literal_matcher_tests.rs`
  pins examples 1 to 9 at the rule level.
- `crates/ripr/src/analysis/language/typescript/assertion_library_tests.rs`
  keeps the loose-equality grading with coercion-reachable literals.
- `fixtures/typescript-verdict-corpus` pins the eight Problem rows.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/mock_payload_guard.rs`:
  `MockCallFacts`, `ts_owner_body_text` (rule 1).
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `strongest_family_matching_oracle` and `ts_changed_value_is_observed`
  read the credited payload as strong (rule 1).
- `crates/ripr/src/analysis/language/typescript/literal_matcher_guard.rs`:
  `loose_equality_pins_literal` (rule 2), `inline_snapshot_is_literal`
  (rule 3).
- `crates/ripr/src/analysis/language/typescript/oracle.rs`: assertion
  extraction applies rules 2 and 3; `mock_payload_from_assertion` records
  multi-argument literal payloads.
- `crates/ripr/src/analysis/language/typescript/static_limit.rs`: a
  strong snapshot carries no snapshot limitation.

## Metrics

- `typescript_literal_matcher_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
