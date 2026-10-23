# Fixture: typescript_then_inline_receiver_shadow

Spec: RIPR-SPEC-0246 rule 2; RIPR-SPEC-0108

## Given

An expression-bodied test arrow returns the real owner's promise. Its `.then`
callback declares an inline local `total` and asserts on that local's result.
Changing the real owner from 30 to 31 does not fail the assertion. The declaration
must remain inline: a newline before `const` hid this defect in the old scan.

## When

Run `ripr check --root input --diff ../diff.patch --format json --mode fast`.
The native `typescript_implicit_promise_return` integration target contrasts
JavaScript/TypeScript, zero/one callback parameters, implicit/explicit/awaited
promises, inline statement boundaries and independent nested scopes. Actual
Vitest 5.0.5 runtime controls establish passing tests for both owner values;
the plain resolved-value assertion fails for the wrong value.

## Then

One nonempty changed return-value finding remains `no_static_path`: the
owner-binding guard conservatively refuses this test's shadowed owner use.
The independent honesty row prohibits promotion even if a wrong golden is blessed.

## Must Not

- Attribute the local function assertion to the imported owner.
- Withhold credit merely because an unused binding exists in a separate scope.
- Change promise completion, runner behavior, the Preview tier or the existing
  nested implicit-return and implicit `test.each` limitations.
