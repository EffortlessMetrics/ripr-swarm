# RIPR-SPEC-0262: TypeScript strict custom matcher

Status: proposed

Owner: product / analysis

Created: 2026-10-13

Linked proposal:

- None yet

Linked ADRs:

- None yet

Linked plan:

- None yet

Linked issues:

- #6654 (TypeScript corpus cases scored false after RIPR-SPEC-0243)
- #5506 (custom matchers registered with `expect.extend`)

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
- Rust is not changed.
- Amends the RIPR-SPEC-0243 matcher table row for any other matcher and
  its custom-matcher non-goal.

## Problem

The TypeScript verdict corpus reads `jest-cents-custom-matcher` as a false
gap:

```ts
// src/cents.ts
export function toCents(amount: number): number {
  return Math.round(amount * 100);
}

// test/cents.test.ts
expect.extend({
  toBeCents(received: number, expected: number) {
    const pass = Number.isInteger(received) && received === expected;
    return { pass, message: () => `...` };
  },
});

test("dollars convert to whole cents", () => {
  expect(toCents(1.5)).toBeCents(150);
});
```

RIPR-SPEC-0243 reads every matcher it does not know as `unknown`, so the
finding stays `weakly_exposed`, although `toBeCents` passes only when the
received value is exactly `150` and the `100 / amount` mutant fails it.

## Behavior

**Rule 1. A same-file matcher that passes only on strict equality reads
as `toBe`.** `expect(x).<name>(y)` reads `exact_value` / strong when the
test file registers `<name>` in the shape below and nothing else in the
file may register or replace it:

- the registration is a top-level statement
  `expect.extend({ <name>(received, expected) { ... } })`, with `extend`
  spelled out (not escaped); the value may also be a `function`
  expression or an arrow function. It is synchronous, not a generator, and
  has exactly two distinct plain identifier parameters with no default
  value (`e = r` may bind `expected` to `received`);
- the body is `return { pass: X, ... }`, or `const pass = X;` followed by
  `return { pass, ... }`. The returned object keys `pass` once and has no
  spread, getter, method or computed key, and every other value is inert:
  a function, a name, or a literal. JavaScript evaluates the properties in
  order, so any other value could write a parameter before `pass` runs;
- `X` is a `&&` chain one of whose terms is `received === expected` (either
  order), and every term is built only from names, literals, templates,
  member reads, calls, and unary, binary, logical, conditional and comma
  operators. Rebinding a parameter needs an assignment, an update, a
  nested function or class (a `for (r of ...)` loop or a static block) or
  the name `eval` or `arguments`, so any of those, or any other form,
  refuses. A call to a function declared outside the matcher cannot
  rebind its parameters, so such a term can only make the matcher fail
  more often;
- every `expect.extend` object in the file has only plain keyed entries,
  the word `extend` occurs in the file exactly as often as those
  top-level calls, `<name>` is registered once, and `<name>` is not a
  matcher RIPR-SPEC-0243 already reads (a re-registered `toBeCloseTo`,
  which that spec reads as unknown, is read from its body);
- the file reaches `expect` only by name: it holds no `\u` escape; every
  mention of the word `expect` is `expect(`, `expect.` or a plain
  `import { expect }`; it has no namespace import, no default import from
  `@jest/globals`, `@jest/expect`, `vitest` or `bun:test`, and no dynamic
  `import(` or `import` mention beyond its import declarations; no `[`
  after a name, `)`, `]`, `}`, `!`, `?.`, a block comment or a non-ASCII
  character holds anything but a number (so no computed member can build
  `"ext" + "end"`; a reserved word starts an array instead, but not after
  `.`, and `of`, `keyof` and `readonly` count as names); and it never
  names `global` (outside the
  whole phrase `declare global`), `globalThis`, `window`, `self`, `this`,
  `document`, `constructor`, `Reflect`, `Function`, `eval`, `require`,
  `requireActual`, `requireMock`, `importActual`, `importMock`,
  `createMockFromModule`, `valueOf`, `getOwnPropertyDescriptor`,
  `getOwnPropertyDescriptors`, `Symbol`, `chai`, `getPrototypeOf` or
  `__proto__`, each of which reaches `expect`, the runner's matcher
  table or an assertion's prototype, or runs code, under a spelling the
  scan cannot see. Comments and strings count too, so a
  mention only refuses;
