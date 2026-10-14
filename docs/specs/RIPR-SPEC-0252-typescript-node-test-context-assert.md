# RIPR-SPEC-0252: TypeScript node:test context assert

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

Support-tier impact:

- No tier change. TypeScript stays Preview.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Amends the RIPR-SPEC-0243 execution-context receiver table, which said
  `t.assert.equal(...)` yields no assertion.
- Rust is not changed.

## Problem

The TypeScript verdict corpus reads `nodetest-latefee-t-assert` as a false
gap:

```ts
// src/latefee.ts
export function lateFee(days: number): number {
  return days * 2;             // before
  return 2 * days;             // after
}

// test/latefee.test.ts
import test from "node:test";
import { lateFee } from "../src/latefee";

test("late fee is two per day", (t) => {
  t.assert.equal(lateFee(10), 20);
});
```

The diff itself is a behavior-preserving rewrite; the corpus truth comes
from the mutant `return 2 / days;` on that line. Node 22's `node:test`
gives each test context an `assert` object that exposes the top-level
`node:assert` functions, and `t.assert.equal(lateFee(10), 20)` fails for
that mutant. RIPR-SPEC-0243 reads the
callback's first parameter through the RIPR-SPEC-0085 receiver table
(`t.is`, `t.deepEqual`, ...), where `t.assert.equal` is not a method, so the
test has no assertion, its oracle is unknown and the finding is
`weakly_exposed`.

## Behavior

**Rule 1. `t.assert.<method>(...)` on a `node:test` context reads as
legacy `node:assert`.** A call `<receiver>.assert.<method>(...)`
(optionally awaited) is an assertion when all of these hold:

- the test is registered through a callee whose root identifier is bound
  to `node:test` (`import test from "node:test"`, `import { it } from
  "node:test"`, or `const test = require("node:test").test`; only the
  default export, `test` and `it` qualify, so `import { describe as test }`
  does not), so a file that imports only a helper from `node:test` and
  registers tests through another runner reads nothing. The callee is
  resolved at the call site: a callback body that redeclares `test` does
  not undo it;
- `<receiver>` is the test callback's first parameter, as the
  execution-context recogniser already requires, including an awaited
  `t.test` subtest's own parameter (RIPR-SPEC-0246 rule 2);
- `assert` is a static member name or a computed member whose key is a
  string literal (`t["assert"]`, `t['assert']`), and `<method>` is a
  static member name;
- `<method>` is in the `node:assert` method table.

It reads exactly as the same call on a legacy `node:assert` binding:
`strictEqual` and `deepStrictEqual` are `exact_value` / strong, `equal` and
`deepEqual` are loose and strong only against a literal coercion cannot
reach (RIPR-SPEC-0247 rule 2), `throws` / `rejects` with an anchored regex
or a literal message object pin the error (RIPR-SPEC-0243 rule 10), and
`ok` keeps its truthiness reading. Argument order is (actual, expected).

Anything else yields no assertion, as before: a method outside the table,
a computed member whose key is not a string literal (`t[key]`,
`` t[`assert`] ``), another receiver
(`helper.assert.equal`), or a file without a `node:test` import.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Recorded on #6654.

1. **Legacy flavor, not strict.** Adopted: Node documents `t.assert` as the
   top-level `node:assert` functions, whose `equal` is loose `==`.
   Rejected: reading `equal` as strict, which would credit a loose
   comparison against a coercible value.
2. **Require a `node:test` registration.** Adopted: `t.assert` as an
   object exists only on the `node:test` context, so the test must be
   registered through a `node:test` binding; AVA's `t.assert(value)` is a
   call and is read by the receiver table. Rejected: accepting any
   receiver, which would credit a user object that happens to have an
   `assert` member. Rejected after review: any file that imports from
   `node:test`, which credits another runner's context in a mixed file.
