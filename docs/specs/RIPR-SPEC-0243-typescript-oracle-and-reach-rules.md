# RIPR-SPEC-0243: TypeScript oracle and reach rules

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

- #1235 (TypeScript exposed observation guard, RIPR-SPEC-0098)
- #1239 (import-alias owner call, RIPR-SPEC-0102)
- #4103 (default-import and dynamic-import owner credit)
- #4554 (workspace package-name resolution)
- #6654 (corpus cases scored false actionable under the first draft)
- #7315 (literal rejection equality observes the rejected reason)

Linked PRs:

- #6686 (TypeScript verdict corpus; mutation evidence for rule 10)


Support-tier impact:

- No tier change. TypeScript stays Preview. Rules 3 to 6 and rule 8
  weaken credit: a finding may move from `exposed` to `weakly_exposed`,
  `static_unknown` or `no_static_path`, and a reported strength may drop.
  Rule 7 adds credit only where an accepted spec promises it: it restores
  the `exposed` class RIPR-SPEC-0102 states for an alias-rename import.
  Rule 1 adds reach credit as an extension: it reads the awaited form of
  the `test` root that RIPR-SPEC-0027 lists. Neither RIPR-SPEC-0027 nor
  RIPR-SPEC-0085 names the `await` form, so rule 1 is recorded as intended
  drift. Rule 2 raises a negated Jest
  assertion from `unknown` to a weak or smoke strength, which matches the
  negated forms RIPR-SPEC-0027 (chai `.not`) and RIPR-SPEC-0085 (`t.not`)
  already state; no class moves from it, because only a strong oracle
  reaches `exposed`. Rule 10 adds credit from mutation evidence (#6686):
  a literal message check in chai `throw` or `node:assert`
  `throws`/`rejects` caught every mutant of its line, so the uncredited
  reading was a false actionable verdict.
  Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name. The TypeScript adapter starts emitting the
  existing `static_unknown` class under rule 8.
- Amends RIPR-SPEC-0027 (snapshot strength), RIPR-SPEC-0097 (base `Error`
  payload), RIPR-SPEC-0098 (guard scope) and RIPR-SPEC-0099 (workspace
  package names).

## Problem

The TypeScript preview adapter decides oracle kind, strength, reach and
class with its own chain in
`crates/ripr/src/analysis/language/typescript/`. Ten specs touch parts of
it. None states the whole chain, and two contradict the code: RIPR-SPEC-0027
calls snapshots weak where the code reads medium, and RIPR-SPEC-0098 says
value families are not guarded where the code guards ReturnValue and
FieldConstruction. Several links overstate or understate.

Measured with `ripr check --format json` on one-function git fixtures
(`src/lib.ts` holds `price(amount)`; the diff changes `return amount - 10`
to `return amount - 20`; the test imports `price` from `../src/lib`). The
binary was built from d52f0eff, whose TypeScript source differs from main
bcb0be576 only by two `miss: None` field additions. The scenario scripts
were not committed; each row below states its input.

| Test snippet | Today | What it pins |
| --- | --- | --- |
| `expect(price(150)).not.toBe(150)` | `unknown` / unknown, `weakly_exposed`, plus `typescript_oracle_helper_gated` naming `expect(...)` | that the result is not 150 |
| file-local `const expect = (x) => ({ toBe() {} })`, then `expect(price(150)).toBe(999)` | `exact_value` / strong, `exposed` | nothing |
| chai-imported `expect(price(150)).toBe(140)` | `exact_value` / strong, `exposed` | nothing: chai has no `toBe` |
| `toThrow(Error)` after `"empty"` changed to `"blank"` | `exact_error_variant` / strong, `exposed` | any thrown `Error` |
| `import price from "../src/lib"` with no default export | `direct_owner_call`, `exposed` | nothing: the binding is not `price` |
| owner `add`; `add(1, 2); expect(address).toBe("x")` | `exposed` | `address`, not the result of `add` |
| `import { price as p }`; `expect(p(150)).toBe(140)` | `weakly_exposed`, `propagation_unknown` limitation | the changed result |
| node:test ESM `await test("price", () => { assert.equal(price(150), 140) })` | `no_static_path`, no disclosure | the changed result |
| `jest.mock("../src/other")` plus `toBe` | `exposed`, `gap_state: static_limitation`, plus `typescript_mock_only_observer` ("observed behavior is opaque") | the changed result |
| `src/lib.ts` returns `fee(amount - 10)` with `fee` imported from `./other`; the test has `jest.mock("../src/other", () => ({ fee: () => 5 }))` and `expect(price(150)).toBe(5)` | `exposed`, `mocked_module` (measured in review) | nothing: the mock swallows the changed value |
| `expect(price(150)).toMatchSnapshot()` | `snapshot` / medium, `weakly_exposed` | an unseen stored value |
| ReturnValue change, only `expect(() => price(150)).toThrow("bad")` | class `weakly_exposed`, finding oracle `exact_error_variant` / strong | a throw the change does not touch |

The rows that read `exposed` while pinning nothing are false exposure. The
alias row and the `await test` row are false downgrades of a real
discriminator. The unrelated-mock row is a correct class with a
contradicting limitation. The rest is either correct but unstated
(snapshot) or a display mismatch filed separately.

## Behavior

Paths are relative to `crates/ripr/src/analysis/language/typescript/`.
Paragraphs headed Rule 1 to Rule 11 mark behavior changes. Rules 1 to 8
are numbered in reading order; rules 9 to 11 were added later. Every other statement below is current behavior and is
normative.

### Verdict ladder

`classifier.rs` decides the class in this order; the first match wins:

1. no related test: `no_static_path`; reach, observe and discriminate are
   No. The missing text names an unresolved alias when one blocked the
   import, otherwise it says no test references the owner.
2. no related test with an oracle-eligible relation (only heuristic
   relations): `weakly_exposed`, all stages Weak.
3. every oracle-eligible relation is `ModuleEntryCall`: `weakly_exposed`.
4. the family-matched strength is Strong, the predicate boundary is
   witnessed (Predicate family only), and the observation guard passes:
   `exposed`, or `static_unknown` when rule 8 applies.
5. Strong, but the boundary or the guard failed: `weakly_exposed` with the
   boundary limitation or the `propagation_unknown` observation limitation.
6. otherwise `weakly_exposed` with a weak-oracle summary for the kind.

The adapter never emits `reachable_unrevealed`, `infection_unknown` or
`propagation_unknown` as a class, and emits `static_unknown` only under
rule 8. Infection and
propagation stage evidence is always Unknown. A related test with no
assertion is `weakly_exposed`, not `reachable_unrevealed`.

### Test registration

- A test file is a routed TS/JS source whose stem is `test` or starts
  with `test-`, or ends in `.test`, `-test`, `_test`, `.spec` or `.cy`; or
  that sits under a `test`, `tests` or `__tests__` directory; or that
  matches `spec/**/*[sS]pec.*`. `src/x.e2e-spec.ts` is not a test file.
  A file with a parse error yields no tests.
- Test roots are `test`, `it` and `specify`; describe roots are
  `describe`, `context` and `suite`, with the modifiers and inactive forms
  RIPR-SPEC-0027 lists.
- A registration is an expression statement whose expression is the root
  call. `const x = test(...)` registers nothing.

**Rule 1. An awaited root registers.** An expression statement whose
expression is `await <root call>` at file or describe level registers
exactly as the bare call does. This covers the node:test ESM form
`await test("name", fn)`. It extends the `test` root RIPR-SPEC-0027 lists;
no accepted spec names the awaited form. An awaited subtest
`await t.test(...)` inside a test body is not a registration.
Before an awaited registration contributes assertions, the existing admission
scanner must recognize its runner root at the call site. File-local helpers,
unrelated imports and enclosing callback/loop bindings cannot turn an unused
callback into test evidence. This fence applies to the newly supported awaited
form; it does not change bare-root registration semantics. A sibling suite's
helper is outside this lexical scope. Re-exports do not create local runner
bindings, and an ESM namespace object is not a callable registration root.
The awaited CommonJS exemption requires an immutable runner declaration and
either ambient `require` syntax or an immutable loader created by a locally
imported `node:module`/`module` `createRequire(import.meta.url)`. Mutable loaders
or runner bindings and same-spelled helper factories remain uncredited.
Runtime module-mode and loader resolution are outside this static claim.
The resolved test/suite kind must agree with the supported local spelling.
Mismatched aliases (`describe as test` or `test as describe`) receive no trusted
test record and retain a typed partial-extraction limitation, including an
assertion-only callback with no nested test call. This does not expand aliases.

### Assertion collection

- Only the test callback's own statement list is walked, through blocks,
  `if`/`else`, loops, labels, `switch`, `try`/`catch`/`finally` and
  `with`. An assertion is read from an expression statement or a
  `return` argument, after removing one `await`.
- An assertion inside a nested callback (`forEach`, `waitFor`, `act`, a
  subtest) or inside a declaration initializer is not collected, except
  the three forms RIPR-SPEC-0246 rule 2 names.

**Rule 11. An inline literal table reads one concrete case per row.**
Added after the TypeScript verdict corpus (#6686, #6654) showed
`test.each` tables that pin a boundary scored false actionable. When a
`test.each` / `it.each` table is an inline array, in the same call, of
at most 64 rows that are each an inline array of literals, and the
callback's plain parameters bind the row positions one to one, each row
reads as the callback body with each parameter replaced by that row's
literal: the input literals reach the boundary witness and the expected
literal is the assertion's expected value, as a hand-written test per
row would. Anything else keeps the row-parameter assertions and
`typescript_table_case_unresolved`: a table held in a variable, a spread
or non-literal cell, object rows, destructured, defaulted or rest
parameters, a row whose length differs from the parameter count, a
parameter used other than as a plain read, a parameter re-declared or
shadowed in the body, a nested `function`, a template substitution, a
regular-expression literal, a non-ASCII parameter or callback body, or a
substituted body that does not re-parse.

### Recogniser order

Each assertion expression is tried in this order; the first match wins:

1. chai BDD `expect(...).<chain>.<terminal>`, only when the file binds
   chai `expect` or the chai module;
2. Jest/Vitest `expect(...)[.resolves|.rejects][.not].<matcher>(...)`;
3. execution-context receiver `<first param>.<method>(...)`, only when
   the callback has a first parameter that does not shadow an imported
   assertion binding;
4. `node:assert` or chai `assert` through an imported binding.

**Rule 2. Jest negation is an assertion with capped strength.** `.not`
between `expect(...)` (or `.resolves` / `.rejects`) and the matcher is
recognised. The negated matcher reads:

- `toBe`, `toEqual`, `toStrictEqual` and the relational list:
  `relational_check` / weak;
- the smoke list: `smoke_only` / smoke;
- `toThrow`, `toThrowError`: `broad_error` / weak, whatever the
  argument; the exact error payload override never applies under
  `.not`;
- the mock list: `mock_expectation` / weak;
- anything else, including snapshots: `unknown` / unknown with
  `typescript_custom_matcher_unresolved`.

A negated matcher operand is not an expected value: a negated assertion
has no `expected_value_or_variant` and no `typescript_oracle_expected`
evidence line, so RIPR-SPEC-0087 G-C never borrows it as a concrete
expected value. A test whose only assertion is negated no longer reports
`typescript_oracle_helper_gated`.

**Rule 3. `expect` must be a test-runner `expect`.** Step 2 accepts an
identifier callee `expect` only when that name is not bound in the file; or
is bound by an import from `vitest`, `@jest/globals`, `@playwright/test` or
`bun:test`; or is bound by destructuring the test callback's context
parameter (Vitest `test("x", ({ expect }) => ...)`). Any other binding of
`expect` makes step 2 produce no assertion: a file-level or test-body
declaration (`const`, `let`, `var`, `function`, `class`), another parameter,
or an import from any other module. A chai-bound `expect` is read only by
step 1, so a chai chain step 1 does not know yields no assertion instead of
falling through to the Jest table. A renamed import (`expect as e`) and
`expect.soft` stay unrecognised.

### Jest and Vitest matcher table

`oracle.rs` `oracle_for_matcher`, after the error payload override:

| Matcher | Kind / strength |
| --- | --- |
| `toBe`, `toEqual`, `toStrictEqual` | `exact_value` / strong |
| `toThrow`, `toThrowError` without an exact payload | `broad_error` / weak |
| `toMatchSnapshot`, `toMatchInlineSnapshot` | `snapshot` / medium (strong for a literal inline snapshot, RIPR-SPEC-0247 rule 3, or a named committed entry, RIPR-SPEC-0258) |
| `toHaveBeenCalled`, `toHaveBeenCalledWith`, `toHaveBeenCalledTimes`, `toHaveBeenLastCalledWith`, `toHaveBeenNthCalledWith` | `mock_expectation` / medium |
| `toBeTruthy`, `toBeFalsy`, `toBeDefined`, `toBeUndefined`, `toBeNull`, `toBeNaN` | `smoke_only` / smoke |
| `toContain`, `toMatch`, `toBeGreaterThan`, `toBeGreaterThanOrEqual`, `toBeLessThan`, `toBeLessThanOrEqual`, `toHaveLength`, `toHaveProperty`, `toBeInstanceOf` (RIPR-SPEC-0249; strong for a predicate's class split), `toMatchObject` (RIPR-SPEC-0250; strong for a listed returned field) | `relational_check` / weak |
| any other matcher | `unknown` / unknown, `typescript_custom_matcher_unresolved` (a same-file strict-equality `expect.extend` matcher reads `exact_value` / strong, RIPR-SPEC-0262) |

`.resolves` uses the same table. For an unshadowed `expect` import from
`vitest` or `@jest/globals`, `.rejects.toBe`, `.rejects.toEqual` and
`.rejects.toStrictEqual` with exactly one concrete literal argument instead
read `exact_error_variant` / strong: they compare the rejected reason, so
they match an error path and cannot credit a normal return value (#7315).
The pinned reason credits a changed throw only when it equals that line's
thrown message; on a message-only change (rule 4) it must equal exactly one
of the old and new messages. A substring of a message passes on neither
version, so it is not credit (2026-10-14).
The existing literal admission also accepts safe all-literal objects.
Their rendered oracle retains `.rejects`. Dynamic arguments, unknown
bindings and implicit globals retain the existing table and limitations;
negated forms retain their existing admission rules. This bounded extension
is pinned by `typescript_rejects_literal_equality` and
`rejection_equality_tests`.

The rejected literal integrates the #6798 expected-value error-liveness
owner: `literal_rejection_credits` reads the pinned reason from
`expected_value_or_variant` (the promoted literal's `error_payload` stays
`None`). On a message-only change the pinned reason must separate the old
and new thrown messages; otherwise it must match the changed line's own
thrown message, so a reason the changed line never produces — the
unexercised second throw — reads `broad_error` / weak for that change.
`rejection_literal_cannot_expose_an_unexercised_error_branch` pins this
verdict.

A medium snapshot never reaches
`exposed` by itself, because only a strong oracle does; it carries
`typescript_snapshot_discriminator_unresolved`. A literal inline snapshot
(RIPR-SPEC-0247 rule 3) or a committed entry the test names
(RIPR-SPEC-0258) reads strong and can. A mock matcher never
reaches `exposed` either, even with a literal payload.

The exact error payload override reads `exact_error_variant` / strong for
`toThrow` or `toThrowError` (with or without `.rejects`) with exactly one
argument that is a string literal, an all-literal object, or a member path
whose first segment starts with an ASCII uppercase letter, and for
`.rejects.toMatchObject` with an all-literal object. A regex, a lowercase
identifier and a template literal stay `broad_error` / weak.

**Rule 4. A payload that passes on both versions is not exact.** A
payload whose whole text is `Error` reads `broad_error` / weak, because
every thrown error is an `Error`, unless both sides throw under the same
tokens before `throw`, one a primitive literal and the other a built-in
error class (`throw "bad"` changed to `throw new Error("bad")`, or back),
because no class check matches a primitive;
`globalThis.Error` already reads `broad_error` under the uppercase-first
gate. A second exception covers a message-only change: the changed line
is a `throw` line or a `Promise.reject(...)` line, and every token that
differs between its old and new text lies inside the message argument of
the thrown or rejected error (a literal in a condition on the same line
does not count). The message argument is the first argument of
`new X(...)` or `X(...)` with `X` one undotted name, or otherwise the
whole thrown or rejected operand when it does not open with a call (a
literal, a name or a ternary). A dotted or generic class or factory
(`new errors.ParseError(...)`, `errors.make(...)`, `make<T>(...)`)
refuses the split, and so does a top-level comma outside a call (the
sequence operator in `throw (log(e), e)`). A changed string literal, a changed `${...}`
interpolation and an added or dropped operand inside a present argument
(`"limit " + max` against `"limit "`) all count; adding the argument
itself (`new Error()` against `new Error("x")`) does not. A factory's
first argument is read as its message even when it is another value
(`make(code, "a")`). A change to the class of `new X(...)` or `X(...)`,
to a later argument or after the call does not count. A class inside an
operand that does not open with a call (a ternary choosing between two
`new` expressions) is part of the operand, so such a change counts, which
only removes credit. A respelling whose literals join to the same
message on both sides (`"unsupported currency"` against
`"unsupported " + "currency"`) does not count either; a respelling that
needs runtime values to compare (two interpolations) is not recognised
and counts, which only removes credit; it keeps the ladder's reading (amended 2026-10-14;
before that, only changed string literals counted). On such a change a
payload credits only if it separates the two versions: it matches
exactly one of the old and new messages. A test that passes on the new
side and one that pins the old message and fails on the new side both
discriminate the change; a payload that matches both, or neither, does
not. A class payload
(`TypeError`, `Errors.ParseError`) reads `broad_error` / weak, because
the same class is thrown on both versions. A string payload is a
substring match in Jest and Vitest, so it reads `exact_error_variant`
only when it is a substring of exactly one of the old and new messages.
Each message is
that side's message argument with adjacent literals of a `+` chain
joined (`"ba" + "d"` is `bad`); when any operand of that chain is not a
literal, the payload reads `broad_error` / weak. A literal respelled
with the same value (`"bad"` to `'bad'`) is not a changed token. `toThrow("blank")` after
`"not blank"` changed to `"blank"` reads `broad_error` / weak. On any
other change, class and string payloads keep
RIPR-SPEC-0097's reading: a class swap, or a changed condition that
decides whether the throw runs, can fail a class check (the seam family
filter still decides the class; see example 13). An object payload is
unaffected.

**Rule 9. Self-comparing matchers are not exact.** A `toBe`, `toEqual`
or `toStrictEqual` whose `expect` argument and matcher argument are the
same token sequence, ignoring whitespace outside string literals
(`expect(price(150)).toBe(price(150))`), reads `relational_check` /
weak. The matcher supplies no independent expected value: both sides
run the code under test, so a change that alters both alike passes. This
mirrors RIPR-SPEC-0238 rule 1.

### Other assertion tables

- Execution-context receiver (AVA, tape, node:test `t`): the table in
  RIPR-SPEC-0085. The receiver is the callback's first parameter whatever
  its name; no runner import is required. A method outside the table
  yields no assertion. In a test registered through a `test` or `it`
  bound to `node:test`, `t.assert.<method>(...)` on the callback's first
  parameter reads as legacy `node:assert` (RIPR-SPEC-0252 rule 1).
- `node:assert` / chai `assert`: `strictEqual` and `deepStrictEqual` are
  `exact_value` / strong; `equal` is strong only under a strict module or
  the `strict` export; `deepEqual` is strong unless the module is legacy
  `node:assert`; a loose `equal` / `deepEqual` against a literal coercion
  cannot reach is strong (RIPR-SPEC-0247 rule 2); the other equality forms and `match` are
  `relational_check` / weak; `ok`, chai `is*` and a callable `assert(x)`
  are `smoke_only` / smoke; `throws`, `doesNotThrow`, `rejects` and
  `doesNotReject` are `broad_error` / weak, except as rule 10 states. A binding shadowed in the test
  body, a callback parameter or an enclosing describe scope does not
  count.
- chai BDD: `equal`, `equals`, `eq`, `eql`, `eqls` are `exact_value` /
  strong, or `relational_check` / weak under `not`; `throw`, `throws` and
  `Throw` are `broad_error` / weak, except as rule 10 states; `include`,
  `match`, `above`, `below`, `least`, `most`, `lengthOf` and the like are
  `relational_check` / weak; the property terminals `true`, `false`, `ok`,
  `null`, `undefined` and `exist` are `smoke_only` / smoke. Any other
  chain word or terminal yields no assertion.

**Rule 10. A literal message check in chai or `node:assert` pins the
error.** Added 2026-10-05 after the TypeScript verdict corpus (#6686)
showed each shape catching every mutant of its line while ripr reported
an actionable `weakly_exposed`. These read `exact_error_variant` /
strong:

- chai BDD `throw`, `throws` or `Throw`, not under `not`, with exactly
  one argument that is a string literal. chai matches it as a substring
  of the message, as Jest matches `toThrow("...")`.
- `node:assert` `throws` or `rejects` (not chai `assert`, whose
  signature differs) whose second argument is a regex literal anchored
  with `^` and `$`, with no unescaped `|` outside a character class and
  no repeated wildcard (`.`, a character class, an escaped class such as
  `\S`, or a group followed by `*`, `+` or `{`), or an all-literal object
  whose top-level `message` is a string literal.

Rule 4's message-only guard applies to all of them: on a message-only
change, a chai string must be a substring of exactly one of the old and
new messages (as rule 4 assembles them), an object `message` must equal
exactly one of them, and an
anchored regex credits only when ripr can match it as a plain literal
(no flags other than `u`, no metacharacters between the anchors other
than escapes) whose text equals `String(err)` on exactly one side.
`node:assert` tests the regex against `String(err)`: ripr knows it as
`Name: message` for a built-in error class (`Error`, `TypeError`,
`RangeError`, `SyntaxError`, `ReferenceError`, `EvalError`, `URIError`)
with a non-empty message, and as the value of a thrown primitive; a user
class may set any `name`, so it does not credit. Otherwise each reads
`broad_error` / weak. A
class argument, an unanchored regex, an identifier and a template literal
stay `broad_error` / weak, as do `doesNotThrow` and `doesNotReject`.

### Aggregation

- The class reads the strongest assertion, over every oracle-eligible
  related test that observes an owner call, whose kind matches the seam
  family under RIPR-SPEC-0104. Ties keep the first one seen.
- `related_tests[].oracle_kind` and `oracle_strength` report each test's
  own strongest assertion, not family-filtered. They describe the test,
  not the class decision.
- `oracle_confidence` is high for a strong oracle with a literal expected
  value, medium for any other strong or medium oracle, low for weak or
  smoke, and unknown otherwise. A non-literal matcher argument (including
  a regex) adds `typescript_dynamic_assertion_unresolved` and does not
  change the class.

### Reach

Candidates come from the first non-empty tier:

1. **Owner-call relations**, over all tests with no package filter:
   - module function: `ModuleValueReference`, the owner name inside an
     `expect(...)` actual, same file or through an import of the owner
     module;
   - method: `ReceiverOwnerCall`, a receiver bound by `new Class(...)` in
     the body or in describe or `beforeEach` scope, a getter read, or a
     constructor call;
   - static method: `ClassMethodCall` through the class name or an import;
   - function, arrow or component, in order: an owner-module mock or a
     spy fabrication blocks every relation; then `DirectOwnerCall` (a
     call boundary, not shadowed, with a declaration anchor);
     `ImportAliasOwnerCall` (`import { owner as local }` and `local(`);
     `ImportedOwnerCall` (a named or namespace import, a default import
     only when the owner is the default export, or
     `const m = await import("<owner module>")` with `m.owner(`); and
     `ReExportChainFollowed` (RIPR-SPEC-0095, at most 4 hops).
2. **Module-entry relations** when tier 1 is empty, package-filtered.
3. **Heuristic relations** when tiers 1 and 2 are empty, package-filtered,
   for function, arrow and component owners, when the test body
   references the owner name and does not import that name from an
   unrelated source. The label is `SameFileProximity` (equal normalized
   file stems), then `DescribeName`, then `TestName`; with no label there
   is no relation.

Relation rank, `uses_oracle` and reason:

| Relation | Rank | Oracle | `relation_reason` |
| --- | --- | --- | --- |
| `DirectOwnerCall`, `ImportAliasOwnerCall` | 5 | yes | `direct_owner_call` / high |
| `ModuleValueReference`, `ReceiverOwnerCall`, `ClassMethodCall` | 4 | yes | `direct_owner_call` / high |
| `ImportedOwnerCall` | 4 | yes | `import_path_affinity` / medium |
| `ReExportChainFollowed` | 4 | yes | `re_export_chain_followed` / medium |
| `ModuleEntryCall` | 3 | yes | `helper_owner_call` / medium |
| `SameFileProximity` | 3 | no | none |
| `DescribeName` | 2 | no | none |
| `TestName` | 1 | no | none |

Specifier resolution: a relative specifier joins lexically, with the
directory `main`, `index` and `outDir` rules of RIPR-SPEC-0099. A
non-relative specifier resolves only through tsconfig `paths` (when
`[typescript] resolve_tsconfig_paths = true`) or through a workspace
package name, which is always on. Anything else is unresolved, and an
owner import through an unresolved alias reports
`typescript_path_alias_unresolved`.

Mocks and spies: `vi.` or `jest.` with `mock`, `doMock`,
`unstable_mockModule` or `setMock`, and `mock.module` from `bun:test` or
`node:test`, are mocks at any depth. A non-string specifier counts as an
owner-module mock (fail closed). `spyOn(x, "owner")` with
`.mockReturnValue`, `.mockImplementation`, `.mockResolvedValue` or
`.mockRejectedValue` removes the trusted relation and adds
`typescript_spy_fabricated_observer`; a bare `spyOn` still relates.

**Rule 5. A default import anchors only a default-exported owner.** A bare
call `local(` through `import local from "<owner module>"` has a declaration
anchor only when the owner is that module's default export. Otherwise the
default binding is an unrelated import that shadows the owner name, for the
direct-call arm and for the shadowing check alike. A namespace binding is
never an anchor for a bare call.

### Observation guard

The guard runs at ladder step 4. Predicate, MatchArm, ErrorPath and
StaticUnknown families always pass it.

- **Value families** (ReturnValue, FieldConstruction) pass when some
  strong, family-matching assertion's `observed_expression` references
  the owner, contains a changed token, or is a bare local whose first
  `local = ...` initializer in the test body references the owner or
  contains a changed token.
- **Effect families** (SideEffect, CallDeletion) pass when a strong
  assertion's observed expression shares a changed identifier token, or
  does not contain the owner name or an admitted free-function import local
  as a substring (the side channel rule, which stays a substring test so
  that it fails closed). For a direct, named-alias or imported free-function
  relation, the same module/export/call/shadow checks used by rules 5 and 7
  and RIPR-SPEC-0251 identify both named and default locals. Mixed direct and
  aliased calls retain every such local even when another call determines
  the relation kind. A same-named free export cannot stand for a method.
  Other effect confirmations and value-family rules stay unchanged (#7514).

**Rule 6. Value-family references match whole identifiers.** In the value
branch, "references the owner" and "contains a changed token" mean a whole
identifier occurrence: the characters on both sides are not `[A-Za-z0-9_$]`.
A member segment counts (`ns.price(` and `c.total(` reference `price` and
`total`). `address` does not reference `add`, and `totalAmount` does not
contain `amount`. The one-hop initializer uses the same test.

**Rule 7. An alias-rename local counts as the owner.** In the value
branch, the observed expression also references the owner when it names,
as a whole identifier, the `local` of an `import { owner as local }` that
gives the same test its `ImportAliasOwnerCall` relation. The one-hop
initializer accepts the same local. This makes RIPR-SPEC-0102's stated
`exposed` class hold. RIPR-SPEC-0251 extends this to a renamed default
import of a default-exported owner, and its rule 2 drops either local
from owner credit when a body-local destructuring names it
(`const { p } = helpers`). For the alias rename this also drops the
`ImportAliasOwnerCall` relation; a default import keeps its
`ImportedOwnerCall` relation, which the import record proves.

### Static limits

`static_limit_for_change` picks at most one limit, in order:
`dynamic_dispatch`, `metaprogramming`, `decorator_indirection`,
`mocked_module`, then `missing_import_graph`. `mocked_module` reads the
mocks of every related test the mock collector selects
(`collect_related_mock_paths`), including heuristic-only links. The limit
is appended to `missing` after the class is chosen. Actionability reads
`gap_state: static_limitation` before the `already_observed` branch
(RIPR-SPEC-0087 F10); RIPR-SPEC-0087 orders actionability only and says
nothing about the class.

**Rule 8. A mocked dependency on the changed line is `static_unknown`.**
When the changed line calls a symbol imported (`imported_symbol_call`)
from a module that the owner file imports (`TypeScriptOwner.imports`),
and every test file holding an assertion that credits `exposed` mocks that
module, a ladder result of `exposed` reads `static_unknown` instead. A
module mock applies to its whole test file and to no other, so it is read
per file: when one crediting assertion sits in a file with no such mock,
`exposed` stands, with the limit as advisory below. ripr reads only mock
calls written in the test file; config-level mocks (`setupFiles`,
`automock`, automatic `__mocks__`) are not modelled. The result then is: reach Yes, observe and
discriminate Unknown, `static_limit_kind: mocked_module`,
`gap_state: static_limitation` and `typescript_mock_only_observer`. The
mock specifier resolves from the test file and the import source from the
owner file; both must name the same module, and the same callee's module
must be mocked in every such file (files that mock different callees each
run the other callee for real, so `exposed` stands). An unresolved mock
specifier counts as a match, and so does an unresolved non-relative
specifier whose last segment names the resolved module's file or
directory (fail closed: its identity is unknown, and the cost is an
abstention, not a credit). Other ladder results are unchanged.

Otherwise a static limit is advisory and never moves the class. An
`exposed` finding then keeps `static_limit_kind`, the limit's `missing`
text and `gap_state: static_limitation`, but does not carry
`typescript_mock_only_observer`, whose text says the observed behavior is
opaque, which the class contradicts.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable documented
decisions and proceed"). Each records the adopted option, why, and the
rejected alternative. Any can be reversed later without touching the rest.

1. **Snapshot strength.** Adopted: `snapshot` / medium, as the code reads,
   matching the Rust chain (RIPR-SPEC-0231 step 6). The class never reaches
   `exposed` from a snapshot, which is what RIPR-SPEC-0027's "weak /
   static-limited" protects. RIPR-SPEC-0027 is amended. Rejected: weak,
   which would split TypeScript from Rust for no class difference.
   Amended by RIPR-SPEC-0247 rule 3: an inline snapshot with literal text
   reads `snapshot` / strong. Amended by RIPR-SPEC-0258: so does a
   `toMatchSnapshot()` whose committed entry the test file names.
2. **Value-family guard.** Adopted: the code's guard on ReturnValue and
   FieldConstruction, because it is the fail-closed reading and existing
   tests pin it (`ts_returnvalue_unrelated_strong_assertion_downgrades`).
   RIPR-SPEC-0098 is amended. Rejected: drop the value guard to match
   RIPR-SPEC-0098's text, which would restore false `exposed` on unrelated
   assertions.
3. **Alias rename (D2).** Adopted: rule 7, alias arm only, because
   RIPR-SPEC-0102 is accepted and states the class stays `exposed`. A
   default-import local got no credit here, because RIPR-SPEC-0102 puts
   default imports out of scope; RIPR-SPEC-0251 now adds it. Rejected: amend
   RIPR-SPEC-0102 to `weakly_exposed`, which would keep a false downgrade
   of the most direct discriminator.
4. **Substring owner match (D3).** Adopted: rule 6 in the value branch
   only. Rejected: whole identifiers in the effect side-channel rule too,
   because there a substring miss grants credit, so tightening it would
   add credit.
5. **Static limits (D4).** Adopted: rule 8, fail closed, read per test
   file so an unmocked crediting file keeps `exposed` (example 33). A mock of a
   module whose symbol the changed line calls can swallow the changed
   value (the measured `fee` counterexample reads `exposed` today), so
   static evidence cannot say the assertion observes the change. The
   owner's imports and the changed-line call are already extracted
   (`TypeScriptOwner.imports`, `imported_symbol_call`). Rejected: keep
   `exposed` under every limit, which the counterexample shows is false
   exposure. Rejected: cap the class under any limit, which would also
   demote exposures where the mocked module is not on the changed line
   (example 24) and where an unmocked imported call runs real code
   (`missing_import_graph` alone).
6. **`toThrow(Error)` (D5).** Adopted: rule 4. RIPR-SPEC-0097 is amended.
   Rejected: drop class payloads entirely, because a specific class does
   pin the thrown type. Rule 4 also closes the class-only payload on a
   message-only change, which passes on both versions, so
   `toThrow(TypeError)` no longer credits one. Rejected: keep it strong,
   which credits that payload. Also rejected: require the class to be a
   changed token, which would demote class payloads on every change that
   is not message-only, such as the condition change in RIPR-SPEC-0097's
   fixture, beyond the over-credit measured here.
7. **Jest `.not`.** Adopted: rule 2, mirroring chai `.not` and AVA `t.not`.
   Rejected: keep `.not` unrecognised, which mislabels `expect` as a
   helper.
8. **`expect` identity.** Adopted: rule 3. Rejected: require an explicit
   runner import, because Jest and Vitest globals are the common form.
9. **`await test(...)` (D8).** Adopted: rule 1, because the false
   `no_static_path` claims no test exists. Rejected: disclose
   `typescript_test_extraction_partial` only, which keeps a real test
   invisible.
10. **Receiver gate.** Adopted: keep "any first parameter". Rejected: a
    runner-import gate, because the fixture
    `fixtures/typescript_tape_equal_oracle` registers tape tests without an
    import and there is no measured over-credit beyond the `done` case.
11. **Mock strength.** Adopted: `mock_expectation` / medium, so a mock
    matcher never yields `exposed`, matching RIPR-SPEC-0231 step 9.
    Rejected: strong for `toHaveBeenCalledWith` with literals, which no
    spec promises. Amended by RIPR-SPEC-0247 rule 1: an all-literal
    payload on a test-local mock the owner calls once through a parameter
    reads `mock_expectation` / strong.
12. **chai and `node:assert` message checks (#6686).** Adopted: rule 10,
    because the corpus mutants show the first draft's `broad_error`
    reading was a false actionable verdict for a literal message check,
    the same evidence the Jest table already credits. Rejected: keep them
    weak, which leaves those cases scored false actionable (#6654).
    Rejected: credit any `throws` argument, because a bare or unanchored
    regex can admit both messages.
13. **Message-only payload reading (#6798).** Adopted: a payload credits
    when it matches exactly one of the old and new messages. Rejected:
    "excludes the old message", the first draft, because the corpus
    scores tests that pin the old message and fail on the new one
    (`jest-spec0234-ex12`, `mocha-spec14-chai-throw-string`) and those
    discriminate the change, while a payload that matches neither
    message fails on both versions whenever the throw runs and so
    separates nothing.
14. **Literal tables (#6798).** Adopted: rule 11, because a literal row
    is the same evidence as a hand-written test with those literals.
    Rejected: resolve tables held in a `const`, which needs scope
    resolution the adapter does not have.

## Required Evidence

- Each Problem table row reads as the acceptance examples state, in JSON
  `class`, `related_tests[]` and `evidence`.
- Rules 2 to 6 and rule 8 move no finding to a stronger class. Golden
  drift lists every finding that moved and why. The RIPR-SPEC-0097
  fixture `typescript_tothrow_exact_oracle` does not move: its change
  (`input === ""` to `input.trim() === ""`) is not message-only, so its
  class, string and object payloads all stay `exact_error_variant`.
- Rules 1, 7, 10 and 11 move a finding to a stronger class only for an
  awaited root (rule 1), an alias-rename local (rule 7; via
  RIPR-SPEC-0251, also a renamed default import), a literal
  chai or `node:assert` message check (rule 10) or an inline literal
  table row (rule 11).
- Rule 8 is the only path to a TypeScript `static_unknown`, and an
  `exposed` finding never carries `typescript_mock_only_observer`.
- No TypeScript finding has class `reachable_unrevealed`.
- The value-family guard keeps the one-hop local credit
  (`const r = price(150); expect(r).toBe(130)` stays `exposed`).

## Non-Goals

- No collection of assertions inside nested callbacks or subtests,
  except the literal `forEach`, returned or awaited `.then` and awaited
  `t.test` forms of RIPR-SPEC-0246 rule 2.
- No resolution of custom matchers, `expect.extend`, or helper bodies,
  except a same-file strict-equality matcher (RIPR-SPEC-0262).
- No expected-value liveness check for ReturnValue or ErrorPath.
- No infection or propagation modeling.
- No change to the per-test display oracle (filed separately).
- No new credit for a default-import local in the observation guard.
  Superseded by RIPR-SPEC-0251: a renamed default import of a
  default-exported owner from the owner's own module now counts as the
  owner; RIPR-SPEC-0095 still does not follow default imports through
  re-export chains.
- No change to Rust or Python oracle classification.

## Acceptance Examples

Base fixture unless stated: `src/lib.ts` is
`export function price(amount: number): number { if (amount > 100) {
return amount - 10; } return amount; }`, the diff changes `amount - 10`
to `amount - 20`, and `tests/lib.test.ts` has
`import { price } from "../src/lib";` and one `test("price", () => { ...
})` holding the snippet. `ripr.toml` enables `typescript`.

1. `expect(price(150)).toBe(130)`: `exact_value` / strong,
   `direct_owner_call`, `exposed` (unchanged).
2. `expect(price(150)).toBeGreaterThan(100)`: `relational_check` / weak,
   `weakly_exposed` (unchanged). `expect(price(150)).toBe(price(150))`:
   `relational_check` / weak, `weakly_exposed` (today `exact_value` /
   strong, `exposed`, inferred; rule 9).
3. `expect(price(150)).toMatchSnapshot()`: `snapshot` / medium,
   `weakly_exposed`, `typescript_snapshot_discriminator_unresolved`
   (unchanged).
   Amended by RIPR-SPEC-0258: with a committed
   `tests/__snapshots__/lib.test.ts.snap` holding the entry `price 1`, it
   reads `snapshot` / strong and `exposed`.
4. `price(150);` with no assertion: oracle `unknown`, `weakly_exposed`
   (unchanged; never `reachable_unrevealed`).
5. `expect(price(150)).not.toBe(150)`: `relational_check` / weak,
   `weakly_exposed`, no `typescript_oracle_helper_gated` (today `unknown`
   / unknown with that limitation).
6. `expect(() => price(150)).not.toThrow()`: `broad_error` / weak,
   `weakly_exposed` (today `unknown` / unknown).
7. `const expect = (x: unknown) => ({ toBe(_: unknown) {} });` at file
   level, then `expect(price(150)).toBe(999)`: no assertion, oracle
   `unknown`, `weakly_exposed` (today `exact_value` / strong, `exposed`).
8. `import { expect } from "chai";` and `expect(price(150)).toBe(130)`:
   no assertion, `weakly_exposed` (today `exposed`).
   `expect(price(150)).to.equal(130)` in the same file: `exact_value` /
   strong, `exposed` (unchanged).
9. `import { expect, test } from "vitest";` and
   `expect(price(150)).toBe(130)`: `exact_value` / strong, `exposed`
   (unchanged, inferred). The same with `@playwright/test` as the import
   source: `exposed` (unchanged, measured in review).
   `test("price", ({ expect }) => { expect(price(150)).toBe(130); })`:
   `exposed` (unchanged, measured in review).
10. `[150].forEach((v) => { expect(price(v)).toBe(130); });`: oracle
    `unknown`, `weakly_exposed` (unchanged). Amended by RIPR-SPEC-0246
    rule 2: the literal-array `forEach` body is collected, `exposed` when
    the assertion pins the changed value.
11. `if (ok) { expect(price(150)).toBe(130); }`: `exposed` (unchanged).
12. `src/lib.ts` is `export function parse(s: string): string { if (s ===
    "") { throw new Error("empty"); } return s; }`, formatted with the
    `throw` on its own line, which is the changed line: `"empty"` changed
    to `"blank"`;
    test `expect(() => parse("")).toThrow("blank")`:
    `exact_error_variant` / strong, `exposed` (unchanged). With the old
    message `"not blank"` instead of `"empty"`: `broad_error` / weak,
    `weakly_exposed` (today `exposed`, inferred; rule 4: `"blank"` matches
    both messages).
13. Same change; `expect(() => parse("")).toThrow(Error)`: `broad_error` /
    weak, `weakly_exposed` (today `exact_error_variant` / strong,
    `exposed`). `toThrow(globalThis.Error)`: `broad_error` / weak
    (unchanged). `toThrow(TypeError)`: `broad_error` / weak (today
    `exact_error_variant` / strong): the change is message-only. With the
    change instead from `throw new Error("empty")` to
    `throw new TypeError("empty")`, `toThrow(TypeError)`:
    `exact_error_variant` / strong, `exposed` (unchanged). With
    `if (s === "") { throw new TypeError("empty"); }` changed to
    `if (s !== "") { throw new TypeError("empty"); }`,
    `toThrow(TypeError)` and `toThrow("empty")`: `exact_error_variant` /
    strong (unchanged): the change is not message-only. The class stays
    `weakly_exposed` (unchanged), because the changed line is a predicate
    and the seam family filter admits no error oracle for a predicate.
    With the throw line changed from `throw "empty";` to
    `throw new Error("empty");`, `toThrow(Error)`: `exact_error_variant` /
    strong, `exposed` (unchanged; the old side throws a primitive).
14. Same change; `expect(() => parse("")).to.throw("blank")` with chai
    `expect`: `exact_error_variant` / strong, `exposed` (today
    `broad_error` / weak, `weakly_exposed`; rule 10, #6686). With the old
    message `"not blank"`: `broad_error` / weak, `weakly_exposed`
    (unchanged). `to.throw(TypeError)` and `to.throw()`: `broad_error` /
    weak (unchanged).
15. Base change; only `expect(() => price(150)).toThrow("bad")`: class
    `weakly_exposed`; `related_tests[0]` reads `exact_error_variant` /
    strong (unchanged; display issue filed).
16. `src/lib.ts` adds `export default function round(n: number): number {
    return Math.round(n); }`. `tests/other.test.ts`:
    `import price from "../src/lib"; test("x", () => {
    expect(price(150)).toBe(130); });`: no owner-call relation,
    `no_static_path` (today `direct_owner_call`, `exposed`, measured with
    no default export).
17. `src/lib.ts` declares `export default function price(...)` with the
    base body. `import price from "../src/lib"` and
    `expect(price(150)).toBe(130)`: `exposed` (unchanged).
    `import cost from "../src/lib"` and `expect(cost(150)).toBe(130)`:
    `import_path_affinity`, `exposed` (RIPR-SPEC-0251; this spec read
    `weakly_exposed`).
18. `import { price as p } from "../src/lib";` and
    `expect(p(150)).toBe(130)`: `direct_owner_call` / high, `exposed`
    (today `weakly_exposed` with `propagation_unknown`).
19. Same import; `const r = p(150); expect(r).toBe(130);`: `exposed`
    (today `weakly_exposed`, inferred).
20. `const r = price(150); expect(r).toBe(130);`: `exposed` (unchanged).
21. `price(150); expect(1 + 1).toBe(2);`: `weakly_exposed`, observation
    limitation naming an unrelated expression (unchanged).
22. `src/lib.ts` is `export function add(a: number, b: number): number {
    return a + b; }`, changed to `a - b`; test imports `add` and runs
    `const address = "x"; add(1, 2); expect(address).toBe("x");`:
    `weakly_exposed` with the `propagation_unknown` observation limitation
    (today `exposed`).
23. `import test from "node:test";
    import assert from "node:assert/strict"; import { price } from
    "../src/lib"; await test("price", () => { assert.equal(price(150),
    130); });`: `exact_value` / strong, `exposed` (today
    `no_static_path`).
24. `jest.mock("../src/other");` and `expect(price(150)).toBe(130)`; the
    base `src/lib.ts` imports nothing: `exposed`,
    `static_limit_kind: mocked_module`, `gap_state: static_limitation`,
    no `typescript_mock_only_observer` (today the class and limit kind are
    the same, and `typescript_mock_only_observer` is present).
25. `jest.mock("../src/lib");` and `expect(price(150)).toBe(130)`: no
    trusted relation, `weakly_exposed` through `test_name`,
    `mocked_module` (unchanged).
26. `import * as lib from "../src/lib";`
    `vi.spyOn(lib, "price").mockReturnValue(130);`
    `expect(price(150)).toBe(130)`: `weakly_exposed`,
    `typescript_spy_fabricated_observer` (unchanged).
27. No static import; `const m = await import("../src/lib");`
    `expect(m.price(150)).toBe(130)`: `import_path_affinity`, `exposed`
    (unchanged).
28. `packages/a/package.json` is `{ "name": "a", "main": "src/lib.ts" }`
    with the base `src/lib.ts`; `packages/b/tests/x.test.ts` imports
    `{ price }` from `"a"` and asserts `toBe(130)`: `exposed` with no
    tsconfig flag (unchanged).
29. `import { price } from "@/lib"` with `resolve_tsconfig_paths` off:
    `no_static_path`, `typescript_path_alias_unresolved` (unchanged).
30. `src/lib.ts` is `export function save(db: Db, v: number) {
    db.write(v + 1); }`, changed to `v + 2`; test
    `save(db, 1); expect(db.write).toHaveBeenCalledWith(3)`:
    `mock_expectation` / medium, `weakly_exposed` (unchanged). With
    `const db = { write: jest.fn() }` in the test, RIPR-SPEC-0247 rule 1
    reads `mock_expectation` / strong, `exposed`.
31. `src/lib.ts` is `import { fee } from "./other";` plus the base
    `price`, whose line reads `return fee(amount - 10);`, changed to
    `return fee(amount - 20);`; the test
    has `jest.mock("../src/other", () => ({ fee: () => 5 }));` and
    `expect(price(150)).toBe(5)`: `static_unknown`,
    `static_limit_kind: mocked_module`, `gap_state: static_limitation`,
    `typescript_mock_only_observer` (today `exposed`, measured in review).
32. Same `src/lib.ts` and change; the test mocks `"../src/unrelated"`
    instead: `exposed`, `static_limit_kind: mocked_module`, no
    `typescript_mock_only_observer` (today `exposed` with it).
33. Same `src/lib.ts` and change, two test files: `tests/a.test.ts`
    has no mock and `expect(price(150)).toBe(130)`; `tests/b.test.ts`
    has the example 31 `jest.mock` and `expect(price(150)).toBe(5)`:
    `exposed`, `static_limit_kind: mocked_module`, no
    `typescript_mock_only_observer`, because the unmocked file credits on
    its own (today `exposed` with it). With only `tests/b.test.ts`: as
    example 31.
34. Same base `parse`, changed to `"blank"`, under `node:test` with
    `import assert from "node:assert";`: `assert.throws(() => parse(""),
    /^Error: blank$/)`: `exact_error_variant` / strong, `exposed` (today
    `broad_error` / weak, `weakly_exposed`; rule 10). With `/blank/`:
    `broad_error` / weak (unchanged; not anchored). With `/^Error:
    (empty|blank)$/`: `broad_error` / weak (unchanged).
35. An async owner `charge(n)` whose line `throw new Error("charge must be
    nonnegative")` changed to `"charge must be positive"`; test `await
    assert.rejects(charge(-1), { message: "charge must be positive" })`:
    `exact_error_variant` / strong, `exposed` (today `broad_error` / weak,
    `weakly_exposed`; rule 10). With `{ name: "Error" }`: `broad_error` /
    weak (unchanged; no `message`).

## Test Mapping

- #7315: `crates/ripr/src/analysis/language/typescript/rejection_equality_tests.rs`
  pins literal rejection equality, resolved-value domain separation,
  unshadowed runner imports and unchanged unresolved forms. Fixture
  `fixtures/typescript_rejects_literal_equality` exercises the real CLI;
  the shared evidence-promotion honesty corpus pins both verdict directions.
- `crates/ripr/src/analysis/language/typescript/tests_extract_tests.rs`:
  `awaited_roots_retain_call_identity_and_complete_extraction` and
  `awaited_unwalked_root_keeps_the_partial_extraction_disclosure` cover rule 1.
- `crates/ripr/tests/cli_smoke/javascript_awaited_roots.rs`: the pinned Nano ID
  consumer covers bare/awaited registration parity, exact/smoke oracles,
  qualification options, assertion and runner shadows, sibling-suite isolation,
  callable local imports, unsupported registration positions,
  and byte-identical upstream provenance through the actual CLI.
- `fixtures/javascript_awaited_registration`: genuine JavaScript production
  source with a separately authored Node consumer and independent runtime contrast.
- Existing: `crates/ripr/src/analysis/language/typescript/tests.rs`:
  `ts_returnvalue_unrelated_strong_assertion_downgrades`,
  `ts_returnvalue_owner_aliased_local_observation_stays_exposed`,
  `ts_returnvalue_unrelated_aliased_local_observation_downgrades`,
  `ts_fieldconstruction_unrelated_strong_assertion_downgrades`,
  `ts_swallowed_console_log_exposed_downgrade`,
  `extract_tests_maps_class_tothrow_to_exact_error_variant_oracle`,
  `extract_tests_keeps_lowercase_ident_tothrow_broad`,
  `find_related_tests_matches_named_import_alias_calls`,
  `default_import_does_not_credit_named_non_default_owner`,
  `overcredit_4103_spy_fabrication_blocks_owner_credit_and_names_limitation`.
- Existing: `crates/ripr/src/analysis/language/typescript/assertion_library_tests.rs`
  and the `tests/` modules beside it: `mock_form_tests.rs`,
  `scope_receiver_tests.rs`, `reexport_chain_tests.rs`.
- Existing: fixture `fixtures/typescript_tape_equal_oracle`.
- Planned: one unit test per changed example (5, 6, 7, 8, 13, 14, 16, 18,
  19, 22, 23, 24, 31, 32, 33, 34, 35), and a verdict corpus holding all
  35 examples, each with
  the asserted parsed subject (registered test, collected assertion,
  relation) checked before the class.

## Implementation Mapping

- #7315: `expect_assertion_from_expression` reads the rejection modifier and
  concrete expected argument; `TypeScriptAssertionBindings` supplies the
  explicit runner import and existing scope filter. The existing
  `ts_oracle_kind_matches_seam` authority handles the resulting error kind,
  and `literal_rejection_credits` in the shared error-payload guard owner
  reads the pinned reason for error-branch liveness.
- `crates/ripr/src/analysis/language/typescript/oracle.rs`: rules 2 to 4, 9
  and 10
  (`expect_call_from_assertion_inner`, `call_expression_is_expect`,
  `oracle_for_matcher`, `safe_error_class_payload_text`).
- `crates/ripr/src/analysis/language/typescript/tests_extract.rs`: rule 1
  (`test_from_statement`, `describe_body_from_statement`) and the
  `expect` binding facts rule 3 reads.
- `crates/ripr/src/analysis/language/typescript/related_tests.rs`: rule 5
  (`direct_owner_call_has_declaration_anchor`,
  `owner_name_shadowed_by_unrelated_import`) and the owner bindings
  rule 7 reads.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`: rules 6
  and 7 (`ts_changed_value_is_observed`,
  `ts_observed_local_aliases_owner`), and rule 8's `static_unknown` arm
  in the verdict ladder.
- `crates/ripr/src/analysis/language/typescript/static_limit.rs`: rule 8
  (`static_limit_for_change`, `imported_symbol_call`,
  `named_limitation_for_static_limit`).
- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `collect_related_mock_paths`, unchanged for the advisory limit; rule 8
  reads mock paths per test file.
- `crates/ripr/src/analysis/language/typescript/actionability.rs`:
  unchanged.

## Metrics

- `typescript_oracle_reach_acceptance_mismatch`: acceptance examples whose
  reported kind, strength, relation or class differs from this spec; must
  be zero.
