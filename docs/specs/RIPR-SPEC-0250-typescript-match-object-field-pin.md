# RIPR-SPEC-0250: TypeScript match-object field pin

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
- #7443 (RIPR-SPEC-0249)

Support-tier impact:

- No tier change. TypeScript stays Preview.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Adds `toMatchObject` to the RIPR-SPEC-0243 relational matcher row.
- Rust is not changed.

## Problem

The TypeScript verdict corpus reads `jest-mined-tomatchobject-field` as a
false gap:

```ts
// src/profile.ts
export function buildProfile(name: string, age: number) {
  return {
    name,
    adult: age >= 18,          // before
    adult: 18 <= age,          // after
    tags: [] as string[],
  };
}

// test/profile.test.ts
expect(buildProfile("ana", 17)).toMatchObject({ name: "ana", adult: false });
expect(buildProfile("bo", 18)).toMatchObject({ name: "bo", adult: true });
```

`toMatchObject` compares every field it lists exactly, as `toEqual` would,
and ignores the fields it leaves out. It lists `adult` at 17 and 18, so
every mutant of `18 <= age` fails it. ripr read `toMatchObject` as an
unrecognised matcher, so the finding was `weakly_exposed`. The same test
written with `toEqual` already read `exposed`.

## Behavior

**Matcher reading.** `expect(x).toMatchObject(o)` (and
`.resolves.toMatchObject(o)`) is a `relational_check` / weak oracle: on its
own it says nothing about the fields `o` leaves out. When `o` is a flat
object literal of scalar literals (`{ name: "ana", adult: false }`, the
shape RIPR-SPEC-0247 reads for mock payloads) the assertion records it as
its expected value. `.rejects.toMatchObject(o)` keeps its error-payload
reading (RIPR-SPEC-0243); one that reading cannot resolve stays `unknown`.

**Rule 1. A match-object pin reads exact for a returned field it lists.**
On a `field_construction` probe, a `toMatchObject(o)` assertion of an
oracle relation reads `exact_value` / strong, in the ladder, in its
related-test row and in the RIPR-SPEC-0098 observation guard, when all of
these hold:

- it observes the whole result of one owner call: `owner(...)`,
  `ns.owner(...)`, optionally awaited, or a local bound once to that call
  (not `f(owner(...))` or `owner(...).field`), and the test does not
  shadow the owner or, for `ns.owner(...)`, the namespace import `ns`
  (`const ns = fake()` in the body reaches the shadow, and a body
  `ns = await import(...)` counts only as one `const` the body uses
  solely as `ns.x`; amended 2026-10-15);
- the owner body has one `return` and no arrow or nested `function`;
- the changed line is `key: value` with `value` on that line (no `{`, `}`,
  `[` or `]` outside literals), found once in the owner;
- the literal that directly holds the changed line opens right after that
  `return` and closes with `}` followed by `;` or by a line that opens
  with `}`,
  so it is the whole returned value (`return { ... }.adult` fails);
- every top-level entry of that literal, the changed line's own included,
  is a plain `name: value`, shorthand `name` or method `name()`, and
  exactly one names `key`: the only entry on the changed line, which
  opens at the start of that line and ends on it (a quoted or computed key, a spread, an accessor
  or a second `key` entry fails);
- `o` lists `key` at its top level, and its string values are spelled
  with ASCII source text only (an object holding a non-ASCII string keeps
  the weak reading; an ASCII escape such as `"\u00e9"` is read; see Known
  limits).

