# RIPR-SPEC-0234: TypeScript oracle and reach rules

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

Linked PRs:

- None yet

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
  reaches `exposed`.
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
Paragraphs headed Rule 1 to Rule 8 mark behavior changes, numbered in
reading order. Every other statement below is current behavior and is
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

### Assertion collection

- Only the test callback's own statement list is walked, through blocks,
  `if`/`else`, loops, labels, `switch`, `try`/`catch`/`finally` and
  `with`. An assertion is read from an expression statement or a
  `return` argument, after removing one `await`.
- An assertion inside a nested callback (`forEach`, `waitFor`, `act`, a
  subtest) or inside a declaration initializer is not collected.

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
| `toMatchSnapshot`, `toMatchInlineSnapshot` | `snapshot` / medium |
| `toHaveBeenCalled`, `toHaveBeenCalledWith`, `toHaveBeenCalledTimes`, `toHaveBeenLastCalledWith`, `toHaveBeenNthCalledWith` | `mock_expectation` / medium |
| `toBeTruthy`, `toBeFalsy`, `toBeDefined`, `toBeUndefined`, `toBeNull`, `toBeNaN` | `smoke_only` / smoke |
| `toContain`, `toMatch`, `toBeGreaterThan`, `toBeGreaterThanOrEqual`, `toBeLessThan`, `toBeLessThanOrEqual`, `toHaveLength`, `toHaveProperty` | `relational_check` / weak |
| any other matcher | `unknown` / unknown, `typescript_custom_matcher_unresolved` |

`.resolves` and `.rejects` use the same table. A snapshot never reaches
`exposed` by itself, because only a strong oracle does; it carries
`typescript_snapshot_discriminator_unresolved`. A mock matcher never
reaches `exposed` either, even with a literal payload.

The exact error payload override reads `exact_error_variant` / strong for
`toThrow` or `toThrowError` (with or without `.rejects`) with exactly one
argument that is a string literal, an all-literal object, or a member path
whose first segment starts with an ASCII uppercase letter, and for
`.rejects.toMatchObject` with an all-literal object. A regex, a lowercase
identifier and a template literal stay `broad_error` / weak.

**Rule 4. The base `Error` class is not an exact payload.** A payload whose
whole text is `Error` reads `broad_error` / weak, because every thrown
error is an `Error`. Only bare `Error` changes: `globalThis.Error` already
reads `broad_error` under the uppercase-first gate, and other PascalCase
class paths (`TypeError`, `Errors.ParseError`) keep RIPR-SPEC-0097's
reading.

### Other assertion tables

- Execution-context receiver (AVA, tape, node:test `t`): the table in
  RIPR-SPEC-0085. The receiver is the callback's first parameter whatever
  its name; no runner import is required. A method outside the table, and
  `t.assert.equal(...)`, yield no assertion.
- `node:assert` / chai `assert`: `strictEqual` and `deepStrictEqual` are
  `exact_value` / strong; `equal` is strong only under a strict module or
  the `strict` export; `deepEqual` is strong unless the module is legacy
  `node:assert`; the other equality forms and `match` are
  `relational_check` / weak; `ok`, chai `is*` and a callable `assert(x)`
  are `smoke_only` / smoke; `throws`, `doesNotThrow`, `rejects` and
  `doesNotReject` are `broad_error` / weak. A binding shadowed in the test
  body, a callback parameter or an enclosing describe scope does not
  count.
- chai BDD: `equal`, `equals`, `eq`, `eql`, `eqls` are `exact_value` /
  strong, or `relational_check` / weak under `not`; `throw`, `throws` and
  `Throw` are `broad_error` / weak with or without an argument; `include`,
  `match`, `above`, `below`, `least`, `most`, `lengthOf` and the like are
  `relational_check` / weak; the property terminals `true`, `false`, `ok`,
  `null`, `undefined` and `exist` are `smoke_only` / smoke. Any other
  chain word or terminal yields no assertion.

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
  does not contain the owner name as a substring (the side channel rule,
  which stays a substring test so that it fails closed).

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
`exposed` class hold. A default-import local is not covered (see
Non-Goals).

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
`exposed` stands, with the limit as advisory below. The result then is: reach Yes, observe and
discriminate Unknown, `static_limit_kind: mocked_module`,
`gap_state: static_limitation` and `typescript_mock_only_observer`. The
mock specifier resolves from the test file and the import source from the
owner file; both must name the same module. An unresolved mock specifier
counts as a match (fail closed). Other ladder results are unchanged.

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
2. **Value-family guard.** Adopted: the code's guard on ReturnValue and
   FieldConstruction, because it is the fail-closed reading and existing
   tests pin it (`ts_returnvalue_unrelated_strong_assertion_downgrades`).
   RIPR-SPEC-0098 is amended. Rejected: drop the value guard to match
   RIPR-SPEC-0098's text, which would restore false `exposed` on unrelated
   assertions.
