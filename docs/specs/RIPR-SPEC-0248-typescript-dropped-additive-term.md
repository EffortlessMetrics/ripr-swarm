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
tested input where the term is not zero.** When the changed line and its
removed counterpart differ by exactly one span, and that span is one
additive term (`a + t` against `a`, `a - t` against `a`, or `t + a`
against `a`, in either direction), an `exposed` ladder result keeps its
class only when some strong assertion of an oracle relation observes an
owner call for which `t` folds to a non-zero number. Otherwise the finding
reads `weakly_exposed`, discrimination `weak`, and its missing line names
the term and asks for an input that makes it non-zero.

The fold binds:

- an owner parameter to the call's argument at its position when the
  argument is a plain numeric literal (or a test-scope constant the
  RIPR-SPEC-0027 boundary path already resolves) and the owner body never
  rebinds the parameter: no assignment, compound assignment, increment,
  shadowing declaration (including destructuring), and no nested
  function or arrow whose parameter could shadow it;
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

### Known limits

- The rebinding check reads the owner body after the change. A diff that
  removes a reassignment on another line can let the fold bind the
  pre-change argument.
- Under string concatenation `0` is not an identity (`"n=" + 0` differs
  from `"n="`), so a zero fold can withdraw credit from a string edit the
  test does catch. That costs a false gap, never false credit.

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
6. As example 2 with `fee = fee - 5;` on the changed line before the
   return: `weakly_exposed` (`fee` is rebound, so it is not bound).
7. `return a + b + 0` against `return a + b`: the rule does not apply.
8. `return a + b * c` against `return a * c`, `return x - a + b` against
   `return x - b`, `total += fee` against `total`, and `return +x`
   against `return`: the rule does not apply.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/dropped_term_tests.rs`
  pins examples 1 to 8.
- `fixtures/typescript-verdict-corpus` pins
  `ts-nodetest-quote-shipping-term` as `gap`.

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
