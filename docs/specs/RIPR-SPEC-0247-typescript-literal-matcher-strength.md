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
  spread that could replace it (amended 2026-10-18: a `...name` entry
  does not, when `name` is a test-file `const` object literal the file names
  only there and after spreads, with plain `key: value` entries and no `m`
  key; see example 4); amended 2026-10-15 (#8138), the mock
  call takes no argument or an arrow implementation that names neither
  `function` nor `this` (optionally after `<T>` type arguments), the
  literal's `m` value is that call and nothing more, and the call (or the
  literal's closing `}`) ends the statement: `;`, the block's `}`, the
  end of the body, or a line break before a line that opens with a word
  other than `in`, `instanceof`, `as` or `satisfies`; amended
  2026-10-15, `const|let|var p =
  f();` also reads as such a literal when `f` is a top-level, unexported,
  non-async, non-generator parameterless test-file factory whose whole
  body returns that literal (`function f() { return { m: jest.fn() }; }`
  or `const f = () => ({ m: jest.fn() });`) with `m` keyed to exactly a
  bare `jest.fn()` or `vi.fn()` (no implementation argument, nothing
  after it; `jest.fn ()` with a space is the same call), the file names `f` only in its declaration and direct calls
  `f(`, `f()` is the whole initializer closed by `;` or the block's end
  (a line break alone can continue the expression), and the file has no
  `eval`, `with` or `\u` escape;
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
`jest.mock`, `jest.spyOn` (except the module-object spy of rule 4) or a
helper, `toHaveBeenCalledTimes`, and every value seam, which
RIPR-SPEC-0104 already closes to mock matchers.

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
callback, where a nested declaration could shadow the name. A
callback with a parameter that the reader takes as its own body (a
returned or awaited `.then((value) => ...)`, or an inline
`[...].forEach((value) => ...)`) reads its own top-level `const`s the
same way, and never its parent's. A zero-parameter `.then(() => ...)`
callback is walked as a nested block and reads no `const`, even its own;
that is a known under-credit, not false credit.

Amended 2026-10-14 (#6654, typed owner): `==` between two numbers
answers as `===` does, so a finite number literal the list above refuses (`0`, `1`,
`-0`, `1.0`), optionally negated, parenthesized or under `as`, also
reads `exact_value` / strong when the owner's own source declares its
return type as exactly `number` (`function f(...): number` or an arrow
`(...): number =>`) and the assertion observes the whole result of one
owner call through a binding the test file imports from the owner's
module: `ns.f(...)` on a namespace import, `g(...)` on a renamed import
`{ f as g }`, or, when the file imports `f` by name from that module,
`f(...)` or a local bound once to it. A bare `f(...)` the file imports
from another module never qualifies, even when a namespace call relates
the test.
A method owner, a generic, `async` or generator owner, any other
annotation (`number | boolean`, `Promise<number>`, `number[]`) or none,
a wrapped, compared or member-read result (`Math.min(f(x), 5)`,
`f(x) > 0`, `f(x).toFixed(0)`), a test that declares its own `f`, a
variable expected value (even a test-local `const one = 1`, which
example 10 does not resolve because `1` is coercible), and chai's
negated `.not.to.equal(1)` keep `relational_check` / weak. chai's TDD
`assert.equal` is loose `==` too and qualifies. The declared type is
trusted: an owner whose body returns a non-number despite it (an `any`
cast, an implicitly `any` value such as `JSON.parse(s).count`) is out
of scope, as it is for every other TypeScript type ripr reads.

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

**Rule 4. A spy on an exported module object pins the changed call.**
(Added 2026-10-15.) On a SideEffect or CallDeletion seam, a
`toHaveBeenCalledWith` assertion on `spy` with rule 1's primitive-literal
payload reads `mock_expectation` / strong when all of these hold:

- the changed line calls `o.m(...)`, the owner names `o` only there
  (parameters included), the owner body makes no other call and runs the
  changed line once (rule 1's loop, recursion and nested-function
  check), its parameter list has no default, its body holds no
  non-ASCII character, and
  every word of the body outside member names is a parameter, a name the
  body declares with `const`, `let` or `var`, `o`, or one of `return`,
  `if`, `else`, `const`, `let`, `var`, `typeof`, `void`, `true`,
  `false`, `null`, `undefined`, `as`, `string`, `number` and `boolean`
  (a module binding such as `config.on` can be a getter or a `toString`
  that calls `o.m` itself), and the owner starts at the module's top
  level;
- the owner's module declares `export const o = { ... }` (no type
  annotation) at its top level, so the exported `o` the test imports is
  the object the owner calls, and the object literal holds only method
  shorthands `m(params): T { ... }` whose bodies make no call and name
  nothing but their parameters and the owner words above outside member
  names (amended 2026-10-17: the spy wraps the original method, so a
  method that calls anything, itself through `this` included, has a
  parameter default (which runs inside the call), or is a spread,
  computed key, getter or property value could record the asserted
  payload whatever the owner sent), and the module has no `import`,
  `require`, `eval`, `from` (re-export), `exports`, `module` or
  `globalThis` and names `o`, member positions included, only in that
  declaration and the owner's call (`o.m = jest.fn(); o.m("x")` at the module's top level
  would leave a mock with calls);