3. **Alias rename (D2).** Adopted: rule 7, alias arm only, because
   RIPR-SPEC-0102 is accepted and states the class stays `exposed`. A
   default-import local gets no new credit, because RIPR-SPEC-0102 puts
   default imports out of scope. Rejected: amend
   RIPR-SPEC-0102 to `weakly_exposed`, which would keep a false downgrade
   of the most direct discriminator.
4. **Substring owner match (D3).** Adopted: rule 6 in the value branch
   only. Rejected: whole identifiers in the effect side-channel rule too,
   because there a substring miss grants credit, so tightening it would
   add credit.
5. **Static limits (D4).** Adopted: rule 8, fail closed. A mock of a
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
   pin the thrown type. A class-only payload on a message-only change
   stays an open over-credit, filed below with expected-value liveness.
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
    spec promises.

## Required Evidence

- Each Problem table row reads as the acceptance examples state, in JSON
  `class`, `related_tests[]` and `evidence`.
- Rules 2 to 6 and rule 8 move no finding to a stronger class. Golden
  drift lists every finding that moved and why.
- Rules 1 and 7 move a finding to a stronger class only for an awaited
  root (rule 1) or an alias-rename local (rule 7).
- Rule 8 is the only path to a TypeScript `static_unknown`, and an
  `exposed` finding never carries `typescript_mock_only_observer`.
- No TypeScript finding has class `reachable_unrevealed`.
- The value-family guard keeps the one-hop local credit
  (`const r = price(150); expect(r).toBe(130)` stays `exposed`).

## Non-Goals

- No collection of assertions inside nested callbacks or subtests.
- No resolution of custom matchers, `expect.extend`, or helper bodies.
- No expected-value liveness check for ReturnValue or ErrorPath.
- No infection or propagation modeling.
- No change to the per-test display oracle (filed separately).
- No new credit for a default-import local in the observation guard.
  RIPR-SPEC-0102 puts default imports out of scope, and RIPR-SPEC-0095
  does not follow default imports through re-export chains. A
  default-import call whose local name differs from the owner's name
  keeps `import_path_affinity` and `weakly_exposed`; a default import
  bound under the owner's own name stays `exposed` (example 17).
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
   `weakly_exposed` (unchanged).
3. `expect(price(150)).toMatchSnapshot()`: `snapshot` / medium,
   `weakly_exposed`, `typescript_snapshot_discriminator_unresolved`
   (unchanged).
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
    `unknown`, `weakly_exposed` (unchanged).
11. `if (ok) { expect(price(150)).toBe(130); }`: `exposed` (unchanged).
12. `src/lib.ts` is `export function parse(s: string): string { if (s ===
    "") { throw new Error("empty"); } return s; }`, changed to `"blank"`;
    test `expect(() => parse("")).toThrow("blank")`:
    `exact_error_variant` / strong, `exposed` (unchanged).
13. Same change; `expect(() => parse("")).toThrow(Error)`: `broad_error` /
    weak, `weakly_exposed` (today `exact_error_variant` / strong,
    `exposed`). `toThrow(globalThis.Error)`: `broad_error` / weak
    (unchanged). `toThrow(TypeError)`: `exact_error_variant` / strong
    (unchanged).
14. Same change; `expect(() => parse("")).to.throw("blank")` with chai
    `expect`: `broad_error` / weak, `weakly_exposed` (unchanged).
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
    `import_path_affinity`, `weakly_exposed` (unchanged, measured in
    review).
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
    `mock_expectation` / medium, `weakly_exposed` (unchanged).
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

## Test Mapping

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
- Planned: one unit test per changed example (5, 6, 7, 8, 13, 16, 18, 19,
  22, 23, 24, 31, 32, 33), and a verdict corpus holding all 33 examples, each with
  the asserted parsed subject (registered test, collected assertion,
  relation) checked before the class.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/oracle.rs`: rules 2 to 4
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
