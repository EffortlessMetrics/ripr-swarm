# Fixture: typescript_caught_assertion

Spec: RIPR-SPEC-0243

Issue: #8242

## Given

The exported async `load` rejects with a primitive reason when its token is
absent. The test awaits a literal rejection check inside `try` with an empty
`catch`. A wrong reason makes the matcher fail, but the catch swallows that
failure, so the test still passes.

## When

From `input`, run `npm ci --ignore-scripts` and `npm test` with the retained
Vitest 5.0.5 lockfile. Repeat with `TOKEN_WRONG` as the thrown reason. Both
runs execute one passing test. Removing the try/catch from the wrong-reason
variant makes that same assertion fail. The production analyzer command is:

```bash
ripr check --root input --diff ../diff.patch --format json --mode fast
```

## Then

One changed error-path finding on line 3 remains `weakly_exposed`. The related
test remains present with an unknown oracle. Assertion-like syntax that is
withheld stays unresolved; it must not be presented as established absence.
The independent evidence-promotion-honesty row prohibits promotion even if a
wrong golden is blessed.

## Must Not

- Supply exposure credit from the protected try body when a catch is present.
- Remove credit from an independent assertion after the caught block.
- Claim that catch handlers, finally blocks or returned promises are analyzed
  for all control-flow effects. Protected-body assertions are conservatively
  withheld even for an always-rethrowing handler.
- Promote TypeScript's support tier or claim mutation adequacy.