3. **String-literal computed member (amended 2026-10-14, #6654).**
   Adopted: `t["assert"]` names the same property as `t.assert`, so it
   reads the same way; the corpus case `nodetest-t-bracket-assert` is a
   discriminated test that read as a gap. Rejected: keep reading no
   assertion, which reports a test the user already wrote as missing.
   A template-literal or variable key stays unread: ripr does not evaluate
   it.

### Known limits

- A file that runs under `node --test` without importing `node:test`
  (globals only), or that names it only in a type-only import, is not
  read. `const test = require("node:test")`, `const { test } =
  require("node:test")` and `require("node:test").test` are read.
- `export { test } from "node:test"` binds nothing in the file, so a name
  it re-exports never counts as a `node:test` registration, even when an
  import from `node:test` also binds it (under-credit).
- A test registered through a global `test` / `it` (no import) or
  through a callee whose root identifier is not bound to `node:test` is
  not read.
- A namespace import (`import * as nt from "node:test"; nt.test(...)`) or
  an import renamed away from `test` / `it` / `specify`
  (`import { test as nt } from "node:test"; nt(...)`) is not recognised as
  a test registration at all, so its `t.assert` calls are not read.
- `` t[`assert`] `` and `t[key]` name the same member at runtime but
  are not read (under-credit): ripr reads only a string-literal key.
- `const { assert } = t;`, `t.assert` passed to a helper, optional or
  parenthesized forms (`t.assert?.equal`, `(t).assert`), and `t.assert`
  inside a `forEach` or `.then` callback or on an outer `t` inside a
  subtest are not read (under-credit).
- Shared with the AVA receiver path: a body block that redeclares the
  receiver (`{ const t = fake; t.assert.equal(...) }`), a `t.todo()` call
  in the body, and an un-awaited `t.assert.rejects(...)` keep their
  assertion reading (#7507).

## Required Evidence

- On the TypeScript verdict corpus, `nodetest-latefee-t-assert` moves from
  `gap` to `credited` and no other row moves.
- Amended 2026-10-14: `nodetest-t-bracket-assert` moves from `gap` to
  `credited` (false verdicts 7/127 to 5/127 together with the
  RIPR-SPEC-0247 amendment), and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No change to the AVA, tape or `node:test` receiver table itself.
- No reading of `t.assert` outside a test callback's first parameter.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. `t.assert.equal(lateFee(10), 20)` in a test registered through a
   `node:test` binding:
   `exact_value` / strong, observed `lateFee(10)`; the corpus case reads
   `credited`.
2. `t.assert.strictEqual(...)` and `t.assert.deepStrictEqual(...)`:
   `exact_value` / strong.
3. `t.assert.equal(lateFee(10), want)` with `const want = 20;` declared
   at the top level of the test body before the assertion:
   `exact_value` / strong (RIPR-SPEC-0247 rule 2). With `let want = 20`,
   a `want` bound to a call, or the assertion nested in a block:
   `relational_check` / weak.
4. `t.assert.throws(() => lateFee(-1), /^Error: days must be positive$/)`
   and `await t.assert.rejects(async () => lateFee(-1), { message: "days
   must be positive" })`: `exact_error_variant` / strong.
5. `await t.test("sub", (t2) => { t2.assert.strictEqual(...) })`: read on
   the subtest's receiver.
6. The same calls in an AVA file, a file with no runner import or only a
   type-only `node:test` import, a test registered through another
   runner in a file that imports `mock` from `node:test`, on
   `helper.assert`, on another context member (`t.check.equal`), through
   `t[key]`, `` t[`assert`] `` or `t.assert.strict`, or with a method
   outside the table (`snapshot`, an unknown name): no assertion.
7. `t["assert"].equal(holdFee(4), 5)` in a test registered through a
   `node:test` binding: read as `t.assert.equal(holdFee(4), 5)`,
   `exact_value` / strong.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/assertion_library_tests.rs`:
  `node_test_context_assert_reads_as_legacy_node_assert` pins examples 1
  to 5 and 7; `node_test_context_assert_needs_the_runner_receiver_and_a_known_method`
  pins example 6.
- `crates/ripr/src/analysis/language/typescript/admission_tests.rs`:
  `assertion_admission_separates_no_assertion_from_unresolved_assertion_like_forms`
  reads `t.assert.ok` as recognized and `t.assert.verifies` as unresolved.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `nodetest-latefee-t-assert` reads `credited`; example 3:
  `nodetest-t-assert-equal-variable` reads `credited`; example 7:
  `nodetest-t-bracket-assert` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `node_test_context_assert_assertion`, `assert_call_assertion` (shared
  with `module_assert_assertion_from_expression`), and the
  `node_test_locals` / `node_test_runner` fields and `for_registration` on
  `TypeScriptAssertionBindings`.
- `crates/ripr/src/analysis/language/typescript/tests_extract.rs`:
  `test_name_and_assertions_from_call` and `callee_root_name`.

## Metrics

- `typescript_node_test_context_assert_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
