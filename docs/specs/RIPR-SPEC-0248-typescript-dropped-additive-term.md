# RIPR-SPEC-0248: TypeScript dropped additive term

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
- #7418 (evaluate a same-module call term)

Linked PRs:

- #6686 (TypeScript verdict corpus)
- #7403 (RIPR-SPEC-0247)

Support-tier impact:

- No tier change. TypeScript stays Preview. The rule only withdraws
  `exposed` credit; it never adds credit.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `policy/doc-artifacts.toml` and
  `.ripr/traceability.toml`.
- No schema version bump. No new oracle kind, strength, class, relation
  reason or limitation name.
- Adds one post-ladder rule after RIPR-SPEC-0246 rule 1 and RIPR-SPEC-0243
  rule 8.
- Rust is not changed.

## Problem

After RIPR-SPEC-0247 the TypeScript verdict corpus has one false
`exposed` verdict, `ts-nodetest-quote-shipping-term`:

```ts
// src/pricing.ts
return { tier, totalCents: total + shippingFee(subtotalCents) };  // before
return { tier, totalCents: total };                                // after

// test/pricing.test.ts
assert.deepEqual(quote(6000, 100), { tier: "bronze", totalCents: 6000 });
```

`shippingFee` returns `0` at or above 5000 cents, so for the only tested
input the dropped term is `0` and the test passes on both versions. The
ladder credits the exact `deepEqual` without asking whether the change
can move the value the test compares.

## Behavior