- the assertion passes exactly one argument, which is not a spread, and
  uses no `.not` or `.rejects` modifier.

Any other shape keeps the RIPR-SPEC-0243 `unknown` reading. The class still
follows the RIPR-SPEC-0243 ladder, including the observation guard. A
credited matcher also carries reference identity on the assertion, so
RIPR-SPEC-0246 rule 1 treats it like `toBe` for a copy-for-identity
change. That property is not inferred from `exact_value`: `toEqual`
shares that kind and does not pin identity.

### Decisions

1. **Read the body, not the name.** The matcher's own `pass` expression
   decides whether a changed value fails it. A name such as `toBeCents`
   says nothing about how it compares. Rejected: credit every custom
   matcher as medium, which would credit `pass: true`.
2. **One registration the file shows whole, or refuse.** A registration
   inside a hook, helper or nested scope of this file, an
   `expect.extend(matchers)` call, or a spread may register the same name
   with another body. Counting the word `extend` across the whole file is
   crude, but it over-counts only, so its errors refuse rather than
   credit.
3. **`&&` with a strict equality term is enough.** Each extra conjunct can
   only turn a pass into a fail, so the matcher is at least as strict as
   `toBe`. `||`, `!` and loose `==` are refused because they can pass a
   changed value.
4. **Refuse writes by shape.** The reader has no scope analysis, so a
   term that may write anything refuses even when the write is harmless.
   The term is checked against a short list of expression forms rather
   than scanned for write tokens, because a nested function or class can
   rebind a parameter with no `=` in sight.
   Rejected: allow writes to names other than the parameters, which needs
   scope resolution the adapter does not have.
5. **Refuse every route to `expect` that does not name it.** A hook may
   register the name again through an alias, a computed key, a module
   object or the global object, and the word count sees none of them. The
   reader refuses the routes by text, which loses credit in files that
   index objects or mention `this`, and keeps the false-credit rate at
   zero on the routes reviewed. Rejected: read `Object.is(received,
   expected)` as strict, because `Object` can be replaced by the same
   routes (`globalThis["Obj" + "ect"]`) and the corpus case does not need
   it.

### Known limits

- A registration outside this file is not seen. A setup file
  (`setupFilesAfterEnv`) or an imported helper that registers the same
  name again, for example from a `beforeEach`, runs after this file's
  top-level registration and replaces it; the credit then reads a body
  the runner does not use. A matcher registered only outside this file
  stays `unknown`.
- An imported function that registers the name again
  (`import { install } from "./matchers"; beforeAll(install);`) is not
  seen, because the registration is in another file.
- The route list is closed, not complete. It refuses the routes review
  found, ordinary and reflective. A file that builds code at runtime, or
  reaches the runner's matcher table through a reflective path the list
  does not name (a timer given a code string, for example), is not seen.
  So is an index that a line comment ending in a reserved word hides
  (`g // return` and then `["exp" + "ect"]` on the next line).
- `Object.is(received, expected)` stays `unknown`.
- A matcher whose comparison goes through a helper (`equals(r, e)`,
  `this.equals`) or another statement stays `unknown`.
- A matcher with a third parameter (`(received, expected, precision)`)
  stays `unknown`, even when its body is strict.

## Required Evidence

- On the TypeScript verdict corpus, `jest-cents-custom-matcher` moves
  from `gap` to `credited` and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No reading of matchers outside the test file.
- No reading of `this.equals`, asymmetric matchers or `toEqual`-style deep
  comparison in a custom matcher.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. The corpus case: `toBeCents` reads `exact_value` / strong and the
   finding is `exposed`. The same matcher as `return { pass: e === r }`,
   as `function (r, e) { return { pass: r === e }; }`, as a
   block-bodied arrow with `r > 0 && (r === e)`, with inert `message`,
   name and literal values beside `pass`, with `typeof`, call and template
   terms (one holding `"a=b"`), or in a file with a plain
   `import { expect }`, a default import from a module of the project,
   array patterns, `number[]`, `xs[0]` and `declare global`: strong.
