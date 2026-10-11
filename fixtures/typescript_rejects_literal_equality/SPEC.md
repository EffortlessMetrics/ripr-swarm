# Fixture: typescript_rejects_literal_equality

Spec: RIPR-SPEC-0243

Issue: #7315

## Given

The exported async `load` function rejects with a primitive string when an
optional token is absent. The awaited Vitest `.rejects.toBe("TOKEN_REQUIRED")`
unwraps the rejection and compares that reason with the literal. It observes
the error path, not the resolved return value. Replacing the thrown string
with `"TOKEN_MISSING"` must fail the actual test after the module loads.

## When

From `input`, `npm ci --ignore-scripts` then `npm test` runs the actual
framework test. Replacing the thrown literal is the negative experiment.
`ripr check --root input --diff ../diff.patch --format json --mode fast`
exercises the production CLI. The committed analyzer regression reads this
same source and test through the real TypeScript extractor and classifier.

## Then

The diff changes the thrown rejection reason on line 3 and the normal return
on line 5. The rejection finding is an `error_path` with an
`exact_error_variant` / strong related-test oracle and is `exposed`.
The return finding stays `weakly_exposed` with no relevant oracle because
this test never observes a resolved value. Its displayed error oracle
preserves the `.rejects` chain. This is static evidence, not a general
runtime or mutation guarantee.

## Controls

- `.resolves.toBe("ready")` remains `exact_value` and cannot expose this throw.
- A `.rejects` equality cannot expose a changed ordinary return value.
- Broad, dynamic, custom and negated rejection checks retain their limits.
- New credit requires an unshadowed `expect` import from `vitest` or
  `@jest/globals`; implicit globals retain their existing preview behavior.
- A relation-only re-export cannot bind a runner assertion. The sibling
  `typescript_rejects_reexport_shadow` fixture consumes both correct and wrong
  rejection reasons and must retain weak error-path credit.

## Must Not

- Credit the rejected reason as a discriminator for a normal return value.
- Extend the new credit to an unknown or shadowed `expect` binding.
- Claim runtime adequacy, a support-tier change, or a complete repair packet.

Observed with Node 24.19.0 / Vitest 4.1.11: the fixture passes; changing the
thrown string fails each of `toBe`, `toEqual` and `toStrictEqual` after module
load. Changing the normal return or replacing the exact rejection assertion
with `toBeTruthy()` leaves the respective wrong-value control passing.
These observations establish the two fixture expectations independently of
the analyzer and its golden outputs.

## Known Integration Blocker

`counterexamples/unexercised-error` adds a second throw that this test never
executes. Copy its owner into the pinned runtime fixture and apply its diff:
the actual Vitest test passes on both versions, so the changed second throw
must not be exposed. The candidate currently promotes it. The active unignored
regression `rejection_literal_cannot_expose_an_unexercised_error_branch`
intentionally fails and blocks readiness until the expected-value error-liveness
owner (#6798) integrates a guard for the rejected literal. The literal stays
in `expected_value_or_variant`, with no `error_payload`; incorporating
#6798 alone is not proof that its guard covers this equality.