**Rule 1. A change that only adds or drops one additive term needs a
tested input where the term is not zero.** On a value seam (a
`return_value`, `field_construction` or `predicate` probe), when the
changed line and its
removed counterpart differ by exactly one span, and that span is one
additive term (`a + t` against `a`, `a - t` against `a`, or `t + a`
against `a`, in either direction), an `exposed` ladder result keeps its
class only when some strong value assertion (`exact_value`,
`whole_object_equality` or `snapshot`; an error matcher such as
`rejects.toThrow` never sees the returned value) of an oracle relation
observes the whole result of an owner call (`owner(...)`, `ns.owner(...)`,
the `local(...)` of an `import { owner as local }` rename, optionally
awaited, or a local bound once to that call; not `owner(...).field` or
`f(owner(...))`; in `owner(owner(a, b), c)` the outer call's arguments) for
which `t` folds to a number of magnitude at least `1e-6`. A double sum
absorbs an addend below 2^-52 of its own magnitude (`6499 + 1e-13 ===
6499`), and a term of at least `1e-6` changes every sum below about
`4.5e9`. Otherwise the finding
reads `weakly_exposed`, discrimination `weak`, and its missing line names
the term and asks for an input that makes it non-zero. The discriminate
summary then says a change-specific rule withdrew the credit and repeats
that line, as it does after RIPR-SPEC-0246 rule 1.

The fold binds:

- an owner parameter to the call's argument at its position when the
  argument is a plain numeric literal (or a test-scope constant the
  RIPR-SPEC-0027 boundary path already resolves) and the owner body never
  rebinds the parameter: no assignment or compound assignment (with or
  without spaces, `fee=-0`, or through `p!`), increment, any `for (...)`
  header that mentions it, an assignment to any bracketed or parenthesized
  target holding it (`[p] = pair`, `[a, [p]] = x`, `({ a: { p } } = o)`,
  `(p as number) = 0`), shadowing declaration, and no `function` or arrow
  in the body;
- a constant-shaped name (`UPPER_CASE`) to the owner module's single
  immutable integer `const`.

Calls, members, strings and anything unbound fail the fold. A term that
folds to `0` for every observed call reads "is 0 for every tested input";
a term some call leaves unfolded reads "is not shown to be non-zero".

The term is recognised only when dropping it leaves the rest of the
expression intact:

- the term holds identifiers, numbers, literals, calls, members and
  arithmetic only, balanced, with no `?`, `:`, `,`, `=`, comparison or
  logical operator at its top level;
- after a trailing term comes the end of the expression or an operator
  that binds no tighter than `+` (not `*`, `/`, `%`, `**`, `.` or a call);
- before a leading term comes `(`, `[`, `,`, `=`, `:`, `?`, `{`, a
  comparison or logical operator, a binary `+`, or `return`;
- a `- t` term has no top-level `+` or `-` of its own (`a - b + c` to `a`
  drops `-b + c`, not `-(b + c)`);
- the text before a trailing term ends in an operand, not a keyword such
  as `return` (`return +x` to `return` is not a dropped term), and
  compound assignment (`+=`) and `++` / `--` are never terms.

A term that names nothing (`+ 0`, `+ ''`, `+ 0x10`) is outside the rule:
it is the same for every input, so a non-zero literal always changes the
value and a zero literal is an equivalent edit this rule does not judge.

### Decisions

The owner delegated these choices on 2026-10-04 ("make reasonable
documented decisions and proceed"). Recorded on #6654.

1. **Refuse unless witnessed.** Adopted: keep `exposed` only on a
   non-zero fold, as RIPR-SPEC-0027 keeps a boundary credit only on a
   witnessed boundary input. Rejected: refuse only when the term folds to
   zero, which leaves the corpus case (a call term) credited. Rejected:
   `static_unknown`, which needs a new static-limit kind across every
   renderer for one shape.
2. **No call evaluation yet.** Adopted: a call term is unwitnessed, which
   can read a discriminated call-term change as a gap (false actionable,
   never false credit). #7418 tracks folding a same-module `if`/ternary
   callee.
3. **Literal terms excluded.** Adopted: a term with no identifier is out
   of scope. Five runner and oracle fixtures use `+ 0` / `+ ''` as a
   neutral edit; reading them as gaps would judge equivalence, which ripr
   does not claim.

Error, side-effect, call-deletion and match-arm seams are outside the
rule: their matchers (`toThrow`, `toHaveBeenCalledWith`, state reads) are
ones the fold cannot read, and demoting them would ask for a repair the
test may not need.

### Known limits

- A non-zero fold is necessary, not sufficient. The fold does not ask
  whether the tested call reaches the changed line (`if (a > 10) return
  a + t;` tested with `f(1, 5)`) or whether the surrounding expression
  still lets the term move the result (`c ? a : b + t`, `a ?? b + t`).
  Those shapes keep the pre-rule credit.
- Method shorthand and accessors in an object or class literal inside
  the owner body (`{ add(fee) { ... } }`) are not read as nested
  functions, and a declarator scan stops at its first `=`, so a nested
  destructuring default (`const { a = 1, b: { fee } } = o`) is missed.
  Each can keep the pre-rule credit.
- The rebinding checks over-refuse: an indexed write keyed by the
  parameter (`rates[fee] = 1`) or a loop bound read from it
  (`for (let i = 0; i < fee; i++)`) unbinds it.
- Any arrow or nested function in the owner body makes every parameter
  unbound, so `items.reduce((s, x) => s + x, 0) + fee` reads
  `weakly_exposed` even with a non-zero `fee` argument.
- The rebinding check reads the owner body after the change. A diff that
  removes a reassignment on another line can let the fold bind the
  pre-change argument.
- A sum beyond about `4.5e9` can absorb a term between `1e-6` and 2^-52 of
  the sum; such a term still keeps the pre-rule credit. A term under
  `1e-6` reads as not shown to be non-zero even where the sum is small
  enough to show it.
- In `owner(owner(a, b), c)` only the outer call's arguments bind, so a
  non-zero term in the inner call (which still moves the outer result)
  does not witness the change.
- String terms never fold, and under string concatenation `0` is not an
  identity (`"n=" + 0` differs from `"n="`). Dropping `+ name` from
  `greeting + name` therefore reads `weakly_exposed` even when the test
  pins `"Hi Bob"`. That costs a false gap, never false credit.

## Required Evidence

- The rule only moves `exposed` to `weakly_exposed`.
- On the TypeScript verdict corpus, `ts-nodetest-quote-shipping-term`
  moves from `credited` to `gap` (false verdicts 10/101 to 9/101, false
  exposed 1/39 to 0/39), and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No multiplicative identity (`* 1`, `/ 1`).
- No equivalence judgment for literal terms.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. `return total + shippingFee(subtotal)` becomes `return total`; the
   only test asserts `quote(6000, 100)` exactly: `weakly_exposed`, missing
   line names `shippingFee(subtotal)` as not shown to be non-zero.
2. `return subtotal + fee + FLAT` drops `+ fee`; test
   `expect(price(6000, 5)).toBe(...)`: `exposed` (`fee` folds to 5).
3. As example 2 dropping `+ FLAT` where `export const FLAT = 499`:
   `exposed`.
4. As example 2 with the only test `price(6000, 0)`: `weakly_exposed`,
   "is 0 for every tested input".
5. As example 4 with a second assertion on `price(6000, 7)`: `exposed`.
6. As example 2 with `fee=fee-5;` on the line before the return:
   `weakly_exposed` (`fee` is rebound, so it is not bound); with
   `audit(fee);` there instead: `exposed`.
7. As example 4 with a second assertion
   `expect(Math.sign(price(6000, 7))).toBe(1)` or
   `await expect(price(0, 7)).rejects.toThrow("bad")`: `weakly_exposed`;
   neither assertion observes the returned value.
8. `return a + b + 0` against `return a + b`: the rule does not apply.
9. `return a + b * c` against `return a * c`, `return x - a + b` against
   `return x - b`, `total += fee` against `total`, and `return +x`
   against `return`: the rule does not apply.
10. `throw new RangeError("limit " + max)` against
    `throw new RangeError("limit ")` with `toThrow("limit ")`: the rule
    does not apply (error seam). The change is message-only under
    RIPR-SPEC-0243 rule 4, and `toThrow("limit ")` matches both `limit 5`
    and `limit `, so the finding reads `weakly_exposed` (it read `exposed`
    before the 2026-10-14 rule 4 amendment; corpus case
    `jest-spec0248-throw-message-term`).

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/dropped_term_tests.rs`
  pins examples 2 to 10, and example 1's message through the analogous
  `shipping(subtotal)` call term.
- `fixtures/typescript-verdict-corpus` pins example 1 itself:
  `ts-nodetest-quote-shipping-term` reads `gap`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/dropped_term_guard.rs`:
  `ts_dropped_additive_term`, `ts_parameter_is_stable`.
- `crates/ripr/src/analysis/language/typescript/classifier.rs`:
  `ts_apply_post_ladder_rules`, `ts_dropped_term_witness`.
- `crates/ripr/src/analysis/language/typescript/mock_payload_guard.rs`:
  `masked_owner_body`, shared with RIPR-SPEC-0247 rule 1.

## Metrics

- `typescript_dropped_term_mismatch`: acceptance examples whose observed
  reading differs from this spec. Target: zero.
