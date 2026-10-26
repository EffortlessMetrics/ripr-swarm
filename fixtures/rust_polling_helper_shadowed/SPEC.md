# Fixture: rust_polling_helper_shadowed

Spec: RIPR-SPEC-0197

## Given

The async owner's return changes from `p + p / 10` to `p / 10 + p`, an equivalent arithmetic rewrite. A local closure shadows the otherwise verified polling helper and discards the owner future before comparing a constant.

## When

`cargo xtask fixtures rust_polling_helper_shadowed` runs the public analyzer and compares JSON and human output. The independent evidence-promotion honesty corpus pins the literal expected class. The matched `owner_pin_execution::polling_helper` controls separately compile correct and deliberately wrong owner variants from these exact sources.

## Then

Exactly one `return_value` finding on line 2 reads `static_unknown`. JSON and full human output agree. Existing unresolved-executor rules may choose their named limitation before one another; bound-expectation controls separately isolate the indirect-owner refusal.

Both correct and wrong owner variants complete one passing test; that assertion does not observe the owner return.

## Must Not

- Promote an unresolved helper, primitive, import root, method, or macro expansion.
- Treat compilation failure, an empty subject inventory, or zero process exit before test completion as a passing discriminator.
- Compute the expected classification from the analyzer decision being tested.
- Claim generalized async execution or acceptance of other #9478 shapes.