Amended 2026-10-15 (#6654, identity helper): a pin on
`f(owner(...))` also observes the whole result of the owner call when `f`
is a test-local identity helper under the conditions RIPR-SPEC-0249
rule 1 lists in its 2026-10-15 amendment: a top-level, unexported
`function f(p) { return p; }` or `const f = (p) => p;` in the same test
file, with one plain identifier parameter, a body of exactly `return p;`,
a name used nowhere else in the file but as a direct call `f(`, and a call
whose one argument is the owner call itself. A helper that rebuilds or
transforms its argument (`return { ...p, adult: true }`) is not an
identity and keeps the weak reading.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Recorded on #6654.

1. **Field-construction changes only.** Adopted: the pin compares exactly
   the fields it lists, so it discriminates a change to one of those fields
   of the returned object. Rejected: reading it strong on other seams,
   where an unlisted field or a value outside the object can carry the
   change.
2. **Weak, not unknown, by default.** Adopted: `toMatchObject` is a
   recognised matcher with known semantics, the same footing as
   `toHaveProperty`. Its default reading moves from `unknown` to
   `relational_check` / weak.
3. **Kind `exact_value`.** Adopted: the same reading `toEqual` gets on the
   whole object, limited to the listed field. No new oracle kind.
4. **One returned literal.** Adopted: an owner with several returns can
   reach the test through a literal the change does not touch. Rejected:
   reading which return a test input reaches.

### Known limits

- `typescript_oracle_confidence` reads the assertion's default (weak)
  strength, so a credited pin shows `low`.
- Expression-bodied arrows (`=> ({ ... })`), a literal assigned and then
  returned, shorthand fields (`adult,`), values that open a nested `{` or
  `[` literal or continue onto the next line (`adult: age >= 18` then
  `&& verified,`), and nested listed objects keep the weak reading.
- An expected object holding a non-ASCII string (`{ name: "José", adult:
  true }`) keeps the weak reading: the shared masker changes its byte
  length, so its keys are not read. The limit is on source spelling, not
  the decoded value: `"Jos\u00e9"` is read, and the pin still compares the
  decoded value exactly.
- A local bound once to the owner call is not checked for later field
  writes (`bo.adult = true`, `Object.assign(bo, ...)`, or a helper that
  mutates `bo`). The same gap applies to `toEqual` through the shared
  whole-result reader; filed as #7497.

## Required Evidence

- On the TypeScript verdict corpus, `jest-mined-tomatchobject-field` moves
  from `gap` to `credited` and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No reading of nested listed objects, arrays or asymmetric matchers
  (`expect.any`).
- No `toMatchObject` reading on predicate, error, side-effect or value
  seams.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. The problem's change and tests: `exposed`; the related row reads
   `exact_value` / strong.
2. As example 1 with `const bo = buildProfile("bo", 18);
   expect(bo).toMatchObject({ adult: true });`: `exposed`.
3. As example 1 asserting `toMatchObject({ name: "bo" })` only: not
   `exposed`.
4. As example 1 with `expect(normalize(buildProfile(...)))` or
   `expect(buildProfile(...).meta)`: not `exposed`. Amended 2026-10-15:
   `expect(normalize(buildProfile(...)))` is `exposed` when `normalize`
   is a test-local identity helper
   (`function normalize<T>(value: T): T { return value; }`); a helper that
   rebuilds its argument, or a `let` binding of it, stays not
   `exposed`.
5. As example 1 with a non-literal argument (`toMatchObject(expected)`) or
   `.rejects.toMatchObject(...)`: not `exposed`.
6. As example 1 with a second `return`, the field in a nested literal, a
   spread, a later `adult:`, `"adult":` or `["adult"]:` entry (on its
   own line, after another entry, or on the changed line), an accessor,
   `return { ... }.adult`, a changed `adult : 18 <= age` line that is
   the second arm of a multi-line `status: admin ?` ternary, or a changed
   `adult: age >= 18, senior: 18 <= age,` line where only `senior` changed:
   rule 1 reads no pin.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/match_object_tests.rs`
  pins examples 1 to 6 and the owner-side conditions;
  `match_object_pin_looks_through_a_test_local_identity_helper` pins the
  amended example 4.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `jest-mined-tomatchobject-field` reads `credited`; the amended example
  4: `jest-spec0250-ex04-normalized-result` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `oracle_for_matcher`, `expect_assertion_from_expression`.
- `crates/ripr/src/analysis/language/typescript/match_object_guard.rs`:
  `ts_owner_returned_field_key`, `ts_match_object_facts`.
- `crates/ripr/src/analysis/language/typescript/instance_class_guard.rs`:
  `PinFacts`, shared with RIPR-SPEC-0249.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `strongest_family_matching_oracle`, `project_credited_pin_rows`,
  `ts_changed_value_is_observed`.
- Amended 2026-10-15 (identity helper):
  `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `ts_observes_whole_owner_result_through_identity`, called by
  `ts_match_object_facts`; the helper list comes from
  `crates/ripr/src/analysis/language/typescript/tests_extract.rs`:
  `identity_helpers`.

## Metrics

- `typescript_match_object_pin_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
