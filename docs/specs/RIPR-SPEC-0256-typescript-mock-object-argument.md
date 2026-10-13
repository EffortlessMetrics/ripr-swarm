# RIPR-SPEC-0256: TypeScript mock passed in an object argument

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
- Rust is not changed.

## Problem

The TypeScript verdict corpus reads `ts-vitest-checkout-notify` as a false
gap:

```ts
// src/cart.ts
export function checkout(owner: string, lines: Line[], notifier: Notifier): number {
  const total = lines.reduce((sum, line) => sum + lineTotal(line), 0);
  notifier.send(owner, `charged ${total}`);         // before
  notifier.send(owner, `charged ${total} cents`);   // after
  return total;
}

// test/cart.test.ts
it("tells the owner what was charged", () => {
  const send = vi.fn();
  checkout("ann", [apple], { send });
  expect(send).toHaveBeenCalledWith("ann", "charged 360");
});
```

RIPR-SPEC-0247 rule 1 credits a literal `toHaveBeenCalledWith` payload as
`mock_expectation` / strong, but refuses here twice:

- the owner body holds a nested function (the `reduce` callback), and rule
  1 refuses any nested function because one could run the changed call
  more than once;
- the test passes its mock as the `send` entry of an inline object, while
  rule 1 accepts only the mock itself, or a test-local object holding it,
  at the parameter's position.

The finding stays `weakly_exposed`, although the assertion fails on the
new message.

## Behavior

Both rules extend RIPR-SPEC-0247 rule 1. Every condition rule 1 states
and these rules do not replace still holds: the changed line calls an
owner parameter `p` as `p.m(...)`, the owner mentions `p` exactly twice
(its declaration and this call), every payload argument is a string,
number, boolean or `null` literal, and the test calls the owner exactly
once.

**Rule 1. A nested function away from the changed line does not refuse.**
The owner body still has no loop and no call back into the owner. A
nested function is an `=>`, a `function`, or any `{` that could open a
method body. Only an allowlist of `{` shapes reads as a plain block or
object literal: one at the start of the body; one after `=`, `(`, `,`,
`[`, `:`, `?`, `&`, `|`, `!`, `;` or `{`; one after `if (...)`,
`for (...)`, `while (...)`, `switch (...)` or `with (...)` when the
keyword starts a statement (after the start of the body, `;`, `}`, `)`,
`else`, or a `{` that itself opens a block), and after `catch (...)` when
`catch` follows a `}`; one after `else`, `try`, `finally`, `do` or `return`; and a template
literal's `${`. Every other `{` counts, among them one after `name(...)`,
after any other word, after `>`, `]`, `}` or a string literal (a typed
method such as `run(): { n: number } {` or `run(): "a" {`, or a class
body), one after a `)` with no matching `(`, an object method named
like a keyword (`{ catch(e) { ... } }`, `{ a: 1, if(x) { ... } }`), and one
after a block word that ends a property access (`run(): typeof cfg.try {`). This
also closes a RIPR-SPEC-0247 gap, where method shorthand
(`const h = { run(x) { p.m(x) } }`) counted as no nested function. When the
body holds a nested function, the changed line is accepted only when it is
a whole top-level statement of the body:

- its masked text occurs exactly once in the masked body;
- it holds no `=>`, no `function` and no `{` that counts as a method
  body, so a one-line method shorthand on the changed line refuses;
- it sits outside every brace, parenthesis and bracket of the body, and
  no closer before it lacks its opener;
- the text before it ends at the start of the body, a `;` or a `}`;
- it opens with an identifier other than `in`, `instanceof`, `as`,
  `satisfies`, `of`, `else`, `catch`, `finally` or `while`, so it cannot
  continue the expression before it (`const each = (id) => id < 0 ? {}`
  then `: p.m(id);`, where the `}` closes an object literal inside an
  arrow, not a block).

So the line cannot be a callback body, the body of an expression-bodied
arrow on the line before, or the branch of a braceless `if` / `else`.

**Both paths: the test's one owner call runs once.** For RIPR-SPEC-0247
rule 1's direct mock and for rule 2 below, the test's single owner call
site must not sit inside a loop or a nested function of the test's own
block: the block holds no `for`, `while` or `do`, and the block's text
before the call holds no `=>` and no method-like `{` (the same allowlist
as rule 1, which also covers `function` and class bodies), so a callback
(`[7, 8].forEach((id) => announce(id, send))`), a method, getter or class
field that may hold the call refuses. So does a call inside a parameter
list, an enclosing `(` whose `)` is followed by `=>`, `{` or a `:` return
type (`(r = announce(7, send)) => r`), since a default runs on every call
that omits it; an `if`, `switch` or `with` condition is not a parameter
list. A `vi.fn` or `jest.fn` factory's own type arguments and
implementation (`vi.fn<(m: string) => void>()`, `jest.fn((a, b) => a)`)
are skipped when they close before the call; type arguments count only
when a `(` follows them and they hold no `;`, `{` or `}` and balanced
parentheses, so a comparison such as `cfg.fn<3` hides nothing.
Any other arrow before the call refuses, even one that ends before it,
which under-credits rather than credit a call that may run twice. A call site that runs twice records
two calls, and `toHaveBeenCalledWith` passes when any of them matches. This
tightens RIPR-SPEC-0247.

