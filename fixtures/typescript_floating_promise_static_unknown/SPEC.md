# Fixture: typescript_floating_promise_static_unknown

Spec: RIPR-SPEC-0027
Spec: RIPR-SPEC-0108

## Given

`f` changes its threshold from `n >= 100` to `n > 100`. The only test asserts
through a floating promise matcher (`expect(Promise.resolve(f(100))).resolves`
without `await`), so the assertion cannot fail the test. The changed value is
built to sink to the owner return, which is exactly the propagation stage.

## When

`cargo xtask fixtures typescript_floating_promise_static_unknown` analyzes
the changed threshold through the real CLI.

## Then

One `static_unknown` finding. The TypeScript adapter never emits
`propagation_unknown` (#12922): an input built to reach the propagation stage
reads `static_unknown`, never `exposed` and never the unemitted class.

## Must Not

Promote the floating assertion to `exposed`, emit `propagation_unknown`, or
emit a delegatable repair packet. The RIPR-SPEC-0108 honesty corpus
independently forbids promotion and notes the unemitted propagation class in
the case vector.
