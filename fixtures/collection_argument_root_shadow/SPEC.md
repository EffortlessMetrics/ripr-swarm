# Fixture: collection_argument_root_shadow

Spec: RIPR-SPEC-0094

## Given

A local `mod std` redirects the qualified vector name to a custom length-only collection from an indexed companion file.

## When

The real public check reads the neutral push rewrite.

## Then

Both effects stay `weakly_exposed`. Neutral and wrong-value compile and pass one named test; deleted push compiles and fails equality.

## Must Not

- Establish standard identity from qualified spelling alone.
- Lose the companion source when compiling runtime controls.