- the test file imports `o` by name (renaming allowed) from a specifier
  that resolves to the owner's module, and its callback declares
  `const|let|var spy = jest.spyOn(o, "m")` (or `vi.spyOn`) at its top
  level, names `o` only there and `spy` only there and in `expect(spy)`,
  calls the owner exactly once, and spells no word but `const`, `let`,
  `var`, `spy`, `o`, `jest`, `vi`, `spyOn`, the owner's name, `expect`,
  `toHaveBeenCalledWith`, `true`, `false` and `null`, with no template
  literal and no non-ASCII character;
- the test file's top level is imports that each bind a name (no empty
  `{}` and no `type` clause), each from the owner's module,
  `@jest/globals` or `vitest`, and then exactly one `test(...)` or
  `it(...)` call (amended 2026-10-17: a hook, an earlier test, a
  top-level statement or another module's top level runs before the test
  body and can leave a mock with calls on `o.m`, and `spyOn` on a method
  that is already a mock returns that mock with those calls);
- the test file spells `spyOn` exactly once (comments included) and has
  no `\u` escape, dynamic `import(`, `each`, `concurrent`, `retryTimes`,
  `mock`, `doMock`, `unstable_mockModule`, `resetModules`,
  `isolateModules`, `isolateModulesAsync`, `require`, `requireActual` or
  `eval`.

A spy records every call that reaches `o.m` while it is installed, from
any code, and `jest.spyOn` on a method that is already a spy returns that
spy with its earlier calls. The conditions close each way a matching call
could come from elsewhere: the owner makes no other call, the test makes
no call but the spy, the one owner call and its assertion, and with no
word but those the test runs no helper, getter, constructor or `await`
(which would let other queued work run) between installing the spy and
asserting. One test after the imports, one `spyOn` in the file and no
repeated test (`each`, `concurrent`, `retryTimes`) means no earlier code
left calls on the spy. Known limit: code ripr does not read, a Jest `automock` or
`restoreMocks` setting, a Jest
`setupFiles` / `setupFilesAfterEnv` module or a Vitest run with
`isolate: false` whose earlier file left `o.m` a mock with calls, can
still satisfy the assertion.
No module mock, reset or isolation means the test's `o` and the owner's
`o` are one object. A spy that keeps the method's implementation records
the call either way. A different spied method, a spy on an object
imported from another module, `.mockImplementation(...)` after the spy,
or anything outside the allowed words keeps `mock_expectation` / medium.

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
7. **Typed number owner (rule 2, amended 2026-10-14).** Adopted: credit
   `0` and `1` (and any other finite number literal) when the owner
   declares `number` as its return type and the assertion observes the
   whole owner result. The reading lives in the classifier
   (`ts_typed_loose_number_facts`), which has the owner; extraction only
   marks the candidate assertion (`loose_number_expected`), so chai's
   negated `equal`, which shares the matcher name, is never a candidate.
   Rejected: read every legacy `equal(x, 1)` strong, which credits
   `assert.equal(isEmpty(x), 1)` against a boolean. Rejected: infer the
   result type from the returned expression, which needs type inference
   ripr does not do. Rejected: method owners, because the whole-result
   check reads only `f(...)` and `ns.f(...)`, so a same-named free
   function the test calls could borrow a method's annotation. Pins are
   keyed by test file, line, observed expression, matcher and expected
   value, so a second assertion on the same line with another expected
   value does not share the credit. Rejected: keep `0` and `1` weak,
   which leaves `nodetest-legacy-equal-literal-one` (a discriminated
   test) a false gap.