2. `r == e`, `r !== e`, `r === e || r === 0`, `!(r === e)`,
   `Math.round(r) === e`, a term that writes a parameter (`r = e`,
   `[r] = [e]`, `r >>= 0`, `r <<= 0`, `r ||= e`, `r++`, `r--`, an `eval`, an
   `arguments` write, an arrow that assigns, a function running
   `for (r of [e])`, a class whose static block runs `for (r in ...)`), an
   object literal in a term, another property or an
   earlier `pass` evaluated before the comparison (`pre: (r = e)`,
   `pre: fix(r)`, `pass: (r = e), pass: r === e`, `pass: true, pass: r === e`),
   `Object.is(r, e)`, a default `e = r` or `r = 150`, an escaped
   `ev\u0061l`, duplicate parameters `(r, r)`, a
   statement before the `return`, a `let pass`, a `const ok` returned as
   `pass: ok` or as a shorthand `pass`, a bare shorthand `pass`, a spread
   in the returned object, an `async` matcher, an expression-bodied arrow,
   a destructured parameter, a one-parameter matcher or a
   three-parameter matcher: `unknown`.
3. A second registration of the name, `expect.extend(matchers)`,
   `expect.extend({ ...matchers })`, or an `expect.extend` inside
   `beforeAll` (also spelled `expect.extend (`, `expect.ext\u0065nd`, a
   computed `expect` member named `"extend"` or `"ext" + "end"`, with or
   without a space or parentheses, through `const e = expect`, or through
   a default import indexed as `g["exp" + "ect"]`), a hook registration
   beside a top-level `expect.ext\u0065nd`, a hook that calls every value
   of `expect`, of a namespace import, of a dynamic import or of any
   refused root (`expect.valueOf()`, `Object.getOwnPropertyDescriptor`,
   `Symbol.for`, `chai.Assertion` among them), a `global` beside
   a comment `declare global_x`, a default import indexed after a
   keyword-named property (`.of[`), through a variable named `of`, a
   non-ASCII name, an object literal or a comment, a default import from
   `@jest/globals`, a matcher set on `Object.getPrototypeOf(expect(0))` or
   `expect(0).__proto__`,
   `import { expect as e }`, or `import { fake as expect }`: `unknown`. A matcher named `toBe` keeps the built-in reading.
4. `toBeCents(150, 2)`, `toBeCents(...args)` or
   `await expect(p).rejects.toBeCents(150)`: `unknown`. `.not.toBeCents(150)`
   records no `toBeCents` assertion. A file that shadows `expect` reaches
   no runner matcher.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/custom_matcher_tests.rs`:
  `strict_equality_custom_matcher_reads_exact` pins example 1;
  `custom_matcher_that_may_pass_without_strict_equality_stays_unknown`
  pins example 2;
  `custom_matcher_another_registration_may_replace_stays_unknown` pins
  example 3;
  `custom_matcher_assertion_shape_must_compare_one_value` pins example 4.
- `crates/ripr/src/analysis/language/typescript/tests/nested_callback_tests.rs`
  pins that a credited matcher carries reference identity for
  RIPR-SPEC-0246 rule 1.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `jest-cents-custom-matcher` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/custom_matcher_guard.rs`:
  `strict_equality_matchers` reads the registrations.
- `crates/ripr/src/analysis/language/typescript/oracle.rs`:
  `expect_assertion_from_expression` reads a registered strict matcher as
  `exact_value` / strong and sets `compares_reference_identity`;
  `TypeScriptAssertionBindings` carries the names.
- `crates/ripr/src/analysis/language/typescript/types.rs`:
  `TypeScriptAssertion::compares_reference_identity`.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `ts_test_pins_identity` reads that property (RIPR-SPEC-0246 rule 1).
- `crates/ripr/src/analysis/language/typescript/tests_extract.rs`: binds
  the names once per test file.

## Metrics

- `typescript_strict_custom_matcher_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