**Rule 2. A mock passed as the `m` entry of an inline object.** For an
asserted target that is a single name `local` (`expect(send)`), the
payload credits the changed `p.m(...)` call when all of these hold:

- `local` is bound at the test callback's top level to `jest.fn(...)` or
  `vi.fn(...)`, as RIPR-SPEC-0247 rule 1 requires of a mock;
- the test mentions `local` exactly twice outside `expect(local)`: its
  declaration and the object entry below;
- the argument of the test's one owner call at `p`'s position, with no
  spread at or before it, is a flat object literal and nothing else: every
  entry is `key: value` or shorthand `key` with a plain identifier key,
  `m` is keyed exactly once, and its value is exactly `local`.

A spread, a method or accessor, a quoted or computed key, a second `m`
key, the mock under another key, an object built elsewhere, a member of
the literal (`{ send }.self`) and any further mention of the mock refuse.

### Decisions

1. **Top-level statement, not a scope analysis.** A text test of the
   changed line's position is enough to show no nested function contains
   it; anything less certain refuses. Rejected: dropping the nested-
   function refusal, which would credit a call inside `lines.forEach`.
2. **Inline literal only.** An object built earlier in the test can be
   changed between construction and the call; the inline literal cannot.
   Rule 1 of RIPR-SPEC-0247 already covers a test-local object.

### Known limits

- A changed line inside a braced `if`, `switch` or `try` block of an owner
  with a nested function refuses (under-credit).
- A changed call that spans several lines refuses when the changed line is
  not its first line.
- A block-bodied arrow owner, or an owner whose return type holds `{` or
  `=>`, puts its body one level down, so rule 1 refuses it. A `{` after a
  word the list above does not name (`label: {`) counts as a nested
  function.
- A mock reached through a nested object (`{ io: { send } }`) or a class
  instance refuses.

## Required Evidence

- On the TypeScript verdict corpus, `ts-vitest-checkout-notify` moves
  from `gap` to `credited` and no other row moves.
- `cargo xtask goldens check` reports no drift.

## Non-Goals

- No credit for object payloads or call counts.
- No change to how a mock assertion is parsed.
- No change to Rust, Python or other languages.

## Acceptance Examples

1. The corpus case: `exposed`.
2. As example 1 with `{ send: notify, flush: jest.fn() }` and
   `expect(notify)`: `exposed`.
3. As example 1 with an owner whose `const total` line has no callback,
   whose changed line follows a braced `if`, or comes first with a
   callback after it: `exposed`.
4. Rule 1 refuses the changed line inside a callback block, after
   `() =>` on the line before, under a braceless `if` or `else`, with a
   `for` loop or a recursive call elsewhere in the owner, when it appears
   twice, when it holds an arrow, a `function` or a one-line method
   itself, or inside a method shorthand, typed method (including a return
   type ending in `}` or a string literal) or class member that runs it
   twice, or a line that opens with `:` or `||` and continues an arrow
   expression ending in `{}` on the line before. A line inside an `if`
   block of an owner with only control blocks and object literals is
   still credited.
5. Rule 2 refuses a spread, a second `send` key, a quoted key, a method
   entry, the mock under `post`, a duplicate `send` key on either side of
   a renamed mock, an object built earlier, the object at another
   position, a member of the literal, a spread argument or a spread before
   the position, an owner call in a `for` loop or a `forEach` callback, a
   mock
   from a factory, a direct call to the mock, a second owner call, and a
   mock declared inside a nested block.
6. An object payload or `toHaveBeenCalledTimes` is not credited.

## Test Mapping

- `crates/ripr/src/analysis/language/typescript/tests/mock_object_argument_tests.rs`:
  `mock_in_an_inline_object_is_credited` pins examples 1 to 3;
  `changed_call_a_nested_function_may_run_is_not_credited` pins example 4;
  `mock_the_object_may_not_carry_is_not_credited` pins examples 5 and 6.
- `crates/ripr/src/analysis/language/typescript/tests/literal_matcher_tests.rs`:
  `mock_payload_another_call_could_satisfy_is_not_credited` pins the
  one-call rule on RIPR-SPEC-0247's direct-mock path.
- `fixtures/typescript-verdict-corpus` pins example 1:
  `ts-vitest-checkout-notify` reads `credited`.

## Implementation Mapping

- `crates/ripr/src/analysis/language/typescript/mock_payload_guard.rs`:
  `runs_changed_line_once`, `has_method_like_block`, `control_keyword_at`,
  `brace_context`,
  `changed_line_is_top_level_statement` and `starts_new_statement` (rule 1);
  `owner_call_argument_at`, `opens_function_before_call`,
  `inside_parameter_list` and `without_closed_mock_factories` (the one-call
  rule); `MockCallFacts::credits_object_argument` and
  `object_literal_binds` (rule 2).

## Metrics

- `typescript_mock_object_argument_mismatch`: acceptance examples whose
  observed reading differs from this spec. Target: zero.
