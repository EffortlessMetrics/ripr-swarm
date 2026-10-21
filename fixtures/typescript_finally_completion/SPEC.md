# Fixture: typescript_finally_completion

Spec: RIPR-SPEC-0243

Issue: #8508

## Given

The exported async `load` rejects with a primitive reason when its token is
absent. The test awaits a literal rejection check inside a catch-free `try`
whose `finally` returns. The finalizer's return replaces the completion, so a
wrong reason makes the matcher reject but the test still passes.

## When

From `input`, run `npm ci --ignore-scripts` and `npm test` with the retained
Vitest lockfile. Repeat with `TOKEN_WRONG` as the thrown reason: both runs pass
one test, while removing the `finally` makes the wrong-reason run fail. The
production analyzer command is:

```bash
ripr check --root input --diff ../diff.patch --format json --mode fast
```

## Then

One changed error-path finding on line 3 remains `weakly_exposed`. The related
test remains present with an unknown oracle. Withheld assertion-like syntax
stays unresolved, not established absence.

## Must Not

- Supply exposure credit from a try body or catch handler whose enclosing
  finalizer can return, throw, or break/continue out of the try.
- Remove credit from a benign finalizer, a finalizer whose return is inside a
  nested function, or a finalizer's own or an outside assertion.
- Claim general abrupt-completion or promise dataflow analysis.
- Promote TypeScript's support tier or claim mutation adequacy.