8. **Test-file mock factory (rule 1, amended 2026-10-15).** Adopted:
   `const db = makeDb();` reads as the flat literal `makeDb` returns when
   the factory has no parameters, its body is only that `return`, and the
   file names it only as direct calls. Each call evaluates the literal
   afresh, so the test's `db.write` is a new `jest.fn()` no other code
   holds, exactly as with the inline literal. Rejected: factories with
   parameters or statements before the `return`, which can hand back a
   shared or caller-supplied mock, and a bare `const send = makeSend()`
   returning a function mock, which this amendment does not need.
   Rejected: keep every helper-made mock medium, which leaves
   `jest-spec0243-ex30-factory-db` (a discriminated test) a false gap.
9. **A mock binding is exactly a fresh mock (rule 1, amended
   2026-10-15, #8138).** Adopted: the binding's value must be the mock
   call alone, with no implementation or an arrow one, ended where the
   statement ends. A named or `function` implementation can be called
   with the mock object as `this` and record the asserted call on
   itself, and a trailing `&& pre.write`, `.inner` or `.bind(pre)`
   (including one on the next line) binds another object. Rejected:
   refuse every implementation argument, which drops arrow
   implementations (`jest.fn((m) => m)`) that cannot reach the mock.
   Rejected: a full expression parse, which the text reader does not
   have; the statement-end check reads only what can follow.
10. **Module-object spy (rule 4, 2026-10-15; amends example 5).**
   Adopted: credit a spy on an exported top-level `const` object when the
   owner's only call is the changed `o.m(...)` and the test body is only
   the spy, one owner call and the assertion, spelled from a fixed word
   list. The word list replaces a list of refused shapes because every
   word outside it (a helper, a getter read, `new`, `await`) is a way
   another call can reach the spy, and a list of refused shapes misses
   the ones nobody wrote down. Rejected: credit any `jest.spyOn` pin,
   which credits a test whose helper or earlier test also calls `o.m`
   with the pinned payload. Rejected: keep every spy medium, which leaves
   `jest-spec0247-ex05-spyon-send` (a discriminated test) a false gap.
   Rejected for now: spies set in hooks, on namespace imports
   (`jest.spyOn(ns.o, ...)`), on objects the owner's module imports from
   a third module, and tests with more statements; each needs its own
   reach argument and none is in the corpus.

## Required Evidence

- Each rule moves a finding to a stronger class only through an assertion
  whose expected value is a literal in the test.
- On the TypeScript verdict corpus, false verdicts fall from 17/101 to
  10/101, seven Problem rows move to `credited` (all but
  `ts-vitest-checkout-notify`), and no other row moves.
- Amended 2026-10-14: `nodetest-t-assert-equal-variable` moves from `gap`
  to `credited` (false verdicts 7/127 to 5/127 together with the
  RIPR-SPEC-0252 amendment), and no other row moves.
- Amended 2026-10-14 (typed owner): `nodetest-legacy-equal-literal-one`
  moves from `gap` to `credited`, and no other row moves.
- Amended 2026-10-15 (mock factory): `jest-spec0243-ex30-factory-db`
  moves from `gap` to `credited`, and no other row moves.
- Amended 2026-10-15 (#8138, decision 9): no corpus row moves; the
  tightened binding refuses only shapes no corpus case uses.
- Added 2026-10-15 (rule 4, decision 10): `jest-spec0247-ex05-spyon-send` moves from
  `gap` to `credited`, and no other row moves.

## Non-Goals

- No credit for `toHaveBeenCalledWith` on a module mock, a spy (except
  rule 4's spy on an exported module object), a mock
  made by a helper (except the test-file mock factory of decision 8), an
  object payload, a mock the owner reaches other
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
   `write` key: `weakly_exposed`. Amended 2026-10-18: `...defaults` is
   `exposed` when `const defaults = { flush: jest.fn() };` is a top-level
   test-file `const` object literal with plain `key: value` entries, no
   `write` key, no spread and no `__proto__`, and the file names `defaults`
   only in that declaration and after `...` spreads; a source with a `write`
   key (quoted, computed or a method included), a nested spread, a shorthand
   entry, a `let`/`var` binding, a call or other initializer, a member
   access or `Object.assign(defaults, ...)`, an import, an `export` or
   `declare` binding, a body-local `defaults` (including an object-rest
   binding `const { a, ...defaults } = cfg()` or a rest parameter), a
   spread outside a `= { ... }` literal, a spread of an expression or
   call, or a file that calls `eval`, uses `with` or holds a `\u` escape
   stays `weakly_exposed`, and a spread inside a test-file factory
   literal stays refused. Amended 2026-10-15: example 2 with
   `function makeDb() { return { write: jest.fn() }; }` in the test file
   and `const db = makeDb();` is `exposed`; a factory with a parameter, a
   spread, a second statement, an `export`, `async`, a generator, a
   `let makeDb` binding, or a `write` value that is not a bare `jest.fn()`
   (`jest.fn(function () { this.write(2); })`, `jest.fn() && pre.write`),
   a file that reassigns, passes on or shadows `makeDb` or calls `eval`,
   or a binding `makeDb().inner`, `makeDb()` continued on the next line
   (`.inner`, `&& pre`) or `makeDb(1)` stays `weakly_exposed`, and so
   does a test that also calls `db.write(2)` itself. Amended 2026-10-15
   (#8138): `const send = jest.fn((m: string) => m)`,
   `vi.fn<(m: string) => void>()`, or `jest.fn()` with no `;` before a
   new statement on the next line still reads `exposed`;
   `jest.fn() && pre`, `jest.fn(record)`, `jest.fn(function () {})`, a
   next line opening with `&&`, `.bind(pre)`, `instanceof` or `(pre)`,
   or for example 2 `{ write: jest.fn() }.inner`,
   `{ write: jest.fn().mockName("w") }` or `{ write: jest.fn() && pre.write }`
   stays `weakly_exposed`.
5. As example 1 with `send` not an owner parameter, with
   `const send = jest.spyOn(mailer, "send")`, a `jest.mock` factory mock
   or `const send = makeSend()`, with an object payload `{ id: 7 }`, or
   for `ship(notifier, cb)` whose changed line is
   `notifier.send("shipped")` while the test passes its `send` mock as
   `cb`: `weakly_exposed`. Amended 2026-10-15: the `jest.spyOn(mailer,
   "send")` case is example 12, `exposed` under rule 4.
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
   `assert.equal(x, 1)` weak, as legacy `node:assert` does. Example 11
   reads `0` and `1` (from either library) strong on the whole result
   of an owner declared to return `number`.
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
    `relational_check` / weak. A `.then((result) => { const want = 30;
    assert.equal(result, want); })` or `[10].forEach((input) => { const
    want = 30; assert.equal(tier(input), want); })` callback declares
    `want` itself: `exact_value` / strong. The same own `const` in a
    zero-parameter `.then(() => ...)` callback: `relational_check` / weak
    (known under-credit).
11. `export function lineCount(items: string[]): number` and
    `assert.equal(lineCount(["a"]), 1)`, `assert.equal(lineCount([]), 0)`,
    `assert.deepEqual(lineCount(["a"]), 1)` or
    `const n = lineCount(["a"]); assert.equal(n, 1)`: `exact_value` /
    strong, `exposed`. The same with no return annotation,
    `: number | boolean`, `Math.min(lineCount(["a"]), 5)`,
    `lineCount(["a"]) > 0` or `lineCount(["a"]).toFixed(0)` against `1`,
    a test-local `lineCount`, or a class method owner
    `lineCount(): number` (the whole-result check reads only a free call,
    which a method's annotation does not cover): `relational_check` / weak.
12. `src/mail.ts` is `export const mailer = { send(m: string) { return
    m; } };` and `export function announceMail(id: number): void {
    mailer.send("order " + id); }`; the test imports `{ announceMail,
    mailer }` from it and runs `const send = jest.spyOn(mailer, "send");
    announceMail(7); expect(send).toHaveBeenCalledWith("order 7");`:
    `mock_expectation` / strong, `exposed`, with `vi.spyOn`, `it`, a
    renamed import, or imports from `@jest/globals`. The same stays
    `weakly_exposed` with any of these in the owner: a second call
    (`log(id)`, `id.toFixed(0)`), a second mention of `mailer`, a read
    of another module binding (`void config.suffix`), a parameter
    default (`pre = warm()`), a non-ASCII name, `export let mailer`, an
    unexported or imported `mailer`, `new Date`, or a getter read inside a
    template (`` `${config.suffix}` ``); in the owner's module: a method
    that calls (`this.send(...)`, `relay(m)`), has a parameter default,
    or is a property value, spread or getter, `export const mailer:
    Mailer =`, `mailer.send = jest.fn()` after the object, an
    `exports.mailer.send = jest.fn()` (CommonJS output makes it the same
    object), an `export * from`, or an `import`; in the test: `mailer.send("order 7")`, `send("order 7")`, a second
    `announceMail(...)`, a helper call, a getter read (`other.warm`), a
    non-ASCII helper name, `` announceMail`7` ``, `await`, an arrow, the
    spy in a nested block, a spy on `"other"`, `.mockImplementation(...)`,
    or `mailer` imported from another module; or in the file: a second
    or commented `spyOn`, a `\u` escape, a dynamic `import(`,
    `test.each`, `test.concurrent`, `jest.mock(...)` or
    `jest.retryTimes(...)`, a hook (`beforeEach`), an earlier test, a
    `describe` wrapper, a top-level statement, a side-effect, empty or
    `type` import, or an import from any other module.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/literal_matcher_tests.rs`
  pins examples 1 to 9 at the rule level;
  `mock_payload_on_a_test_file_factory_mock_is_credited` pins the amended
  example 4, and `mock_payload_binding_must_be_exactly_a_fresh_mock` pins
  decision 9.
- `fixtures/typescript-verdict-corpus` pins the amended example 4:
  `jest-spec0243-ex30-factory-db` reads `credited`.
- `crates/ripr/src/analysis/language/typescript/tests/mock_spread_source_tests.rs`
  pins the 2026-10-18 spread-source amendment of example 4;
  `jest-mock-fn-spread-object` reads `credited`.
- `crates/ripr/src/analysis/language/typescript/tests/spy_module_object_tests.rs`
  pins example 12; `spy_file_checks_read_the_committed_test_file` pins that
  the spy file checks read HEAD's test file, not the working tree's, under
  committed-history analysis; and
  `crates/ripr/src/analysis/language/typescript/mock_payload_guard/spy.rs`
  `module_object_call_needs_an_owner_at_the_module_top_level` pins the
  top-level owner condition, which the owner extractor cannot reach today.
- `fixtures/typescript-verdict-corpus` pins example 12:
  `jest-spec0247-ex05-spyon-send` reads `credited`.
- `crates/ripr/src/analysis/language/typescript/assertion_library_tests.rs`
  keeps the loose-equality grading with coercion-reachable literals, and
  `loose_equality_reads_a_test_local_literal_const` pins example 10.
- `crates/ripr/src/analysis/language/typescript/tests/typed_loose_equality_tests.rs`
  pins example 11.
- `fixtures/typescript-verdict-corpus` pins example 11:
  `nodetest-legacy-equal-literal-one` reads `credited`.
- `fixtures/typescript-verdict-corpus` pins example 10:
  `nodetest-t-assert-equal-variable` reads `credited`.
- `fixtures/typescript-verdict-corpus` pins the eight Problem rows: seven
  `credited`, and `ts-vitest-checkout-notify` still `gap`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/mock_payload_guard.rs`:
  `MockCallFacts` (rule 1); `credits_with_factories`,
  `factory_call_is_initializer` and `keys_mock_once` (decision 8),
  `closed_mock_call` and `ends_statement` (decision 9), fed by
  `crates/ripr/src/analysis/language/typescript/tests_extract.rs`:
  `mock_factories`.
- `crates/ripr/src/analysis/language/typescript/mock_payload_guard/spy.rs`:
  `module_object_calls`, `spy_safe_test_file` and `credits_spy` (rule 4),
  read through `MockCallFacts::read_module_object_calls`,
  `admit_spy_files` and `credits_in_test`.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `strongest_family_matching_oracle` and `ts_changed_value_is_observed`
  read the credited payload as strong (rules 1 and 4), and `project_credited_mock_rows`
  shows it as strong on the related-test row (rule 1).
- `crates/ripr/src/analysis/language/typescript/literal_matcher_guard.rs`:
  `loose_equality_pins_literal` (rule 2), `loose_literal_consts` and
  `resolve_loose_literal_const` (rule 2's test-local `const`),
  `inline_snapshot_is_literal` (rule 3), `finite_number_literal_text`
  (rule 2's typed owner).
- `crates/ripr/src/analysis/language/typescript/typed_loose_equality_guard.rs`:
  `ts_owner_declares_number_return` and `ts_typed_loose_number_facts`
  (rule 2's typed owner), read by the classifier's pin facts.
- `crates/ripr/src/analysis/language/typescript/oracle.rs`: assertion
  extraction applies rules 2 and 3; `mock_payload_from_assertion` records
  multi-argument literal payloads.
- `crates/ripr/src/analysis/language/typescript/static_limit.rs`: a
  strong snapshot carries no snapshot limitation.

## Metrics

- `typescript_literal_matcher_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
