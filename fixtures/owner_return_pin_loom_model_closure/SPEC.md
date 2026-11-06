# Fixture: owner_return_pin_loom_model_closure

Spec: RIPR-SPEC-0240

## Given

The return of `weight(input)` changes from `input * 2` to `input * 3`.
The only related test puts its exact `assert_eq!(weight(4), 12)` in the closure
passed to `loom::model`. ripr does not read the runner, so it cannot say how
often the runner calls the closure (#11543).

## When

`cargo xtask fixtures owner_return_pin_loom_model_closure` runs the public analyzer.

## Then

The finding is `static_unknown` (a limit of ripr's reading), with no
"make the check run on every default test run" repair advice, and the
refusal note names the model-checking runner closure.

## Must Not

- Read a runner closure as a test gap or a repair claim.
- Credit the assertion as an `exposed` discriminator.
- Widen the limit to a closure handed to an unknown function
  (`owner_return_pin_uncalled_closure` stays a real gap).
